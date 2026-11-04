//! 7.6.5 集成式单元测试：流字典上显式 `/Crypt` 的 `/Filter`／`/DecodeParms`
//! 写成**间接引用**（7.3.10）时的 crypt filter 选择。
//!
//! 立场：选中 `/Crypt` 后，引用缺失、指向非 Name／非字典、成环、超跳数一律
//! **明确报错**（选错 filter 会拿错密钥、产出静默乱码），不退回 `/Identity`
//! 也不退回文件级默认 filter；没有 `/Crypt` 的普通加密流则不受这些诊断影响。

use super::*;
use crate::core::types::Array;

/// 间接对象表的一条条目：`(对象号, 代数)` 到其值。
type IndirectEntry = ((i64, i64), Value);

/// 数组元素是引用、参数字典也是引用的混合形态。
fn mixed_reference_crypt_entries() -> (Dict, Vec<IndirectEntry>) {
    (
        Dict::new()
            .insert(
                "Filter",
                Value::Array(Array(vec![Value::Name("Crypt".into()), Value::Ref(7, 0)])),
            )
            .insert(
                "DecodeParms",
                Value::Array(Array(vec![Value::Ref(8, 0), Value::Null])),
            ),
        vec![
            ((7, 0), Value::Name("FlateDecode".into())),
            (
                (8, 0),
                Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
            ),
        ],
    )
}

/// 全直接对象写法的等价形态（Table 5 的规范写法）。
fn direct_crypt_entries() -> Dict {
    Dict::new()
        .insert(
            "Filter",
            Value::Array(Array(vec![
                Value::Name("Crypt".into()),
                Value::Name("FlateDecode".into()),
            ])),
        )
        .insert(
            "DecodeParms",
            Value::Array(Array(vec![
                Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
                Value::Null,
            ])),
        )
}

/// `/Filter`／`/DecodeParms` 写成引用（含「引用 → 数组」「数组元素 → 引用」）
/// 与写成直接对象必须选出**同一个** [`CryptMethod`]。
#[test]
fn crypt_filter_selection_is_identical_for_direct_and_indirect_filter_entries() {
    let crypto = RustCryptoBackend;
    let (dict, id, _file_key) = revision_four_dictionary(&crypto);
    let handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();

    let expected = explicit_crypt_filter(
        &direct_crypt_entries(),
        &indirect_resolver(&[]),
        &handler.options,
    )
    .expect("直接对象形态应选出 crypt filter");
    assert_eq!(expected, Some(CryptMethod::Aes128));

    let (mixed, table) = mixed_reference_crypt_entries();
    assert_eq!(
        explicit_crypt_filter(&mixed, &indirect_resolver(&table), &handler.options).unwrap(),
        expected,
        "数组元素写成引用的形态应与直接对象形态选出同一 filter"
    );

    // 整个 `/Filter`／`/DecodeParms` 值本身是引用（7.3.7 例 3 的写法）。
    let whole = Dict::new()
        .insert("Filter", Value::Ref(9, 0))
        .insert("DecodeParms", Value::Ref(10, 0));
    let whole_table = [
        (
            (9, 0),
            Value::Array(Array(vec![Value::Name("Crypt".into()), Value::Ref(7, 0)])),
        ),
        (
            (10, 0),
            Value::Array(Array(vec![Value::Ref(8, 0), Value::Null])),
        ),
        ((7, 0), Value::Name("FlateDecode".into())),
        (
            (8, 0),
            Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
        ),
    ];
    assert_eq!(
        explicit_crypt_filter(&whole, &indirect_resolver(&whole_table), &handler.options).unwrap(),
        expected,
        "值本体是引用的形态应与直接对象形态选出同一 filter"
    );
}

/// 解密走真实字节：引用形态的 `/Crypt` 条目应解出与明文相同的流内容。
#[test]
fn decrypt_stream_decrypts_with_a_reference_valued_crypt_filter_array() {
    let crypto = RustCryptoBackend;
    let (dict, id, file_key) = revision_four_dictionary(&crypto);
    let handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    let plaintext = b"R4 indirect crypt filter stream";
    let encrypted = encrypt_aes_stream(plaintext, &file_key, 12, 0, &crypto);
    let (stream_dict, table) = mixed_reference_crypt_entries();
    assert_eq!(
        handler
            .decrypt_stream(12, 0, &stream_dict, &encrypted, &indirect_resolver(&table))
            .expect("引用形态应选中 /CF 里的 StdCF 并解密"),
        plaintext
    );
}

/// 引用指向**缺失对象**：明确报错，不退回 `/Identity`、不返回未解密字节。
#[test]
fn absent_crypt_filter_reference_is_an_error_instead_of_identity() {
    let crypto = RustCryptoBackend;
    let (dict, id, file_key) = revision_four_dictionary(&crypto);
    let handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    let stream_dict = Dict::new()
        .insert("Filter", Value::Name("Crypt".into()))
        .insert("DecodeParms", Value::Ref(8, 0));
    let encrypted = encrypt_aes_stream(b"absent reference", &file_key, 11, 0, &crypto);

    let error = handler
        .decrypt_stream(11, 0, &stream_dict, &encrypted, &indirect_resolver(&[]))
        .expect_err("缺失的参数字典引用必须报错");
    assert!(
        error.contains("absent object") && error.contains("/DecodeParms"),
        "错误应带上解引用诊断与键名上下文：{error}"
    );
}

/// 引用指向**非字典**（此处是整数）：报错并保留「解析结果不符」的上下文。
#[test]
fn crypt_filter_reference_to_a_wrong_type_is_an_error() {
    let crypto = RustCryptoBackend;
    let (dict, id, file_key) = revision_four_dictionary(&crypto);
    let handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    let stream_dict = Dict::new()
        .insert("Filter", Value::Name("Crypt".into()))
        .insert("DecodeParms", Value::Ref(8, 0));
    let encrypted = encrypt_aes_stream(b"wrong type reference", &file_key, 11, 0, &crypto);

    let error = handler
        .decrypt_stream(
            11,
            0,
            &stream_dict,
            &encrypted,
            &indirect_resolver(&[((8, 0), Value::Integer(3))]),
        )
        .expect_err("参数字典引用指向非字典必须报错");
    assert!(
        error.contains("resolves to") && error.contains("/DecodeParms"),
        "错误应指出解析出的类型：{error}"
    );
}

/// 引用**成环**：规范化器的环检测诊断升级为错误，绝不静默解密。
#[test]
fn cyclic_crypt_filter_reference_is_an_error() {
    let crypto = RustCryptoBackend;
    let (dict, id, file_key) = revision_four_dictionary(&crypto);
    let handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    let stream_dict = Dict::new()
        .insert("Filter", Value::Ref(7, 0))
        .insert("DecodeParms", Value::Dict(Dict::new()));
    let encrypted = encrypt_aes_stream(b"cyclic reference", &file_key, 11, 0, &crypto);

    let error = handler
        .decrypt_stream(
            11,
            0,
            &stream_dict,
            &encrypted,
            &indirect_resolver(&[((7, 0), Value::Ref(8, 0)), ((8, 0), Value::Ref(7, 0))]),
        )
        .expect_err("成环的 /Filter 引用必须报错");
    assert!(
        error.contains("cyclic") && error.contains("/Filter"),
        "错误应指出引用成环：{error}"
    );
}

/// `/CF` 的条目值写成引用（`<< /CF << /StdCF 11 0 R >> >>`）仍能选出同一方法。
#[test]
fn crypt_filter_cf_entry_reference_selects_the_same_method_as_the_direct_entry() {
    let crypto = RustCryptoBackend;
    let (dict, id, file_key) = revision_four_dictionary(&crypto);
    let mut handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    let params = Dict::new()
        .insert("CFM", Value::Name("AESV2".into()))
        .insert("Length", Value::Integer(16));
    let stream_dict = Dict::new()
        .insert("Filter", Value::Name("Crypt".into()))
        .insert(
            "DecodeParms",
            Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
        );
    let plaintext = b"CF entry as indirect reference";
    let encrypted = encrypt_aes_stream(plaintext, &file_key, 13, 0, &crypto);

    let direct = handler
        .decrypt_stream(13, 0, &stream_dict, &encrypted, &indirect_resolver(&[]))
        .expect("直接 /CF 条目应照常选中 StdCF");
    assert_eq!(direct, plaintext);

    handler.options.crypt_filters =
        Some(Value::Dict(Dict::new().insert("StdCF", Value::Ref(11, 0))));
    assert_eq!(
        handler
            .decrypt_stream(
                13,
                0,
                &stream_dict,
                &encrypted,
                &indirect_resolver(&[((11, 0), Value::Dict(params.clone()))]),
            )
            .expect("/CF 条目是引用时应解引用后选中同一 filter"),
        plaintext
    );
    let reference_form = explicit_crypt_filter(
        &stream_dict,
        &indirect_resolver(&[((11, 0), Value::Dict(params))]),
        &handler.options,
    )
    .expect("/CF 条目是引用时应解引用后选中同一 filter");
    assert_eq!(reference_form, Some(CryptMethod::Aes128));
    let direct_form = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    assert_eq!(
        reference_form,
        explicit_crypt_filter(&stream_dict, &indirect_resolver(&[]), &direct_form.options).unwrap(),
        "/CF 条目写成引用与写成直接对象应选出同一个 CryptMethod"
    );
}

/// `/CF` 条目引用解不到字典：报错，不退回 `/Identity`。
#[test]
fn unresolvable_cf_entry_reference_is_an_error() {
    let crypto = RustCryptoBackend;
    let (dict, id, file_key) = revision_four_dictionary(&crypto);
    let mut handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    handler.options.crypt_filters =
        Some(Value::Dict(Dict::new().insert("StdCF", Value::Ref(11, 0))));
    let stream_dict = Dict::new()
        .insert("Filter", Value::Name("Crypt".into()))
        .insert(
            "DecodeParms",
            Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
        );
    let encrypted = encrypt_aes_stream(b"unresolvable CF", &file_key, 14, 0, &crypto);
    let absent = handler
        .decrypt_stream(14, 0, &stream_dict, &encrypted, &indirect_resolver(&[]))
        .expect_err("/CF 条目引用缺失必须报错");
    assert!(
        absent.contains("/CF entry: references an absent object"),
        "{absent}"
    );
    // 指向非字典同样报错。
    let non_dict = handler
        .decrypt_stream(
            14,
            0,
            &stream_dict,
            &encrypted,
            &indirect_resolver(&[((11, 0), Value::Integer(4))]),
        )
        .expect_err("/CF 条目引用指向非字典必须报错");
    assert!(
        non_dict.contains("/CF entry: must be a dictionary"),
        "{non_dict}"
    );
}

/// 没有 `/Crypt` 的加密流（链本身可解、只是某个元素解不开）不因这些诊断变红：
/// 仍按文件级默认 stream filter 解密。
#[test]
fn stream_without_crypt_filter_keeps_the_file_default_despite_broken_array_element() {
    let crypto = RustCryptoBackend;
    let (dict, id, file_key) = revision_four_dictionary(&crypto);
    let handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    let plaintext = b"ordinary encrypted content stream";
    let encrypted = encrypt_aes_stream(plaintext, &file_key, 15, 0, &crypto);
    for stream_dict in [
        // 数组元素是引用、指向缺失对象，但链里没有 /Crypt。
        Dict::new().insert(
            "Filter",
            Value::Array(Array(vec![
                Value::Name("FlateDecode".into()),
                Value::Ref(8, 0),
            ])),
        ),
        // Table 5 个数违例（单字典配两个筛选器），同样与 crypt filter 选择无关。
        Dict::new()
            .insert(
                "Filter",
                Value::Array(Array(vec![
                    Value::Name("FlateDecode".into()),
                    Value::Name("LZWDecode".into()),
                ])),
            )
            .insert("DecodeParms", Value::Dict(Dict::new())),
        // 合法但带参数字典的普通流。
        Dict::new()
            .insert("Filter", Value::Name("FlateDecode".into()))
            .insert(
                "DecodeParms",
                Value::Dict(Dict::new().insert("Predictor", Value::Integer(1))),
            ),
    ] {
        assert_eq!(
            explicit_crypt_filter(&stream_dict, &indirect_resolver(&[]), &handler.options)
                .expect("无 /Crypt 的流不应因解引用诊断失败"),
            None,
        );
        assert_eq!(
            handler
                .decrypt_stream(15, 0, &stream_dict, &encrypted, &indirect_resolver(&[]))
                .expect("无 /Crypt 的流应沿用文件级 stream filter 解密"),
            plaintext
        );
    }
}

/// `/Filter` 本体指向缺失对象时**无法判定**该流是否选了 `/Crypt`，按严格立场
/// 报错（不静默退回文件级默认 filter），与「引用缺失即报错」同一条线。
#[test]
fn dangling_filter_reference_cannot_be_proved_free_of_crypt_and_errors() {
    let crypto = RustCryptoBackend;
    let (dict, id, file_key) = revision_four_dictionary(&crypto);
    let handler = authenticate_without_references(&dict, &id, b"user", &crypto).unwrap();
    let stream_dict = Dict::new().insert("Filter", Value::Ref(7, 0));
    let encrypted = encrypt_aes_stream(b"dangling filter", &file_key, 16, 0, &crypto);
    let error = handler
        .decrypt_stream(16, 0, &stream_dict, &encrypted, &indirect_resolver(&[]))
        .expect_err("解不开的 /Filter 引用必须报错");
    assert!(
        error.contains("invalid PDF stream /Filter") && error.contains("absent object"),
        "错误应同时给出选择失败与解引用诊断：{error}"
    );
}
