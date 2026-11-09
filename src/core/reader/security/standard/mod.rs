//! `Filter /Standard` 标准安全处理器（ISO 32000-1 7.6.3.2）：认证、
//! 文件密钥派生与字符串/流解密。
//!
//! 目录模块：
//! - `auth`：`/Encrypt` 字典解析与用户/所有者密码认证流程；
//! - `key`：Algorithm 2 文件密钥派生与密码填充/校验；
//! - `filters`：7.6.5 crypt filter（`/CF` `/StrF` `/StmF` `/EFF` 与
//!   流上显式 `/Crypt`）到 [`CryptMethod`] 的选择；
//! - `decrypt`：对象值/字节串的按对象密钥解密；
//! - `encrypt`：**写端**加密（Algorithm 1/3/4/5 的正向路径、Table 20/Table 21
//!   加密字典构造与 `Document::encrypt_with_standard_handler`）；
//! - `strings`：加密字典 PDF 字面串与十六进制串的编解码辅助。
//!
//! 本文件只保留入口类型 [`SecurityHandler`]、聚合状态
//! [`SecurityOptions`] 与 `ObjectSecurity` 分派。

use crate::core::filters::ObjectResolver;
use crate::core::types::ObjectSecurity;
use crate::core::types::{Dict, Object, Value};
use zeroize::Zeroizing;

use super::crypto::{CryptoBackend, RustCryptoBackend};
use decrypt::{decrypt_bytes, decrypt_value};
use filters::CryptMethod;
use filters::explicit_crypt_filter;

mod auth;
mod decrypt;
mod encrypt;
mod filters;
mod key;
mod strings;
#[cfg(test)]
mod tests;

pub(crate) use encrypt::{StandardCryptFilter, StandardEncryptionRequest};
pub(crate) use key::pad_password;

struct SecurityOptions {
    pub revision: u8,
    pub version: u8,
    /// 加密字典 `/P` 的有符号 32 位原值：既用于文件密钥派生
    /// （Algorithm 2 d），也是读端权限视图的唯一来源（7.6.3.2、Table 22）。
    pub permissions: i32,
    pub file_key: Zeroizing<Vec<u8>>,
    pub string_method: CryptMethod,
    pub stream_method: CryptMethod,
    pub embedded_file_method: CryptMethod,
    pub encrypt_metadata: bool,
    pub encrypt_dict_ref: Option<(i64, i64)>,
    /// 加密字典 `/CF` 的**原值**（字典或指向字典的间接引用，7.3.10），
    /// 由 `filters::named_crypt_filter` 按需解引用。
    crypt_filters: Option<crate::core::types::Value>,
}

pub(crate) struct SecurityHandler {
    options: SecurityOptions,
}

impl std::fmt::Debug for SecurityHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityHandler")
            .field("revision", &self.options.revision)
            .field("version", &self.options.version)
            .field("permissions", &self.options.permissions)
            .field("file_key", &"[REDACTED]")
            .finish()
    }
}

impl SecurityHandler {
    pub(crate) fn authenticate(
        encrypt: &Dict,
        file_id: &Value,
        password: &[u8],
        encrypt_dict_ref: Option<(i64, i64)>,
        crypto: &dyn CryptoBackend,
        resolver: &crate::core::filters::ObjectResolver<'_>,
    ) -> Result<Self, String> {
        let options = SecurityOptions::from_dict(encrypt, file_id, password, crypto, resolver)?;
        Ok(Self {
            options: SecurityOptions {
                encrypt_dict_ref,
                ..options
            },
        })
    }

    #[cfg(test)]
    fn file_key(&self) -> &[u8] {
        &self.options.file_key
    }
}

impl ObjectSecurity for SecurityHandler {
    fn decrypt_object(&self, object: &mut Object) -> Result<(), String> {
        if self.options.encrypt_dict_ref == Some((object.num, object.generation)) {
            return Ok(());
        }
        decrypt_value(
            &mut object.value,
            object.num,
            object.generation,
            false,
            &self.options,
            &RustCryptoBackend,
        )?;
        Ok(())
    }

    /// `resolver` 由调用方（`Document`，持有 `File` 与 `/Encrypt` trailer）提供：
    /// 显式 `/Crypt` 的筛选器字典允许写成间接引用
    /// （`/Filter [ /Crypt 7 0 R ]`、`/DecodeParms [ << /Name /StdCF >> 8 0 R ]`、
    /// `/CF << /StdCF 9 0 R >>`），解不到就明确报错，不静默退回默认 stream filter。
    fn decrypt_stream(
        &self,
        object_num: i64,
        generation: i64,
        dict: &Dict,
        bytes: &[u8],
        resolver: &ObjectResolver<'_>,
    ) -> Result<Vec<u8>, String> {
        if dict.get("Type").and_then(Value::as_name) == Some("XRef")
            || (!self.options.encrypt_metadata
                && dict.get("Type").and_then(Value::as_name) == Some("Metadata"))
        {
            return Ok(bytes.to_vec());
        }

        let method = explicit_crypt_filter(dict, resolver, &self.options)?.unwrap_or_else(|| {
            match dict.get("Type").and_then(Value::as_name) {
                Some("EmbeddedFile") => self.options.embedded_file_method,
                _ => self.options.stream_method,
            }
        });
        decrypt_bytes(
            &self.options.file_key,
            object_num,
            generation,
            method,
            bytes,
            &RustCryptoBackend,
        )
    }

    /// 认证成功即可公开 `/P` 原值与 revision：Table 21 把 `/P` 与 `/R`
    /// 都列为 Required，而本模块的认证已要求二者是 `i32`/`u8` 范围内的
    /// 整数，故此处恒为 `Some`（读端解码见 `permissions.rs`）。
    fn permission_entry(&self) -> Option<(i32, u8)> {
        Some((self.options.permissions, self.options.revision))
    }
}
