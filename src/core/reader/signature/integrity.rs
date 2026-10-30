//! CMS `messageDigest` 与 `/ByteRange` 覆盖字节摘要的绑定校验（12.8.1）。
//!
//! 本层只回答「`/Contents` 中的 CMS 是否声明了与当前覆盖字节一致的摘要」。
//! 它**不**验证签名值（RSA/ECDSA）、证书链与信任、`signingTime` 可信度
//! 或时间戳；这些属于后续切片/上层签名后端的能力。

use crate::core::types::Document;

use super::cms::CmsSignedData;
use super::entries::{DigestAlgorithm, DocumentSignature};
use super::{Result, SignatureBackend, SignatureError};

/// 一次完整性校验的汇总结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureIntegrity {
    /// 从 `/Contents` 解析出的 CMS 结构。
    pub cms: CmsSignedData,
    /// 与 `cms.signers` 一一对应的逐签署者摘要绑定结果。
    pub signers: Vec<SignerIntegrity>,
}

impl SignatureIntegrity {
    /// 是否至少有一个签署者且全部为 [`SignerIntegrity::Match`]。
    pub fn all_matched(&self) -> bool {
        !self.signers.is_empty()
            && self
                .signers
                .iter()
                .all(|status| matches!(status, SignerIntegrity::Match))
    }
}

/// 单个 `SignerInfo` 的 `messageDigest` 绑定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignerIntegrity {
    /// `messageDigest` 与重新计算的覆盖字节摘要一致。
    Match,
    /// 不一致：`expected` 为 CMS 声明的值，`actual` 为重新计算值。
    Mismatch { expected: Vec<u8>, actual: Vec<u8> },
    /// `signedAttrs` 中没有 `messageDigest` 属性。
    MissingAttribute,
    /// 摘要算法 OID 未识别，本库后端无法计算。
    UnsupportedAlgorithm,
}

impl Document {
    /// 校验 CMS signedAttrs 的 `messageDigest` 与 `/ByteRange` 覆盖字节摘要
    /// 的绑定。
    ///
    /// 流程：先按 [`Document::validate_signature_byte_range`] 校验 ByteRange
    /// 结构，再经 [`Document::signature_cms`] 解析 `/Contents`，最后按每个
    /// `SignerInfo` 的摘要算法用 [`Document::signature_digest`] 重算摘要并
    /// 与该签署者的 `messageDigest` 比较。同一摘要算法只计算一次。
    ///
    /// 返回值为逐签署者结果（[`SignerIntegrity`]）；若 `/Contents` 缺失、
    /// 为空或全零占位，返回 [`SignatureError::NoContents`]。
    ///
    /// # 能力边界
    ///
    /// 这是**摘要绑定校验**，不是签名验证：不校验 RSA/ECDSA 签名值、
    /// 证书链/信任锚、`signingTime` 可信度与时间戳。命名中的
    /// `integrity` 指「覆盖字节未被改动且与 CMS 声明一致」。
    pub fn verify_signature_integrity(
        &self,
        signature: &DocumentSignature,
        backend: &dyn SignatureBackend,
    ) -> Result<SignatureIntegrity> {
        self.validate_signature_byte_range(signature)?;
        let Some(cms) = self.signature_cms(signature)? else {
            return Err(SignatureError::NoContents);
        };
        let mut digests: Vec<(DigestAlgorithm, Vec<u8>)> = Vec::new();
        let mut statuses = Vec::with_capacity(cms.signers.len());
        for signer in &cms.signers {
            let Some(algorithm) = signer.digest_algorithm.known() else {
                statuses.push(SignerIntegrity::UnsupportedAlgorithm);
                continue;
            };
            let Some(expected) = signer.message_digest.as_ref() else {
                statuses.push(SignerIntegrity::MissingAttribute);
                continue;
            };
            let actual = match digests.iter().find(|(known, _)| *known == algorithm) {
                Some((_, cached)) => cached.clone(),
                None => {
                    let digest = self.signature_digest(signature, algorithm, backend)?;
                    digests.push((algorithm, digest.clone()));
                    digest
                }
            };
            if &actual == expected {
                statuses.push(SignerIntegrity::Match);
            } else {
                statuses.push(SignerIntegrity::Mismatch {
                    expected: expected.clone(),
                    actual,
                });
            }
        }
        Ok(SignatureIntegrity {
            cms,
            signers: statuses,
        })
    }
}
