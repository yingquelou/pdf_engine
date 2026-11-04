//! 7.4.1 / Table 5：`/Filter` 与 `/DecodeParms` 间接引用形态的规范化测试。
//!
//! 四种合法形态（引用→Name、引用→Name 数组、引用→参数字典、引用→参数
//! 字典数组）都断言「规范化后的字典」与「解码结果」和直接对象写法
//! **完全相同**；失败形态（引用指向字典、引用成环、引用指向缺失对象）断言
//! 保留原值 + 可诊断信息，且解码明确失败（不静默丢字节）。

use super::*;
use crate::core::filters::decode_stream;
use std::collections::HashMap;

/// 测试用「伪 xref」：对象号 → 对象值，交给 [`ObjectResolver`]。
fn resolver(
    table: &[((i64, i64), Value)],
) -> impl Fn(&Value) -> std::result::Result<Option<Value>, String> + '_ {
    let map: HashMap<(i64, i64), Value> = table.iter().cloned().collect();
    move |value: &Value| match value {
        Value::Ref(num, generation) => Ok(map.get(&(*num, *generation)).cloned()),
        other => Err(format!("resolver called with {}", other.type_name())),
    }
}

/// 规范化并断言诊断为空，返回新字典。
fn normalized_without_issues(dict: &Dict, table: &[((i64, i64), Value)]) -> Dict {
    let (dict, issues) = normalize_filter_entries(dict, &resolver(table));
    assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    dict
}

fn ccitt_parms() -> Dict {
    Dict::new()
        .insert("K", Value::Integer(-1))
        .insert("Columns", Value::Integer(8))
        .insert("Rows", Value::Integer(3))
}

/// 全白 + 两行 4 白 4 黑的 Group 4 条带（与 `filters/tests.rs` 同一样本）。
const CCITT_DATA: &[u8] = &[0x9B, 0x78, 0x00, 0x80, 0x08];

#[test]
fn filter_reference_to_name_matches_direct_name() {
    let direct = Dict::new().insert("Filter", Value::Name("CCITTFaxDecode".into()));
    let indirect = Dict::new().insert("Filter", Value::Ref(3, 0));
    let table = [((3, 0), Value::Name("CCITTFaxDecode".into()))];

    let normalized = normalized_without_issues(&indirect, &table);
    assert_eq!(normalized, direct);
    let parms = Value::Dict(ccitt_parms());
    let decoded_direct = decode_stream(
        &direct.clone().insert("DecodeParms", parms.clone()),
        CCITT_DATA,
    )
    .expect("direct form decodes");
    let decoded_indirect =
        decode_stream(&normalized.insert("DecodeParms", parms), CCITT_DATA).expect("ref decodes");
    assert_eq!(decoded_indirect, decoded_direct);
    assert_eq!(decoded_direct, vec![0xFF, 0xF0, 0xF0]);
}

#[test]
fn filter_reference_to_array_matches_direct_array() {
    let names = Value::Array(
        Array::new()
            .push(Value::Name("ASCIIHexDecode".into()))
            .push(Value::Name("RunLengthDecode".into())),
    );
    let direct = Dict::new().insert("Filter", names.clone());
    let indirect = Dict::new().insert("Filter", Value::Ref(8, 0));
    let table = [((8, 0), names)];

    let normalized = normalized_without_issues(&indirect, &table);
    assert_eq!(normalized, direct);
    // `014141>` → 字节 01 41 41 → RunLength → `AA`。
    let encoded = b"014141>";
    assert_eq!(
        decode_stream(&normalized, encoded).unwrap(),
        decode_stream(&direct, encoded).unwrap()
    );
    assert_eq!(decode_stream(&direct, encoded).unwrap(), b"AA");
}

#[test]
fn filter_array_element_reference_is_resolved() {
    let direct = Dict::new().insert(
        "Filter",
        Value::Array(
            Array::new()
                .push(Value::Name("ASCIIHexDecode".into()))
                .push(Value::Name("RunLengthDecode".into())),
        ),
    );
    let indirect = Dict::new().insert(
        "Filter",
        Value::Array(
            Array::new()
                .push(Value::Ref(11, 0))
                .push(Value::Name("RunLengthDecode".into())),
        ),
    );
    let table = [((11, 0), Value::Name("ASCIIHexDecode".into()))];
    assert_eq!(normalized_without_issues(&indirect, &table), direct);
    // 未解引用的原值解码失败（不支持的数组元素），不静默返回原始字节。
    assert!(
        decode_stream(&indirect, b"014141>")
            .unwrap_err()
            .to_string()
            .contains("unsupported /Filter array element")
    );
    assert_eq!(decode_stream(&direct, b"014141>").unwrap(), b"AA");
}

#[test]
fn decode_parms_reference_to_dict_matches_direct_dict() {
    let direct = Dict::new()
        .insert("Filter", Value::Name("CCITTFaxDecode".into()))
        .insert("DecodeParms", Value::Dict(ccitt_parms()));
    let indirect = Dict::new()
        .insert("Filter", Value::Name("CCITTFaxDecode".into()))
        .insert("DecodeParms", Value::Ref(5, 0));
    let table = [((5, 0), Value::Dict(ccitt_parms()))];

    let normalized = normalized_without_issues(&indirect, &table);
    assert_eq!(normalized, direct);
    assert_eq!(
        decode_stream(&normalized, CCITT_DATA).unwrap(),
        decode_stream(&direct, CCITT_DATA).unwrap()
    );
    assert_eq!(
        decode_stream(&normalized, CCITT_DATA).unwrap(),
        vec![0xFF, 0xF0, 0xF0]
    );
}

#[test]
fn decode_parms_reference_to_array_matches_direct_array() {
    let parms = Value::Array(
        Array::new()
            .push(Value::Null)
            .push(Value::Dict(ccitt_parms())),
    );
    let filter = Value::Array(
        Array::new()
            .push(Value::Name("ASCIIHexDecode".into()))
            .push(Value::Name("CCITTFaxDecode".into())),
    );
    let direct = Dict::new()
        .insert("Filter", filter.clone())
        .insert("DecodeParms", parms.clone());
    // 引用指向参数数组，且数组元素自身也是引用（7.3.10 允许）。
    let indirect = Dict::new()
        .insert("Filter", filter)
        .insert("DecodeParms", Value::Ref(6, 0));
    let table = [((6, 0), parms), ((7, 0), Value::Dict(ccitt_parms()))];
    let (normalized, issues) = normalize_filter_entries(&indirect, &resolver(&table));
    assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    assert_eq!(normalized.get("Filter"), direct.get("Filter"));

    // 元素级引用形态：[null, 7 0 R]。
    let element_indirect = direct.clone().insert(
        "DecodeParms",
        Value::Array(Array::new().push(Value::Null).push(Value::Ref(7, 0))),
    );
    assert_eq!(normalized_without_issues(&element_indirect, &table), direct);
    // `9b78008008>` 是 CCITT 样本的 ASCIIHex 形式。
    let encoded = b"9b78008008>";
    assert_eq!(
        decode_stream(&direct, encoded).unwrap(),
        vec![0xFF, 0xF0, 0xF0]
    );
    assert_eq!(
        decode_stream(&normalized, encoded).unwrap(),
        decode_stream(&direct, encoded).unwrap()
    );
    assert_eq!(
        decode_stream(
            &normalized_without_issues(&element_indirect, &table),
            encoded
        )
        .unwrap(),
        vec![0xFF, 0xF0, 0xF0]
    );
    // 未解引用时 CCITT 拿不到参数字典 → 解码失败（不返回错误像素）。
    assert!(decode_stream(&element_indirect, encoded).is_err());
    assert!(decode_stream(&indirect, encoded).is_err());
}

#[test]
fn filter_reference_to_dict_keeps_original_and_diagnoses() {
    let indirect = Dict::new().insert("Filter", Value::Ref(4, 0));
    let table = [((4, 0), Value::Dict(Dict::new()))];
    let (normalized, issues) = normalize_filter_entries(&indirect, &resolver(&table));

    assert_eq!(normalized, indirect, "original value must be kept");
    assert_eq!(issues.len(), 1, "got: {issues:?}");
    assert!(
        issues[0].contains("/Filter") && issues[0].contains("resolves to dict"),
        "got: {issues:?}"
    );
    // 原值保留 → 解码按不支持失败，绝不静默返回未解码字节。
    let error = decode_stream(&normalized, b"\x00").unwrap_err();
    assert!(
        error.to_string().contains("unsupported /Filter value"),
        "got: {error}"
    );
}

#[test]
fn cyclic_filter_reference_keeps_original_and_diagnoses() {
    let indirect = Dict::new().insert("Filter", Value::Ref(9, 0));
    let table = [((9, 0), Value::Ref(9, 0))];
    let (normalized, issues) = normalize_filter_entries(&indirect, &resolver(&table));

    assert_eq!(normalized, indirect);
    assert_eq!(issues.len(), 1, "got: {issues:?}");
    assert!(issues[0].contains("cyclic"), "got: {issues:?}");
    assert!(decode_stream(&normalized, b"x").is_err());
}

#[test]
fn absent_filter_reference_object_keeps_original_and_diagnoses() {
    let indirect = Dict::new().insert("Filter", Value::Ref(12, 0));
    let (normalized, issues) = normalize_filter_entries(&indirect, &resolver(&[]));

    assert_eq!(normalized, indirect);
    assert_eq!(issues.len(), 1, "got: {issues:?}");
    assert!(issues[0].contains("absent object"), "got: {issues:?}");
    assert!(decode_stream(&normalized, b"x").is_err());
}

#[test]
fn filter_reference_to_null_keeps_original_and_diagnoses() {
    // 7.3.10 引用可以指向任何对象；指向 null 不是 Table 5 的合法形态，
    // 按「保留原值 + 诊断」处理（不取 7.3.9 的降级，见模块文档）。
    let indirect = Dict::new().insert("Filter", Value::Ref(5, 0));
    let table = [((5, 0), Value::Null)];
    let (normalized, issues) = normalize_filter_entries(&indirect, &resolver(&table));

    assert_eq!(normalized, indirect, "original value must be kept");
    assert_eq!(issues.len(), 1, "got: {issues:?}");
    assert!(
        issues[0].contains("/Filter") && issues[0].contains("resolves to null"),
        "got: {issues:?}"
    );
    assert!(decode_stream(&normalized, b"x").is_err());
}

#[test]
fn decode_parms_reference_to_null_keeps_original_and_diagnoses() {
    let indirect = Dict::new()
        .insert("Filter", Value::Name("CCITTFaxDecode".into()))
        .insert("DecodeParms", Value::Ref(6, 0));
    let table = [((6, 0), Value::Null)];
    let (normalized, issues) = normalize_filter_entries(&indirect, &resolver(&table));

    assert_eq!(normalized.get("DecodeParms"), Some(&Value::Ref(6, 0)));
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("resolves to null")),
        "got: {issues:?}"
    );
}

#[test]
fn decode_parms_reference_to_integer_keeps_original_and_diagnoses() {
    let indirect = Dict::new()
        .insert("Filter", Value::Name("CCITTFaxDecode".into()))
        .insert("DecodeParms", Value::Ref(2, 0));
    let table = [((2, 0), Value::Integer(7))];
    let (normalized, issues) = normalize_filter_entries(&indirect, &resolver(&table));

    assert_eq!(normalized.get("DecodeParms"), Some(&Value::Ref(2, 0)));
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("resolves to integer")),
        "got: {issues:?}"
    );
}

#[test]
fn single_parms_dict_for_multiple_filters_is_diagnosed() {
    let dict = Dict::new()
        .insert(
            "Filter",
            Value::Array(
                Array::new()
                    .push(Value::Name("ASCIIHexDecode".into()))
                    .push(Value::Name("RunLengthDecode".into())),
            ),
        )
        .insert("DecodeParms", Value::Dict(ccitt_parms()));
    let (normalized, issues) = normalize_filter_entries(&dict, &resolver(&[]));

    assert_eq!(normalized, dict, "diagnostics must not rewrite data");
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("one entry for each filter")),
        "got: {issues:?}"
    );
}

#[test]
fn parms_array_length_mismatch_is_diagnosed() {
    let three = Value::Array(
        Array::new()
            .push(Value::Dict(ccitt_parms()))
            .push(Value::Dict(ccitt_parms()))
            .push(Value::Dict(ccitt_parms())),
    );
    let dict = Dict::new()
        .insert(
            "Filter",
            Value::Array(
                Array::new()
                    .push(Value::Name("ASCIIHexDecode".into()))
                    .push(Value::Name("RunLengthDecode".into())),
            ),
        )
        .insert("DecodeParms", three.clone());
    let (_, issues) = normalize_filter_entries(&dict, &resolver(&[]));
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("has 3 entries but /Filter has 2")),
        "got: {issues:?}"
    );

    let short = Value::Array(Array::new().push(Value::Null));
    let issues =
        normalize_filter_entries(&dict.clone().insert("DecodeParms", short), &resolver(&[])).1;
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("has 1 entries but /Filter has 2")),
        "got: {issues:?}"
    );
}

#[test]
fn parms_without_filter_is_diagnosed() {
    let dict = Dict::new().insert("DecodeParms", Value::Dict(ccitt_parms()));
    let (normalized, issues) = normalize_filter_entries(&dict, &resolver(&[]));
    assert_eq!(normalized, dict);
    assert!(
        issues.iter().any(|issue| issue.contains("without /Filter")),
        "got: {issues:?}"
    );
    // `/DP` 缩写同样受核对。
    let abbreviated = Dict::new().insert("DP", Value::Ref(13, 0));
    let table = [((13, 0), Value::Dict(ccitt_parms()))];
    let (normalized, issues) = normalize_filter_entries(&abbreviated, &resolver(&table));
    assert_eq!(
        normalized.get("DP"),
        Some(&Value::Dict(ccitt_parms())),
        "缩写 /DP 也应解引用"
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("/DP present without /Filter")),
        "got: {issues:?}"
    );
}

#[test]
fn direct_forms_produce_no_issues() {
    let dict = Dict::new()
        .insert("Filter", Value::Name("FlateDecode".into()))
        .insert(
            "DecodeParms",
            Value::Array(Array::new().push(Value::Dict(
                Dict::new().insert("Predictor", Value::Integer(15)),
            ))),
        );
    let (normalized, issues) = normalize_filter_entries(&dict, &resolver(&[]));
    assert_eq!(normalized, dict);
    assert!(issues.is_empty(), "got: {issues:?}");
}
