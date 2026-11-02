//! FreeText 子类型专有条目读取（Table 174）：全条目、缺省条目与未知名宽容、
//! 引线四/六项、`/LE` 裸名字归属，以及类型化 malformed 错误。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Value};

use super::super::support::*;

#[test]
fn free_text_annotation_reads_table_174_entries() {
    let dict = base_annotation("FreeText")
        .insert("DA", Value::String("/F1 12 Tf 0 g".into()))
        .insert("Q", Value::Integer(1))
        .insert("RC", Value::String("<xml/>".into()))
        .insert(
            "DS",
            Value::String("font: ArialMT 12pt; color:#ff0000;".into()),
        )
        .insert(
            "CL",
            Value::Array(Array(vec![
                Value::Integer(20),
                Value::Integer(100),
                Value::Float(60.5),
                Value::Float(140.0),
                Value::Integer(120),
                Value::Integer(180),
            ])),
        )
        .insert("IT", Value::Name("FreeTextCallout".into()))
        .insert(
            "BE",
            Value::Dict(
                Dict::new()
                    .insert("S", Value::Name("C".into()))
                    .insert("I", Value::Float(1.5)),
            ),
        )
        .insert(
            "RD",
            Value::Array(Array(vec![
                Value::Integer(0),
                Value::Float(2.5),
                Value::Integer(0),
                Value::Float(2.5),
            ])),
        )
        .insert("LE", Value::Name("OpenArrow".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("free text");
    assert_eq!(annotation.subtype, AnnotationSubtype::FreeText);
    assert_eq!(
        annotation.default_appearance.as_deref(),
        Some("/F1 12 Tf 0 g")
    );
    assert_eq!(annotation.quadding, Some(1));
    assert_eq!(
        annotation.rich_contents,
        Some(Value::String("<xml/>".into()))
    );
    assert_eq!(
        annotation.default_style.as_deref(),
        Some("font: ArialMT 12pt; color:#ff0000;")
    );
    assert_eq!(
        annotation.callout_line,
        Some(CalloutLine::Knee([20.0, 100.0, 60.5, 140.0, 120.0, 180.0]))
    );
    assert_eq!(
        annotation.free_text_intent,
        Some(FreeTextIntent::FreeTextCallout)
    );
    assert_eq!(
        annotation.border_effect,
        Some(BorderEffect {
            style: BorderEffectStyle::Cloudy,
            intensity: Some(1.5)
        })
    );
    assert_eq!(annotation.text_rectangle, Some([0.0, 2.5, 0.0, 2.5]));
    assert_eq!(
        annotation.callout_ending,
        Some(LineEndStyle::OpenArrow),
        "裸名字 /LE 只填引线端点样式"
    );
    assert!(
        annotation.line_endings.is_none(),
        "裸名字 /LE 不套用到 Line 的双端点数组"
    );
    assert!(
        annotation.line_intent.is_none(),
        "/IT 命中 FreeText 集合时不填 Line 意图"
    );
}

#[test]
fn free_text_annotation_fills_absent_entries_and_tolerates_unknown_names() {
    let document = doc(vec![catalog_object()]);

    // 只写必填的 `/DA`：其余条目保持 None，缺省由读端调用方按表格处理。
    let dict = base_annotation("FreeText").insert("DA", Value::String("/F1 10 Tf".into()));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("min free text");
    assert_eq!(annotation.default_appearance.as_deref(), Some("/F1 10 Tf"));
    assert_eq!(annotation.quadding, None);
    assert!(annotation.callout_line.is_none());
    assert!(annotation.free_text_intent.is_none());
    assert!(annotation.border_effect.is_none());
    assert!(annotation.text_rectangle.is_none());
    assert!(annotation.callout_ending.is_none());

    // 未知意图与边框效果样式保留原文；缺省 `/S` 视为无效果。
    let dict = base_annotation("FreeText")
        .insert("IT", Value::Name("FreeTextRubber".into()))
        .insert("Q", Value::Float(2.0))
        .insert(
            "BE",
            Value::Dict(Dict::new().insert("S", Value::Name("Z".into()))),
        );
    let annotation = parse_annotation(&document, 0, None, &dict).expect("unknown free text names");
    assert_eq!(
        annotation.free_text_intent,
        Some(FreeTextIntent::Other("FreeTextRubber".into())),
        "两家都不认识的 /IT 按声明的 `/Subtype` 归属落进 FreeText 的 Other"
    );
    assert_eq!(annotation.quadding, Some(2));
    assert_eq!(
        annotation.border_effect,
        Some(BorderEffect {
            style: BorderEffectStyle::Other("Z".into()),
            intensity: None
        })
    );
    assert_eq!(
        annotation.line_intent, None,
        "同一个 /IT 名字只落进一家意图"
    );
}

#[test]
fn free_text_callout_line_accepts_four_or_six_numbers() {
    let document = doc(vec![catalog_object()]);

    let dict = base_annotation("FreeText").insert(
        "CL",
        Value::Array(Array(vec![
            Value::Integer(10),
            Value::Integer(20),
            Value::Float(30.5),
            Value::Integer(40),
        ])),
    );
    let annotation = parse_annotation(&document, 0, None, &dict).expect("straight callout");
    assert_eq!(
        annotation.callout_line,
        Some(CalloutLine::Straight([10.0, 20.0, 30.5, 40.0]))
    );

    // `/CL` 非数组形态由 `parse_number_array` 报类型错误。
    let dict = base_annotation("FreeText").insert("CL", Value::Name("line".into()));
    assert!(
        matches!(
            parse_annotation(&document, 0, None, &dict),
            Err(AnnotationError::Malformed(_))
        ),
        "expect malformed for {dict:?}"
    );
}

#[test]
fn free_text_bare_line_ending_name_fills_callout_ending_only() {
    let document = doc(vec![catalog_object()]);

    // 同一键 `/LE` 的两种形态互不报错：数组归 Line，裸名字归 FreeText。
    let array_form = base_annotation("FreeText").insert(
        "LE",
        Value::Array(Array(vec![
            Value::Name("Square".into()),
            Value::Name("Butt".into()),
        ])),
    );
    let annotation = parse_annotation(&document, 0, None, &array_form).expect("array /LE");
    assert_eq!(
        annotation.line_endings,
        Some([LineEndStyle::Square, LineEndStyle::Butt])
    );
    assert!(annotation.callout_ending.is_none());

    // 非标准端点名仍按 `Other` 保留原文。
    let dict = base_annotation("FreeText").insert("LE", Value::Name("Tapered".into()));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("unknown /LE");
    assert_eq!(
        annotation.callout_ending,
        Some(LineEndStyle::Other("Tapered".into()))
    );
}

#[test]
fn malformed_free_text_entries_are_typed_errors() {
    let document = doc(vec![catalog_object()]);
    let cases = vec![
        // /Q 只能是 0、1、2
        base_annotation("FreeText").insert("Q", Value::Integer(3)),
        base_annotation("FreeText").insert("Q", Value::Float(1.5)),
        base_annotation("FreeText").insert("Q", Value::Name("Center".into())),
        // /CL 只能四或六项
        base_annotation("FreeText").insert(
            "CL",
            Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
                Value::Integer(4),
                Value::Integer(5),
            ])),
        ),
        base_annotation("FreeText").insert("CL", Value::Array(Array(Vec::new()))),
        // /RD 恰好四项且均 ≥0
        base_annotation("FreeText").insert("RD", Value::Array(Array(vec![Value::Integer(1)]))),
        base_annotation("FreeText").insert(
            "RD",
            Value::Array(Array(vec![
                Value::Integer(0),
                Value::Float(-0.5),
                Value::Integer(0),
                Value::Integer(0),
            ])),
        ),
        // /BE 必须是字典，/S 必须是名字，/I 落在 0–2
        base_annotation("FreeText").insert("BE", Value::Array(Array(Vec::new()))),
        base_annotation("FreeText").insert(
            "BE",
            Value::Dict(Dict::new().insert("S", Value::Integer(1))),
        ),
        base_annotation("FreeText").insert(
            "BE",
            Value::Dict(
                Dict::new()
                    .insert("S", Value::Name("C".into()))
                    .insert("I", Value::Float(3.0)),
            ),
        ),
        base_annotation("FreeText").insert(
            "BE",
            Value::Dict(Dict::new().insert("I", Value::String("2".into()))),
        ),
        // /DA /DS 必须是名字或文本串
        base_annotation("FreeText").insert("DA", Value::Array(Array(Vec::new()))),
        base_annotation("FreeText").insert("DS", Value::Integer(7)),
    ];
    for dict in cases {
        assert!(
            matches!(
                parse_annotation(&document, 0, None, &dict),
                Err(AnnotationError::Malformed(_))
            ),
            "expect malformed for {dict:?}"
        );
    }
}
