//! 数字签名验证编排（12.8 F1.3）。
//!
//! 库只做「摘要绑定 → 后端签名值验证 → 后端链验证 → 后端时间戳验证」的
//! 调用编排与结果聚合。RSA/ECDSA 验签、证书链构建/吊销、时间戳权威交互
//! 全在 [`SignatureVerifyBackend`] 实现里，库内不持密钥、不做信任决策。
//! 未注入后端时逐签署者状态为 `NotProvided`，任何聚合口径都不得报「通过」。

use crate::core::types::Document;

use super::entries::DocumentSignature;
use super::integrity::SignerIntegrity;
use super::reference::TimestampReference;
use super::{CmsAlgorithmIdentifier, Result, SignatureBackend};

/// 签名值验证结果（后端给出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureCheck {
    /// 签名值对 signedAttrs 有效。
    Match,
    /// 签名值无效。
    Mismatch,
    /// 未注入后端，或缺少验证所需材料（如 signedAttrs）。
    NotProvided,
    /// 算法/格式不受后端支持，与「不匹配」区分。
    Unsupported,
}

/// 证书链信任结果（后端给出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainCheck {
    /// 链在注入的信任锚下可信。
    Trusted,
    /// 链不可信。
    Untrusted,
    /// 未注入信任锚或后端，或证书集为空。
    NotProvided,
}

/// RFC 3161 时间戳验证结果（后端给出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampCheck {
    Valid,
    Invalid,
    /// 未注入后端或时间戳条目无可验证的令牌字节。
    NotProvided,
}

/// 验证选项：信任锚由调用方以 DER 证书字节注入，库不内置任何锚。
#[derive(Debug, Clone, Default)]
pub struct VerifyOptions {
    pub trust_anchors: Vec<Vec<u8>>,
}

/// 验证后端：三个彼此独立的判定，均由实现负责密码学与信任细节。
pub trait SignatureVerifyBackend {
    /// 校验 `signed_attrs_der`（`SET OF` 编码）上的签名值。
    fn verify_signature(
        &self,
        signed_attrs_der: &[u8],
        signature: &[u8],
        algorithm: &CmsAlgorithmIdentifier,
        certificates: &[Vec<u8>],
    ) -> SignatureCheck;

    /// 在给定信任锚下校验证书集。
    fn verify_chain(&self, certificates: &[Vec<u8>], anchors: &[Vec<u8>]) -> ChainCheck;

    /// 校验 RFC 3161 令牌（原样 DER 字节）。
    fn verify_timestamp(&self, token_der: &[u8]) -> TimestampCheck;
}

/// 单个签署者的聚合验证结果。
#[derive(Debug, Clone, PartialEq)]
pub struct SignerVerification {
    /// signedAttrs `messageDigest` 与覆盖字节摘要的绑定（库内校验）。
    pub integrity: SignerIntegrity,
    /// 签名值验证（后端）。
    pub signature: SignatureCheck,
    /// 证书链验证（后端）。
    pub chain: ChainCheck,
}

/// 单个时间戳条目的聚合结果；`reference` 与 `token` 原样保留。
#[derive(Debug, Clone, PartialEq)]
pub struct TimestampVerification {
    pub reference: TimestampReference,
    /// `/Data` 流解码后的令牌字节；不是流或读取失败时为 `None`。
    pub token: Option<Vec<u8>>,
    pub status: TimestampCheck,
}

/// 一次签名的完整验证结果。
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureVerification {
    pub signers: Vec<SignerVerification>,
    pub timestamps: Vec<TimestampVerification>,
}

impl SignatureVerification {
    /// 严格「全部通过」：所有签署者的摘要绑定、签名值、证书链均通过，
    /// 且所有时间戳有效。任一 `NotProvided` / `Unsupported` 均不通过。
    pub fn all_matched(&self) -> bool {
        !self.signers.is_empty()
            && self.signers.iter().all(|item| {
                item.integrity == SignerIntegrity::Match
                    && item.signature == SignatureCheck::Match
                    && item.chain == ChainCheck::Trusted
            })
            && self
                .timestamps
                .iter()
                .all(|item| item.status == TimestampCheck::Valid)
    }
}

impl Document {
    /// 验证签名的编排入口（12.8 F1.3）。
    ///
    /// `verify_backend` 为 `None` 时只做库内的摘要绑定校验，签名值/链/时间戳
    /// 一律 `NotProvided`；调用方不得据此声称签名有效。
    pub fn verify_signature(
        &self,
        signature: &DocumentSignature,
        options: &VerifyOptions,
        digest_backend: &dyn SignatureBackend,
        verify_backend: Option<&dyn SignatureVerifyBackend>,
    ) -> Result<SignatureVerification> {
        let integrity = self.verify_signature_integrity(signature, digest_backend)?;
        let certificates = integrity.cms.certificates.clone();
        let mut signers = Vec::with_capacity(integrity.signers.len());
        for (signer, digest_status) in integrity.cms.signers.iter().zip(&integrity.signers) {
            let (signature_check, chain_check) = match verify_backend {
                None => (SignatureCheck::NotProvided, ChainCheck::NotProvided),
                Some(backend) => {
                    let signature_check = match signer.signed_attributes_der.as_deref() {
                        Some(attrs) => backend.verify_signature(
                            attrs,
                            &signer.signature,
                            &signer.signature_algorithm,
                            &certificates,
                        ),
                        None => SignatureCheck::NotProvided,
                    };
                    let chain_check = if certificates.is_empty() || options.trust_anchors.is_empty()
                    {
                        ChainCheck::NotProvided
                    } else {
                        backend.verify_chain(&certificates, &options.trust_anchors)
                    };
                    (signature_check, chain_check)
                }
            };
            signers.push(SignerVerification {
                integrity: digest_status.clone(),
                signature: signature_check,
                chain: chain_check,
            });
        }

        let mut timestamps = Vec::new();
        for reference in self.signature_timestamp_references(signature)? {
            let token = self.timestamp_reference_token(&reference);
            let status = match (verify_backend, token.as_deref()) {
                (Some(backend), Some(bytes)) => backend.verify_timestamp(bytes),
                _ => TimestampCheck::NotProvided,
            };
            timestamps.push(TimestampVerification {
                reference,
                token,
                status,
            });
        }
        Ok(SignatureVerification {
            signers,
            timestamps,
        })
    }
}
