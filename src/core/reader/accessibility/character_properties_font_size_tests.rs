//! Table 332 `FontSize`（14.8.2.4.3）的显示时刻推导单元测试：字号取
//! 既有文本抽取路径（`reader::text`）里「字符显示时刻」的
//! `Tf` 字号 × 文本矩阵 `Tm` 纵向缩放 `hypot(c, d)`，CTM 不折算。

use crate::core::reader::accessibility::CharacterProperties;
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

const CATALOG: i64 = 1;
const PAGES: i64 = 2;
const PAGE: i64 = 3;
const FONT: i64 = 4;
const FORM: i64 = 5;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

/// 单页文档：字体资源为 `/Font << /F1 4 0 R >>`（标准 14 Helvetica，
/// 无描述符，走众所周知值），内容流为给定字节。
fn page_document(content: &[u8]) -> Document {
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(Dict::new().insert("F1", Value::Ref(FONT, 0))),
    );
    page_document_with_resources(resources, content)
}

/// 同 [`page_document`]，但页面 `/Resources` 由调用方给出。
fn page_document_with_resources(resources: Dict, content: &[u8]) -> Document {
    let mut document = Document::new();
    document.set_object(Object::new(
        FONT,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Font"))
                .insert("Subtype", name("Type1"))
                .insert("BaseFont", name("Helvetica")),
        ),
    ));
    document.set_object(Object::new(
        PAGES,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", Value::Array(Array(vec![Value::Ref(PAGE, 0)])))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    document.set_object(Object::new(
        PAGE,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(PAGES, 0))
                .insert("Resources", Value::Dict(resources))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
                ),
        ),
    ));
    document.set_object(Object::new(
        CATALOG,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES, 0)),
        ),
    ));
    document.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    document
}

/// 页面里 `/F1` 的推导结果；`Tf` 选中却没显示串时也要能读到（字号缺席）。
fn property_of(content: &[u8]) -> CharacterProperties {
    let document = page_document(content);
    let collected = document
        .page_font_character_properties(0)
        .unwrap_or_else(|error| panic!("页面推导应成功: {error}"));
    collected
        .get("F1")
        .unwrap_or_else(|| panic!("F1 应被 Tf 选中: {collected:?}"))
        .clone()
}

fn font_size_of(content: &[u8]) -> Option<f64> {
    property_of(content).font_size
}

#[test]
fn identity_text_matrix_keeps_the_tf_size_as_font_size() {
    // 写作端最常见形态：Tm 只平移，字号完全来自 `Tf`。
    assert_eq!(
        font_size_of(b"BT /F1 12 Tf 72 700 Td (Hi) Tj ET"),
        Some(12.0),
        "1 0 0 1 平移的 Tm 不改变高度"
    );
}

#[test]
fn text_matrix_scale_multiplies_the_tf_size() {
    assert_eq!(
        font_size_of(b"BT 2 0 0 2 72 700 Tm /F1 12 Tf (Hi) Tj ET"),
        Some(24.0)
    );
    // 字号写进 Tm、`Tf` 用 1 的形态给出同一个高度。
    assert_eq!(
        font_size_of(b"BT 12 0 0 12 72 700 Tm /F1 1 Tf (Hi) Tj ET"),
        Some(12.0),
        "Table 332 的字面高度只由 a/b/c/d 与 Tf 字号共同决定，与写法无关"
    );
}

#[test]
fn anisotropic_text_matrix_uses_the_height_direction() {
    // 纵向缩放取 hypot(c, d)：横向 3 倍、纵向 0.5 倍时高度是 0.5 倍。
    assert_eq!(
        font_size_of(b"BT 3 0 0 0.5 72 700 Tm /F1 12 Tf (Hi) Tj ET"),
        Some(6.0)
    );
}

#[test]
fn pure_rotation_of_the_text_matrix_keeps_the_size() {
    // 3-4-5 旋转矩阵：hypot(-0.8, 0.6) 恰为 1，纯旋转不改变字面高度。
    assert_eq!(
        font_size_of(b"BT 0.6 0.8 -0.8 0.6 72 700 Tm /F1 10 Tf (Hi) Tj ET"),
        Some(10.0)
    );
}

#[test]
fn mirrored_text_and_negative_tf_size_report_positive_height() {
    // Table 332 要 "a positive number"：负字号与镜像 Tm 只翻转字形，
    // 高度取模。
    assert_eq!(
        font_size_of(b"BT /F1 -12 Tf 72 700 Td (Hi) Tj ET"),
        Some(12.0)
    );
    assert_eq!(
        font_size_of(b"BT 1 0 0 -1 72 700 Tm /F1 9 Tf (Hi) Tj ET"),
        Some(9.0)
    );
}

#[test]
fn zero_singular_or_malformed_matrix_yields_no_font_size() {
    // 规范要求正数：`Tf` 字号为 0、Tm 的纵向分量为 0（奇异矩阵）都推不出
    // 高度，留 None 而不是给出 0。
    assert_eq!(font_size_of(b"BT /F1 0 Tf 72 700 Td (Hi) Tj ET"), None);
    assert_eq!(
        font_size_of(b"BT 1 0 0 0 72 700 Tm /F1 12 Tf (Hi) Tj ET"),
        None
    );
}

#[test]
fn tf_without_numeric_size_operand_derives_no_font_size() {
    // `Tf` 的字号操作数缺失或不是数字时不臆造字号，抽取路径仍容忍该算子
    // （只登记字体、字号留空）。走文本抽取入口断言，不经 `page_content_ops`。
    let document = page_document(b"BT /F1 Tf 72 700 Td (Hi) Tj ET");
    let sizes = document.page_font_effective_sizes(0).expect("抽取应成功");
    assert!(sizes.is_empty(), "缺字号操作数不应登记 FontSize: {sizes:?}");
    let document = page_document(b"BT /F1 (12) Tf 72 700 Td (Hi) Tj ET");
    let sizes = document.page_font_effective_sizes(0).expect("抽取应成功");
    assert!(
        sizes.is_empty(),
        "字号操作数非数字不应登记 FontSize: {sizes:?}"
    );
}

#[test]
fn td_line_movement_preserves_the_matrix_scale() {
    // 9.4.2：`Td` 只把平移并入 Tlm，a/b/c/d 不变，故缩放延续。
    assert_eq!(
        font_size_of(b"BT 3 0 0 3 0 0 Tm 72 700 Td /F1 12 Tf (Hi) Tj ET"),
        Some(36.0)
    );
}

#[test]
fn transformation_matrix_scaling_is_not_folded_in() {
    // 规范只点名「当前文本矩阵」的 a/b/c/d：`cm`（CTM）与表单 `/Matrix`
    // 的缩放按措辞不折算（取舍见模块文档）。
    assert_eq!(
        font_size_of(b"q 2 0 0 2 0 0 cm BT /F1 12 Tf 72 700 Td (Hi) Tj ET Q"),
        Some(12.0)
    );
}

#[test]
fn font_selected_but_never_shown_has_no_font_size() {
    // 14.8.2.4.3 的特征只在字符显示时刻存在：没有显示串就没有 Tm 可言。
    let properties = property_of(b"BT /F1 12 Tf ET");
    assert_eq!(properties.font_size, None);
    assert_eq!(
        properties.font_family.as_deref(),
        Some("Helvetica"),
        "其余 Table 331/332 条目仍按字体字典推导"
    );
}

#[test]
fn replaced_text_section_produces_no_font_size() {
    // `/ActualText` 替换的文本不存在这些特征（14.8.2.4.3），
    // 故被替换区段不给字号。
    assert_eq!(
        font_size_of(b"BT /F1 12 Tf /Span <</ActualText (R)>> BDC 72 700 Td (orig) Tj ET EMC"),
        None
    );
}

#[test]
fn several_sizes_of_one_font_keep_the_largest_observed() {
    // 本模型按资源名键控，无法逐次表达多个字号 → 取最大观测值。
    assert_eq!(
        font_size_of(b"BT /F1 12 Tf 72 700 Td (A) Tj 3 0 0 3 72 600 Tm (B) Tj ET"),
        Some(36.0)
    );
    assert_eq!(
        font_size_of(b"BT 3 0 0 3 72 700 Tm /F1 12 Tf (A) Tj 1 0 0 1 72 600 Tm (B) Tj ET"),
        Some(36.0),
        "取最大而非取首个显示时刻"
    );
}

#[test]
fn page_font_property_by_name_agrees_with_the_page_collection() {
    // 两个页面级入口的 `FontSize` 必须一致（既有往返测试按结构体相等断言）。
    let document = page_document(b"BT 2 0 0 2 72 700 Tm /F1 12 Tf (Hi) Tj ET");
    let collected = document
        .page_font_character_properties(0)
        .expect("页面集合推导应成功");
    let single = document
        .page_font_character_property(0, "F1")
        .expect("按资源名推导应成功");
    assert_eq!(single.font_size, Some(24.0));
    assert_eq!(single, collected["F1"]);
}

#[test]
fn tf_size_survives_q_and_restore_and_form_boundaries() {
    // 9.3.1：`Tf` 属文本状态，`q`/`Q` 与表单 XObject 边界要能还原。
    assert_eq!(
        font_size_of(b"BT 2 0 0 2 0 0 Tm q /F1 12 Tf 72 700 Td (Hi) Tj Q ET"),
        Some(24.0)
    );
}

#[test]
fn show_inside_form_xobject_resets_the_text_matrix() {
    // 文本对象不跨流（9.4.1）：进入表单后 Tm 归零，表单 `/Matrix` 与调用方
    // Tm 的缩放都不带入字号。
    let resources = Dict::new()
        .insert(
            "Font",
            Value::Dict(Dict::new().insert("F1", Value::Ref(FONT, 0))),
        )
        .insert(
            "XObject",
            Value::Dict(Dict::new().insert("Fm0", Value::Ref(FORM, 0))),
        );
    let document = page_document_with_resources(resources, b"q 3 0 0 3 0 0 cm /Fm0 Do Q");
    document.set_object(Object::new(
        FORM,
        0,
        Value::Stream(
            Dict::new()
                .insert("Type", name("XObject"))
                .insert("Subtype", name("Form"))
                .insert(
                    "BBox",
                    Value::Array(Array(vec![
                        Value::Integer(0),
                        Value::Integer(0),
                        Value::Integer(100),
                        Value::Integer(100),
                    ])),
                )
                .insert(
                    "Matrix",
                    Value::Array(Array(vec![
                        Value::Integer(2),
                        Value::Integer(0),
                        Value::Integer(0),
                        Value::Integer(2),
                        Value::Integer(0),
                        Value::Integer(0),
                    ])),
                ),
            StreamData::Owned(b"BT /F1 12 Tf 10 10 Td (form) Tj ET".to_vec()),
        ),
    ));
    let sizes = document
        .page_font_effective_sizes(0)
        .expect("表单内显示应登记字号");
    assert_eq!(sizes.get("F1"), Some(&12.0), "Tm 归零后只剩 Tf 字号");
}
