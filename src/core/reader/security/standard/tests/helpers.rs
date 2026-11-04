//! 标准安全处理器测试的共享入口：把「加密字典全为直接对象」这一常见前提
//! 收在一处，避免每个用例都重复写解析器参数。

use super::*;

/// 测试入口：加密字典**全为直接对象**时的认证。
///
/// 这些用例不涉及 7.3.10 的间接写法，解析器给空表即可（空表下不会出现引用）；
/// 需要间接形态的用例直接调 `SecurityHandler::authenticate` 并自备对象表。
pub(super) fn authenticate_without_references(
    dict: &Dict,
    id: &Value,
    password: &[u8],
    crypto: &dyn CryptoBackend,
) -> Result<SecurityHandler, String> {
    SecurityHandler::authenticate(dict, id, password, None, crypto, &indirect_resolver(&[]))
}
