//! 写端加密的类型化入口（ISO 32000-1 7.6.3 标准安全处理器）。
//!
//! 密钥与字典的算法本体在 `reader::security::standard::encrypt`（复用读端
//! 原语，保证读写对称）；本模块只负责**构建期校验**与把参数交给
//! [`PdfBuilder::emit`]，任何拒绝都发生在装配与序列化之前，故零字节输出。
//!
//! 校验依据：
//! - Table 20（7.6.3.2 前）：`/V 1` 为 40 位 RC4，`/V 2` 为 PDF 1.4 起允许
//!   更长的 RC4 密钥；`/Length`「shall be a multiple of 8, in the range 40
//!   to 128」且「only if V is 2 or 3」；
//! - Table 21：`/R` 为 2 的条件——`/V` 小于 2 **且** Table 22 中
//!   「Security handlers of revision 3 or greater」的权限位一个都没被置 0；
//!   `/O` `/U` 为 32 字节串；`/EncryptMetadata` 仅 `/V 4` 有意义；
//! - Table 22 与其后的 NOTE：位序 1（低序）到 32（高序），保留高位须为 1，
//!   因而 `/P` 一律是负整数（例：`/R 2` 下 `-44` = 允许打印与复制）；
//!   读端只解释位 3、4、5、6、9、10、11、12；
//! - 7.5.7：加密字典**不得**存入对象流 ⇒ 本 writer 的对象流布局由
//!   目标版本 `>= 1.5` 触发，故加密输出锁定 1.0–1.4 的经典布局；
//! - 7.6.1：加密是 PDF 1.1 特性；trailer `/ID`、加密字典内的串、以及
//!   已加密流内部的串不参与逐对象加密。

use std::io::Write;

use crate::core::builder::PdfBuilder;
use crate::core::builder::metadata::FileIdentifier;
use crate::core::builder::output::EncryptionSpec;
use crate::core::types::{Document, PdfVersion};
use crate::core::writer::{DocumentWriter, WriteError};

/// 库策略：口令上限（字节）。
///
/// **待核**：ISO 32000-1 未规定口令长度上限，Algorithm 2 a 只做「超过 32
/// 字节取前 32 字节、不足 32 字节用标准填充串补齐」。此处沿用广泛实现的
/// 127 字节上限，属本库的保守输入契约，不改变任何密钥派生结果。
const MAX_PASSWORD_BYTES: usize = 127;

/// Table 22 NOTE：保留高位全为 1 时的最小合法形态，即 `/P = -4`
/// （位 3–32 全 1、位 1–2 为 0）——全部权限授予。
const ALL_PERMISSIONS: i32 = -4;

/// Table 22 中「Security handlers of revision 3 or greater」的位（9–12）。
/// Table 21 要求：`/R 2` 的文档不得把这些位中任一个置 0。
const REVISION_3_BITS: u32 = 0x0000_0F00;

/// 位 1–2（低序两位）：Table 22 未定义、读端忽略，按规范写法保持 0。
const LOW_RESERVED_BITS: u32 = 0x0000_0003;

/// 位 25–32（高序 8 位）：Table 22 NOTE 要求全部为 1。
const HIGH_RESERVED_BITS: u32 = 0xFF00_0000;

impl PdfBuilder {
    /// 以标准安全处理器 `/V 1`（RC4-40，`/R 2`）加密并序列化文档。
    ///
    /// 与 [`PdfBuilder::build`] 相同的对象号与交叉引用布局，只在写出前把
    /// 每个对象的字符串与流载荷按 Algorithm 1 加密，并追加 Table 20/Table 21
    /// 加密字典（trailer 挂 `/Encrypt`）。
    ///
    /// 口令按 Algorithm 2 a 截断/补齐到 32 字节参与密钥派生；明文口令只在
    /// 调用栈内存在，不写盘、不入日志。`owner_password` 为 `None` 时按
    /// Algorithm 3 a 用 `user_password` 代替（此时 `/P` 必须是全授予，
    /// 否则限制可被用户口令绕过，返回错误）。
    ///
    /// 下列情形在任何字节写出前返回 [`WriteError`]：
    /// - `user_password` 为空或超过 127 字节，`owner_password`（显式给出时）
    ///   为空或超过 127 字节；`/P` 高位保留位非全 1、低序位 1–2 非 0，或
    ///   `/V 1` 下 Table 22 的位 9–12 被置 0（应为 `/V 2`）；
    /// - 设置了流压缩（`compress_streams`）：读端先解密再解 `/Filter`，
    ///   与逐对象 RC4 不能混用；
    /// - 目标版本低于 1.1（7.6.1）或高于 1.4（7.5.7 禁止加密字典入对象流）。
    pub fn build_encrypted_rc4_40<W: Write>(
        &mut self,
        w: &mut W,
        user_password: &[u8],
        owner_password: Option<&[u8]>,
        permissions: i32,
    ) -> Result<(), WriteError> {
        let spec = EncryptionSpec {
            user_password,
            owner_password,
            permissions,
            handler_version: 1,
            key_bits: 40,
        };
        self.validate_encryption(&spec)?;
        self.emit(w, Some(spec))
    }

    /// 以标准安全处理器 `/V 2`（RC4，`/R 3`，`/Length = key_bits`）加密并
    /// 序列化文档；`key_bits` 须为 40–128 之间 8 的倍数（Table 20 `/Length`）。
    ///
    /// 校验与拒绝条件同 [`PdfBuilder::build_encrypted_rc4_40`]，另要求目标
    /// 版本不低于 1.4（Table 20 标注 `/V 2` 为 PDF 1.4 特性），且 `/P` 的
    /// revision 3 权限位（9–12）可自由置位。
    pub fn build_encrypted_rc4_128<W: Write>(
        &mut self,
        w: &mut W,
        user_password: &[u8],
        owner_password: Option<&[u8]>,
        permissions: i32,
        key_bits: u8,
    ) -> Result<(), WriteError> {
        let spec = EncryptionSpec {
            user_password,
            owner_password,
            permissions,
            handler_version: 2,
            key_bits,
        };
        self.validate_encryption(&spec)?;
        self.emit(w, Some(spec))
    }

    /// 构建期校验（Table 20/Table 21/Table 22 + 布局与压缩冲突），零副作用。
    fn validate_encryption(&self, spec: &EncryptionSpec<'_>) -> Result<(), WriteError> {
        if self.compression.is_some() {
            return Err(WriteError::Compression(
                "stream compression cannot be combined with standard-handler encryption: a \
                 conforming reader decrypts each stream before applying its /Filter"
                    .to_string(),
            ));
        }
        if !matches!(spec.handler_version, 1 | 2) {
            return Err(WriteError::InvalidObject(format!(
                "encryption dictionary /V {} is not supported when writing: only 1 (RC4-40) \
                 and 2 (RC4 with a longer key) are handled",
                spec.handler_version
            )));
        }
        if spec.handler_version == 1 && spec.key_bits != 40 {
            return Err(WriteError::InvalidObject(format!(
                "/V 1 fixes the encryption key at 40 bits, got {}",
                spec.key_bits
            )));
        }
        if spec.handler_version == 2
            && (!spec.key_bits.is_multiple_of(8) || !(40..=128).contains(&spec.key_bits))
        {
            return Err(WriteError::InvalidObject(format!(
                "invalid /Length {}: it shall be a multiple of 8 in the range 40 to 128 \
                 (Table 20)",
                spec.key_bits
            )));
        }
        check_password(spec.user_password, "user")?;
        if let Some(owner) = spec.owner_password {
            check_password(owner, "owner")?;
        } else if spec.permissions != ALL_PERMISSIONS {
            return Err(WriteError::InvalidObject(
                "restrictive /P needs a separate owner password: without one, Algorithm 3 \
                 substitutes the user password and the permissions can be bypassed with it"
                    .to_string(),
            ));
        }
        let flags = spec.permissions as u32;
        if flags & LOW_RESERVED_BITS != 0 {
            return Err(WriteError::InvalidObject(format!(
                "/P {flags:#010x} sets bit positions 1-2, which Table 22 leaves reserved at 0"
            )));
        }
        if flags & HIGH_RESERVED_BITS != HIGH_RESERVED_BITS {
            return Err(WriteError::InvalidObject(format!(
                "/P {flags:#010x} must keep the reserved high-order bits set to 1, i.e. it \
                 shall be given as a negative integer (Table 22 NOTE)"
            )));
        }
        if spec.handler_version == 1 && flags & REVISION_3_BITS != REVISION_3_BITS {
            return Err(WriteError::InvalidObject(format!(
                "/P {flags:#010x} clears one of bit positions 9-12, which are \
                 \u{201c}Security handlers of revision 3 or greater\u{201d}; such a document must \
                 use /V 2 (/R 3) instead of /V 1 (/R 2) (Table 21)"
            )));
        }
        if self.version < PdfVersion::V1_1 {
            return Err(WriteError::UnsupportedVersion(format!(
                "encryption needs PDF 1.1 or later (7.6.1), got target {}",
                self.version.as_str()
            )));
        }
        if self.version > PdfVersion::V1_4 {
            return Err(WriteError::UnsupportedVersion(format!(
                "encrypted output targets {} but only 1.0-1.4 are possible: at PDF 1.5 and \
                 above this writer packs objects into object streams, and 7.5.7 forbids \
                 storing a document's encryption dictionary there",
                self.version.as_str()
            )));
        }
        if spec.handler_version == 2 && self.version < PdfVersion::V1_4 {
            return Err(WriteError::UnsupportedVersion(format!(
                "encryption dictionary /V 2 is a PDF 1.4 feature (Table 20), got target {}",
                self.version.as_str()
            )));
        }
        Ok(())
    }

    /// Algorithm 2 e 需要的 trailer `/ID` 首元素字节。
    ///
    /// 显式 `set_file_identifier` 时复用其永久 ID；否则把**未加密**文档序列化
    /// 到内存缓冲作哈希材料（14.4 推荐以文件内容派生），确定性生成后写回
    /// trailer，保证密钥与盘上 `/ID` 一致。
    pub(in crate::core::builder) fn ensure_standard_file_id(
        &self,
        doc: &mut Document,
    ) -> Result<Vec<u8>, WriteError> {
        if let Some((permanent, _)) = doc.file_identifier() {
            return Ok(permanent.to_vec());
        }
        let mut material = Vec::new();
        DocumentWriter::new()
            .with_version(self.version)
            .build(doc, &mut material)?;
        let id = FileIdentifier::generate(&material);
        doc.trailer.dict.0.insert("ID".to_string(), id.to_value());
        Ok(id.permanent.to_vec())
    }
}

/// 口令长度契约：非空且不超过 [`MAX_PASSWORD_BYTES`] 字节。
fn check_password(password: &[u8], who: &str) -> Result<(), WriteError> {
    if password.is_empty() {
        return Err(WriteError::InvalidObject(format!(
            "the {who} password must not be empty: an empty user password makes the document \
             open without authentication, which this API does not offer"
        )));
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(WriteError::InvalidObject(format!(
            "the {who} password is {} bytes, above the {MAX_PASSWORD_BYTES}-byte limit this \
             library accepts (Algorithm 2 a truncates to its first 32 bytes anyway)",
            password.len()
        )));
    }
    Ok(())
}
