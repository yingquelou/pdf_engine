//! 外部数据字典 `/ExData` 读端测试（13.6.6，Table 170/313）。
//!
//! 覆盖 `/Subtype /Markup3D` 的 `/3DA` `/3DV` `/MD5` 类型化解析、`/Type`
//! 的两种写法（Table 313 的 `ExData` 与 13.6.6 示例的 `Markup3D`）、
//! 文本串命名的目标、未识别子类型的原样保留，以及各类畸形输入的
//! `AnnotationError::Malformed` 契约。

use crate::core::reader::annotation::*;
use crate::core::types::{Dict, Object, Value};

use super::support::*;

/// 最小 3D 视图字典（Table 304 必填 `/XN`），附带可选条目以便断言。
fn view() -> Dict {
    Dict::new()
        .insert("Type", Value::Name("3DView".into()))
        .insert("XN", Value::String("CommentView1".into()))
        .insert("IN", Value::String("Inside".into()))
        .insert("CO", Value::Float(2.5))
}

fn view_value() -> Value {
    Value::Dict(view())
}

/// 把外部数据字典挂到最小注解上并解析。
fn annotation_with_exdata(document: &crate::core::types::Document, ex_data: Dict) -> Annotation {
    let dict = base_annotation("Text").insert("ExData", Value::Dict(ex_data));
    parse_annotation(document, 0, None, &dict).expect("annotation with /ExData")
}

fn malformed(ex_data: Dict) -> AnnotationError {
    let document = doc(vec![catalog_object()]);
    let dict = base_annotation("Text").insert("ExData", Value::Dict(ex_data));
    parse_annotation(&document, 0, None, &dict).expect_err("malformed /ExData")
}

fn assert_malformed(ex_data: Dict, fragment: &str) {
    let error = malformed(ex_data);
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains(fragment)),
        "expected message containing {fragment:?}, got {error:?}"
    );
}

#[test]
fn exdata_reads_markup_3d_entries() {
    // `/3DV` 走间接引用：对象 5 是视图字典。
    let document = doc(vec![catalog_object(), Object::new(5, 0, view_value())]);
    let annotation = annotation_with_exdata(
        &document,
        Dict::new()
            .insert("Type", Value::Name("ExData".into()))
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DA", Value::Ref(4, 0))
            .insert("3DV", Value::Ref(5, 0))
            .insert(
                "MD5",
                Value::HexString("0123456789ABCDEF0123456789ABCDEF".into()),
            ),
    );

    let ex_data = annotation.ex_data.expect("/ExData");
    assert_eq!(ex_data.subtype, ExDataSubtype::Markup3D);
    let markup = ex_data.markup_3d.expect("Markup3D payload");
    assert_eq!(
        markup.annotation,
        Markup3DAnnotation::Reference {
            number: 4,
            generation: 0
        }
    );
    assert_eq!(
        markup.view.external_name,
        Value::String("CommentView1".into())
    );
    assert_eq!(
        markup.view.internal_name,
        Some(Value::String("Inside".into()))
    );
    assert_eq!(markup.view.center_of_orbit, Some(2.5));
    assert_eq!(
        markup.md5,
        Some([
            0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
            0xCD, 0xEF,
        ])
    );
    // 原始字典保留，未建模条目仍可访问。
    assert_eq!(
        ex_data.dict.get("Type").and_then(|value| value.as_name()),
        Some("ExData")
    );
}

#[test]
fn exdata_accepts_markup_3d_type_and_named_target() {
    let document = doc(vec![catalog_object()]);

    // 13.6.6 的示例写 `/Type /Markup3D`，`/3DA` 用文本串命名 3D 注解。
    let annotation = annotation_with_exdata(
        &document,
        Dict::new()
            .insert("Type", Value::Name("Markup3D".into()))
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DA", Value::String("CommentView".into()))
            .insert("3DV", view_value()),
    );
    let markup = annotation
        .ex_data
        .expect("/ExData")
        .markup_3d
        .expect("payload");
    assert_eq!(
        markup.annotation,
        Markup3DAnnotation::Name(Value::String("CommentView".into()))
    );
    assert_eq!(markup.md5, None);

    // 十六进制串同样按名字保留（"CommentView" 的 ASCII 十六进制）。
    let annotation = annotation_with_exdata(
        &document,
        Dict::new()
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DA", Value::HexString("436F6D6D656E7456696577".into()))
            .insert("3DV", view_value()),
    );
    let markup = annotation
        .ex_data
        .expect("/ExData")
        .markup_3d
        .expect("payload");
    assert_eq!(
        markup.annotation,
        Markup3DAnnotation::Name(Value::HexString("436F6D6D656E7456696577".into()))
    );

    // 未识别子类型：保留原名与原始字典，不解析 Markup3D 载荷。
    let annotation = annotation_with_exdata(
        &document,
        Dict::new()
            .insert("Type", Value::Name("ExData".into()))
            .insert("Subtype", Value::Name("Future".into()))
            .insert("Marker", Value::Integer(1)),
    );
    let ex_data = annotation.ex_data.expect("/ExData");
    assert_eq!(ex_data.subtype, ExDataSubtype::Other("Future".into()));
    assert_eq!(ex_data.subtype.name(), "Future");
    assert!(ex_data.markup_3d.is_none());
    assert_eq!(
        ex_data
            .dict
            .get("Marker")
            .and_then(|value| value.as_integer()),
        Some(1)
    );
}

#[test]
fn malformed_exdata_is_a_typed_error() {
    // `/Subtype` 必填且须为名字。
    assert_malformed(
        Dict::new().insert("Type", Value::Name("ExData".into())),
        "missing required /Subtype",
    );
    assert_malformed(
        Dict::new()
            .insert("Type", Value::Name("ExData".into()))
            .insert("Subtype", Value::Integer(1)),
        "/Subtype must be a name",
    );
    // `/Type` 只允许 ExData 或 Markup3D。
    assert_malformed(
        Dict::new()
            .insert("Type", Value::Name("SomethingElse".into()))
            .insert("Subtype", Value::Name("Markup3D".into())),
        "/Type is SomethingElse",
    );
    // `/3DA` 必填且只能是间接引用或文本串。
    assert_malformed(
        Dict::new()
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DV", view_value()),
        "missing required /3DA",
    );
    assert_malformed(
        Dict::new()
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DA", Value::Integer(7))
            .insert("3DV", view_value()),
        "/3DA must be an indirect reference or a text string",
    );
    // `/3DV` 必填且必须解析为字典。
    assert_malformed(
        Dict::new()
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DA", Value::Ref(4, 0)),
        "missing required /3DV",
    );
    assert_malformed(
        Dict::new()
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DA", Value::Ref(4, 0))
            .insert("3DV", Value::Integer(9)),
        "/3DV is not a 3D view dictionary",
    );
    // `/MD5` 须为字节串且恰 16 字节。
    assert_malformed(
        Dict::new()
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DA", Value::Ref(4, 0))
            .insert("3DV", view_value())
            .insert("MD5", Value::Integer(16)),
        "/MD5 must be a literal or hexadecimal byte string",
    );
    assert_malformed(
        Dict::new()
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DA", Value::Ref(4, 0))
            .insert("3DV", view_value())
            .insert("MD5", Value::HexString("AABB".into())),
        "/MD5 must decode to exactly 16 bytes, got 2",
    );
}
