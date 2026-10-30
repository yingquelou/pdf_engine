//! 签名字典（12.8.4.1 Table 257）、`/SubFilter`（Table 258）与摘要算法模型。

use crate::core::types::{Dict, Value};

use super::{Result, malformed};

/// 摘要算法（Table 258 SubFilter 与后端能力的交集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestAlgorithm {
    /// 遗留 `/R <= 2` 签名用 MD5。
    Md5,
    /// `adbe.pkcs7.sha1` 与 `adbe.x509.rsa_sha1`。
    Sha1,
    /// detached PKCS#7 / CAdES 的当前推荐。
    Sha256,
    /// CMS 摘要算法 `2.16.840.1.101.3.4.2.2`。
    Sha384,
    /// CMS 摘要算法 `2.16.840.1.101.3.4.2.3`。
    Sha512,
}

impl DigestAlgorithm {
    /// 摘要输出字节数。
    pub fn output_len(&self) -> usize {
        match self {
            Self::Md5 => 16,
            Self::Sha1 => 20,
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }
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

    /// `/M` 签署时间解析为 PDF 日期（7.9.4）；缺失或格式非法返回 `None`。
    pub fn date_parsed(&self) -> Option<crate::core::types::PdfDate> {
        self.date
            .as_ref()
            .and_then(crate::core::reader::metadata::parse_pdf_date)
    }
}

/// `/ByteRange` 必须是 4 个非负整数。
pub(super) fn parse_byte_range(value: Option<&Value>) -> Result<[i64; 4]> {
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
