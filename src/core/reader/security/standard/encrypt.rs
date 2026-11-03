//! 写端标准安全处理器（ISO 32000-1 7.6.3.3、7.6.3.4）：加密字典 `/O` `/U`
//! 的生成、Table 20/Table 21 加密字典构造，以及字符串与流载荷的按对象加密。
//!
//! RC4 是对称算法（7.6.2「Because the RC4 algorithm … shall be used both to
//! encrypt and to decrypt data」），故本模块**复用读端原语**而不是另写一套：
//! - [`key::pad_password`]（Algorithm 2 a 的 32 字节填充/截断）、
//!   [`key::derive_owner_key`]（Algorithm 3 b–d）、
//!   [`key::derive_file_key`]（Algorithm 2 全部步骤）；
//! - [`decrypt::decrypt_bytes`]（Algorithm 1 的对象密钥派生 + 逐字节变换）；
//! - [`strings::decode_hex`] / [`strings::encode_hex`]。
//!
//! 明文口令只在调用栈内以 `Zeroizing` 缓冲参与密钥派生，不写盘、不入日志。

use zeroize::Zeroizing;

use crate::core::reader::security::crypto::{CryptoBackend, RustCryptoBackend};
use crate::core::types::{Dict, Document, Object, StreamData, Value};

use super::decrypt::decrypt_bytes;
use super::filters::CryptMethod;
use super::key::{
    FileKeyParams, PASSWORD_PADDING, derive_file_key, derive_owner_key, pad_password,
};
use super::strings::{decode_hex, encode_hex};

/// Table 21 – 加密字典 `/O` 与 `/U` 的串长（字节）。
const ENTRY_LEN: usize = 32;

/// 7.6.3.3.3 Algorithm 3 f/g – 用所有者密钥加密填充后的用户密码得到 `/O`。
///
/// R 2 只做一次 RC4；R ≥ 3 再以「原密钥逐字节 XOR 迭代计数（1–19）」为密钥
/// 追加 19 次 RC4。读端 `key::recover_user_password`（Algorithm 7）是它的逆。
fn compute_owner_entry(
    owner_key: &[u8],
    padded_user: &[u8],
    revision: u8,
    crypto: &dyn CryptoBackend,
) -> Result<Vec<u8>, String> {
    let mut value = crypto.rc4(owner_key, padded_user)?;
    if revision >= 3 {
        for iteration in 1..20u8 {
            let key = Zeroizing::new(
                owner_key
                    .iter()
                    .map(|byte| byte ^ iteration)
                    .collect::<Vec<_>>(),
            );
            value = crypto.rc4(&key, &value)?;
        }
    }
    Ok(value)
}

/// Table 21 `/U`：R 2 走 7.6.3.4.2 Algorithm 4，R ≥ 3 走 Algorithm 5。
///
/// Algorithm 4 b：`RC4(文件密钥, 32 字节填充串)`；Algorithm 5 b–f：
/// `MD5(填充串 ‖ /ID 首元素)` 的 16 字节摘要先 RC4 一次、再按迭代计数
/// XOR 密钥做 19 次，最后追加 16 字节任意填充凑满 32 字节。
fn compute_user_entry(
    file_key: &[u8],
    file_id: &[u8],
    revision: u8,
    crypto: &dyn CryptoBackend,
) -> Result<Vec<u8>, String> {
    if revision == 2 {
        return crypto.rc4(file_key, &PASSWORD_PADDING);
    }
    let mut input = Zeroizing::new(Vec::with_capacity(PASSWORD_PADDING.len() + file_id.len()));
    input.extend_from_slice(&PASSWORD_PADDING);
    input.extend_from_slice(file_id);
    let digest = crypto.md5(&input);
    let mut value = crypto.rc4(file_key, &digest)?;
    for iteration in 1..20u8 {
        let key = Zeroizing::new(
            file_key
                .iter()
                .map(|byte| byte ^ iteration)
                .collect::<Vec<_>>(),
        );
        value = crypto.rc4(&key, &value)?;
    }
    // Algorithm 5 f：「Append 16 bytes of arbitrary padding」——读端 Algorithm 6
    // 只比对前 16 字节，故此处取标准填充串前 16 字节，保持输出确定性。
    value.extend_from_slice(&PASSWORD_PADDING[..ENTRY_LEN / 2]);
    Ok(value)
}

/// Table 20/Table 21 – 标准安全处理器的加密字典。
///
/// `/Filter /Standard`、`/V`、`/R`、`/O`、`/U`、`/P` 为 Table 20 + Table 21
/// 的必需条目；`/Length` 仅当 `/V` 为 2 时写出（Table 20 括注）。`/O` `/U`
/// 是直接对象且**不加密**（7.6.1「Any strings in an Encrypt dictionary」例外）。
fn encryption_dictionary(
    handler_version: u8,
    revision: u8,
    key_bits: u8,
    permissions: i32,
    owner_entry: &[u8],
    user_entry: &[u8],
) -> Dict {
    let mut dict = Dict::new()
        .insert("Filter", Value::Name("Standard".into()))
        .insert("V", Value::Integer(i64::from(handler_version)))
        .insert("R", Value::Integer(i64::from(revision)))
        .insert("O", Value::HexString(encode_hex(owner_entry)))
        .insert("U", Value::HexString(encode_hex(user_entry)))
        .insert("P", Value::Integer(i64::from(permissions)));
    if handler_version == 2 {
        dict = dict.insert("Length", Value::Integer(i64::from(key_bits)));
    }
    dict
}

/// Algorithm 1 – 按对象加密一个对象值。
///
/// 与读端 `decrypt::decrypt_value` 严格镜像：字面串按「每字符一字节」取样
/// （读端同样以 `chars().map(|c| c as u8)` 还原），密文以十六进制串写出以
/// 保持字节精确；签名值 `Contents`（`/Type /Sig` 或 `/FT /Sig`）不加密；
/// 流字典条目递归、流载荷按其自身对象号加密并同步 `/Length`。
fn encrypt_value(
    value: &mut Value,
    object_num: i64,
    generation: i64,
    signature_contents: bool,
    file_key: &[u8],
    method: CryptMethod,
    crypto: &dyn CryptoBackend,
) -> Result<(), String> {
    match value {
        Value::String(text) if !signature_contents => {
            let plain: Vec<u8> = text.chars().map(|char| char as u8).collect();
            let encrypted =
                decrypt_bytes(file_key, object_num, generation, method, &plain, crypto)?;
            *value = Value::HexString(encode_hex(&encrypted));
        }
        Value::HexString(hex) if !signature_contents => {
            let plain = decode_hex(hex)?;
            let encrypted =
                decrypt_bytes(file_key, object_num, generation, method, &plain, crypto)?;
            *hex = encode_hex(&encrypted);
        }
        Value::Array(array) => {
            for item in &mut array.0 {
                encrypt_value(
                    item, object_num, generation, false, file_key, method, crypto,
                )?;
            }
        }
        Value::Dict(dict) => {
            let is_signature = dict.get("Type").and_then(Value::as_name) == Some("Sig")
                || dict.get("FT").and_then(Value::as_name) == Some("Sig");
            for (key, item) in &mut dict.0 {
                encrypt_value(
                    item,
                    object_num,
                    generation,
                    is_signature && key == "Contents",
                    file_key,
                    method,
                    crypto,
                )?;
            }
        }
        Value::Stream(dict, data) => {
            let is_signature = dict.get("Type").and_then(Value::as_name) == Some("Sig")
                || dict.get("FT").and_then(Value::as_name) == Some("Sig");
            for (key, item) in &mut dict.0 {
                encrypt_value(
                    item,
                    object_num,
                    generation,
                    is_signature && key == "Contents",
                    file_key,
                    method,
                    crypto,
                )?;
            }
            let StreamData::Owned(bytes) = data else {
                return Err(
                    "encrypting a document requires inline stream bytes (StreamData::Owned)"
                        .to_string(),
                );
            };
            let encrypted = decrypt_bytes(file_key, object_num, generation, method, bytes, crypto)?;
            *bytes = encrypted;
            // 写端在流字典里显式写了 /Length（见 assemble/*），密文长度虽与
            // RC4 明文相同（RC4 不改变长度），仍按实际字节重算以免漂移。
            if dict.get("Length").and_then(Value::as_integer).is_some() {
                let len = bytes.len();
                dict.0
                    .insert("Length".to_string(), Value::Integer(len as i64));
            }
        }
        _ => {}
    }
    Ok(())
}

impl Document {
    /// 7.6.3 – 以标准安全处理器加密整篇文档（写端唯一入口）。
    ///
    /// 流程：Algorithm 3 求 `/O` → Algorithm 2 派生文件密钥 → Algorithm 4/5
    /// 求 `/U` → 把 Table 20/Table 21 加密字典作为**未压缩的顶层间接对象**
    /// 追加（7.5.7：加密字典不得存于对象流）→ trailer 挂 `/Encrypt` 并校正
    /// `/Size` → 按 Algorithm 1 加密既有对象里的全部字符串与流载荷。
    ///
    /// `handler_version` 为加密字典 `/V`（`1` = RC4-40/`R 2`，`2` = RC4-128/
    /// `R 3`）；`key_bits` 为 `/Length`（仅 `/V 2` 写盘，R 2 恒为 40 位 5 字节）；
    /// `owner_password` 为 `None` 时按 Algorithm 3 a 用用户密码代替；
    /// `file_id` 为 trailer `/ID` 首元素（Algorithm 2 e）。
    ///
    /// 失败返回错误且**不改动任何对象**（先算密钥、后改文档）。
    pub(crate) fn encrypt_with_standard_handler(
        &mut self,
        user_password: &[u8],
        owner_password: Option<&[u8]>,
        permissions: i32,
        handler_version: u8,
        key_bits: u8,
        file_id: &[u8],
    ) -> Result<(), String> {
        if file_id.is_empty() {
            return Err("encrypted documents need a trailer /ID element".to_string());
        }
        let revision = match handler_version {
            1 => 2,
            2 => 3,
            other => {
                return Err(format!(
                    "unsupported standard security handler /V {other} (write side handles 1 and 2)"
                ));
            }
        };
        let key_len = if revision == 2 {
            5
        } else {
            if !key_bits.is_multiple_of(8) || !(40..=128).contains(&key_bits) {
                return Err(format!(
                    "invalid encryption key length {key_bits} bits: /Length must be a multiple of 8 in 40..=128"
                ));
            }
            usize::from(key_bits / 8)
        };
        let crypto = &RustCryptoBackend;

        // —— Algorithm 3（/O）与 Algorithm 2（文件密钥）、Algorithm 4/5（/U）——
        let padded_user = pad_password(user_password);
        let padded_owner = pad_password(owner_password.unwrap_or(user_password));
        let owner_key = derive_owner_key(&padded_owner, revision, key_len, crypto);
        let owner_entry = compute_owner_entry(&owner_key, &padded_user, revision, crypto)?;
        let file_key = derive_file_key(
            FileKeyParams {
                padded_password: &padded_user,
                owner_entry: &owner_entry,
                permissions,
                id: file_id,
                revision,
                // /EncryptMetadata 仅在 /V 4 有意义（Table 21），R 2/R 3 恒按
                // 「元数据也加密」处理，与读端 `auth.rs` 的默认值一致。
                encrypt_metadata: true,
                key_len,
            },
            crypto,
        );
        let user_entry = compute_user_entry(&file_key, file_id, revision, crypto)?;
        if owner_entry.len() != ENTRY_LEN || user_entry.len() != ENTRY_LEN {
            return Err("encryption dictionary /O and /U must be 32-byte strings".to_string());
        }

        // —— Algorithm 1：加密已装配对象里的字符串与流 ——
        let method = CryptMethod::Rc4 {
            key_len: file_key.len(),
        };
        let mut objects = self.objects_for_write();
        let mut max_num = 0i64;
        for object in &mut objects {
            max_num = max_num.max(object.num);
            encrypt_value(
                &mut object.value,
                object.num,
                object.generation,
                false,
                &file_key,
                method,
                crypto,
            )?;
        }
        for object in objects {
            self.set_object(object);
        }

        // —— 加密字典：顶层未压缩间接对象（7.5.7）——
        let encrypt_num = max_num + 1;
        self.set_object(Object::new(
            encrypt_num,
            0,
            Value::Dict(encryption_dictionary(
                handler_version,
                revision,
                key_bits,
                permissions,
                &owner_entry,
                &user_entry,
            )),
        ));
        self.trailer
            .dict
            .0
            .insert("Encrypt".to_string(), Value::Ref(encrypt_num, 0));
        self.trailer
            .dict
            .0
            .insert("Size".to_string(), Value::Integer(encrypt_num + 1));
        Ok(())
    }
}
