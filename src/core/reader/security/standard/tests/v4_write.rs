//! 写端 `/V 4`（R 4）输出单元测试（ISO 32000-1 7.6.3.3/7.6.3.4、7.6.5、
//! Table 20/21/25/28）：加密字典的 `/CF` `/StmF` `/StrF` 形态、Algorithm 1
//! 的 AES 正向变换（含 7.6.2 填充示例）与 catalog `/Version` 声明。

use crate::core::reader::security::crypto::RustCryptoBackend;
use crate::core::types::{Dict, Document, Object, Value};

use super::StandardCryptFilter;
use super::StandardEncryptionRequest;
use super::decrypt::decrypt_bytes;
use super::encrypt::encryption_dictionary;
use super::filters::CryptMethod;

/// Algorithm 1 + 7.6.2 的 AES 正向路径：密文 = 16 字节 IV ‖ CBC(填充块)；
/// 9 字节明文按「pad shall consist of 16 - (M mod 16) bytes whose value
/// shall also be 16 - (M mod 16)」补 7 个 0x07（7.6.2 EXAMPLE），且能被
/// 读端 `decrypt_bytes` 原样还原。
#[test]
fn v4_encrypt_bytes_round_trips_through_the_decrypt_side() {
    let file_key = [0x11u8; 16];
    let plain = b"123456789";
    let encrypted = super::encrypt::encrypt_bytes(
        &file_key,
        3,
        0,
        CryptMethod::Aes128,
        plain,
        &RustCryptoBackend,
    )
    .expect("AESV2 正向加密应成功");
    assert_eq!(encrypted.len(), 32, "IV(16) + 一个填充块(16)");
    assert_eq!(
        decrypt_bytes(
            &file_key,
            3,
            0,
            CryptMethod::Aes128,
            &encrypted,
            &RustCryptoBackend
        )
        .expect("读端应能解密自家写端产物"),
        plain
    );
    // 16 字节整块明文按 7.6.2 NOTE 补一整块 0x10。
    let block = super::encrypt::encrypt_bytes(
        &file_key,
        3,
        0,
        CryptMethod::Aes128,
        &[0x42u8; 16],
        &RustCryptoBackend,
    )
    .expect("整块明文同样可加密");
    assert_eq!(block.len(), 48, "IV(16) + 两个块（明文 + 0x10 填充块）");
}

/// Table 20/Table 25：`/V 4` 字典须带 `CF`（键 `StdCF`，`CFM`、`Length`
/// 按标准处理器口径的字节数、`AuthEvent /DocOpen`）与 `StmF`/`StrF` 选择器；
/// `/Length`（位）与筛选器 `Length`（字节）保持一致（7.6.3.1 限定
/// AESV2 亦只有 `StdCF` 一名）。
#[test]
fn v4_encryption_dictionary_carries_the_crypt_filter_and_selectors() {
    for (filter, cfm) in [
        (StandardCryptFilter::AesV2, "AESV2"),
        (StandardCryptFilter::Rc4V2, "V2"),
    ] {
        let dict = encryption_dictionary(4, 4, 128, -4, &[0xAA; 32], &[0xBB; 32], Some(filter));
        assert_eq!(dict.get("V"), Some(&Value::Integer(4)));
        assert_eq!(dict.get("R"), Some(&Value::Integer(4)));
        assert_eq!(dict.get("Length"), Some(&Value::Integer(128)));
        assert_eq!(dict.get("StmF"), Some(&Value::Name("StdCF".into())));
        assert_eq!(dict.get("StrF"), Some(&Value::Name("StdCF".into())));
        let std_cf = dict
            .get("CF")
            .and_then(Value::as_dict)
            .and_then(|cf| cf.get("StdCF"))
            .and_then(Value::as_dict)
            .expect("/CF 应含 StdCF 筛选器字典");
        assert_eq!(std_cf.get("CFM"), Some(&Value::Name(cfm.into())));
        // Table 25：标准处理器把 Length 表达为 8 的倍数（16 即 128 位）。
        assert_eq!(std_cf.get("Length"), Some(&Value::Integer(16)));
        assert_eq!(
            std_cf.get("AuthEvent"),
            Some(&Value::Name("DocOpen".into()))
        );
        assert_eq!(std_cf.get("Type"), Some(&Value::Name("CryptFilter".into())));
    }
}

/// Table 20/Table 21：`/V ≤ 2` 的输出不带任何 `CF`/`StmF`/`StrF` 条目，
/// 顶层 `/Length` 仍只在 `/V 2` 写出（V≤3 行为零变化的回归护栏）。
#[test]
fn v2_and_v1_encryption_dictionaries_stay_filter_free() {
    let v1 = encryption_dictionary(1, 2, 40, -4, &[0; 32], &[0; 32], None);
    assert!(v1.get("Length").is_none());
    assert!(v1.get("CF").is_none());
    let v2 = encryption_dictionary(2, 3, 128, -4, &[0; 32], &[0; 32], None);
    assert_eq!(v2.get("Length"), Some(&Value::Integer(128)));
    assert!(v2.get("CF").is_none() && v2.get("StmF").is_none() && v2.get("StrF").is_none());
}

/// 最小可加密文档：一个 catalog 对象 + trailer `/Root`。
fn minimal_document() -> Document {
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
    doc
}

/// 7.5.2 + Table 28：`/V 4` 输出在 catalog 写 `/Version` **名字**条目——
/// `CFM /AESV2` 声明 `/1.6`（Table 25 标 PDF 1.6），`CFM /V2` 声明 `/1.5`
/// （Table 20 的 `/V` `/CF` `/StmF` `/StrF` 标 PDF 1.5）。
#[test]
fn v4_encrypt_declares_the_catalog_version_override() {
    for (filter, declared) in [
        (StandardCryptFilter::AesV2, "1.6"),
        (StandardCryptFilter::Rc4V2, "1.5"),
    ] {
        let mut doc = minimal_document();
        doc.encrypt_with_standard_handler(StandardEncryptionRequest {
            user_password: b"user",
            owner_password: Some(b"owner"),
            permissions: -4,
            handler_version: 4,
            key_bits: 128,
            crypt_filter: Some(filter),
            file_id: b"id-bytes",
        })
        .expect("最小文档应能走 /V 4 加密");
        let catalog = doc
            .objects_for_write()
            .into_iter()
            .find(|object| object.num == 1)
            .map(|object| object.value)
            .expect("catalog 对象应仍在文档中");
        let Value::Dict(catalog) = catalog else {
            panic!("catalog 应仍是字典对象");
        };
        assert_eq!(
            catalog.get("Version"),
            Some(&Value::Name(declared.into())),
            "Table 28：/Version 必须是名字对象"
        );
    }
}

/// Table 25 + 读端 `filters::crypt_method`：AESV2 需要 16 字节文件密钥；
/// `/V 4` 下 `key_bits < 128` 的 AESV2 必须在改动任何对象前被拒绝。
#[test]
fn v4_aesv2_rejects_a_short_file_key_without_touching_objects() {
    let mut doc = minimal_document();
    let before = doc.objects_for_write();
    let error = doc
        .encrypt_with_standard_handler(StandardEncryptionRequest {
            user_password: b"user",
            owner_password: None,
            permissions: -4,
            handler_version: 4,
            key_bits: 64,
            crypt_filter: Some(StandardCryptFilter::AesV2),
            file_id: b"id-bytes",
        })
        .expect_err("AESV2 配 64 位密钥应被拒");
    assert!(error.contains("128-bit"), "报错应指向密钥长度：{error}");
    assert_eq!(
        doc.objects_for_write().len(),
        before.len(),
        "拒绝不得改动对象"
    );
}

/// 7.6.5：「the value of the V entry shall be 4 to use crypt filters」——
/// `/V ≤ 2` 与 `crypt_filter` 参数的组合在入口即被拒绝。
#[test]
fn crypt_filters_are_only_accepted_with_handler_version_four() {
    let mut doc = minimal_document();
    let error = doc
        .encrypt_with_standard_handler(StandardEncryptionRequest {
            user_password: b"user",
            owner_password: None,
            permissions: -4,
            handler_version: 2,
            key_bits: 128,
            crypt_filter: Some(StandardCryptFilter::Rc4V2),
            file_id: b"id-bytes",
        })
        .expect_err("/V 2 不应接受 crypt filter");
    assert!(
        error.contains("/V 4"),
        "报错应引用 7.6.5 的 /V 4 约束：{error}"
    );
}
