//! 可插拔签名摘要后端（Roadmap F1：密码学经 trait 隔离）。

use md5::Md5;
use sha1::Sha1;
use sha2::{Sha256, Sha384, Sha512};

use super::entries::DigestAlgorithm;

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
            DigestAlgorithm::Sha384 => Box::new(GenericDigest(Sha384::default())),
            DigestAlgorithm::Sha512 => Box::new(GenericDigest(Sha512::default())),
        }
    }
}
