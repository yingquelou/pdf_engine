//! `Document::simple_font_widths`：Table 111 布局（`/Type1` 与 §9.6.2.3 的
//! `/MMType1` 实例）共用的字宽查找入口，以及非 MM 字体保持原行为。

use crate::core::types::{Array, Dict, Document, Value};

use super::{FontError, SimpleFontWidths};

/// `/Widths` 数组：每个字符码的宽度为 `100 + code`，便于逐码断言。
fn widths(first: i64, last: i64) -> Value {
    let mut array = Array::new();
    for code in first..=last {
        array = array.push(Value::Integer(100 + code));
    }
    Value::Array(array)
}

fn descriptor(missing_width: bool) -> Value {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("FontDescriptor".into()))
        .insert("Ascent", Value::Integer(750));
    if missing_width {
        dict = dict.insert("MissingWidth", Value::Integer(500));
    }
    Value::Dict(dict)
}

/// 一份合法的简单字体字典：Table 111 条目齐全，只有 `/Subtype` 随调用方给定。
///
/// `BASE` 的下划线命名约定（§9.6.2.3）只对 MM 有意义，此处按字面保存。
fn font_dict(subtype: &str, base_font: &str, missing_width: bool) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name(subtype.into()))
        .insert("BaseFont", Value::Name(base_font.into()))
        .insert("FirstChar", Value::Integer(32))
        .insert("LastChar", Value::Integer(34))
        .insert("Widths", widths(32, 34))
        .insert("FontDescriptor", descriptor(missing_width))
}

/// 去掉宽度条目（`/FirstChar`、`/LastChar`、`/Widths`）后的字典。
fn without_widths(dict: Dict) -> Dict {
    Dict(
        dict.0
            .into_iter()
            .filter(|(key, _)| key != "Widths" && key != "FirstChar" && key != "LastChar")
            .collect(),
    )
}

fn resolve(document: &Document, dict: &Dict) -> Option<SimpleFontWidths> {
    document
        .simple_font_widths(&Value::Dict(dict.clone()))
        .unwrap()
}

#[test]
fn simple_font_widths_returns_the_type1_lane_result_unchanged() {
    let document = Document::new();
    let value = Value::Dict(font_dict("Type1", "ExampleType1", true));
    let expected = document.type1_font(&value).unwrap();

    let resolved = document.simple_font_widths(&value).unwrap();
    match resolved {
        Some(SimpleFontWidths::Type1(ref font)) => assert_eq!(Some(font.clone()), expected),
        other => panic!("/Type1 应落在 Type1 分支: {other:?}"),
    }
    let font = resolved.expect("Type1 宽度");
    assert_eq!(font.base_font(), "ExampleType1");
    assert_eq!(font.width_for(32), Some(132.0));
    assert_eq!(font.width_for(34), Some(134.0));
    assert_eq!(font.width_for(31), Some(500.0));
}

#[test]
fn simple_font_widths_resolves_mmtype1_instance_widths() {
    let document = Document::new();
    let dict = font_dict("MMType1", "MinionMM_366_465_11_", true);
    let font = resolve(&document, &dict).expect("MMType1 宽度应可解析");

    assert_eq!(font.base_font(), "MinionMM_366_465_11_");
    assert_eq!(font.width_for(32), Some(132.0));
    assert_eq!(font.width_for(34), Some(134.0));
    // 覆盖范围外回退描述符 /MissingWidth（Table 111 `Widths` 条目语义）。
    assert_eq!(font.width_for(31), Some(500.0));
    assert_eq!(
        font,
        SimpleFontWidths::MultipleMaster(
            document
                .mmtype1_font(&Value::Dict(dict))
                .unwrap()
                .expect("MMType1 字典")
        )
    );
}

/// §9.6.2.2 的标准 14 豁免只属 `/Type1`：同一份条目换成 MM `/Subtype` 后，
/// 宽度数组之外只能回退 `/MissingWidth`，不得取内置标准 14 宽度。
#[test]
fn simple_font_widths_withholds_the_standard_14_fallback_from_mmtype1() {
    let document = Document::new();

    let type1 = resolve(&document, &font_dict("Type1", "Helvetica", false)).expect("Type1 宽度");
    assert!(
        type1.width_for(65).is_some(),
        "标准 14 的 /Type1 在数组外仍取内置宽度"
    );

    let mm = resolve(&document, &font_dict("MMType1", "Helvetica", false)).expect("MMType1 宽度");
    assert_eq!(mm.width_for(65), None, "MM 路径不套用标准 14 豁免");
    assert_eq!(mm.width_for(33), Some(133.0), "数组内的 MM 宽度照常给出");
}

#[test]
fn simple_font_widths_reports_other_subtypes_as_the_type1_lane_did() {
    let document = Document::new();
    for subtype in ["TrueType", "Type3", "Type0", "CIDFontType0"] {
        let dict = font_dict(subtype, "SomeFont", true)
            .insert("Encoding", Value::Name("WinAnsiEncoding".into()));
        assert_eq!(
            resolve(&document, &dict),
            None,
            "/Subtype /{subtype} 不走 Table 111 宽度入口"
        );
    }
}

#[test]
fn simple_font_widths_keeps_the_type1_errors_without_consulting_the_mm_lane() {
    let document = Document::new();

    let error = document
        .simple_font_widths(&Value::Array(Array::new()))
        .expect_err("非字典应报错");
    assert!(
        matches!(&error, FontError::Invalid(message) if message == "font value is not a dictionary"),
        "{error}"
    );

    let malformed = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name("Type1".into()))
        .insert("BaseFont", Value::Name("ExampleType1".into()));
    let error = document
        .simple_font_widths(&Value::Dict(malformed))
        .expect_err("非标准 14 的 /Type1 缺宽度应报错");
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("/FirstChar")),
        "{error}"
    );
}

/// `/MMType1` 缺 Table 111 的宽度三件套时按 MM 入口报类型化错误，
/// 不套用标准 14 的省略豁免，也不回退到其他模型。
#[test]
fn simple_font_widths_reports_mmtype1_missing_width_table_as_error() {
    let document = Document::new();
    let error = document
        .simple_font_widths(&Value::Dict(without_widths(font_dict(
            "MMType1",
            "MinionMM_366_465_11_",
            true,
        ))))
        .expect_err("MMType1 必须自带宽度表");
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("/FirstChar")),
        "{error}"
    );
}
