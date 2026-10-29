//! 子类型专有条目读取（12.5.6）。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Value};

use super::support::*;

#[test]
fn text_annotation_reads_icon_open_flag_and_markup_state() {
    let dict = base_annotation("Text")
        .insert("Name", Value::Name("Paragraph".into()))
        .insert("Open", Value::Bool(false))
        .insert("State", Value::String("Accepted".into()))
        .insert("StateModel", Value::HexString("4D61726B6564".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("text annotation");
    assert_eq!(annotation.icon_name.as_deref(), Some("Paragraph"));
    assert_eq!(annotation.is_open, Some(false));
    assert_eq!(annotation.state.as_deref(), Some("Accepted"));
    assert_eq!(annotation.state_model.as_deref(), Some("Marked"));
    assert_eq!(annotation.symbol, None);
    assert!(annotation.ink_paths.is_empty());
}

#[test]
fn popup_annotation_reads_parent_reference_and_open_flag() {
    let dict = base_annotation("Popup")
        .insert("Parent", Value::Ref(9, 0))
        .insert("Open", Value::Bool(true));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("popup annotation");
    assert_eq!(annotation.parent, Some((9, 0)));
    assert_eq!(annotation.is_open, Some(true));

    let plain = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Popup"),
    )
    .expect("popup without /Parent");
    assert!(plain.parent.is_none());
    assert!(plain.is_open.is_none());

    let bad_parent = base_annotation("Popup").insert("Parent", Value::Dict(Dict::new()));
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_parent),
        Err(AnnotationError::Malformed(_))
    ));
}

#[test]
fn caret_annotation_reads_symbol_and_absent_entries_stay_none() {
    let dict = base_annotation("Caret").insert("Symbol", Value::Name("NewLine".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("caret annotation");
    assert_eq!(annotation.symbol.as_deref(), Some("NewLine"));
    assert!(annotation.icon_name.is_none());
    assert!(annotation.is_open.is_none());
    assert!(annotation.state.is_none());
    assert!(annotation.state_model.is_none());

    // `/Symbol` 也可写成文本串（真实文件偶见），按文本解码。
    let loose = base_annotation("Caret").insert("Symbol", Value::String("Cross".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &loose).expect("loose symbol");
    assert_eq!(annotation.symbol.as_deref(), Some("Cross"));
}

#[test]
fn ink_annotation_reads_ink_list_paths() {
    let dict = base_annotation("Ink").insert(
        "InkList",
        Value::Array(Array(vec![
            Value::Array(Array(vec![
                Value::Integer(10),
                Value::Float(20.5),
                Value::Integer(30),
                Value::Float(40.0),
            ])),
            Value::Array(Array(vec![
                Value::Float(1.5),
                Value::Float(2.5),
                Value::Integer(3),
                Value::Integer(4),
                Value::Integer(5),
                Value::Integer(6),
            ])),
        ])),
    );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("ink annotation");
    assert_eq!(
        annotation.ink_paths,
        vec![
            vec![10.0, 20.5, 30.0, 40.0],
            vec![1.5, 2.5, 3.0, 4.0, 5.0, 6.0]
        ]
    );
}

#[test]
fn line_annotation_reads_all_table_175_entries() {
    let dict = base_annotation("Line")
        .insert(
            "L",
            Value::Array(Array(vec![
                Value::Integer(72),
                Value::Float(700.5),
                Value::Integer(300),
                Value::Integer(650),
            ])),
        )
        .insert(
            "LE",
            Value::Array(Array(vec![
                Value::Name("ClosedArrow".into()),
                Value::Name("OpenArrow".into()),
            ])),
        )
        .insert(
            "IC",
            Value::Array(Array(vec![Value::Float(1.0), Value::Integer(0)])),
        )
        .insert("LL", Value::Float(12.5))
        .insert("LLE", Value::Integer(3))
        .insert("Cap", Value::Bool(true))
        .insert("IT", Value::Name("LineDimension".into()))
        .insert("LLO", Value::Float(6.0))
        .insert("CP", Value::Name("Top".into()))
        .insert(
            "CO",
            Value::Array(Array(vec![Value::Float(-1.5), Value::Integer(20)])),
        );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("line annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::Line);
    assert_eq!(annotation.line_endpoints, Some([72.0, 700.5, 300.0, 650.0]));
    assert_eq!(
        annotation.line_endings,
        Some([LineEndStyle::ClosedArrow, LineEndStyle::OpenArrow])
    );
    assert_eq!(annotation.interior_color, Some(vec![1.0, 0.0]));
    assert_eq!(annotation.leader_length, Some(12.5));
    assert_eq!(annotation.leader_extension, Some(3.0));
    assert_eq!(annotation.caption, Some(true));
    assert_eq!(annotation.line_intent, Some(LineIntent::LineDimension));
    assert_eq!(annotation.leader_line_offset, Some(6.0));
    assert_eq!(annotation.caption_position, Some(CaptionPosition::Top));
    assert_eq!(annotation.caption_offset, Some([-1.5, 20.0]));
}

#[test]
fn line_annotation_fills_absent_arrow_end_style_and_tolerates_unknown_names() {
    let document = doc(vec![catalog_object()]);

    // `/LE` 只写一端：另一端按 Table 175 的缺省 `/None` 补齐。
    let dict =
        base_annotation("Line").insert("LE", Value::Array(Array(vec![Value::Name("Butt".into())])));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("single arrow ending");
    assert_eq!(
        annotation.line_endings,
        Some([LineEndStyle::Butt, LineEndStyle::None])
    );

    // 非标准名保留原文；端点样式、意图与题注位置各自宽容读取。
    let dict = base_annotation("Line")
        .insert(
            "LE",
            Value::Array(Array(vec![
                Value::Name("HexArrow".into()),
                Value::Name("Slash".into()),
            ])),
        )
        .insert("IT", Value::Name("LineCloud".into()))
        .insert("CP", Value::String("Below".into()));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("unknown names");
    assert_eq!(
        annotation.line_endings,
        Some([LineEndStyle::Other("HexArrow".into()), LineEndStyle::Slash])
    );
    assert_eq!(
        annotation.line_intent,
        Some(LineIntent::Other("LineCloud".into()))
    );
    assert_eq!(
        annotation.caption_position,
        Some(CaptionPosition::Other("Below".into()))
    );
    assert!(
        annotation.line_endpoints.is_none(),
        "/L 缺失时不报错，归属校验由写端负责"
    );
    assert!(annotation.leader_length.is_none());
    assert_eq!(annotation.caption, None);
}

#[test]
fn malformed_line_entries_are_typed_errors() {
    let document = doc(vec![catalog_object()]);
    let cases = vec![
        // /L 项数与类型
        base_annotation("Line").insert(
            "L",
            Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ])),
        ),
        base_annotation("Line").insert("L", Value::Integer(4)),
        base_annotation("Line").insert(
            "L",
            Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
                Value::Float(f64::NAN),
            ])),
        ),
        // /LE 项数与名字
        base_annotation("Line").insert("LE", Value::Array(Array(Vec::new()))),
        base_annotation("Line").insert(
            "LE",
            Value::Array(Array(vec![
                Value::Name("Square".into()),
                Value::Name("Circle".into()),
                Value::Name("Diamond".into()),
            ])),
        ),
        base_annotation("Line").insert(
            "LE",
            Value::Array(Array(vec![Value::Integer(1), Value::Integer(2)])),
        ),
        // /Cap 必须是布尔
        base_annotation("Line").insert("Cap", Value::Integer(1)),
        // /LL /LLE /LLO 必须是数值
        base_annotation("Line").insert("LL", Value::Name("long".into())),
        base_annotation("Line").insert("LLE", Value::String("3".into())),
        base_annotation("Line").insert("LLO", Value::Array(Array(vec![Value::Integer(1)]))),
        // /CO 恰好两项
        base_annotation("Line").insert(
            "CO",
            Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ])),
        ),
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

#[test]
fn malformed_subtype_entries_are_typed_errors() {
    let document = doc(vec![catalog_object()]);
    let cases = vec![
        base_annotation("Text").insert("Open", Value::Integer(1)),
        base_annotation("Text").insert("State", Value::Integer(2)),
        base_annotation("Caret").insert("Symbol", Value::Integer(0)),
        base_annotation("Ink").insert("InkList", Value::Integer(3)),
        base_annotation("Ink").insert("InkList", Value::Array(Array(vec![Value::Integer(1)]))),
        base_annotation("Ink").insert(
            "InkList",
            Value::Array(Array(vec![Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]))])),
        ),
        base_annotation("Ink").insert(
            "InkList",
            Value::Array(Array(vec![Value::Array(Array(vec![
                Value::Integer(1),
                Value::Float(f64::NAN),
            ]))])),
        ),
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
