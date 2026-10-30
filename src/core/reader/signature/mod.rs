//! 数字签名字典读取（12.8.1–12.8.5）。
//!
//! 子模块：
//! - `entries`：签名字典（Table 257）、SubFilter 与摘要算法模型；
//! - `reference`：变换引用（DocMDP/FieldMDP/UR）模型与参数解析；
//! - `perms`：目录 `/Perms`（Table 268）条目模型；
//! - `digest`：可插拔摘要后端（默认 RustCrypto：MD5/SHA-1/SHA-256/384/512）；
//! - `document`：`Document` 上的签名发现、ByteRange 校验与分段摘要；
//! - `cms`：`/Contents` 的 PKCS#7/CMS SignedData DER 解析；
//! - `integrity`：signedAttrs `messageDigest` 与覆盖字节摘要的绑定校验。
//!
//! 能力边界：
//! - [`Document::document_signatures`](crate::core::types::Document::document_signatures)
//!   沿目录 `/AcroForm` 字段树收集 `/FT /Sig` 字段的 `/V` 签名字典；
//! - [`Document::signature`](crate::core::types::Document::signature)
//!   解析任意签名字典（直接字典或间接引用）；
//! - ByteRange（`[offset1 len1 offset2 len2]`）结构校验与覆盖范围；
//!   覆盖字段的分段摘要经 [`SignatureBackend`] 计算；
//! - [`Document::signature_cms`](crate::core::types::Document::signature_cms)
//!   解析 `/Contents` 的 CMS SignedData；
//!   [`Document::verify_signature_integrity`](crate::core::types::Document::verify_signature_integrity)
//!   只校验 `messageDigest`
//!   与 ByteRange 摘要的绑定；
//! - RSA/ECDSA 签名值、证书链与信任、时间戳验证不在本层（签名后端
//!   可插入更完整的验证器）。
//! - 防护：摘要要求文件式 `Document`（源字节可读）；ByteRange 必须
//!   单调、留正间隔给 `/Contents` 占位且不越过文件末尾；CMS 解析对
//!   非可信 DER 施加字节数/嵌套深度/元素数上限。
//! - 变换引用（12.8.2–12.8.4，Table 252–257）：`/Reference` 数组的
//!   签名字典引用条目建模为 [`SignatureReference`]，`/TransformParams`
//!   按 `/TransformMethod` 解析为 [`TransformParams`]
//!   （DocMDP / FieldMDP / UR）。
//! - 目录 `/Perms`（12.8.5，Table 268）：文档级权限/变更允许条目经
//!   [`Document::perms_entry`](crate::core::types::Document::perms_entry)
//!   解析为 [`PermsEntry`]，签名字典数组经
//!   [`Document::perms_signatures`](crate::core::types::Document::perms_signatures)
//!   读取。

use std::fmt;

mod bytes;
mod cms;
mod digest;
mod document;
mod entries;
mod integrity;
mod perms;
mod reference;

pub use cms::{
    CmsAlgorithmIdentifier, CmsDigestAlgorithm, CmsError, CmsLimits, CmsSignedData,
    CmsSignerIdentifier, CmsSignerInfo, CmsSigningTime,
};
pub use digest::{DigestSink, RustCryptoSignatureBackend, SignatureBackend};
pub use entries::{DigestAlgorithm, DocumentSignature, SignatureSubFilter};
pub use integrity::{SignatureIntegrity, SignerIntegrity};
pub use perms::{PermsEntry, PermsEntryName};
pub use reference::{
    DocmdpPermission, FieldMdPermission, SignatureReference, TransformMethod, TransformParams,
    UrScope,
};

#[cfg(test)]
mod tests;

/// 单段读取的分块大小（与增量写出复制源字节的分块一致）。
pub(super) const DIGEST_CHUNK: usize = 64 * 1024;

/// 签名读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureError {
    /// 文档目录缺失或无法解析为字典。
    CatalogMissing,
    /// 签名字典、ByteRange 或 SubFilter 条目结构损坏。
    Malformed(String),
    /// 计算覆盖字节摘要要求从文件解析、保留源字节的 `Document`。
    NoSource,
    /// 需要完整性校验，但 `/Contents` 中没有 CMS 数据（缺失/空/全零占位）。
    NoContents,
    /// `/Contents` 解码后超出 CMS 解析上限。
    ContentsTooLarge { len: usize, limit: usize },
    /// `/Contents` 的 CMS（PKCS#7 SignedData）DER 结构损坏。
    Cms(CmsError),
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogMissing => write!(f, "document catalog is missing or malformed"),
            Self::Malformed(message) => write!(f, "{message}"),
            Self::NoSource => {
                write!(f, "signature digest requires a file-backed document")
            }
            Self::NoContents => {
                write!(f, "signature has no CMS data in /Contents")
            }
            Self::ContentsTooLarge { len, limit } => write!(
                f,
                "signature /Contents is {len} bytes, exceeding the {limit}-byte limit"
            ),
            Self::Cms(error) => write!(f, "invalid CMS signature data: {error}"),
        }
    }
}

impl std::error::Error for SignatureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Cms(error) => Some(error),
            _ => None,
        }
    }
}

type Result<T> = std::result::Result<T, SignatureError>;

pub(super) fn malformed(message: impl Into<String>) -> SignatureError {
    SignatureError::Malformed(message.into())
}
