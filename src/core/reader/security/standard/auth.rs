//! `/Encrypt` 字典解析与认证流程（ISO 32000-1 7.6.3.2）：校验
//! `Filter /Standard`、版本/revision 组合、`/Length`、`/EncryptMetadata`，
//! 先按用户密码走 Algorithm 2，失败再用所有者密码恢复后重试。

use crate::core::reader::security::crypto::CryptoBackend;
use crate::core::types::{Dict, Value};
use zeroize::Zeroizing;

use super::filters::crypt_filter;
use super::key::{
    FileKeyParams, derive_file_key, derive_owner_key, pad_password, recover_user_password,
    validate_user_password,
};
use super::strings::pdf_string;
use super::{CryptMethod, SecurityOptions};

impl SecurityOptions {
    pub(super) fn from_dict(
        encrypt: &Dict,
        file_id: &Value,
        password: &[u8],
        crypto: &dyn CryptoBackend,
    ) -> Result<Self, String> {
        if encrypt.get("Filter").and_then(Value::as_name) != Some("Standard") {
            return Err("unsupported PDF security handler".to_string());
        }

        let version = required_integer(encrypt, "V")?;
        let revision = required_integer(encrypt, "R")?;
        let version = u8::try_from(version).map_err(|_| "invalid encryption /V".to_string())?;
        let revision = u8::try_from(revision).map_err(|_| "invalid encryption /R".to_string())?;
        if !matches!((version, revision), (1, 2) | (2, 3) | (4, 4)) {
            return Err(format!(
                "unsupported Standard Security Handler version/revision ({version}/{revision})"
            ));
        }

        // `/P`（Table 21 列为 Required）参与文件密钥派生，缺失/非整数/超出
        // 有符号 32 位范围都无法得到正确密钥，故按本模块既有错误模型报错，
        // 不做「宽进」降级（读端权限视图见 `permissions.rs`）。
        let permissions = required_integer(encrypt, "P")?;
        let permissions =
            i32::try_from(permissions).map_err(|_| "invalid encryption permissions".to_string())?;
        let owner = pdf_string(encrypt.get("O").ok_or("missing encryption /O")?)?;
        let user = pdf_string(encrypt.get("U").ok_or("missing encryption /U")?)?;
        if owner.len() != 32 || user.len() != 32 {
            return Err("invalid encryption /O or /U string length".to_string());
        }
        let id_array = file_id
            .as_array()
            .ok_or_else(|| "missing trailer /ID array".to_string())?;
        let id = id_array
            .iter()
            .next()
            .ok_or_else(|| "empty trailer /ID array".to_string())
            .and_then(pdf_string)?;

        let key_len = match revision {
            2 => {
                if let Some(value) = encrypt.get("Length")
                    && value.as_integer() != Some(40)
                {
                    return Err("revision 2 requires a 40-bit encryption key".to_string());
                }
                5
            }
            3 | 4 => {
                let bits = match encrypt.get("Length") {
                    Some(Value::Integer(bits)) => *bits,
                    None => 40,
                    Some(_) => return Err("invalid encryption /Length".to_string()),
                };
                if !(40..=128).contains(&bits) || bits % 8 != 0 {
                    return Err("invalid encryption key length".to_string());
                }
                usize::try_from(bits / 8)
                    .map_err(|_| "invalid encryption key length".to_string())?
            }
            _ => unreachable!(),
        };

        let encrypt_metadata = match encrypt.get("EncryptMetadata") {
            Some(Value::Bool(value)) if revision == 4 => *value,
            Some(Value::Bool(false)) => {
                return Err("/EncryptMetadata is only valid for revision 4".to_string());
            }
            Some(Value::Bool(true)) | None => true,
            Some(_) => return Err("invalid encryption /EncryptMetadata".to_string()),
        };
        let padded_password = pad_password(password);
        let file_key = derive_file_key(
            FileKeyParams {
                padded_password: &padded_password,
                owner_entry: &owner,
                permissions,
                id: &id,
                revision,
                encrypt_metadata,
                key_len,
            },
            crypto,
        );

        let user_valid = validate_user_password(&file_key, &user, &id, revision, crypto)?;
        if !user_valid {
            let owner_key = derive_owner_key(&padded_password, revision, key_len, crypto);
            let recovered = recover_user_password(&owner_key, &owner, revision, crypto)?;
            let owner_file_key = derive_file_key(
                FileKeyParams {
                    padded_password: &recovered,
                    owner_entry: &owner,
                    permissions,
                    id: &id,
                    revision,
                    encrypt_metadata,
                    key_len,
                },
                crypto,
            );
            if !validate_user_password(&owner_file_key, &user, &id, revision, crypto)? {
                return Err("invalid password for encrypted PDF".to_string());
            }
            return Self::with_filters(
                encrypt,
                version,
                revision,
                permissions,
                owner_file_key,
                encrypt_metadata,
                crypto,
            );
        }

        Self::with_filters(
            encrypt,
            version,
            revision,
            permissions,
            file_key,
            encrypt_metadata,
            crypto,
        )
    }

    fn with_filters(
        encrypt: &Dict,
        version: u8,
        revision: u8,
        permissions: i32,
        file_key: Zeroizing<Vec<u8>>,
        encrypt_metadata: bool,
        _crypto: &dyn CryptoBackend,
    ) -> Result<Self, String> {
        let default_method = if version < 4 {
            CryptMethod::Rc4 {
                key_len: file_key.len(),
            }
        } else {
            CryptMethod::Identity
        };
        let file_key_len = file_key.len();
        let string_method = if version < 4 {
            default_method
        } else {
            crypt_filter(encrypt, "StrF", default_method, file_key_len)?
        };
        let stream_method = if version < 4 {
            default_method
        } else {
            crypt_filter(encrypt, "StmF", default_method, file_key_len)?
        };
        let embedded_file_method = if version < 4 {
            default_method
        } else {
            crypt_filter(encrypt, "EFF", stream_method, file_key_len)?
        };
        if [string_method, stream_method, embedded_file_method].contains(&CryptMethod::Aes128)
            && file_key_len != 16
        {
            return Err("AESV2 requires a 128-bit file encryption key".to_string());
        }

        Ok(Self {
            revision,
            version,
            permissions,
            file_key,
            string_method,
            stream_method,
            embedded_file_method,
            encrypt_metadata,
            encrypt_dict_ref: None,
            crypt_filters: encrypt.get("CF").and_then(Value::as_dict).cloned(),
        })
    }
}

fn required_integer(dict: &Dict, key: &str) -> Result<i64, String> {
    dict.get(key)
        .and_then(Value::as_integer)
        .ok_or_else(|| format!("missing or invalid encryption /{key}"))
}
