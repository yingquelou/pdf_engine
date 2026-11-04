//! 对象值与字节串的按对象密钥解密（7.6.3.2、7.6.5）：递归处理
//! 字符串/十六进制串/数组/字典/流，签名 `Contents` 不解密，
//! 流按 [`CryptMethod`] 走 RC4 或 AES-128-CBC。

use crate::core::reader::security::crypto::CryptoBackend;
use crate::core::types::{StreamData, Value};
use zeroize::Zeroizing;

use super::SecurityOptions;
use super::filters::CryptMethod;
use super::strings::{decode_hex, encode_hex};

pub(super) fn decrypt_value(
    value: &mut Value,
    object_num: i64,
    generation: i64,
    signature_contents: bool,
    options: &SecurityOptions,
    crypto: &dyn CryptoBackend,
) -> Result<(), String> {
    match value {
        Value::String(text) if !signature_contents => {
            let encrypted: Vec<u8> = text.chars().map(|c| c as u8).collect();
            let plain = decrypt_bytes(
                &options.file_key,
                object_num,
                generation,
                options.string_method,
                &encrypted,
                crypto,
            )?;
            *text = plain.into_iter().map(char::from).collect();
        }
        Value::HexString(hex) if !signature_contents => {
            let encrypted = decode_hex(hex)?;
            let plain = decrypt_bytes(
                &options.file_key,
                object_num,
                generation,
                options.string_method,
                &encrypted,
                crypto,
            )?;
            *hex = encode_hex(&plain);
        }
        Value::Array(array) => {
            for item in &mut array.0 {
                decrypt_value(item, object_num, generation, false, options, crypto)?;
            }
        }
        Value::Dict(dict) => {
            let is_signature = dict.get("Type").and_then(Value::as_name) == Some("Sig")
                || dict.get("FT").and_then(Value::as_name) == Some("Sig");
            for (key, item) in &mut dict.0 {
                decrypt_value(
                    item,
                    object_num,
                    generation,
                    is_signature && key == "Contents",
                    options,
                    crypto,
                )?;
            }
        }
        Value::Stream(dict, data) => {
            let is_signature = dict.get("Type").and_then(Value::as_name) == Some("Sig")
                || dict.get("FT").and_then(Value::as_name) == Some("Sig");
            for (key, item) in &mut dict.0 {
                decrypt_value(
                    item,
                    object_num,
                    generation,
                    is_signature && key == "Contents",
                    options,
                    crypto,
                )?;
            }
            if dict.get("Type").and_then(Value::as_name) != Some("XRef")
                && (options.encrypt_metadata
                    || dict.get("Type").and_then(Value::as_name) != Some("Metadata"))
                && let StreamData::Ref { offset, len } = data
            {
                *data = StreamData::EncryptedRef {
                    offset: *offset,
                    len: *len,
                    object_num,
                    generation,
                };
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn decrypt_bytes(
    file_key: &[u8],
    object_num: i64,
    generation: i64,
    method: CryptMethod,
    encrypted: &[u8],
    crypto: &dyn CryptoBackend,
) -> Result<Vec<u8>, String> {
    match method {
        CryptMethod::Identity => Ok(encrypted.to_vec()),
        CryptMethod::Rc4 { .. } | CryptMethod::Aes128 => {
            let object_key = Zeroizing::new(object_encryption_key(
                file_key, object_num, generation, method, crypto,
            )?);
            match method {
                CryptMethod::Rc4 { .. } => crypto.rc4(&object_key, encrypted),
                CryptMethod::Aes128 => {
                    if encrypted.len() < 32 || (encrypted.len() - 16) % 16 != 0 {
                        return Err("invalid AES-encrypted PDF string or stream".to_string());
                    }
                    let mut plain = Zeroizing::new(crypto.aes128_cbc_decrypt(
                        &object_key,
                        &encrypted[..16],
                        &encrypted[16..],
                    )?);
                    let padding = *plain.last().ok_or("empty AES plaintext")? as usize;
                    if padding == 0 || padding > 16 || plain.len() < padding {
                        return Err("invalid AES-encrypted PDF padding".to_string());
                    }
                    if !plain[plain.len() - padding..]
                        .iter()
                        .all(|byte| *byte as usize == padding)
                    {
                        return Err("invalid AES-encrypted PDF padding".to_string());
                    }
                    let unpadded_len = plain.len() - padding;
                    plain.truncate(unpadded_len);
                    Ok(std::mem::take(&mut *plain))
                }
                CryptMethod::Identity => unreachable!(),
            }
        }
    }
}

/// Algorithm 1 b–d（7.6.2）——按对象把文件密钥扩展成对象密钥。
///
/// 写端 `encrypt::encrypt_bytes` 与读端 `decrypt_bytes` 共用：7.6.2
/// 「Because the RC4 algorithm and AES algorithms are symmetric, this same
/// sequence of steps shall be used both to encrypt and to decrypt data」。
/// AES 路径额外拼上 `"sAlT"`（Algorithm 1 b 括注），密钥取
/// 「(n + 5) 字节，至多 16」（Algorithm 1 d）。
pub(super) fn object_encryption_key(
    file_key: &[u8],
    object_num: i64,
    generation: i64,
    method: CryptMethod,
    crypto: &dyn CryptoBackend,
) -> Result<Vec<u8>, String> {
    let mut input = Zeroizing::new(Vec::with_capacity(file_key.len() + 9));
    input.extend_from_slice(file_key);
    let object =
        u32::try_from(object_num).map_err(|_| "invalid encrypted object number".to_string())?;
    let generation =
        u16::try_from(generation).map_err(|_| "invalid encrypted object generation".to_string())?;
    input.extend_from_slice(&object.to_le_bytes()[..3]);
    input.extend_from_slice(&generation.to_le_bytes());
    if method == CryptMethod::Aes128 {
        input.extend_from_slice(b"sAlT");
    }
    let digest = crypto.md5(&input);
    let file_key_len = match method {
        CryptMethod::Rc4 { key_len } => key_len,
        CryptMethod::Aes128 => file_key.len(),
        CryptMethod::Identity => unreachable!(),
    };
    let key_len = (file_key_len + 5).min(16);
    Ok(digest[..key_len].to_vec())
}
