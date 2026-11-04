//! Algorithm 3 的所有者密钥派生（ISO 32000-1 7.6.3.4）：50 次 MD5 续哈希
//! **不截断**，密钥长度只在步 (d) 生效。
//!
//! 规范依据（`pdftotext -raw docs/PDF32000_2008.pdf` 实测行号）：
//! - 步 (c)：raw L3092–L3093「(Security handlers of revision 3 or greater) Do
//!   the following 50 times: Take the output from the previous MD5 hash and
//!   pass **it** as input into a new MD5 hash.」
//! - 步 (d)：raw L3096–L3098「Create an RC4 encryption key using the **first n
//!   bytes** of the output from the final MD5 hash…」
//! - 对照 Algorithm 2 步 (h)：raw L3072–L3074 明确要求「pass the first n bytes
//!   of the output as input into a new MD5 hash」——两者只差这一处截断。
//! - `/Length` 缺省 40 位：Table 20，raw L2804–L2805。
//!
//! 期望值用 Python `hashlib`＋手写 RC4 按上述条文**独立**算出（规范未给 `/O`
//! 十六进制算例），不取自本仓库实现。

use super::*;

/// 整摘要续哈希（步 (c)）在 `key_len == 16` 时与截断变体同值，故 KAT 以 40 位
/// 密钥（raw L2805 的缺省 `/Length`）为准。
const OWNER_KEY_40_BIT: [u8; 5] = [0xb4, 0xa8, 0x93, 0x27, 0xb4];
/// 同一续哈希链的完整 16 字节输出（步 (c) 与 `/Length` 无关）。
const OWNER_KEY_128_BIT: [u8; 16] = [
    0xb4, 0xa8, 0x93, 0x27, 0xb4, 0xf6, 0xf8, 0xf0, 0x0a, 0x0b, 0x6e, 0x21, 0xe6, 0x33, 0x77, 0x8e,
];
/// 截断续哈希（把 Algorithm 2 步 (h) 误用于步 (c)）在 40 位密钥下的结果，仅作
/// 反向对照。
const TRUNCATED_CHAIN_40_BIT: [u8; 5] = [0xa6, 0x5c, 0xf0, 0x02, 0xe2];

/// Algorithm 3 全链（b–h）在 `/V 2 /R 3 /Length 40`、用户密码 `user`、所有者
/// 密码 `owner`、`/ID` 首元素 `id-bytes`、`/P -4` 下的三件套。
const CONFORMING_O: &str = "3C482162008FAFCB228B7DB3C43A1090BC5B56E9B1556E89FC0656FD291F4908";
const CONFORMING_FILE_KEY: [u8; 5] = [0xdf, 0xa0, 0xe9, 0x17, 0x14];
/// Algorithm 5 a–e 的 16 字节 RC4 输出。步 f（raw L3130–L3131）只要求
/// 「Append 16 bytes of **arbitrary** padding」，故后 16 字节不参与比对。
const CONFORMING_U_PREFIX: &str = "6EDA5AEA04D9F31F07A5932B51272B53";
/// 读端字典用零填充凑满 32 字节；`validate_user_password` 只比前 16 字节。
const CONFORMING_U: &str = "6EDA5AEA04D9F31F07A5932B51272B53\
    00000000000000000000000000000000";

/// 反向转录：本缺陷形态（40 位密钥下与正确值不同）曾经存在于实现里。
fn truncated_chain_owner_key(
    padded: &[u8],
    revision: u8,
    key_len: usize,
    crypto: &dyn CryptoBackend,
) -> Vec<u8> {
    let mut digest = crypto.md5(padded);
    if revision >= 3 {
        for _ in 0..50 {
            digest = crypto.md5(&digest[..key_len]);
        }
    }
    digest[..key_len].to_vec()
}

fn conforming_encryption_dictionary() -> (Dict, Value) {
    let id = Value::Array(crate::core::types::Array(vec![
        Value::HexString(hex(b"id-bytes")),
        Value::HexString(hex(b"id-bytes")),
    ]));
    let dict = Dict::new()
        .insert("Filter", Value::Name("Standard".into()))
        .insert("V", Value::Integer(2))
        .insert("R", Value::Integer(3))
        .insert("Length", Value::Integer(40))
        .insert("O", Value::HexString(CONFORMING_O.into()))
        .insert("U", Value::HexString(CONFORMING_U.into()))
        .insert("P", Value::Integer(-4));
    (dict, id)
}

#[test]
fn owner_key_md5_chain_hashes_the_whole_digest_per_algorithm_3_step_c() {
    let crypto = RustCryptoBackend;
    let padded = pad_password(b"owner");
    for revision in [3u8, 4] {
        assert_eq!(
            &*derive_owner_key(&padded, revision, 5, &crypto),
            &OWNER_KEY_40_BIT,
            "revision {revision} 的步 (c) 续哈希整个摘要（raw L3092-L3093）"
        );
    }
}

/// 步 (c) 通篇不提密钥长度，截断只发生在步 (d)（raw L3096–L3098）：故同一
/// 密码与 revision 下，`/Length` 只能让结果成为同一串的前缀。
#[test]
fn owner_key_chain_is_independent_of_the_declared_key_length() {
    let crypto = RustCryptoBackend;
    let padded = pad_password(b"owner");
    for key_len in [5usize, 8, 12, 16] {
        assert_eq!(
            &*derive_owner_key(&padded, 3, key_len, &crypto),
            &OWNER_KEY_128_BIT[..key_len],
            "/Length {key_len} 字节应是 128 位结果的同一前缀"
        );
    }
}

/// 缺陷的可观测边界：`key_len == 16` 时两种链同值（16 字节摘要切片是恒等），
/// 截断只在密钥短于 16 字节时显形——正是缺省 40 位（Table 20 raw L2805）的形态。
#[test]
fn owner_key_truncated_chain_defect_diverges_only_below_a_16_byte_key() {
    let crypto = RustCryptoBackend;
    let padded = pad_password(b"owner");
    assert_eq!(
        truncated_chain_owner_key(&padded, 3, 16, &crypto),
        &OWNER_KEY_128_BIT[..],
        "128 位密钥下两种链无法区分，故旧用例覆盖不到本缺陷"
    );
    assert_eq!(
        truncated_chain_owner_key(&padded, 3, 5, &crypto),
        TRUNCATED_CHAIN_40_BIT,
        "截断链自身的独立算例"
    );
    for key_len in [5usize, 8, 12] {
        let defective = truncated_chain_owner_key(&padded, 3, key_len, &crypto);
        let correct = derive_owner_key(&padded, 3, key_len, &crypto);
        assert_ne!(
            &*correct, &defective,
            "/Length {key_len} 字节时截断链给出另一个密钥"
        );
        assert_eq!(&*correct, &OWNER_KEY_128_BIT[..key_len]);
    }
}

/// 步 (c) 只适用于 R ≥ 3；R 2 直接用步 (b) 的摘要取前 5 字节（步 (d) 的
/// 「n shall always be 5 for security handlers of revision 2」，raw L3096–L3097）。
#[test]
fn owner_key_skips_the_md5_chain_below_revision_three() {
    let crypto = RustCryptoBackend;
    let padded = pad_password(b"owner");
    for revision in [1u8, 2] {
        assert_eq!(
            &*derive_owner_key(&padded, revision, 5, &crypto),
            &RustCryptoBackend.md5(&padded)[..5],
            "revision {revision} 不做续哈希"
        );
    }
    assert_eq!(
        &*derive_owner_key(&padded, 2, 5, &crypto),
        &[0xc6, 0xd3, 0x0a, 0xb0, 0x20],
        "R 2 的 40 位所有者密钥独立算例"
    );
}

/// 写端产物合规：`/V 2 /R 3 /Length 40` 文档写出的 `/O` 逐字节等于独立算出的
/// Algorithm 3 结果，`/U` 的前 16 字节等于 Algorithm 5 的 RC4 输出。
#[test]
fn written_encryption_dictionary_matches_the_algorithm_3_known_answer() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
    ));
    doc.trailer
        .dict
        .0
        .insert("Root".to_string(), Value::Ref(1, 0));
    doc.encrypt_with_standard_handler(StandardEncryptionRequest {
        user_password: b"user",
        owner_password: Some(b"owner"),
        permissions: -4,
        handler_version: 2,
        key_bits: 40,
        crypt_filter: None,
        file_id: b"id-bytes",
    })
    .expect("最小文档应能走 /V 2 /R 3 /Length 40 加密");
    let encrypt = doc
        .objects_for_write()
        .into_iter()
        .find_map(|object| match object.value {
            Value::Dict(dict)
                if dict.get("Filter").and_then(Value::as_name) == Some("Standard") =>
            {
                Some(dict)
            }
            _ => None,
        })
        .expect("加密字典应作为间接对象写出");
    let hex_of = |entry: &str| match encrypt.get(entry) {
        Some(Value::HexString(value)) => value.clone(),
        other => panic!("{entry} 应是十六进制串，实际为 {other:?}"),
    };
    assert_eq!(hex_of("O"), CONFORMING_O, "Algorithm 3 b–h 的 /O");
    let user_entry = hex_of("U");
    assert_eq!(
        user_entry.len(),
        64,
        "Algorithm 5 f：/U 须是 32 字节（十六进制 64 位）"
    );
    assert_eq!(
        &user_entry[..CONFORMING_U_PREFIX.len()],
        CONFORMING_U_PREFIX,
        "Algorithm 5 a–e 的 16 字节输出；后 16 字节是任意味填充（raw L3130-L3131）"
    );
    assert_eq!(
        encrypt.get("Length"),
        Some(&Value::Integer(40)),
        "Table 20 raw L2804-L2805：/Length 以位计"
    );
}

/// 端到端回归：以**第三方**（此处为独立算出的）合规 `/O` 打开文档时，所有者密码
/// 必须恢复出正确文件密钥。修复前读端按截断链算出 `a65cf002e2`，解出的用户密码
/// 是乱码，认证失败——本用例因此把实现钉死在规范一侧。
#[test]
fn owner_password_authenticates_a_conforming_40_bit_revision_three_document() {
    let crypto = RustCryptoBackend;
    let (dict, id) = conforming_encryption_dictionary();
    let handler = authenticate_without_references(&dict, &id, b"owner", &crypto)
        .expect("所有者密码应通过认证");
    assert_eq!(handler.file_key(), &CONFORMING_FILE_KEY[..]);
    let by_user =
        authenticate_without_references(&dict, &id, b"user", &crypto).expect("用户密码同样应通过");
    assert_eq!(by_user.file_key(), &CONFORMING_FILE_KEY[..]);
    assert!(
        authenticate_without_references(&dict, &id, b"wrong", &crypto).is_err(),
        "错误密码不得通过"
    );
}
