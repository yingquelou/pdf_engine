//! 读端安全（ISO 32000-1 7.6）：标准安全处理器的认证与解密，以及
//! 加密字典 `/P` 权限位的读端解释。
//!
//! 目录模块：
//! - `crypto`：可注入的 MD5/RC4/AES-128-CBC 后端；
//! - `standard`：`Filter /Standard` 认证、Algorithm 2 文件密钥派生、
//!   字符串/流解密（含 7.6.5 crypt filter）；
//! - `permissions`：`/P` 的类型化用户访问权限视图与
//!   [`crate::core::types::Document::permissions`] 访问器。
//!
//! 仅 `Permissions` 对外可见；处理器本身是实现细节（`pub(super)`）。

mod crypto;
mod permissions;
mod standard;

pub use permissions::Permissions;

pub(crate) use crypto::{CryptoBackend, RustCryptoBackend};
pub(crate) use standard::pad_password;
pub(super) use standard::SecurityHandler;
pub(crate) use standard::{StandardCryptFilter, StandardEncryptionRequest};
