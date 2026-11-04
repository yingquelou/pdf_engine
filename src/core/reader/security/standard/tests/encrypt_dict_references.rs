//! 加密字典**自身**一侧的间接引用形态（7.6.5 选择器 + `/CF`，配合 7.3.10）：
//! `/CF` 本体、`/CF` 条目值与 `StrF`／`StmF`／`EFF` 选择器写成间接引用时，
//! 必须解引用后选出与直接写法**同一个** crypt filter。
//!
//! 立场依据：7.6.1 只要求加密字典里的**字符串**为直接对象（「Unlike strings
//! within the body of the document, those in the encryption dictionary shall be
//! direct objects」），并未限制 `/CF` 或其条目值为直接对象；7.3.10 允许任意值
//! 写成间接引用。故这里解引用是「支持规范允许的写法」，不是放宽诊断——解不到
//! （对象缺失、指向非字典、成环、超跳数）一律**明确报错**，不退回 `/Identity`，
//! 因为选错 filter 会拿错密钥、产出静默乱码。

use super::*;

/// 从 R4 测试字典里取出 `/CF` 的直接写法，供引用形态用例复用。
fn cf_dictionary(crypto: &dyn CryptoBackend) -> Value {
    let (encrypt, _, _) = revision_four_dictionary(crypto);
    encrypt.get("CF").cloned().expect("R4 测试字典应带 /CF")
}

/// 只替换 `/CF` 本体写法、其余条目保持不变，再认证一次。
fn authenticate_with_cf(
    crypto: &dyn CryptoBackend,
    cf_value: Value,
    table: &[((i64, i64), Value)],
) -> Result<SecurityHandler, String> {
    let (encrypt, id, _) = revision_four_dictionary(crypto);
    let encrypt = encrypt.insert("CF", cf_value);
    SecurityHandler::authenticate(
        &encrypt,
        &id,
        b"user",
        None,
        crypto,
        &indirect_resolver(table),
    )
}

fn selected_methods(handler: &SecurityHandler) -> (CryptMethod, CryptMethod, CryptMethod) {
    (
        handler.options.string_method,
        handler.options.stream_method,
        handler.options.embedded_file_method,
    )
}

/// `/CF` 写成指向字典的引用（`<< /CF 9 0 R >>`）与写成内联字典必须选出同一组
/// 文件级默认 filter。
#[test]
fn cf_dictionary_reference_selects_the_same_methods_as_the_direct_form() {
    let crypto = RustCryptoBackend;
    let direct = authenticate_with_cf(&crypto, cf_dictionary(&crypto), &[]).unwrap();

    let handler = authenticate_with_cf(
        &crypto,
        Value::Ref(9, 0),
        &[((9, 0), cf_dictionary(&crypto))],
    )
    .expect("/CF 写成间接引用应照常解引用");
    assert_eq!(selected_methods(&handler), selected_methods(&direct));
    assert_eq!(handler.options.stream_method, CryptMethod::Aes128);
}

/// `/CF` 的**条目值**写成引用（`<< /CF << /StdCF 10 0 R >> >>`）同样解引用后
/// 选中同一 filter。
#[test]
fn cf_entry_reference_selects_the_same_methods_as_the_direct_form() {
    let crypto = RustCryptoBackend;
    let direct = authenticate_with_cf(&crypto, cf_dictionary(&crypto), &[]).unwrap();

    let std_cf = match cf_dictionary(&crypto) {
        Value::Dict(dict) => dict.get("StdCF").cloned().expect("/CF 应带 StdCF"),
        other => panic!("测试字典的 /CF 应是字典，实际为 {other:?}"),
    };
    let handler = authenticate_with_cf(
        &crypto,
        Value::Dict(Dict::new().insert("StdCF", Value::Ref(10, 0))),
        &[((10, 0), std_cf)],
    )
    .expect("/CF 条目值是引用时应解引用后选中同一 filter");
    assert_eq!(selected_methods(&handler), selected_methods(&direct));
}

/// 选择器名字本身写成引用（`/StmF 11 0 R` → `/StdCF`）也要解引用后按名字选
/// filter；指向非名字则报错。
#[test]
fn crypt_filter_name_reference_is_dereferenced() {
    let crypto = RustCryptoBackend;
    let (encrypt, _, _) = revision_four_dictionary(&crypto);
    let cf = encrypt.get("CF").cloned().expect("R4 测试字典应带 /CF");
    let table = [((11, 0), Value::Name("StdCF".into()))];

    let by_name = encrypt.get("StmF").cloned().expect("R4 字典应带 /StmF");
    let direct = crypt_filter(
        &encrypt,
        "StmF",
        CryptMethod::Identity,
        16,
        &indirect_resolver(&[]),
    )
    .expect("直接写法应照常选出 StdCF");
    assert_eq!(by_name, Value::Name("StdCF".into()));

    let by_reference = crypt_filter(
        &encrypt.insert("StmF", Value::Ref(11, 0)),
        "StmF",
        CryptMethod::Identity,
        16,
        &indirect_resolver(&table),
    )
    .expect("/StmF 写成引用应解引用后按名字选 filter");
    assert_eq!(by_reference, direct);

    // 引用指向非名字：报错，不退回文件级默认 filter。
    let error = crypt_filter(
        &Dict::new()
            .insert("CF", cf.clone())
            .insert("StmF", Value::Ref(12, 0)),
        "StmF",
        CryptMethod::Identity,
        16,
        &indirect_resolver(&[((12, 0), Value::Integer(4))]),
    )
    .expect_err("选择器引用指向非名字必须报错");
    assert!(error.contains("invalid encryption /StmF"), "{error}");
}

/// 直接对象写法下解析器压根不会被调用：给一张空表也照常工作。
#[test]
fn direct_encrypt_dictionary_needs_no_objects_from_the_resolver() {
    let crypto = RustCryptoBackend;
    let handler = authenticate_with_cf(&crypto, cf_dictionary(&crypto), &[]).unwrap();
    assert_eq!(handler.options.stream_method, CryptMethod::Aes128);
}

/// `/CF` 引用指向缺失对象：明确报错，不退回 `/Identity`。
#[test]
fn unresolvable_cf_reference_is_an_error() {
    let crypto = RustCryptoBackend;
    let error =
        authenticate_with_cf(&crypto, Value::Ref(99, 0), &[]).expect_err("/CF 引用缺失必须报错");
    assert!(
        error.contains("encryption /CF dictionary: references an absent object"),
        "{error}"
    );
}

/// 引用链（`/CF` → 引用 → 字典）逐跳解引用；成环则报错。
#[test]
fn chained_cf_reference_resolves_and_a_cycle_is_an_error() {
    let crypto = RustCryptoBackend;
    let direct = authenticate_with_cf(&crypto, cf_dictionary(&crypto), &[]).unwrap();
    let handler = authenticate_with_cf(
        &crypto,
        Value::Ref(30, 0),
        &[
            ((30, 0), Value::Ref(31, 0)),
            ((31, 0), cf_dictionary(&crypto)),
        ],
    )
    .expect("引用链应逐跳解引用");
    assert_eq!(selected_methods(&handler), selected_methods(&direct));

    let error = authenticate_with_cf(
        &crypto,
        Value::Ref(40, 0),
        &[((40, 0), Value::Ref(41, 0)), ((41, 0), Value::Ref(40, 0))],
    )
    .expect_err("引用链成环必须报错");
    assert!(
        error.contains("encryption /CF dictionary: reference chain is cyclic"),
        "{error}"
    );
}

/// `/CF` 引用链解开后仍不是字典（这里是数组）：按「必须是字典」报错。
#[test]
fn cf_reference_to_a_non_dictionary_is_an_error() {
    let crypto = RustCryptoBackend;
    let error = authenticate_with_cf(
        &crypto,
        Value::Ref(50, 0),
        &[(
            (50, 0),
            Value::Array(crate::core::types::Array(vec![Value::Integer(1)])),
        )],
    )
    .expect_err("/CF 引用指向非字典必须报错");
    assert!(
        error.contains("encryption /CF dictionary: must be a dictionary"),
        "{error}"
    );
}
