//! Printer's mark 注解专有条目读取（14.11.3，Table 362/363）：`/MN`
//! 标记名称与 `/AP`/`/AS` 选定的表单 `/MarkStyle`、`/Colorants`。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Object, StreamData, Value};

use super::support::*;

/// 构造带指定条目的 Form XObject 流；`entries` 覆盖同名默认条目。
fn form_object(num: i64, entries: Dict) -> Object {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("XObject".into()))
        .insert("Subtype", Value::Name("Form".into()))
        .insert("Length", Value::Integer(0));
    for (key, value) in entries.0 {
        dict = dict.insert(key, value);
    }
    Object::new(num, 0, Value::Stream(dict, StreamData::Owned(Vec::new())))
}

/// Table 363 `/Colorants` 值：色料名 + DeviceGray 备用空间 + Type 2 tint。
fn separation(colorant: &str) -> Value {
    Value::Array(Array(vec![
        Value::Name("Separation".into()),
        Value::Name(colorant.into()),
        Value::Name("DeviceGray".into()),
        Value::Dict(
            Dict::new()
                .insert("FunctionType", Value::Integer(2))
                .insert(
                    "Domain",
                    Value::Array(Array(vec![Value::Integer(0), Value::Integer(1)])),
                )
                .insert("C0", Value::Array(Array(vec![Value::Float(0.0)])))
                .insert("C1", Value::Array(Array(vec![Value::Float(1.0)])))
                .insert("N", Value::Float(1.0)),
        ),
    ]))
}

fn appearance(reference: Value) -> Value {
    Value::Dict(Dict::new().insert("N", reference))
}

fn assert_malformed(error: AnnotationError, needle: &str) {
    match error {
        AnnotationError::Malformed(message) => {
            assert!(
                message.contains(needle),
                "expected {needle:?} in {message:?}"
            );
        }
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn printer_mark_reads_name_and_form_entries() {
    let form = form_object(
        10,
        Dict::new()
            .insert("MarkStyle", Value::String("Cut".into()))
            .insert(
                "Colorants",
                Value::Dict(Dict::new().insert("Cyan", separation("Cyan"))),
            ),
    );
    let document = doc(vec![catalog_object(), form]);
    let dict = base_annotation("PrinterMark")
        .insert("F", Value::Integer(68))
        .insert("MN", Value::Name("Registration".into()))
        .insert("AP", appearance(Value::Ref(10, 0)));
    let annotation =
        parse_annotation(&document, 0, None, &dict).expect("printer's mark annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::PrinterMark);
    let mark = annotation
        .printer_mark
        .expect("/MN and form entries present");
    assert_eq!(mark.mark_name.as_deref(), Some("Registration"));
    assert_eq!(mark.form.reference, Some((10, 0)));
    assert_eq!(mark.form.mark_style.as_deref(), Some("Cut"));
    assert_eq!(mark.form.colorants.len(), 1);
    assert_eq!(mark.form.colorants["Cyan"].colorant, "Cyan");
    assert_eq!(
        mark.form.dict.get("Subtype"),
        Some(&Value::Name("Form".into()))
    );
}

#[test]
fn printer_mark_selects_state_appearance_via_as() {
    let document = doc(vec![
        catalog_object(),
        form_object(10, Dict::new()),
        form_object(
            11,
            Dict::new().insert("MarkStyle", Value::String("Cut".into())),
        ),
    ]);
    let dict = base_annotation("PrinterMark")
        .insert("F", Value::Integer(68))
        .insert("AS", Value::Name("Off".into()))
        .insert(
            "AP",
            appearance(Value::Dict(
                Dict::new()
                    .insert("On", Value::Ref(10, 0))
                    .insert("Off", Value::Ref(11, 0)),
            )),
        );
    let annotation =
        parse_annotation(&document, 0, None, &dict).expect("state sub-dictionary appearance");
    let mark = annotation.printer_mark.expect("printer's mark annotation");
    assert_eq!(
        mark.form.reference,
        Some((11, 0)),
        "/AS 选中的状态流决定 reference"
    );
    assert_eq!(mark.form.mark_style.as_deref(), Some("Cut"));
    assert_eq!(mark.mark_name, None);
}

#[test]
fn printer_mark_without_optional_entries_reads_form_only() {
    // 直接流：`/AP /N` 即表单，无 `/MN` `/MarkStyle` `/Colorants`。
    let document = doc(vec![catalog_object()]);
    let direct = base_annotation("PrinterMark")
        .insert("F", Value::Integer(68))
        .insert(
            "AP",
            appearance(Value::Stream(
                Dict::new().insert("Subtype", Value::Name("Form".into())),
                StreamData::Owned(Vec::new()),
            )),
        );
    let annotation = parse_annotation(&document, 0, None, &direct).expect("direct form appearance");
    let mark = annotation.printer_mark.expect("printer's mark annotation");
    assert_eq!(mark.mark_name, None);
    assert_eq!(mark.form.reference, None, "直接流没有间接引用");
    assert_eq!(mark.form.mark_style, None);
    assert!(mark.form.colorants.is_empty());

    // 单状态子字典且无 `/AS`：自动选中唯一条目。
    let document = doc(vec![catalog_object(), form_object(10, Dict::new())]);
    let single = base_annotation("PrinterMark")
        .insert("F", Value::Integer(68))
        .insert(
            "AP",
            appearance(Value::Dict(Dict::new().insert("Only", Value::Ref(10, 0)))),
        );
    let annotation =
        parse_annotation(&document, 0, None, &single).expect("single state appearance");
    let mark = annotation.printer_mark.expect("printer's mark annotation");
    assert_eq!(mark.form.reference, Some((10, 0)));
}

#[test]
fn malformed_printer_mark_entries_are_typed_errors() {
    let document = doc(vec![
        catalog_object(),
        form_object(10, Dict::new()),
        form_object(11, Dict::new().insert("MarkStyle", Value::Integer(7))),
        form_object(
            12,
            Dict::new().insert("Subtype", Value::Name("Image".into())),
        ),
        form_object(
            13,
            Dict::new().insert(
                "Colorants",
                Value::Dict(Dict::new().insert("Magenta", separation("Cyan"))),
            ),
        ),
    ]);
    let base = || {
        base_annotation("PrinterMark")
            .insert("F", Value::Integer(68))
            .insert("AP", appearance(Value::Ref(10, 0)))
    };

    // `/MN` 须为 name。
    let dict = base().insert("MN", Value::String("Registration".into()));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("non-name /MN"),
        "/MN",
    );

    // `/MN` 只属于 PrinterMark 注解。
    let dict = base_annotation("Text").insert("MN", Value::Name("Registration".into()));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("/MN on Text"),
        "/MN",
    );

    // `/F` 与 `/AP`（含 `/N`）对 PrinterMark 为必填。
    let dict = base_annotation("PrinterMark").insert("AP", appearance(Value::Ref(10, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("missing /F"),
        "/F",
    );
    let dict = base_annotation("PrinterMark").insert("F", Value::Integer(68));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("missing /AP"),
        "/AP",
    );
    let dict = base().insert("AP", Value::Dict(Dict::new()));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("missing /AP /N"),
        "/N",
    );
    let dict = base().insert("AP", Value::Integer(4));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("non-dictionary /AP"),
        "/AP",
    );

    // `/AS` 须为 name 且命中状态键；流外观不接受 `/AS`。
    let dict = base()
        .insert(
            "AP",
            appearance(Value::Dict(Dict::new().insert("On", Value::Ref(10, 0)))),
        )
        .insert("AS", Value::Name("Off".into()));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("/AS misses state"),
        "unknown appearance state",
    );
    let dict = base().insert("AS", Value::Name("On".into()));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("/AS with stream /N"),
        "/AS",
    );

    // 多状态无 `/AS` 与空状态字典均为错误。
    let dict = base().insert(
        "AP",
        appearance(Value::Dict(
            Dict::new()
                .insert("On", Value::Ref(10, 0))
                .insert("Off", Value::Ref(10, 0)),
        )),
    );
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("multiple states without /AS"),
        "multiple appearances",
    );
    let dict = base().insert("AP", appearance(Value::Dict(Dict::new())));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("empty state dictionary"),
        "empty",
    );

    // 选中的外观须为 Form XObject；`/MarkStyle` 须为文本串；
    // `/Colorants` 键与 Separation 色料名须一致。
    let dict = base().insert("AP", appearance(Value::Ref(12, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("/Subtype not Form"),
        "/Subtype",
    );
    let dict = base().insert("AP", appearance(Value::Ref(11, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("non-string /MarkStyle"),
        "/MarkStyle",
    );
    let dict = base().insert("AP", appearance(Value::Ref(13, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("colorant key mismatch"),
        "/Colorants",
    );
}
