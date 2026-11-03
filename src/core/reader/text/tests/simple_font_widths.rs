//! `Tf` 资源解析的字宽入口（`resolve_fonts` → [`TextState::font_width`]）：
//! Table 111 的 `/Type1` 与 §9.6.2.3 的 `/MMType1` 共用一条宽度查找，
//! 其余简单/组合字体子类型保持取不到宽度（解码行为不变）。

use crate::core::reader::text::fonts::resolve_fonts;
use crate::core::reader::text::state::TextState;
use crate::core::types::{Array, Dict, Document, Object, Value};

use super::support::{document_from_objects, resources_with_font, winansi_type1_font};

/// Table 111 条目齐全的简单字体字典；宽度为 `100 + code`（65..=69）。
fn font_dict(subtype: &str, base_font: &str) -> Value {
    let mut width_codes = Array::new();
    for code in 65..=69 {
        width_codes = width_codes.push(Value::Integer(100 + code));
    }
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name(subtype.into()))
            .insert("BaseFont", Value::Name(base_font.into()))
            .insert("Encoding", Value::Name("WinAnsiEncoding".into()))
            .insert("FirstChar", Value::Integer(65))
            .insert("LastChar", Value::Integer(69))
            .insert("Widths", Value::Array(width_codes))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert("FontName", Value::Name(base_font.into()))
                        .insert("MissingWidth", Value::Integer(500)),
                ),
            ),
    )
}

/// §9.6.2.3 的 MM 实例字典：`/BaseFont` 用下划线代替实例名里的空格。
fn mm_font() -> Value {
    font_dict("MMType1", "MinionMM_366_465_11_")
}

/// 只登记字体资源并跑一遍 `Tf` 解析，返回文本状态。
fn state_with_font(document: &Document, name: &str, font: Value) -> TextState {
    let resources = resources_with_font(name, font);
    let mut state = TextState::new();
    resolve_fonts(&resources, document, &mut state);
    state
}

#[test]
fn tf_font_lookup_resolves_mmtype1_widths_from_page_resources() {
    let document = Document::new();
    let state = state_with_font(&document, "F1", mm_font());

    assert_eq!(state.font_width("F1", 65), Some(165.0));
    assert_eq!(state.font_width("F1", 69), Some(169.0));
    // Table 111 `Widths`：范围外的字符码取描述符的 /MissingWidth。
    assert_eq!(state.font_width("F1", 70), Some(500.0));
    assert_eq!(state.font_width("F2", 65), None, "未登记的资源名无宽度");

    // 同一个 MM 字体仍走既有解码路径提取文本。
    let resources = resources_with_font("F1", mm_font());
    assert_eq!(
        document.extract_text(b"BT /F1 12 Tf (AB) Tj ET", &resources),
        "AB"
    );
}

#[test]
fn tf_font_lookup_resolves_an_indirect_mmtype1_font_dictionary() {
    let document = document_from_objects(vec![Object::new(10, 0, mm_font())]);
    let state = state_with_font(&document, "F1", Value::Ref(10, 0));

    assert_eq!(state.font_width("F1", 67), Some(167.0));
}

/// 回归：`/Type1` 的宽度与本增量之前一样，逐值等于 `Document::type1_font`
/// 的结果，标准 14 内置度量回退也原样保留。
#[test]
fn tf_font_lookup_resolves_type1_widths_identically_to_the_type1_lane() {
    let document = Document::new();

    let explicit = font_dict("Type1", "ExampleType1");
    let lane = document
        .type1_font(&explicit)
        .unwrap()
        .expect("Type1 宽度模型");
    let state = state_with_font(&document, "F1", explicit.clone());
    for code in [64u8, 65, 69, 70] {
        assert_eq!(
            state.font_width("F1", code),
            lane.width_for(code),
            "字符码 {code} 的 Type1 宽度应与 Type1 入口一致"
        );
    }

    // 标准 14 允许省略宽度三件套：回退到内置度量，与 `type1_font` 相同。
    let standard = winansi_type1_font();
    let fallback = document
        .type1_font(&standard)
        .unwrap()
        .expect("标准 14 宽度模型");
    let state = state_with_font(&document, "F1", standard);
    assert_eq!(state.font_width("F1", 65), fallback.width_for(65));
    assert!(
        state.font_width("F1", 65).is_some(),
        "标准 14 的 /Type1 应取到内置宽度"
    );
}

#[test]
fn tf_font_lookup_gives_no_widths_for_other_simple_and_composite_subtypes() {
    let document = Document::new();
    for subtype in ["TrueType", "Type3", "Type0"] {
        let state = state_with_font(&document, "F1", font_dict(subtype, "SomeFont"));
        assert_eq!(
            state.font_width("F1", 65),
            None,
            "/Subtype /{subtype} 不使用 Table 111 宽度入口"
        );
    }
}
