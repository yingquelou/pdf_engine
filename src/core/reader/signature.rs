//! 数字签名字典读取（12.8.1–12.8.4）。
//!
//! - [`Document::document_signatures`]：沿目录 `/AcroForm` 字段树收集
//!   `/FT /Sig` 字段的 `/V` 签名字典（Table 257）。
//! - [`Document::signature`]：解析任意签名字典（直接字典或间接引用）。
//! - ByteRange（`[offset1 len1 offset2 len2]`）结构校验与覆盖范围；
//!   覆盖字段的分段摘要经 [`SignatureBackend`] 计算（默认
//!   [`RustCryptoSignatureBackend`]：MD5/SHA-1/SHA-256），PKCS#7
//!   语法与证书链验证不在本层（签名后端可插入更完整的验证器）。
//! - 防护：摘要要求文件式 `Document`（源字节可读）；ByteRange 必须
//!   单调、留正间隔给 `/Contents` 占位且不越过文件末尾。

use std::fmt;

use md5::Md5;
use sha1::Sha1;
use sha2::Sha256;

use crate::core::reader::content::text_value;
use crate::core::reader::form::FieldType;
use crate::core::reader::navigation::{catalog, resolve};
use crate::core::types::{Dict, Document, Value};

/// 单段读取的分块大小（与增量写出复制源字节的分块一致）。
const DIGEST_CHUNK: usize = 64 * 1024;

/// 签名读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureError {
    /// 文档目录缺失或无法解析为字典。
    CatalogMissing,
    /// 签名字典、ByteRange 或 SubFilter 条目结构损坏。
    Malformed(String),
    /// 计算覆盖字节摘要要求从文件解析、保留源字节的 `Document`。
    NoSource,
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogMissing => write!(f, "document catalog is missing or malformed"),
            Self::Malformed(message) => write!(f, "{message}"),
            Self::NoSource => {
                write!(f, "signature digest requires a file-backed document")
            }
        }
    }
}

impl std::error::Error for SignatureError {}

type Result<T> = std::result::Result<T, SignatureError>;

fn malformed(message: impl Into<String>) -> SignatureError {
    SignatureError::Malformed(message.into())
}

/// 摘要算法（Table 258 SubFilter 与后端能力的交集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestAlgorithm {
    /// 遗留 `/R <= 2` 签名用 MD5。
    Md5,
    /// `adbe.pkcs7.sha1` 与 `adbe.x509.rsa_sha1`。
    Sha1,
    /// detached PKCS#7 / CAdES 的当前推荐。
    Sha256,
}

/// 签名字典 `/SubFilter`（12.8.4.3 Table 258）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureSubFilter {
    /// `adbe.pkcs7.sha1`：Contents 内嵌完整 PKCS#7（含签名数据）。
    Pkcs7Sha1,
    /// `adbe.pkcs7.detached`：Contents 内嵌 detached PKCS#7。
    Pkcs7Detached,
    /// `adbe.x509.rsa_sha1`（遗留形态）。
    X509RsaSha1,
    /// `ETSI.CAdES.detached`：CAdES detached。
    CadesDetached,
    /// `ETSI.RFC3161`：时间戳签名。
    Rfc3161,
    /// 其他/自定义名称，原样保留。
    Unknown(String),
}

impl SignatureSubFilter {
    /// 由 `/SubFilter` 名称解析。
    pub fn from_name(name: &str) -> Self {
        match name {
            "adbe.pkcs7.sha1" => Self::Pkcs7Sha1,
            "adbe.pkcs7.detached" => Self::Pkcs7Detached,
            "adbe.x509.rsa_sha1" => Self::X509RsaSha1,
            "ETSI.CAdES.detached" => Self::CadesDetached,
            "ETSI.RFC3161" => Self::Rfc3161,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// 名称（不含前导 `/`）。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pkcs7Sha1 => "adbe.pkcs7.sha1",
            Self::Pkcs7Detached => "adbe.pkcs7.detached",
            Self::X509RsaSha1 => "adbe.x509.rsa_sha1",
            Self::CadesDetached => "ETSI.CAdES.detached",
            Self::Rfc3161 => "ETSI.RFC3161",
            Self::Unknown(name) => name,
        }
    }

    /// 名称隐含的默认摘要算法；detached 形态取决于 PKCS#7 内部，返回 `None`。
    pub fn default_digest(&self) -> Option<DigestAlgorithm> {
        match self {
            Self::Pkcs7Sha1 | Self::X509RsaSha1 => Some(DigestAlgorithm::Sha1),
            _ => None,
        }
    }
}

/// 签名字典（12.8.4.1 Table 257，`/Type /Sig`）。
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSignature {
    /// 签名字典对象的间接引用；直接嵌入时为 `None`。
    pub reference: Option<(i64, i64)>,
    /// `/Filter` 签名处理程序名（如 `Adobe.PPKLite`）。
    pub filter: Option<String>,
    /// `/SubFilter` 子过滤器。
    pub sub_filter: Option<SignatureSubFilter>,
    /// `/ByteRange [offset1 len1 offset2 len2]`。
    pub byte_range: [i64; 4],
    /// `/Contents` 原始值（占位十六进制/字面串）。
    pub contents: Option<Value>,
    /// `/Name` 签署者名（文本串解码）。
    pub name: Option<String>,
    /// `/Location` 签署地点。
    pub location: Option<String>,
    /// `/Reason` 签署原因。
    pub reason: Option<String>,
    /// `/ContactInfo` 联系方式。
    pub contact_info: Option<String>,
    /// `/M` 签署日期（文本串原样保留）。
    pub date: Option<Value>,
    /// `/Reference` 变换/时间戳引用数组（原样保留）。
    pub references: Option<Value>,
    /// `/PropBuild` 等遗留字段不进模型；完整字典经此访问。
    pub dict: Dict,
}

impl DocumentSignature {
    /// ByteRange 覆盖的两段 `(offset, len)`；已通过结构校验。
    pub fn covered_ranges(&self) -> [(u64, u64); 2] {
        [
            (self.byte_range[0] as u64, self.byte_range[1] as u64),
            (self.byte_range[2] as u64, self.byte_range[3] as u64),
        ]
    }

    /// 覆盖总字节数（两段长度之和）。
    pub fn covered_len(&self) -> u64 {
        self.byte_range[1].max(0) as u64 + self.byte_range[3].max(0) as u64
    }

    /// `/Contents` 为十六进制串时返回其解码后的占位字节数。
    pub fn contents_len(&self) -> Option<usize> {
        match self.contents.as_ref() {
            Some(Value::HexString(hex)) => Some(hex.len() / 2),
            _ => None,
        }
    }
}

/// 增量摘要接收器：按块喂入覆盖字节，产出最终摘要。
pub trait DigestSink {
    /// 追加一块字节。
    fn push(&mut self, bytes: &[u8]);

    /// 结束并返回摘要。
    fn finish(self: Box<Self>) -> Vec<u8>;
}

/// 可替换的签名摘要后端（Roadmap F1：密码学经 trait 隔离）。
pub trait SignatureBackend {
    /// 为指定算法启动一个摘要累加器。
    fn start(&self, algorithm: DigestAlgorithm) -> Box<dyn DigestSink>;
}

struct GenericDigest<D: md5::Digest>(D);

impl<D: md5::Digest + Default> DigestSink for GenericDigest<D> {
    fn push(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn finish(self: Box<Self>) -> Vec<u8> {
        self.0.finalize().to_vec()
    }
}

/// 内置 RustCrypto 后端（md-5 / sha1 / sha2，digest 0.10 家族）。
#[derive(Debug, Clone, Copy, Default)]
pub struct RustCryptoSignatureBackend;

impl SignatureBackend for RustCryptoSignatureBackend {
    fn start(&self, algorithm: DigestAlgorithm) -> Box<dyn DigestSink> {
        match algorithm {
            DigestAlgorithm::Md5 => Box::new(GenericDigest(Md5::default())),
            DigestAlgorithm::Sha1 => Box::new(GenericDigest(Sha1::default())),
            DigestAlgorithm::Sha256 => Box::new(GenericDigest(Sha256::default())),
        }
    }
}

impl Document {
    /// 目录 `/SigFlags`（Table 246）；缺失返回 `None`。
    pub fn signature_flags(&self) -> Result<Option<i64>> {
        let cat = catalog(self).map_err(|_| SignatureError::CatalogMissing)?;
        match cat.get("SigFlags") {
            None => Ok(None),
            Some(value) => Some(
                value
                    .as_integer()
                    .ok_or_else(|| malformed("/SigFlags is not an integer")),
            )
            .transpose(),
        }
    }

    /// 收集目录 `/AcroForm` 字段树中全部 `/FT /Sig` 字段的签名字典。
    ///
    /// 只取字段自身字典的 `/V`（不读继承值，避免父节点重复）；`/V` 缺失
    /// 或非签名字典的字段被跳过（未签名字段是常态）。
    pub fn document_signatures(&self) -> Result<Vec<DocumentSignature>> {
        let form = self.acro_form().map_err(|error| match error {
            crate::core::reader::form::FormError::CatalogMissing => SignatureError::CatalogMissing,
            other => malformed(other.to_string()),
        })?;
        let mut out = Vec::new();
        if let Some(form) = form {
            self.collect_field_signatures(&form.fields, &mut out)?;
        }
        Ok(out)
    }

    fn collect_field_signatures(
        &self,
        fields: &[crate::core::reader::form::Field],
        out: &mut Vec<DocumentSignature>,
    ) -> Result<()> {
        for field in fields {
            if field.field_type == Some(FieldType::Signature)
                && let Some(value) = field.dict.get("V")
            {
                out.push(self.signature(value)?);
            }
            self.collect_field_signatures(&field.kids, out)?;
        }
        Ok(())
    }

    /// 解析单个签名字典值（直接字典或间接引用）。
    pub fn signature(&self, value: &Value) -> Result<DocumentSignature> {
        let reference = value.as_ref();
        let resolved = resolve(self, value).map_err(|error| malformed(error.to_string()))?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| malformed("signature value is not a dictionary"))?;
        if let Some(item) = dict.get("Type")
            && item.as_name() != Some("Sig")
        {
            return Err(malformed(format!(
                "signature dictionary has /Type /{}",
                item.as_name().unwrap_or("?")
            )));
        }
        let byte_range = parse_byte_range(dict.get("ByteRange"))?;
        let sub_filter = match dict.get("SubFilter") {
            None => None,
            Some(item) => Some(SignatureSubFilter::from_name(
                item.as_name()
                    .ok_or_else(|| malformed("/SubFilter of a signature is not a name"))?,
            )),
        };
        Ok(DocumentSignature {
            reference,
            filter: dict
                .get("Filter")
                .and_then(|item| item.as_name())
                .map(str::to_string),
            sub_filter,
            byte_range,
            contents: dict.get("Contents").cloned(),
            name: dict.get("Name").and_then(text_value),
            location: dict.get("Location").and_then(text_value),
            reason: dict.get("Reason").and_then(text_value),
            contact_info: dict.get("ContactInfo").and_then(text_value),
            date: dict.get("M").cloned(),
            references: dict.get("Reference").cloned(),
            dict: dict.clone(),
        })
    }

    /// ByteRange 相对当前源文件的结构校验，并返回覆盖两段。
    ///
    /// 要求：全部非负；第一段起点为 0（惯例，覆盖「文件头到 Contents」）；
    /// 两段单调且之间留出正间隔给 `/Contents` 占位；第二段不超过文件末尾。
    pub fn validate_signature_byte_range(
        &self,
        signature: &DocumentSignature,
    ) -> Result<[(u64, u64); 2]> {
        let [o1, l1, o2, l2] = signature.byte_range;
        if o1 != 0 {
            return Err(malformed(format!(
                "/ByteRange first offset must be 0, got {o1}"
            )));
        }
        if l1 <= 0 || l2 <= 0 || o2 <= o1 + l1 {
            return Err(malformed(format!(
                "/ByteRange {o1} {l1} {o2} {l2} is not monotonic with a positive gap"
            )));
        }
        let file_len = self.source_len().ok_or(SignatureError::NoSource)?;
        if o2 as u64 + l2 as u64 > file_len {
            return Err(malformed(format!(
                "/ByteRange extends to {} past end of file ({file_len})",
                o2 as u64 + l2 as u64
            )));
        }
        Ok(signature.covered_ranges())
    }

    /// 计算 ByteRange 覆盖字段的分段摘要（12.8.1）。
    ///
    /// 要求文件式 `Document`；按 [`DIGEST_CHUNK`] 分块读取，不整档驻留内存。
    /// `/Contents` 占位字节不在覆盖范围内，因此摘要对「先写占位、后填
    /// Contents」的签名流程稳定。
    pub fn signature_digest(
        &self,
        signature: &DocumentSignature,
        algorithm: DigestAlgorithm,
        backend: &dyn SignatureBackend,
    ) -> Result<Vec<u8>> {
        let ranges = self.validate_signature_byte_range(signature)?;
        let mut sink = backend.start(algorithm);
        for (offset, len) in ranges {
            let mut remaining = len;
            let mut cursor = offset;
            while remaining > 0 {
                let take = remaining.min(DIGEST_CHUNK as u64) as usize;
                let Some(bytes) = self.read_at(cursor, take) else {
                    return Err(malformed(format!(
                        "failed to read /ByteRange segment at {cursor} ({take} bytes)"
                    )));
                };
                if bytes.is_empty() {
                    return Err(malformed(format!(
                        "unexpected end of file inside /ByteRange segment at {cursor}"
                    )));
                }
                sink.push(&bytes);
                cursor += bytes.len() as u64;
                remaining -= bytes.len() as u64;
            }
        }
        Ok(sink.finish())
    }
}

fn parse_byte_range(value: Option<&Value>) -> Result<[i64; 4]> {
    let items = value
        .and_then(|item| match item {
            Value::Array(array) => Some(&array.0),
            _ => None,
        })
        .ok_or_else(|| malformed("/ByteRange must be a 4-integer array"))?;
    if items.len() != 4 {
        return Err(malformed(format!(
            "/ByteRange must have exactly 4 entries, got {}",
            items.len()
        )));
    }
    let mut range = [0i64; 4];
    for (slot, item) in range.iter_mut().zip(items) {
        let number = item
            .as_integer()
            .ok_or_else(|| malformed("/ByteRange entries must be integers"))?;
        if number < 0 {
            return Err(malformed(format!(
                "/ByteRange entry must be non-negative, got {number}"
            )));
        }
        *slot = number;
    }
    Ok(range)
}

#[cfg(test)]
mod tests;
