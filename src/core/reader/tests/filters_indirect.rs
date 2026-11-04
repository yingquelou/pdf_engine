//! 7.4.1 / Table 5：`/Filter` 与 `/DecodeParms` **间接引用形态**在
//! `Document` 解码入口（`decode_stream_bytes` / `decode_stream_bytes_except`）
//! 的读端行为。
//!
//! 立场：解引用走既有懒加载设施 `Document::get_object`；四种合法形态的解码
//! 结果与「直接对象」写法**逐字节相同**；失败形态保留原值 + 产出 warning，
//! 解码明确失败（不 panic、不静默返回未解码字节）。

use crate::core::types::{
    Array, Dict, Document, Object, StartXref, StreamData, Trailer, Value, Xref,
};

/// 全白 + 两行 4 白 4 黑的 Group 4 条带（与 `filters` 主题同一份样本）。
const CCITT_DATA: &[u8] = &[0x9B, 0x78, 0x00, 0x80, 0x08];

/// `CCITTFaxDecode` 的参数字典。
fn ccitt_parms() -> Value {
    Value::Dict(
        Dict::new()
            .insert("K", Value::Integer(-1))
            .insert("Columns", Value::Integer(8))
            .insert("Rows", Value::Integer(3)),
    )
}

/// 以给定 `/Filter` + `/DecodeParms` 组成一个内存流对象。
fn stream(filter: Value, parms: Value) -> Object {
    let dict = Dict::new()
        .insert("Length", Value::Integer(CCITT_DATA.len() as i64))
        .insert("Filter", filter)
        .insert("DecodeParms", parms);
    Object::new(
        1,
        0,
        Value::Stream(dict, StreamData::Owned(CCITT_DATA.to_vec())),
    )
}

/// 写入端形态的内存文档：对象全部在缓存里，`get_object` 直接命中。
fn document(objects: Vec<Object>) -> Document {
    Document::from_objects(objects, Xref::new(), Trailer::new(), StartXref(0))
}

fn decode(doc: &Document) -> Vec<u8> {
    let value = doc
        .get_object(1)
        .expect("get_object")
        .expect("stream object")
        .value;
    doc.decode_stream_bytes(&value).expect("decode")
}

/// 参照组：两个条目都是直接对象。
#[test]
fn decode_stream_bytes_matches_direct_objects_baseline() {
    let doc = document(vec![stream(
        Value::Name("CCITTFaxDecode".into()),
        ccitt_parms(),
    )]);
    assert_eq!(decode(&doc), vec![0xFF, 0xF0, 0xF0]);
    assert!(doc.warnings().is_empty(), "got: {:?}", doc.warnings());
}

/// 形态一：`/Filter` 是引用，指向 Name。
#[test]
fn filter_reference_to_name_decodes_identically() {
    let indirect = document(vec![
        stream(Value::Ref(2, 0), ccitt_parms()),
        Object::new(2, 0, Value::Name("CCITTFaxDecode".into())),
    ]);
    let direct = document(vec![stream(
        Value::Name("CCITTFaxDecode".into()),
        ccitt_parms(),
    )]);
    assert_eq!(decode(&indirect), decode(&direct));
    assert!(
        indirect.warnings().is_empty(),
        "got: {:?}",
        indirect.warnings()
    );
}

/// 形态二：`/Filter` 是引用，指向 Name 数组（含缩写名 `CCF`）。
#[test]
fn filter_reference_to_array_decodes_identically() {
    let names = Value::Array(Array::new().push(Value::Name("CCF".into())));
    let indirect = document(vec![
        stream(Value::Ref(3, 0), ccitt_parms()),
        Object::new(3, 0, names.clone()),
    ]);
    let direct = document(vec![stream(names, ccitt_parms())]);
    assert_eq!(decode(&indirect), decode(&direct));
    assert_eq!(decode(&indirect), vec![0xFF, 0xF0, 0xF0]);
    assert!(
        indirect.warnings().is_empty(),
        "got: {:?}",
        indirect.warnings()
    );
}

/// 形态三：`/DecodeParms` 是引用，指向参数字典（含缩写键 `/DP`）。
#[test]
fn decode_parms_reference_to_dict_decodes_identically() {
    let direct = document(vec![stream(
        Value::Name("CCITTFaxDecode".into()),
        ccitt_parms(),
    )]);
    let indirect = document(vec![
        Object::new(
            1,
            0,
            Value::Stream(
                Dict::new()
                    .insert("Length", Value::Integer(CCITT_DATA.len() as i64))
                    .insert("Filter", Value::Name("CCITTFaxDecode".into()))
                    .insert("DP", Value::Ref(4, 0)),
                StreamData::Owned(CCITT_DATA.to_vec()),
            ),
        ),
        Object::new(4, 0, ccitt_parms()),
    ]);
    assert_eq!(decode(&indirect), decode(&direct));
}

/// 形态四：`/DecodeParms` 是引用，指向「参数字典数组」，且数组元素本身
/// 也是引用（7.3.10 允许数组元素是间接引用）。
#[test]
fn decode_parms_reference_to_array_decodes_identically() {
    let direct = document(vec![stream(
        Value::Name("CCITTFaxDecode".into()),
        Value::Array(Array::new().push(ccitt_parms())),
    )]);
    let indirect = document(vec![
        stream(Value::Name("CCITTFaxDecode".into()), Value::Ref(5, 0)),
        Object::new(5, 0, Value::Array(Array::new().push(Value::Ref(6, 0)))),
        Object::new(6, 0, ccitt_parms()),
    ]);
    assert_eq!(decode(&indirect), decode(&direct));
    assert_eq!(decode(&direct), vec![0xFF, 0xF0, 0xF0]);
}

/// 引用指向字典（既非 Name 也非数组）：保留原值 + 诊断，解码失败。
#[test]
fn filter_reference_to_illegal_type_keeps_value_and_diagnoses() {
    let doc = document(vec![
        stream(Value::Ref(7, 0), ccitt_parms()),
        Object::new(7, 0, Value::Dict(Dict::new())),
    ]);
    let value = doc.get_object(1).unwrap().unwrap().value;
    let error = doc
        .decode_stream_bytes(&value)
        .expect_err("非法 /Filter 形态必须失败");
    // 残留的引用在 `/Crypt` 剥离与筛选器调度两处都会拒绝（前者更靠前）。
    assert!(error.contains("/Filter"), "got: {error}");
    // 原值仍在字典里，字节也仍在流里（不静默丢内容）。
    assert_eq!(
        value.as_dict().expect("stream dict").get("Filter"),
        Some(&Value::Ref(7, 0))
    );
    assert_eq!(doc.try_stream_bytes(&value).unwrap(), CCITT_DATA);
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| warning.contains("7.4.1") && warning.contains("resolves to dict")),
        "got: {:?}",
        doc.warnings()
    );
}

/// 引用成环：保留原值 + 环诊断，解码失败且不无限递归。
#[test]
fn cyclic_filter_reference_is_diagnosed() {
    let doc = document(vec![
        stream(Value::Ref(8, 0), ccitt_parms()),
        Object::new(8, 0, Value::Ref(8, 0)),
    ]);
    let value = doc.get_object(1).unwrap().unwrap().value;
    assert!(doc.decode_stream_bytes(&value).is_err());
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| warning.contains("cyclic")),
        "got: {:?}",
        doc.warnings()
    );
}

/// 引用指向缺失对象：`get_object` 给 `Ok(None)`，同样保留原值 + 诊断。
#[test]
fn filter_reference_to_absent_object_is_diagnosed() {
    let doc = document(vec![stream(Value::Ref(9, 0), ccitt_parms())]);
    let value = doc.get_object(1).unwrap().unwrap().value;
    assert!(doc.decode_stream_bytes(&value).is_err());
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| warning.contains("absent object")),
        "got: {:?}",
        doc.warnings()
    );
}

/// Table 5 的 shall 个数约束在 `Document` 侧同样产出诊断（不改数据）。
#[test]
fn decode_parms_count_mismatch_surfaces_as_warning() {
    let doc = document(vec![stream(
        Value::Array(
            Array::new()
                .push(Value::Name("ASCIIHexDecode".into()))
                .push(Value::Name("CCITTFaxDecode".into())),
        ),
        ccitt_parms(),
    )]);
    let value = doc.get_object(1).unwrap().unwrap().value;
    // 参数字典按 Table 5 只作用于首个过滤器 → CCITT 拿不到参数 → 解码失败，
    // 且诊断里能看到 shall 约束违例。
    assert!(doc.decode_stream_bytes(&value).is_err());
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| warning.contains("one entry for each filter")),
        "got: {:?}",
        doc.warnings()
    );
}

/// `decode_stream_bytes_except`（JPX/JBIG2 透传入口）同样先解引用。
#[test]
fn skip_entry_point_also_resolves_references() {
    let doc = document(vec![
        stream(Value::Ref(2, 0), ccitt_parms()),
        Object::new(2, 0, Value::Name("CCITTFaxDecode".into())),
    ]);
    let value = doc.get_object(1).unwrap().unwrap().value;
    assert_eq!(
        doc.decode_stream_bytes_except(&value, &|name| name == "DCTDecode")
            .expect("decode"),
        vec![0xFF, 0xF0, 0xF0]
    );
}
