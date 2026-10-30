//! `/Contents` 的 PKCS#7/CMS SignedData（RFC 5652）DER 解析（12.8.4.3）。
//!
//! 解析范围：
//! - `ContentInfo`：只接受 `signedData`（`1.2.840.113549.1.7.2`）与其
//!   `[0] EXPLICIT` 内的 `SignedData`；
//! - `SignedData`：版本、`digestAlgorithms`、`encapContentInfo`（`data`，
//!   可带封装内容）、`certificates [0]`（保留原始 DER）、`crls [1]`
//!   （按长度跳过）与 `signerInfos`（至少一个）；
//! - `SignerInfo`：`sid` 的两种形态、摘要算法、`signedAttrs` 中的
//!   `messageDigest`/`signingTime`、签名算法、签名值与 `unsignedAttrs`
//!   （按长度跳过）。
//!
//! 能力边界：本模块只做结构解析与摘要绑定所需的字段提取，不验证
//! RSA/ECDSA 签名值、证书链/信任与时间戳。所有输入按不可信数据处理：
//! 只接受定长 DER，长度越过剩余字节即错误，深度/元素数/字节数受
//! [`CmsLimits`] 限制，未知条目按声明长度跳过而非扫描。

use std::fmt;

use crate::core::types::{Document, Value};

use super::bytes::pdf_string_bytes;
use super::entries::{DigestAlgorithm, DocumentSignature};
use super::{SignatureError, malformed};

mod der;
mod signer;

use der::{Cursor, ParseState, Tlv, decode_integer, decode_oid};
use signer::{parse_algorithm_identifier, parse_signer_info};

/// PKCS#7 `signedData` ContentInfo 类型 OID。
const OID_SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
/// PKCS#7 `data` 内容类型 OID（detached 签名的 `encapContentInfo`）。
const OID_DATA: &str = "1.2.840.113549.1.7.1";

/// CMS 解析上限（默认 16 MiB / 32 层 / 4096 个 TLV）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmsLimits {
    /// DER 总字节上限。
    pub max_der_bytes: usize,
    /// 构造容器嵌套深度上限。
    pub max_depth: usize,
    /// 读取的 TLV 元素数上限。
    pub max_elements: usize,
}

impl Default for CmsLimits {
    fn default() -> Self {
        Self {
            max_der_bytes: 16 * 1024 * 1024,
            max_depth: 32,
            max_elements: 4096,
        }
    }
}

/// CMS DER 解析中的类型化错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmsError {
    /// 数据在元素中途结束。
    Truncated { offset: usize },
    /// 出现 DER 不允许的 indefinite length。
    IndefiniteLength { offset: usize },
    /// 长度字段超出支持的位宽（长度字节数 > 8 或累加溢出）。
    LengthOverflow { offset: usize },
    /// 声明长度越过当前容器剩余字节。
    LengthExceedsRemaining {
        offset: usize,
        length: u64,
        remaining: usize,
    },
    /// 输入总字节数超过 [`CmsLimits::max_der_bytes`]。
    DerTooLarge { len: usize, limit: usize },
    /// 嵌套深度超过 [`CmsLimits::max_depth`]。
    DepthLimit { limit: usize },
    /// TLV 元素数超过 [`CmsLimits::max_elements`]。
    ElementLimit { limit: usize },
    /// 标签与期望不符（含不支持的高位标签号）。
    UnexpectedTag {
        expected: &'static str,
        tag: u8,
        offset: usize,
    },
    /// `ContentInfo.contentType` 不是 `signedData`。
    NotSignedData(String),
    /// `encapContentInfo.eContentType` 不是 `data`。
    UnsupportedContentType(String),
    /// `signerInfos` 为空。
    NoSigners,
    /// 版本整数超过 `i64` 或内容为空。
    IntegerOverflow { offset: usize },
    /// OBJECT IDENTIFIER 内容非法。
    InvalidOid { offset: usize },
    /// IA5/ASCII 文本（UTCTime/GeneralizedTime）不是合法 UTF-8。
    InvalidText { field: &'static str, offset: usize },
    /// 容器在期望位置之后仍有非填充数据。
    TrailingData { offset: usize, len: usize },
}

impl fmt::Display for CmsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { offset } => write!(f, "truncated DER element at offset {offset}"),
            Self::IndefiniteLength { offset } => {
                write!(f, "DER indefinite length at offset {offset} is not allowed")
            }
            Self::LengthOverflow { offset } => {
                write!(
                    f,
                    "DER length at offset {offset} overflows the supported range"
                )
            }
            Self::LengthExceedsRemaining {
                offset,
                length,
                remaining,
            } => write!(
                f,
                "DER element at offset {offset} declares {length} bytes but only {remaining} remain"
            ),
            Self::DerTooLarge { len, limit } => {
                write!(
                    f,
                    "CMS data is {len} bytes, exceeding the {limit}-byte limit"
                )
            }
            Self::DepthLimit { limit } => {
                write!(f, "CMS nesting exceeds the {limit}-level depth limit")
            }
            Self::ElementLimit { limit } => {
                write!(f, "CMS structure exceeds the {limit}-element limit")
            }
            Self::UnexpectedTag {
                expected,
                tag,
                offset,
            } => write!(
                f,
                "expected {expected} at offset {offset}, found tag 0x{tag:02X}"
            ),
            Self::NotSignedData(oid) => {
                write!(f, "CMS content type {oid} is not signedData")
            }
            Self::UnsupportedContentType(oid) => {
                write!(f, "CMS encapsulated content type {oid} is not data")
            }
            Self::NoSigners => write!(f, "CMS SignedData has no SignerInfo entries"),
            Self::IntegerOverflow { offset } => {
                write!(f, "DER integer at offset {offset} does not fit in i64")
            }
            Self::InvalidOid { offset } => {
                write!(f, "invalid OBJECT IDENTIFIER at offset {offset}")
            }
            Self::InvalidText { field, offset } => {
                write!(f, "invalid text in {field} at offset {offset}")
            }
            Self::TrailingData { offset, len } => write!(
                f,
                "CMS container has {len} trailing non-padding bytes at offset {offset}"
            ),
        }
    }
}

impl std::error::Error for CmsError {}

/// 摘要算法标识：已知 OID 映射到 [`DigestAlgorithm`]，其余原样保留。
///
/// 未知 OID 用 [`CmsDigestAlgorithm::Other`] 显式表示，绝不 panic；
/// 完整性校验会把 `Other` 报告为 [`super::SignerIntegrity::UnsupportedAlgorithm`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmsDigestAlgorithm {
    /// 已知且本库后端支持的摘要算法。
    Known(DigestAlgorithm),
    /// 未识别的点分 OID 字符串。
    Other(String),
}

impl CmsDigestAlgorithm {
    /// 由点分 OID 解析；未知 OID 归入 [`Self::Other`]。
    pub fn from_oid(oid: &str) -> Self {
        match oid {
            "1.2.840.113549.2.5" => Self::Known(DigestAlgorithm::Md5),
            "1.3.14.3.2.26" => Self::Known(DigestAlgorithm::Sha1),
            "2.16.840.1.101.3.4.2.1" => Self::Known(DigestAlgorithm::Sha256),
            "2.16.840.1.101.3.4.2.2" => Self::Known(DigestAlgorithm::Sha384),
            "2.16.840.1.101.3.4.2.3" => Self::Known(DigestAlgorithm::Sha512),
            other => Self::Other(other.to_string()),
        }
    }

    /// 点分 OID 字符串。
    pub fn oid(&self) -> &str {
        match self {
            Self::Known(DigestAlgorithm::Md5) => "1.2.840.113549.2.5",
            Self::Known(DigestAlgorithm::Sha1) => "1.3.14.3.2.26",
            Self::Known(DigestAlgorithm::Sha256) => "2.16.840.1.101.3.4.2.1",
            Self::Known(DigestAlgorithm::Sha384) => "2.16.840.1.101.3.4.2.2",
            Self::Known(DigestAlgorithm::Sha512) => "2.16.840.1.101.3.4.2.3",
            Self::Other(oid) => oid,
        }
    }

    /// 已知算法；未知返回 `None`。
    pub fn known(&self) -> Option<DigestAlgorithm> {
        match self {
            Self::Known(algorithm) => Some(*algorithm),
            Self::Other(_) => None,
        }
    }
}

/// `AlgorithmIdentifier`（RFC 5652）的轻量模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmsAlgorithmIdentifier {
    /// 点分 OID。
    pub oid: String,
    /// 参数 TLV 的原始 DER（可为 `None`）。
    pub parameters: Option<Vec<u8>>,
}

/// `SignerInfo.sid`（RFC 5652 CHOICE）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmsSignerIdentifier {
    /// `issuerAndSerialNumber`：签发者 Name 的完整 DER 与序列号原始大端字节。
    IssuerAndSerialNumber {
        issuer_der: Vec<u8>,
        serial: Vec<u8>,
    },
    /// `[0] subjectKeyIdentifier`（IMPLICIT OCTET STRING）。
    SubjectKeyIdentifier(Vec<u8>),
}

/// `signedAttrs` 中的 `signingTime`（OID `1.2.840.113549.1.9.5`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmsSigningTime {
    /// `true` 表示 GeneralizedTime（标签 0x18），`false` 表示 UTCTime（0x17）。
    pub generalized: bool,
    /// 时间字符串原样（尚未按 PDF 日期或 RFC 3339 解析）。
    pub text: String,
}

/// 单个 `SignerInfo`（RFC 5652）的建模结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmsSignerInfo {
    /// SignerInfo 版本。
    pub version: i64,
    /// 签署者标识（issuerAndSerialNumber 或 subjectKeyIdentifier）。
    pub signer_id: CmsSignerIdentifier,
    /// 摘要算法（未知 OID 为 [`CmsDigestAlgorithm::Other`]）。
    pub digest_algorithm: CmsDigestAlgorithm,
    /// `signedAttrs` 中 `messageDigest` 属性值（缺失为 `None`）。
    pub message_digest: Option<Vec<u8>>,
    /// `signedAttrs` 中 `signingTime` 属性（缺失为 `None`）。
    pub signing_time: Option<CmsSigningTime>,
    /// 签名算法标识。
    pub signature_algorithm: CmsAlgorithmIdentifier,
    /// `signature` OCTET STRING 内容。
    pub signature: Vec<u8>,
    /// `signedAttrs` 的 DER：重新打上 SET OF 标签（0x31），即 CMS 签名
    /// 计算覆盖的编码，可供后续签名值验证直接使用。
    pub signed_attributes_der: Option<Vec<u8>>,
}

/// 解析后的 CMS `SignedData`（PKCS#7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmsSignedData {
    /// SignedData 版本。
    pub version: i64,
    /// `encapContentInfo.eContentType`（点分 OID，正常为 `data`）。
    pub content_type: String,
    /// 封装内容（detached 签名为 `None`）。
    pub content: Option<Vec<u8>>,
    /// `digestAlgorithms` 集合（按出现顺序）。
    pub digest_algorithms: Vec<CmsDigestAlgorithm>,
    /// `certificates [0]` 中每个 `CertificateChoices` 的原始 DER。
    /// `certificates.len()` 即证书条目数；本层不解析证书内容。
    pub certificates: Vec<Vec<u8>>,
    /// `signerInfos`（至少一个）。
    pub signers: Vec<CmsSignerInfo>,
}

impl CmsSignedData {
    /// 以默认上限解析 DER 编码的 PKCS#7 `ContentInfo`。
    ///
    /// 尾部全零字节按 PDF `/Contents` 填充惯例接受；其他尾随数据报错。
    pub fn parse(der: &[u8]) -> Result<Self, CmsError> {
        Self::parse_with_limits(der, CmsLimits::default())
    }

    /// 以自定义上限解析（测试或受限环境可收紧上限）。
    pub fn parse_with_limits(der: &[u8], limits: CmsLimits) -> Result<Self, CmsError> {
        if der.len() > limits.max_der_bytes {
            return Err(CmsError::DerTooLarge {
                len: der.len(),
                limit: limits.max_der_bytes,
            });
        }
        let mut state = ParseState::new(limits);
        let mut cursor = Cursor::new(der, 0);
        let content_info = cursor.read_tag(0x30, "ContentInfo SEQUENCE", &mut state)?;
        let signed = parse_content_info(&content_info, &mut state)?;
        let trailing = &der[cursor.position()..];
        if trailing.iter().any(|byte| *byte != 0) {
            return Err(CmsError::TrailingData {
                offset: cursor.position(),
                len: trailing.len(),
            });
        }
        Ok(signed)
    }
}

impl Document {
    /// 解析签名字典 `/Contents` 中的 CMS（PKCS#7 `SignedData`）结构。
    ///
    /// - `/Contents` 缺失、解码为空或全零占位返回 `Ok(None)`；
    /// - DER 结构损坏返回 [`SignatureError::Cms`]；
    /// - 解码后超过 16 MiB 返回 [`SignatureError::ContentsTooLarge`]。
    ///
    /// 只做结构解析：签名值、证书链与时间戳验证不在本层。
    pub fn signature_cms(
        &self,
        signature: &DocumentSignature,
    ) -> Result<Option<CmsSignedData>, SignatureError> {
        let Some(value) = signature.contents.as_ref() else {
            return Ok(None);
        };
        let limit = CmsLimits::default().max_der_bytes;
        let approx = approximate_len(value);
        if approx > limit {
            return Err(SignatureError::ContentsTooLarge { len: approx, limit });
        }
        let Some(bytes) = pdf_string_bytes(value) else {
            return Err(malformed("/Contents is not a decodable string"));
        };
        if bytes.len() > limit {
            return Err(SignatureError::ContentsTooLarge {
                len: bytes.len(),
                limit,
            });
        }
        if bytes.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }
        CmsSignedData::parse(&bytes)
            .map(Some)
            .map_err(SignatureError::Cms)
    }
}

/// `/Contents` 值解码前的近似字节数（十六进制按半字节、ASCII85 按 4/5）。
fn approximate_len(value: &Value) -> usize {
    match value {
        Value::HexString(text) => text.len().div_ceil(2),
        Value::String(text) => text.len(),
        Value::Ascii85String(text) => text.len() / 5 * 4,
        _ => 0,
    }
}

fn parse_content_info(tlv: &Tlv<'_>, state: &mut ParseState) -> Result<CmsSignedData, CmsError> {
    state.enter()?;
    let mut cursor = Cursor::new(tlv.content, tlv.content_offset());
    let oid_tlv = cursor.read_tag(0x06, "contentType OBJECT IDENTIFIER", state)?;
    let content_type = decode_oid(oid_tlv.content, oid_tlv.content_offset())?;
    if content_type != OID_SIGNED_DATA {
        return Err(CmsError::NotSignedData(content_type));
    }
    let explicit = cursor.read_tag(0xA0, "signedData [0] EXPLICIT", state)?;
    cursor.finish()?;
    let mut inner = Cursor::new(explicit.content, explicit.content_offset());
    let signed_data = inner.read_tag(0x30, "SignedData SEQUENCE", state)?;
    inner.finish()?;
    parse_signed_data(&signed_data, state)
}

fn parse_signed_data(tlv: &Tlv<'_>, state: &mut ParseState) -> Result<CmsSignedData, CmsError> {
    state.enter()?;
    let mut cursor = Cursor::new(tlv.content, tlv.content_offset());
    let version_tlv = cursor.read_tag(0x02, "SignedData version INTEGER", state)?;
    let version = decode_integer(version_tlv.content, version_tlv.content_offset())?;
    let digest_set = cursor.read_tag(0x31, "digestAlgorithms SET", state)?;
    let digest_algorithms = parse_digest_algorithms(&digest_set, state)?;
    let encap = cursor.read_tag(0x30, "encapContentInfo SEQUENCE", state)?;
    let (content_type, content) = parse_encapsulated_content(&encap, state)?;
    let mut certificates = Vec::new();
    if cursor.peek_tag() == Some(0xA0) {
        let certs = cursor.read_tag(0xA0, "certificates [0]", state)?;
        let mut cert_cursor = Cursor::new(certs.content, certs.content_offset());
        while !cert_cursor.is_empty() {
            let certificate = cert_cursor.read(state)?;
            certificates.push(certificate.raw.to_vec());
        }
    }
    if cursor.peek_tag() == Some(0xA1) {
        // crls [1]：只按声明长度跳过，不进入内容。
        let _crls = cursor.read_tag(0xA1, "crls [1]", state)?;
    }
    let signer_set = cursor.read_tag(0x31, "signerInfos SET", state)?;
    let signers = parse_signers(&signer_set, state)?;
    if signers.is_empty() {
        return Err(CmsError::NoSigners);
    }
    cursor.finish()?;
    state.leave();
    Ok(CmsSignedData {
        version,
        content_type,
        content,
        digest_algorithms,
        certificates,
        signers,
    })
}

fn parse_digest_algorithms(
    tlv: &Tlv<'_>,
    state: &mut ParseState,
) -> Result<Vec<CmsDigestAlgorithm>, CmsError> {
    state.enter()?;
    let mut cursor = Cursor::new(tlv.content, tlv.content_offset());
    let mut out = Vec::new();
    while !cursor.is_empty() {
        let algorithm_tlv = cursor.read_tag(0x30, "AlgorithmIdentifier SEQUENCE", state)?;
        let identifier = parse_algorithm_identifier(&algorithm_tlv, state)?;
        out.push(CmsDigestAlgorithm::from_oid(&identifier.oid));
    }
    state.leave();
    Ok(out)
}

fn parse_encapsulated_content(
    tlv: &Tlv<'_>,
    state: &mut ParseState,
) -> Result<(String, Option<Vec<u8>>), CmsError> {
    state.enter()?;
    let mut cursor = Cursor::new(tlv.content, tlv.content_offset());
    let oid_tlv = cursor.read_tag(0x06, "eContentType OBJECT IDENTIFIER", state)?;
    let content_type = decode_oid(oid_tlv.content, oid_tlv.content_offset())?;
    if content_type != OID_DATA {
        return Err(CmsError::UnsupportedContentType(content_type));
    }
    let content = if cursor.peek_tag() == Some(0xA0) {
        let explicit = cursor.read_tag(0xA0, "eContent [0] EXPLICIT", state)?;
        let mut inner = Cursor::new(explicit.content, explicit.content_offset());
        let octets = inner.read_tag(0x04, "eContent OCTET STRING", state)?;
        inner.finish()?;
        Some(octets.content.to_vec())
    } else {
        None
    };
    cursor.finish()?;
    state.leave();
    Ok((content_type, content))
}

fn parse_signers(tlv: &Tlv<'_>, state: &mut ParseState) -> Result<Vec<CmsSignerInfo>, CmsError> {
    state.enter()?;
    let mut cursor = Cursor::new(tlv.content, tlv.content_offset());
    let mut out = Vec::new();
    while !cursor.is_empty() {
        let signer_tlv = cursor.read_tag(0x30, "SignerInfo SEQUENCE", state)?;
        out.push(parse_signer_info(&signer_tlv, state)?);
    }
    state.leave();
    Ok(out)
}
