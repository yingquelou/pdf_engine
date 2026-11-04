//! 12.7.7.3 的 **FDF 侧解密**（Table 244 `/EncryptionRevision`）：加密的
//! 嵌入式 FDF 流的密钥派生与 RC4 解扰。
//!
//! 规范依据（`pdftotext -raw docs/PDF32000_2008.pdf` 实测行号）：
//! - raw L24893–L24900（12.7.7.3，Table 243 续表 `/EmbeddedFDFs` 段落尾）：
//!   加密的嵌入式 FDF 流的流字典**额外必需** `/EncryptionRevision`，且
//!   「Although the FDF encryption mechanism is separate from the one for
//!   PDF file encryption described in 7.6, "Encryption," revision 1 (the
//!   only one defined) uses a similar RC4 encryption algorithm based on a
//!   40-bit encryption key. The key shall be computed by means of an MD5
//!   hash, using a padded user-supplied password as input. The computation
//!   shall be identical to steps (a) and (b) of the "Algorithm 2: Computing
//!   an encryption key" in 7.6.3.3 …; the first 5 bytes of the result shall
//!   be the encryption key for the embedded FDF file.」
//! - raw L3015–L3018 + L3054–L3059（7.6.3.3 Algorithm 2 的步 (a)/(b)）：
//!   步 (a) 把口令**截断或填充**到恰好 32 字节（不足则从固定填充串开头补齐；
//!   空口令 ⇒ 整串即填充串），步 (b)「Initialize the MD5 hash function and
//!   pass the result of step (a) as input to this function.」⇒ FDF 密钥 =
//!   `MD5(padded_password)` 的**前 5 字节**（40 位）；`/O`、`/P`、`/ID`
//!   属 Algorithm 2 的步 (c)(d)(e)，FDF 明文只引用 (a) 与 (b)，故
//!   **不参与**本密钥派生（与 7.6 的 PDF 文件加密无关）。
//! - raw L24935–L24939（Table 244）：`EncryptionRevision` 是 integer，
//!   「Required if the FDF file is encrypted; PDF 1.4」，且「This value …
//!   shall be defined at the time of publication is 1」⇒ 只有 revision 1
//!   有已定义算法；[`super::expand`] 对其余值只记诊断、不 Attempt 解密。
//!
//! 填充串与 7.6.3.3 步 (a) 的既有实现（本库
//! `src/core/reader/security/standard/key.rs` 的 `PASSWORD_PADDING` +
//! `pad_password`）逐字节一致；那里两者都是 `pub(super)` 可见性，而
//! `security/**` 属另一条车道（本切片禁改），故在此新增**本地**实现而不
//! 跨模块提可见性。改动点已记入汇报。
//!
//! 安全：派生密钥与解密明文一律由 [`Zeroizing`] 承载（与 `key.rs` /
//! `parser.rs` 同风格）；密钥字节绝不写进诊断、[`super::view`] 的字段或
//! `Display`，解密失败一律「跳过 + 记录诊断 + 保留原值」。

use md5::{Digest, Md5};
use rc4::{KeyInit, Rc4, StreamCipher, consts::U5};
use zeroize::Zeroizing;

/// 7.6.3.3 步 (a) 的固定填充串（raw L3054–L3055），与本库
/// `security/standard/key.rs` 的 `PASSWORD_PADDING` 同值同序。
const PASSWORD_PADDING: [u8; 32] = [
    0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01, 0x08,
    0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53, 0x69, 0x7a,
];

/// Table 244 revision 1 的密钥长度：MD5 摘要的**前 5 字节**（40 位；
/// raw L24898–L24900「based on a 40-bit encryption key … the first 5
/// bytes of the result」）。
pub(crate) const FDF_ENCRYPTION_KEY_LEN: usize = 5;

/// 7.6.3.3 步 (a)（raw L3015–L3018）：把口令截断/填充到恰好 32 字节。
/// 短于 32 字节时从 [`PASSWORD_PADDING`] 开头补齐余下的字节，长于 32
/// 字节时只取前 32 字节；**空口令等价于整串填充串**。
pub(crate) fn pad_password(password: &[u8]) -> Zeroizing<Vec<u8>> {
    let take = password.len().min(PASSWORD_PADDING.len());
    let mut padded = Zeroizing::new(Vec::with_capacity(PASSWORD_PADDING.len()));
    padded.extend_from_slice(&password[..take]);
    let remaining = PASSWORD_PADDING.len() - padded.len();
    padded.extend_from_slice(&PASSWORD_PADDING[..remaining]);
    padded
}

/// 12.7.7.3 revision 1 的嵌入式 FDF 加密密钥：步 (a) 的填充结果喂给
/// MD5（步 (b)，raw L3054–L3059），取摘要**前 5 字节**（raw
/// L24896–L24900）。不含 `/O`、`/P`、`/ID`（那是 Algorithm 2 的步
/// (c)(d)(e)，FDF 明文只引用 (a) 与 (b)）。
pub(crate) fn derive_key(password: &[u8]) -> Zeroizing<[u8; FDF_ENCRYPTION_KEY_LEN]> {
    let padded = pad_password(password);
    let digest = Md5::digest(padded.as_slice());
    // MD5 摘要恒 16 字节，故前 FDF_ENCRYPTION_KEY_LEN 字节总是可取。
    let mut key = [0u8; FDF_ENCRYPTION_KEY_LEN];
    key.copy_from_slice(&digest[..FDF_ENCRYPTION_KEY_LEN]);
    Zeroizing::new(key)
}

/// RC4 解扰：加解密是**同一**运算（流密码异或密钥流），故本函数既用于
/// 读端解密，也用于测试里构造密文载体。`Rc4::<U5>` 把密钥**钉死为恰好 5
/// 字节**（Table 244 revision 1 的 40 位密钥），长度不符时
/// [`rc4_transform`] 返回 `None`（调用方降级为「跳过 + 记录诊断 + 保留
/// 原值」，不 panic）。
pub(crate) fn rc4_transform(key: &[u8], input: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let mut cipher = Rc4::<U5>::new_from_slice(key).ok()?;
    let mut output = Zeroizing::new(input.to_vec());
    cipher.apply_keystream(&mut output);
    Some(output)
}

/// 按 12.7.7.3 解密一个 `/EncryptionRevision 1` 的嵌入式 FDF 载体流字节。
/// 返回 `None` 表示无法解扰（密钥长度不符），调用方须保留原值并记诊断。
pub(crate) fn decrypt(carrier: &[u8], password: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let key = derive_key(password);
    rc4_transform(key.as_slice(), carrier)
}
