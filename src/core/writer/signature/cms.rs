//! 写端 detached CMS SignedData 构造（12.8 F1.2）。
//!
//! 本模块只做 DER 结构编排：摘要由 [`crate::core::reader::signature::SignatureBackend`]
//! 算出，签名值与证书集由 [`CmsSignBackend`] 注入，库内不实现 RSA/ECDSA，
//! 也不做证书链与信任判定。密文材料回填到 PDF `/Contents` 时，末尾允许补
//! 全零字节（读端 `CmsSignedData::parse` 按 PDF 惯例接受尾随全零填充）。

use crate::core::reader::signature::{CmsDigestAlgorithm, DigestAlgorithm};

use std::fmt;

const OID_DATA: &str = "1.2.840.113549.1.7.1";
const OID_SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
const OID_MESSAGE_DIGEST: &str = "1.2.840.113549.1.9.4";

/// CMS 构造期的类型化错误（只覆盖结构编排能失败的情形）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmsBuildError {
    /// 后端给出的点分 OID 无法编码为 DER OBJECT IDENTIFIER。
    InvalidOid(String),
    /// 构造出的 CMS 超出 `/Contents` 已分配的字节数。
    Oversized { len: usize, limit: usize },
}

impl fmt::Display for CmsBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOid(oid) => write!(f, "invalid CMS object identifier {oid:?}"),
            Self::Oversized { len, limit } => write!(
                f,
                "constructed CMS is {len} bytes, exceeding the {limit}-byte /Contents slot"
            ),
        }
    }
}

impl std::error::Error for CmsBuildError {}

/// 签名者标识（写入 `SignerInfo.sid`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmsSignerId {
    /// `issuerAndSerialNumber`：`issuer_der` 是完整 `Name` 的 DER，
    /// `serial` 是 `serialNumber` 整数的最小补码内容字节（不含 INTEGER 头）。
    IssuerAndSerial {
        issuer_der: Vec<u8>,
        serial: Vec<u8>,
    },
    /// `subjectKeyIdentifier [0]`：OID 2.5.29.14 对应的 SKI 值。
    SubjectKeyIdentifier(Vec<u8>),
}

/// 签名后端返回的材料：签名值与签署者标识，以及要写入的证书集。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmsSignerMaterial {
    /// `signatureAlgorithm` 的点分 OID（如 rsaEncryption）。
    pub signature_algorithm: String,
    /// 签署者标识。
    pub signer_id: CmsSignerId,
    /// 签名值字节。
    pub signature: Vec<u8>,
    /// 证书集，每项为一份完整 DER 编码的 Certificate。
    pub certificates: Vec<Vec<u8>>,
}

/// 签名值后端（F1.2）：密码学只在实现里，库内不持密钥、不验签。
///
/// 输入是 `signedAttrs` 的 `SET OF` DER（tag `0x31`）与摘要算法；实现自行
/// 决定签名方案（PKCS#1 v1.5、ECDSA 等）如何消费这些字节。
pub trait CmsSignBackend {
    /// 对待签名的 signedAttrs DER 签名，返回写入 CMS 所需的全部材料。
    /// 实现返回 `Err(String)` 时写端以类型化错误中止，不产出部分文件。
    fn sign(
        &self,
        signed_attrs_der: &[u8],
        digest_algorithm: DigestAlgorithm,
    ) -> Result<CmsSignerMaterial, String>;
}

/// DER 长度编码（定长，含长形式）。
fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    if content.len() < 0x80 {
        out.push(content.len() as u8);
    } else {
        let bytes = content.len().to_be_bytes();
        let start = bytes
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(bytes.len() - 1);
        out.push(0x80 | (bytes.len() - start) as u8);
        out.extend_from_slice(&bytes[start..]);
    }
    out.extend_from_slice(content);
    out
}

fn sequence(parts: &[Vec<u8>]) -> Vec<u8> {
    der(0x30, &flatten(parts))
}

fn set(parts: &[Vec<u8>]) -> Vec<u8> {
    der(0x31, &flatten(parts))
}

fn context(number: u8, content: &[u8]) -> Vec<u8> {
    der(0xA0 + number, content)
}

fn flatten(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.iter().flatten().copied().collect()
}

/// 点分 OID → DER OBJECT IDENTIFIER（每个弧用 7 位分组编码）。
fn encode_oid(dotted: &str) -> Result<Vec<u8>, CmsBuildError> {
    let arcs: Vec<u64> = dotted
        .split('.')
        .map(|arc| arc.parse::<u64>())
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| CmsBuildError::InvalidOid(dotted.to_string()))?;
    if arcs.len() < 2 || arcs[0] > 2 || (arcs[0] < 2 && arcs[1] > 39) {
        return Err(CmsBuildError::InvalidOid(dotted.to_string()));
    }
    let mut content = Vec::new();
    let mut values = arcs.into_iter();
    let first = values.next().unwrap() * 40 + values.next().unwrap();
    for value in std::iter::once(first).chain(values) {
        let mut buf = [0u8; 10];
        let mut index = buf.len() - 1;
        let mut remaining = value;
        buf[index] = (remaining & 0x7F) as u8;
        remaining >>= 7;
        while remaining > 0 {
            index -= 1;
            buf[index] = (remaining & 0x7F) as u8 | 0x80;
            remaining >>= 7;
        }
        content.extend_from_slice(&buf[index..]);
    }
    Ok(der(0x06, &content))
}

/// 最小补码 INTEGER。
fn integer(value: i64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let mut start = 0;
    while start + 1 < bytes.len()
        && ((bytes[start] == 0x00 && bytes[start + 1] & 0x80 == 0)
            || (bytes[start] == 0xFF && bytes[start + 1] & 0x80 != 0))
    {
        start += 1;
    }
    der(0x02, &bytes[start..])
}

fn octet(bytes: &[u8]) -> Vec<u8> {
    der(0x04, bytes)
}

/// `signedAttrs` 的公共内容：目前只有 `messageDigest` 一个属性。
fn signed_attribute_content(digest: &[u8]) -> Vec<u8> {
    let value = octet(digest);
    sequence(&[encode_oid_or_empty(OID_MESSAGE_DIGEST), set(&[value])])
}

/// 常量 OID 的编码不会失败；用 `expect` 固化，失败即为代码缺陷。
fn encode_oid_or_empty(dotted: &str) -> Vec<u8> {
    encode_oid(dotted).expect("built-in CMS OID must be valid")
}

/// 供后端签名的 `signedAttrs`：`SET OF Attribute`（tag `0x31`）。
/// 写出时同一内容换成 `[0] IMPLICIT`（tag `0xA0`），读端会还原为 SET OF
/// 供验证，故两者逐字节对应。
pub(crate) fn signed_attrs_der(digest: &[u8]) -> Vec<u8> {
    set(&[signed_attribute_content(digest)])
}

/// 构造 detached CMS `ContentInfo`。
///
/// `digest` 是 `/ByteRange` 覆盖字节的摘要，写入 `signedAttrs.messageDigest`；
/// `material` 由后端注入。`Oversized` 由调用方映射为写端错误。
pub(crate) fn build_detached_cms(
    algorithm: DigestAlgorithm,
    digest: &[u8],
    material: &CmsSignerMaterial,
) -> Result<Vec<u8>, CmsBuildError> {
    let digest_algorithm = sequence(&[encode_oid(CmsDigestAlgorithm::Known(algorithm).oid())?]);
    let signature_algorithm = sequence(&[encode_oid(&material.signature_algorithm)?]);
    let (signer_version, sid) = match &material.signer_id {
        CmsSignerId::IssuerAndSerial { issuer_der, serial } => {
            (1, sequence(&[issuer_der.clone(), der(0x02, serial)]))
        }
        // SubjectKeyIdentifier ::= OCTET STRING，[0] IMPLICIT 下是原始标签 0x80。
        CmsSignerId::SubjectKeyIdentifier(ski) => (3, der(0x80, ski)),
    };
    let signed_attrs = context(0, &signed_attribute_content(digest));
    let signer_info = sequence(&[
        integer(signer_version),
        sid,
        digest_algorithm.clone(),
        signed_attrs,
        signature_algorithm,
        octet(&material.signature),
    ]);

    let encap_content_info = sequence(&[encode_oid_or_empty(OID_DATA)]);
    let mut fields = vec![
        integer(signer_version.max(1)),
        set(&[digest_algorithm]),
        encap_content_info,
    ];
    if !material.certificates.is_empty() {
        fields.push(context(0, &flatten(&material.certificates)));
    }
    fields.push(set(&[signer_info]));
    let signed_data = sequence(&fields);
    Ok(sequence(&[
        encode_oid_or_empty(OID_SIGNED_DATA),
        context(0, &signed_data),
    ]))
}
