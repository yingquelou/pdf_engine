//! 标准安全处理器单元测试（ISO 32000-1 7.6.3.2/7.6.5）：认证、密钥派生、crypt filter 与 `/P` 权限位解读。
use super::filters::*;
use super::key::*;
use super::*;
use crate::core::reader::security::Permissions;
use crate::core::reader::security::crypto::RustCryptoBackend;
use crate::core::types::Document;
use aes::Aes128;
use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct CountingBackend {
    calls: AtomicUsize,
}

impl CryptoBackend for CountingBackend {
    fn md5(&self, input: &[u8]) -> [u8; 16] {
        self.calls.fetch_add(1, Ordering::Relaxed);
        RustCryptoBackend.md5(input)
    }

    fn rc4(&self, key: &[u8], input: &[u8]) -> Result<Vec<u8>, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        RustCryptoBackend.rc4(key, input)
    }

    fn aes128_cbc_decrypt(&self, key: &[u8], iv: &[u8], input: &[u8]) -> Result<Vec<u8>, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        RustCryptoBackend.aes128_cbc_decrypt(key, iv, input)
    }

    fn aes128_cbc_encrypt(&self, key: &[u8], iv: &[u8], input: &[u8]) -> Result<Vec<u8>, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        RustCryptoBackend.aes128_cbc_encrypt(key, iv, input)
    }
}

#[test]
fn security_handler_uses_an_injectable_crypto_backend() {
    let reference = RustCryptoBackend;
    let (dict, id, expected_key) = revision_three_dictionary(&reference);
    let backend = CountingBackend::default();
    let handler = authenticate_without_references(&dict, &id, b"user", &backend).unwrap();
    assert_eq!(handler.file_key(), &*expected_key);
    assert!(backend.calls.load(Ordering::Relaxed) > 0);
}

#[test]
fn password_padding_truncates_at_32_bytes() {
    assert_eq!(pad_password(b"abc").len(), 32);
    assert_eq!(
        &pad_password(b"01234567890123456789012345678901")[..32],
        b"01234567890123456789012345678901"
    );
    assert_eq!(
        &pad_password(b"01234567890123456789012345678901extra")[..32],
        b"01234567890123456789012345678901"
    );
}

#[test]
fn crypt_filter_names_select_supported_methods() {
    let filters = Dict::new()
        .insert(
            "StdCF",
            Value::Dict(Dict::new().insert("CFM", Value::Name("AESV2".into()))),
        )
        .insert(
            "Rc4CF",
            Value::Dict(Dict::new().insert("CFM", Value::Name("V2".into()))),
        );
    let dict = Dict::new()
        .insert("CF", Value::Dict(filters))
        .insert("StrF", Value::Name("StdCF".into()))
        .insert("StmF", Value::Name("Rc4CF".into()));
    assert_eq!(
        crypt_filter(
            &dict,
            "StrF",
            CryptMethod::Identity,
            16,
            &indirect_resolver(&[])
        )
        .unwrap(),
        CryptMethod::Aes128
    );
    assert_eq!(
        crypt_filter(
            &dict,
            "StmF",
            CryptMethod::Identity,
            16,
            &indirect_resolver(&[])
        )
        .unwrap(),
        CryptMethod::Rc4 { key_len: 16 }
    );
}

#[test]
fn security_dictionary_authenticates_user_and_owner_passwords() {
    let crypto = RustCryptoBackend;
    let id = b"known file id";
    let user_password = pad_password(b"user");
    let owner_password = pad_password(b"owner");
    let key_len = 5;
    let owner_key = derive_owner_key(&owner_password, 2, key_len, &crypto);
    let owner_entry = crypto.rc4(&owner_key, &user_password).unwrap();
    let permissions = -4i32;
    let file_key = derive_file_key(
        FileKeyParams {
            padded_password: &user_password,
            owner_entry: &owner_entry,
            permissions,
            id,
            revision: 2,
            encrypt_metadata: true,
            key_len,
        },
        &crypto,
    );
    let user_entry = crypto.rc4(&file_key, &PASSWORD_PADDING).unwrap();
    let dict = Dict::new()
        .insert("Filter", Value::Name("Standard".into()))
        .insert("V", Value::Integer(1))
        .insert("R", Value::Integer(2))
        .insert("O", Value::HexString(hex(&owner_entry)))
        .insert("U", Value::HexString(hex(&user_entry)))
        .insert("P", Value::Integer(permissions as i64));
    let id_value = Value::Array(crate::core::types::Array(vec![
        Value::HexString(hex(id)),
        Value::HexString(hex(id)),
    ]));

    let user = authenticate_without_references(&dict, &id_value, b"user", &crypto).unwrap();
    assert_eq!(user.file_key(), &*file_key);
    let owner = authenticate_without_references(&dict, &id_value, b"owner", &crypto).unwrap();
    assert_eq!(owner.file_key(), &*file_key);
    assert!(authenticate_without_references(&dict, &id_value, b"wrong", &crypto).is_err());
}

#[test]
fn revision_three_authenticates_user_and_owner_passwords() {
    let crypto = RustCryptoBackend;
    let (dict, id, expected_key) = revision_three_dictionary(&crypto);
    for password in [b"user".as_slice(), b"owner".as_slice()] {
        let handler = authenticate_without_references(&dict, &id, password, &crypto).unwrap();
        assert_eq!(handler.file_key(), &*expected_key);
    }
    assert!(authenticate_without_references(&dict, &id, b"wrong", &crypto).is_err());
}

#[test]
fn revision_four_aesv2_crypt_filter_decrypts_streams() {
    let crypto = RustCryptoBackend;
    let (dict, id, file_key) = revision_four_dictionary(&crypto);
    let handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    let stream_dict = Dict::new()
        .insert("Filter", Value::Name("Crypt".into()))
        .insert(
            "DecodeParms",
            Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
        );
    let encrypted = encrypt_aes_stream(b"R4 AESV2 stream", &file_key, 7, 0, &crypto);
    assert_eq!(
        handler
            .decrypt_stream(7, 0, &stream_dict, &encrypted, &indirect_resolver(&[]))
            .unwrap(),
        b"R4 AESV2 stream"
    );
    let metadata_dict = Dict::new().insert("Type", Value::Name("Metadata".into()));
    let metadata_bytes = b"unencrypted metadata stream";
    assert_eq!(
        handler
            .decrypt_stream(
                8,
                0,
                &metadata_dict,
                metadata_bytes,
                &indirect_resolver(&[])
            )
            .unwrap(),
        metadata_bytes
    );
}

/// 7.6.3.2 / Table 22：`/P` 按无符号 32 位位词解释，位号 1（最低）–32
/// （最高）。NOTE 规定合法 `/P` 因保留高位须置 1 而写成补码负整数，
/// 并以 revision 2 的 `-44` 为例：允许打印与复制、禁止修改与注解。
#[test]
fn permission_flag_word_is_read_as_the_unsigned_32_bit_quantity() {
    let permissions = Permissions::from_p_entry(-44, 2);
    assert_eq!(permissions.p_value(), -44);
    assert_eq!(permissions.flags(), 0xFFFF_FFD4);
    assert_eq!(permissions.bit(3), Some(true));
    assert_eq!(permissions.bit(4), Some(false));
    assert_eq!(permissions.bit(5), Some(true));
    assert_eq!(permissions.bit(6), Some(false));
    assert!(permissions.can_print());
    assert!(permissions.can_copy());
    assert!(!permissions.can_modify());
    assert!(!permissions.can_annotate());
}

#[test]
fn permission_views_and_key_derivation_share_the_same_32_bits() {
    // Algorithm 2 d：`/P` 转 32 位无符号二进制、低位字节在前喂给 MD5；
    // 读端视图的位词必须就是那 4 个位字节。
    let all_set = Permissions::from_p_entry(-1, 4);
    assert_eq!(all_set.flags(), u32::MAX);
    assert_eq!(
        all_set.p_value().to_le_bytes(),
        all_set.flags().to_le_bytes()
    );
    assert_eq!(all_set.bit(32), Some(true));

    let high_bit_only = Permissions::from_p_entry(i32::MIN, 4);
    assert_eq!(high_bit_only.flags(), 0x8000_0000);
    assert_eq!(high_bit_only.bit(32), Some(true));
    assert_eq!(high_bit_only.bit(1), Some(false));
    // 位号越界只报 `None`，不 panic。
    assert_eq!(high_bit_only.bit(0), None);
    assert_eq!(high_bit_only.bit(33), None);
}

#[test]
fn authenticated_handler_publishes_the_permissions_it_authenticated() {
    let crypto = RustCryptoBackend;
    let (dict, id, _key) = revision_three_dictionary(&crypto);
    let handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    assert_eq!(handler.permission_entry(), Some((-4, 3)));
}

#[test]
fn unencrypted_document_publishes_no_permissions() {
    assert!(Document::new().permissions().is_none());
    let builder_side = Document::new();
    builder_side.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
    ));
    assert!(builder_side.permissions().is_none());
}

#[test]
fn revision_two_reads_operations_from_the_bits_it_defines() {
    // Table 21：R2 只在「R3 或更高」的权限位一个都不为 0 时成立，故
    // conforming 的 R2 文件位 9–12 必为 1。非 conforming 的清零不返回
    // 猜出来的 false，而回落到 R2 自己的位（模块文档的库策略）。
    let granted = Permissions::from_p_entry(p_with_permission_bits(&[3, 4, 5, 6, 7, 8]), 2);
    assert_eq!(granted.bit(9), Some(false));
    assert!(!granted.is_defined(9));
    assert!(granted.can_fill_forms()); // 位 6（Table 22 位 6 行不限 revision）
    assert!(granted.can_extract_for_accessibility()); // 位 5 的 R2 表述含无障碍
    assert!(granted.can_assemble_document()); // 位 4（7.6.3.1 的 R2「修改内容」）
    assert!(granted.can_print_faithfully()); // R2 无打印质量分级 → 位 3

    let denied = Permissions::from_p_entry(p_with_permission_bits(&[3, 7, 8, 9, 10, 11, 12]), 2);
    assert!(denied.can_print());
    assert!(denied.can_print_faithfully());
    assert!(!denied.can_modify());
    assert!(!denied.can_assemble_document());
    assert!(!denied.can_copy());
    assert!(!denied.can_extract_for_accessibility());
    assert!(!denied.can_annotate());
    assert!(!denied.can_fill_forms());
}

#[test]
fn revisions_three_and_four_interpret_bits_nine_to_twelve_alike() {
    // Table 22 只为 R3+ 定义位 9–12，且未按 R4 另设位义。
    let flags = p_with_permission_bits(&[3, 4, 5, 6, 9, 10, 11, 12]);
    for revision in [3u8, 4] {
        let permissions = Permissions::from_p_entry(flags, revision);
        assert!(
            permissions.can_print()
                && permissions.can_print_faithfully()
                && permissions.can_modify()
                && permissions.can_copy()
                && permissions.can_annotate()
                && permissions.can_fill_forms()
                && permissions.can_create_form_fields()
                && permissions.can_extract_for_accessibility()
                && permissions.can_assemble_document(),
            "R{revision} 下全位置位应放行 Table 22 的全部操作"
        );
    }
    // 位 9 单独置位即可填写既有表单（「即使位 6 为清」）。
    let fill_only = Permissions::from_p_entry(p_with_permission_bits(&[9]), 3);
    assert!(fill_only.can_fill_forms());
    assert!(!fill_only.can_annotate());
    assert!(!fill_only.can_create_form_fields());
}

#[test]
fn faithful_print_additionally_requires_bit_12() {
    // Table 22 位 12：该位为清且位 3 置位时打印限于低保真外观表示。
    let low_fidelity = Permissions::from_p_entry(p_with_permission_bits(&[3]), 4);
    assert!(low_fidelity.can_print());
    assert!(!low_fidelity.can_print_faithfully());
    assert!(Permissions::from_p_entry(p_with_permission_bits(&[3, 12]), 4).can_print_faithfully());
    // 位 12 单独置位不放行任何打印。
    let print_denied = Permissions::from_p_entry(p_with_permission_bits(&[12]), 3);
    assert!(!print_denied.can_print());
    assert!(!print_denied.can_print_faithfully());
}

#[test]
fn form_field_creation_needs_bit_6_and_bit_4_together() {
    // Table 22 位 6：「and, if bit 4 is also set, create or modify
    // interactive form fields (including signature fields)」。
    for revision in [2u8, 3, 4] {
        let annotate_only = Permissions::from_p_entry(p_with_permission_bits(&[6]), revision);
        assert!(annotate_only.can_annotate());
        assert!(!annotate_only.can_create_form_fields());
        assert!(
            Permissions::from_p_entry(p_with_permission_bits(&[4, 6]), revision)
                .can_create_form_fields()
        );
    }
}

#[test]
fn copy_and_modify_stop_short_of_the_revision_three_operations() {
    // Table 22 位 5（R3+ 表述）排除位 10 所控的抽取，位 4 排除位 11 所控
    // 的组装：无障碍抽取与文档组装各自单独受控。
    let copy_only = Permissions::from_p_entry(p_with_permission_bits(&[5]), 3);
    assert!(copy_only.can_copy());
    assert!(!copy_only.can_extract_for_accessibility());
    let extract_only = Permissions::from_p_entry(p_with_permission_bits(&[10]), 3);
    assert!(extract_only.can_extract_for_accessibility());
    assert!(!extract_only.can_copy());
    let modify_only = Permissions::from_p_entry(p_with_permission_bits(&[4]), 4);
    assert!(modify_only.can_modify());
    assert!(!modify_only.can_assemble_document());
    let assemble_only = Permissions::from_p_entry(p_with_permission_bits(&[11]), 4);
    assert!(assemble_only.can_assemble_document());
    assert!(!assemble_only.can_modify());
}

#[test]
fn reserved_bit_positions_define_no_permission_in_any_revision() {
    // 7.6.3.2 shall：除位 3、4、5、6、9、10、11、12 外的全部标志位一律
    // 被 conforming reader 忽略；位 1、2 与 25–32 在任何 revision 下无定义。
    for revision in [2u8, 3, 4] {
        let all_set = Permissions::from_p_entry(-1, revision);
        for position in [1u8, 2, 7, 8, 13, 24, 25, 32] {
            assert_eq!(
                all_set.bit(position),
                Some(true),
                "位 {position} 应按原值读出"
            );
            assert!(
                !all_set.is_defined(position),
                "位 {position} 在 R{revision} 下无定义"
            );
        }
        let only_reserved =
            Permissions::from_p_entry(p_with_permission_bits(&[1, 2, 7, 8, 25, 32]), revision);
        assert!(!only_reserved.can_print());
        assert!(!only_reserved.can_modify());
        assert!(!only_reserved.can_copy());
        assert!(!only_reserved.can_annotate());
        assert!(!only_reserved.can_fill_forms());
        assert!(!only_reserved.can_extract_for_accessibility());
        assert!(!only_reserved.can_assemble_document());
        assert!(!only_reserved.can_print_faithfully());
    }
}

#[test]
fn absent_or_non_integer_permission_entry_fails_authentication() {
    // Table 21 把 `/P` 列为 Required，且它参与文件密钥派生：缺失或非
    // 整数不适用「宽进」降级，按本模块既有错误模型报错。
    let crypto = RustCryptoBackend;
    for permissions in [
        None,
        Some(Value::Name("all".into())),
        Some(Value::Bool(true)),
        Some(Value::String("-44".into())),
        Some(Value::Null),
    ] {
        let dict = standard_handler_dict(2, 3, permissions);
        let error = authenticate_without_references(&dict, &Value::Null, b"", &crypto)
            .expect_err("无有效 /P 的标准安全字典不应认证通过");
        assert!(error.contains("/P"), "报错应指向 /P：{error}");
    }
}

#[test]
fn float_p_entry_keeps_the_existing_integer_reader_leniency() {
    // 库策略：本模块既有的 `required_integer` 走 `Value::as_integer`，浮点
    // 按截断读成整数；`/P` 沿用同一宽容而不额外报错，权限视图只看到截断后
    // 的位词（Table 21 声明 `/P` 类型为 integer，conforming 写端不产出浮点）。
    let truncated = Value::Float(-44.0).as_integer().unwrap();
    let permissions = Permissions::from_p_entry(i32::try_from(truncated).unwrap(), 2);
    assert_eq!(permissions.flags(), 0xFFFF_FFD4);
    assert!(permissions.can_print() && permissions.can_copy());
    assert!(!permissions.can_modify() && !permissions.can_annotate());
}

#[test]
fn permission_entry_outside_signed_32_bits_fails_authentication() {
    let crypto = RustCryptoBackend;
    for out_of_range in [4_294_967_295i64, 2_147_483_648, -2_147_483_649] {
        let dict = standard_handler_dict(2, 3, Some(Value::Integer(out_of_range)));
        let error = authenticate_without_references(&dict, &Value::Null, b"", &crypto)
            .expect_err("超出 Table 22 位词范围的 /P 不应认证通过");
        assert!(error.contains("permissions"), "报错应指向 /P 位词：{error}");
    }
}

#[test]
fn unsupported_revision_is_rejected_before_permissions_are_read() {
    // PDF 2.0 的 R5/R6（7.6.3.1 明确 ❌ 非目标）与公钥处理器（7.6.4 ❌）
    // 在 /P 之前即被拒绝，故读端不会拿到未知 revision 的权限视图。
    let crypto = RustCryptoBackend;
    let dict = standard_handler_dict(5, 6, Some(Value::Integer(-1)));
    let error = authenticate_without_references(&dict, &Value::Null, b"", &crypto)
        .expect_err("R5/R6 不受支持");
    assert!(error.contains("unsupported"), "报错应说明不受支持：{error}");
}

/// 按 1 基位号拼出 `/P` 值（其余位为 0），对应 Table 22 的位义。
fn p_with_permission_bits(bits: &[u8]) -> i32 {
    let mut flags = 0u32;
    for &bit in bits {
        if let 1..=32 = bit {
            flags |= 1 << (bit - 1);
        }
    }
    flags as i32
}

/// 只填到 `/P` 检查之前的必填项（`/V` `/R` `/O` `/U`），`/P` 由参数决定。
fn standard_handler_dict(version: i64, revision: i64, permissions: Option<Value>) -> Dict {
    let dict = Dict::new()
        .insert("Filter", Value::Name("Standard".into()))
        .insert("V", Value::Integer(version))
        .insert("R", Value::Integer(revision))
        .insert("O", Value::HexString("00".repeat(32)))
        .insert("U", Value::HexString("00".repeat(32)));
    match permissions {
        Some(value) => dict.insert("P", value),
        None => dict,
    }
}

fn revision_three_dictionary(crypto: &dyn CryptoBackend) -> (Dict, Value, Zeroizing<Vec<u8>>) {
    revision_three_or_four_dictionary(3, false, crypto)
}

fn revision_four_dictionary(crypto: &dyn CryptoBackend) -> (Dict, Value, Zeroizing<Vec<u8>>) {
    revision_three_or_four_dictionary(4, true, crypto)
}

fn revision_three_or_four_dictionary(
    revision: u8,
    aes: bool,
    crypto: &dyn CryptoBackend,
) -> (Dict, Value, Zeroizing<Vec<u8>>) {
    let id_bytes = b"R3-R4 test identifier";
    let id = Value::Array(crate::core::types::Array(vec![
        Value::HexString(hex(id_bytes)),
        Value::HexString(hex(id_bytes)),
    ]));
    let user_password = pad_password(b"user");
    let owner_password = pad_password(b"owner");
    let key_len = 16;
    let owner_key = derive_owner_key(&owner_password, revision, key_len, crypto);
    let mut owner_entry = crypto.rc4(&owner_key, &user_password).unwrap();
    for iteration in 1..20u8 {
        let key = Zeroizing::new(
            owner_key
                .iter()
                .map(|byte| byte ^ iteration)
                .collect::<Vec<_>>(),
        );
        owner_entry = crypto.rc4(&key, &owner_entry).unwrap();
    }
    let permissions = -4i32;
    let encrypt_metadata = !aes;
    let file_key = derive_file_key(
        FileKeyParams {
            padded_password: &user_password,
            owner_entry: &owner_entry,
            permissions,
            id: id_bytes,
            revision,
            encrypt_metadata,
            key_len,
        },
        crypto,
    );
    let mut user_material = Zeroizing::new(Vec::from(PASSWORD_PADDING));
    user_material.extend_from_slice(id_bytes);
    let user_digest = crypto.md5(&user_material);
    let mut user_entry = crypto.rc4(&file_key, &user_digest).unwrap();
    for iteration in 1..20u8 {
        let key = Zeroizing::new(
            file_key
                .iter()
                .map(|byte| byte ^ iteration)
                .collect::<Vec<_>>(),
        );
        user_entry = crypto.rc4(&key, &user_entry).unwrap();
    }
    user_entry.extend_from_slice(&[0; 16]);
    let mut encrypt = Dict::new()
        .insert("Filter", Value::Name("Standard".into()))
        .insert("V", Value::Integer(if aes { 4 } else { 2 }))
        .insert("R", Value::Integer(revision as i64))
        .insert("Length", Value::Integer(128))
        .insert("O", Value::HexString(hex(&owner_entry)))
        .insert("U", Value::HexString(hex(&user_entry)))
        .insert("P", Value::Integer(permissions as i64));
    if aes {
        encrypt = encrypt
            .insert(
                "CF",
                Value::Dict(
                    Dict::new().insert(
                        "StdCF",
                        Value::Dict(
                            Dict::new()
                                .insert("CFM", Value::Name("AESV2".into()))
                                .insert("Length", Value::Integer(16)),
                        ),
                    ),
                ),
            )
            .insert("StmF", Value::Name("StdCF".into()))
            .insert("StrF", Value::Name("StdCF".into()));
        if !encrypt_metadata {
            encrypt = encrypt.insert("EncryptMetadata", Value::Bool(false));
        }
    }
    (encrypt, id, file_key)
}

fn encrypt_aes_stream(
    plaintext: &[u8],
    file_key: &[u8],
    object_num: i64,
    generation: i64,
    crypto: &dyn CryptoBackend,
) -> Vec<u8> {
    let mut key_material = file_key.to_vec();
    key_material.extend_from_slice(&(object_num as u32).to_le_bytes()[..3]);
    key_material.extend_from_slice(&(generation as u16).to_le_bytes());
    key_material.extend_from_slice(b"sAlT");
    let key = crypto.md5(&key_material);
    let iv = [0x2a; 16];
    let padding = 16 - plaintext.len() % 16;
    let mut padded = plaintext.to_vec();
    padded.extend(std::iter::repeat_n(padding as u8, padding));
    let cipher = Aes128::new_from_slice(&key[..16]).unwrap();
    let mut previous = iv;
    for chunk in padded.chunks_mut(16) {
        for (byte, prior) in chunk.iter_mut().zip(previous) {
            *byte ^= prior;
        }
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
        previous.copy_from_slice(chunk);
    }
    [iv.as_slice(), padded.as_slice()].concat()
}

/// 测试用间接引用解引用器：`(对象号, 代号) → 值`，模拟
/// `Document::indirect_object_resolver` 的 xref 查找（对象缺失即 `Ok(None)`，
/// 非引用值原样返回，与 `filters::indirect::dereference` 的调用口径一致）。
pub(super) fn indirect_resolver(
    table: &[((i64, i64), Value)],
) -> impl Fn(&Value) -> Result<Option<Value>, String> + '_ {
    use std::collections::HashMap;

    let map: HashMap<(i64, i64), Value> = table.iter().cloned().collect();
    move |value| match value {
        Value::Ref(num, generation) => Ok(map.get(&(*num, *generation)).cloned()),
        other => Ok(Some(other.clone())),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

mod crypt_filter_references;
mod encrypt_dict_references;
mod helpers;
mod v4_write;

use helpers::authenticate_without_references;
