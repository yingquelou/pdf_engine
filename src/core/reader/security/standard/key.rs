//! 密码填充与 Algorithm 2 文件密钥派生（ISO 32000-1 7.6.3.3）。

use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::core::reader::security::crypto::CryptoBackend;

pub(super) const PASSWORD_PADDING: [u8; 32] = [
    0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01, 0x08,
    0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53, 0x69, 0x7a,
];

/// 7.6.3.3 步 (a)：把口令截断/填充到恰好 32 字节。
///
/// 供标准安全处理器与 FDF 侧密钥派生共用，避免同一算法存在副本。
pub(crate) fn pad_password(password: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut padded = Zeroizing::new(Vec::with_capacity(PASSWORD_PADDING.len()));
    padded.extend_from_slice(&password[..password.len().min(PASSWORD_PADDING.len())]);
    let remaining = PASSWORD_PADDING.len() - padded.len();
    padded.extend_from_slice(&PASSWORD_PADDING[..remaining]);
    padded
}

/// Algorithm 3 b–d – 派生 `/O` 用的 RC4 所有者密钥。
///
/// 步 (c)（raw L3092–L3093）与 Algorithm 2 步 (h)（raw L3072–L3074）**只差在
/// 是否截断**：Algorithm 2 每次续哈希只喂「上一次输出的前 n 字节」，而
/// Algorithm 3 把「上一次 MD5 的输出」整体喂给下一次——密钥长度的截断只发生
/// 在步 (d)（raw L3096–L3098）。故这里循环内不得切片。
///
/// 二者在 `key_len == 16`（128 位）时结果相同（16 字节摘要切片是恒等），
/// 缺陷只在 `/Length` < 128 且 R ≥ 3 时显形；`/Length` 缺省即 40 位
/// （Table 20），所以这不是纯理论分支。
pub(super) fn derive_owner_key(
    padded_password: &[u8],
    revision: u8,
    key_len: usize,
    crypto: &dyn CryptoBackend,
) -> Zeroizing<Vec<u8>> {
    let mut digest = crypto.md5(padded_password);
    if revision >= 3 {
        for _ in 0..50 {
            digest = crypto.md5(&digest);
        }
    }
    Zeroizing::new(digest[..key_len].to_vec())
}

pub(super) fn recover_user_password(
    owner_key: &[u8],
    owner_entry: &[u8],
    revision: u8,
    crypto: &dyn CryptoBackend,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let mut recovered = Zeroizing::new(owner_entry.to_vec());
    if revision == 2 {
        *recovered = crypto.rc4(owner_key, &recovered)?;
    } else {
        for iteration in (0..20).rev() {
            let key = Zeroizing::new(owner_key.iter().map(|b| b ^ iteration).collect::<Vec<_>>());
            *recovered = crypto.rc4(&key, &recovered)?;
        }
    }
    Ok(recovered)
}

pub(super) struct FileKeyParams<'a> {
    pub(super) padded_password: &'a [u8],
    pub(super) owner_entry: &'a [u8],
    pub(super) permissions: i32,
    pub(super) id: &'a [u8],
    pub(super) revision: u8,
    pub(super) encrypt_metadata: bool,
    pub(super) key_len: usize,
}

pub(super) fn derive_file_key(
    params: FileKeyParams<'_>,
    crypto: &dyn CryptoBackend,
) -> Zeroizing<Vec<u8>> {
    let mut input = Zeroizing::new(Vec::with_capacity(72 + params.id.len()));
    input.extend_from_slice(params.padded_password);
    input.extend_from_slice(params.owner_entry);
    // Algorithm 2 步骤 d：把 `/P` 的整数值转为 32 位无符号二进制、低位字节
    // 在前喂给 MD5。`i32::to_le_bytes` 取的就是同一批 4 个位字节，负数按
    // 二进制补码展开（7.6.3.2 NOTE：保留高位须置 1 故 `/P` 常写成负整数），
    // 与读端权限视图 `Permissions::flags`（`p_value as u32`）看到的位一致。
    input.extend_from_slice(&params.permissions.to_le_bytes());
    input.extend_from_slice(params.id);
    if params.revision >= 4 && !params.encrypt_metadata {
        input.extend_from_slice(&[0xff; 4]);
    }
    let mut digest = crypto.md5(&input);
    if params.revision >= 3 {
        for _ in 0..50 {
            digest = crypto.md5(&digest[..params.key_len]);
        }
    }
    Zeroizing::new(digest[..params.key_len].to_vec())
}

pub(super) fn validate_user_password(
    file_key: &[u8],
    user_entry: &[u8],
    id: &[u8],
    revision: u8,
    crypto: &dyn CryptoBackend,
) -> Result<bool, String> {
    let expected = if revision == 2 {
        crypto.rc4(file_key, &PASSWORD_PADDING)?
    } else {
        let mut input = Zeroizing::new(Vec::with_capacity(PASSWORD_PADDING.len() + id.len()));
        input.extend_from_slice(&PASSWORD_PADDING);
        input.extend_from_slice(id);
        let digest = crypto.md5(&input);
        let mut value = crypto.rc4(file_key, &digest)?;
        for iteration in 1..20u8 {
            let key = Zeroizing::new(file_key.iter().map(|b| b ^ iteration).collect::<Vec<_>>());
            value = crypto.rc4(&key, &value)?;
        }
        value
    };
    let compare_len = if revision == 2 { 32 } else { 16 };
    Ok(expected[..compare_len]
        .ct_eq(&user_entry[..compare_len])
        .into())
}
