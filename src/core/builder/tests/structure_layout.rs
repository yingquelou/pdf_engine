//! Layout 标准属性写端测试（ISO 32000-1 14.8.5.4 Table 342–346）。
//!
//! 覆盖块级、行内与表格单元条目的构造 → 写出 → 解析往返，缺省值不物化，
//! 值形态与元素适用性校验，`/ClassMap` 仅形态检查，以及 PDF 1.4/1.5/1.6
//! 版本门禁；失败均要求零字节输出。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::reader::structure::{
    BlockAlign as ReaderBlockAlign, BorderColor as ReaderBorderColor,
    BorderStyle as ReaderBorderStyle, BorderStyles as ReaderBorderStyles,
    BorderThickness as ReaderBorderThickness, GlyphOrientationVertical as ReaderGlyphOrientation,
    InlineAlign as ReaderInlineAlign, LayoutAttributes as ReaderLayoutAttributes,
    LineHeight as ReaderLineHeight, NumberOrAuto as ReaderNumberOrAuto,
    NumberOrFour as ReaderNumberOrFour, NumberOrNumbers as ReaderNumberOrNumbers,
    Placement as ReaderPlacement, TextAlign as ReaderTextAlign,
    TextDecorationType as ReaderTextDecorationType, WritingMode as ReaderWritingMode,
};
use crate::core::types::{Array, Document, PdfFeature, PdfVersion};

/// 写出后重新解析；结构树只在 `build` 的导航后处理中物化。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-structure-layout-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn assert_invalid(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

fn assert_version_error(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

fn one_page(version: PdfVersion) -> PdfBuilder {
    let mut b = PdfBuilder::with_version(version);
    b.page(PageSize::A4);
    b
}

fn reader_features(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}

/// 单个结构元素 + Layout 属性 + MCR 的最小结构树构建器。
fn builder_with(element: StructureElement, version: PdfVersion) -> PdfBuilder {
    let tree = StructureTree::new().with_element(element);
    let mut b = one_page(version);
    b.set_structure_tree(tree);
    b
}

fn assert_invalid_element(element: StructureElement, expected: &str) {
    let mut b = builder_with(element, PdfVersion::V1_7);
    assert_invalid(&mut b, expected);
}

fn raw_layout(entries: Dict) -> StructureAttribute {
    let mut dict = Dict::new().insert("O", Value::Name("Layout".into()));
    for (key, value) in entries.0 {
        dict = dict.insert(key, value);
    }
    StructureAttribute::new(Value::Dict(dict))
}

fn number_array(values: [f64; 3]) -> Value {
    let mut array = Array::new();
    for value in values {
        array = array.push(Value::Float(value));
    }
    Value::Array(array)
}

fn value_array(values: Vec<Value>) -> Value {
    let mut array = Array::new();
    for value in values {
        array = array.push(value);
    }
    Value::Array(array)
}

#[test]
fn layout_block_attributes_roundtrip_through_parser() {
    let attributes = LayoutAttributes::new()
        .with_placement(Placement::Block)
        .with_writing_mode(WritingMode::RlTb)
        .with_background_color([0.9, 0.8, 0.7])
        .with_border_color(BorderColor::Uniform([1.0, 0.0, 0.0]))
        .with_border_style(BorderStyles::Edges([
            Some(BorderStyle::Solid),
            None,
            Some(BorderStyle::Dashed),
            None,
        ]))
        .with_border_thickness(BorderThickness::Edges([Some(1.0), None, Some(0.5), None]))
        .with_color([0.0, 0.0, 0.0])
        .with_padding(NumberOrFour::Edges([2.0, 3.0, 4.0, 5.0]))
        .with_space_before(12.0)
        .with_space_after(6.0)
        .with_start_indent(18.0)
        .with_end_indent(9.0)
        .with_text_indent(-4.0)
        .with_text_align(TextAlign::Justify)
        .with_bbox([10.0, 20.0, 300.0, 400.0]);
    let element = StructureElement::new("P")
        .with_attribute(StructureAttribute::layout(attributes))
        .with_marked_content(0, 0);
    let mut b = builder_with(element, PdfVersion::V1_5);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let standard = tree.standard_attributes(0).expect("standard attributes");
    let layout = standard.layout.expect("Layout attributes");
    assert!(standard.list.is_none());
    assert!(standard.print_field.is_none());
    assert!(standard.table.is_none());
    assert!(standard.other.is_empty(), "Layout 不应归入 other");
    assert_eq!(layout.placement, Some(ReaderPlacement::Block));
    assert_eq!(layout.writing_mode, Some(ReaderWritingMode::RlTb));
    assert_eq!(layout.background_color, Some([0.9, 0.8, 0.7]));
    assert_eq!(
        layout.border_color,
        Some(ReaderBorderColor::Uniform([1.0, 0.0, 0.0]))
    );
    assert_eq!(
        layout.border_style,
        Some(ReaderBorderStyles::Edges([
            Some(ReaderBorderStyle::Solid),
            None,
            Some(ReaderBorderStyle::Dashed),
            None,
        ]))
    );
    assert_eq!(
        layout.border_thickness,
        Some(ReaderBorderThickness::Edges([
            Some(1.0),
            None,
            Some(0.5),
            None,
        ]))
    );
    assert_eq!(layout.color, Some([0.0, 0.0, 0.0]));
    assert_eq!(
        layout.padding,
        Some(ReaderNumberOrFour::Edges([2.0, 3.0, 4.0, 5.0]))
    );
    assert_eq!(layout.space_before, Some(12.0));
    assert_eq!(layout.space_after, Some(6.0));
    assert_eq!(layout.start_indent, Some(18.0));
    assert_eq!(layout.end_indent, Some(9.0));
    assert_eq!(layout.text_indent, Some(-4.0));
    assert_eq!(layout.text_align, Some(ReaderTextAlign::Justify));
    assert_eq!(layout.bbox, Some([10.0, 20.0, 300.0, 400.0]));

    let features = reader_features(&doc);
    assert!(
        features.contains(&PdfFeature::StandardStructureAttributes),
        "{features:?}"
    );
    assert!(
        features.contains(&PdfFeature::LayoutExtendedAttributes),
        "{features:?}"
    );
    assert!(
        !features.contains(&PdfFeature::LayoutColumnAttributes),
        "{features:?}"
    );
}

#[test]
fn layout_inline_attributes_roundtrip_through_parser() {
    // `/Placement /Before` 的 ILSE 按 BLSE 处理，允许 SpaceBefore。
    let attributes = LayoutAttributes::new()
        .with_placement(Placement::Before)
        .with_space_before(3.0)
        .with_line_height(LineHeight::Number(1.5))
        .with_baseline_shift(-2.0)
        .with_text_decoration_type(TextDecorationType::Underline)
        .with_text_decoration_color([0.2, 0.3, 0.4])
        .with_text_decoration_thickness(0.75)
        .with_glyph_orientation_vertical(GlyphOrientationVertical::Angle(90.0));
    let element = StructureElement::new("Span")
        .with_attribute(StructureAttribute::layout(attributes))
        .with_marked_content(0, 1);
    let mut b = builder_with(element, PdfVersion::V1_5);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let layout = tree
        .standard_attributes(0)
        .expect("standard attributes")
        .layout
        .expect("Layout attributes");
    assert_eq!(layout.placement, Some(ReaderPlacement::Before));
    assert_eq!(layout.space_before, Some(3.0));
    assert_eq!(layout.line_height, Some(ReaderLineHeight::Number(1.5)));
    assert_eq!(layout.baseline_shift, Some(-2.0));
    assert_eq!(
        layout.text_decoration_type,
        Some(ReaderTextDecorationType::Underline)
    );
    assert_eq!(layout.text_decoration_color, Some([0.2, 0.3, 0.4]));
    assert_eq!(layout.text_decoration_thickness, Some(0.75));
    assert_eq!(
        layout.glyph_orientation_vertical,
        Some(ReaderGlyphOrientation::Angle(90.0))
    );
    let features = reader_features(&doc);
    assert!(
        features.contains(&PdfFeature::LayoutExtendedAttributes),
        "{features:?}"
    );
}

#[test]
fn layout_table_cell_attributes_roundtrip_through_parser() {
    let attributes = LayoutAttributes::new()
        .with_width(NumberOrAuto::Number(120.0))
        .with_height(NumberOrAuto::Auto)
        .with_block_align(BlockAlign::Middle)
        .with_inline_align(InlineAlign::Center)
        .with_t_border_style(BorderStyles::Edges([
            Some(BorderStyle::Solid),
            Some(BorderStyle::Hidden),
            None,
            Some(BorderStyle::Double),
        ]))
        .with_t_padding(NumberOrFour::Edges([1.0, 2.0, 3.0, 4.0]));
    let element = StructureElement::new("TH")
        .with_attribute(StructureAttribute::layout(attributes))
        .with_marked_content(0, 2);
    let mut b = builder_with(element, PdfVersion::V1_5);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let layout = tree
        .standard_attributes(0)
        .expect("standard attributes")
        .layout
        .expect("Layout attributes");
    assert_eq!(layout.width, Some(ReaderNumberOrAuto::Number(120.0)));
    assert_eq!(layout.height, Some(ReaderNumberOrAuto::Auto));
    assert_eq!(layout.block_align, Some(ReaderBlockAlign::Middle));
    assert_eq!(layout.inline_align, Some(ReaderInlineAlign::Center));
    assert_eq!(
        layout.t_border_style,
        Some(ReaderBorderStyles::Edges([
            Some(ReaderBorderStyle::Solid),
            Some(ReaderBorderStyle::Hidden),
            None,
            Some(ReaderBorderStyle::Double),
        ]))
    );
    assert_eq!(
        layout.t_padding,
        Some(ReaderNumberOrFour::Edges([1.0, 2.0, 3.0, 4.0]))
    );
}

#[test]
fn layout_column_attributes_roundtrip_through_parser() {
    let attributes = LayoutAttributes::new()
        .with_column_count(2)
        .with_column_gap(NumberOrNumbers::Array(vec![10.0, 20.0]))
        .with_column_widths(NumberOrNumbers::Number(80.0));
    let element = StructureElement::new("Div")
        .with_attribute(StructureAttribute::layout(attributes))
        .with_marked_content(0, 3);
    let mut b = builder_with(element, PdfVersion::V1_6);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let layout = tree
        .standard_attributes(0)
        .expect("standard attributes")
        .layout
        .expect("Layout attributes");
    assert_eq!(layout.column_count, Some(2));
    assert_eq!(
        layout.column_gap,
        Some(ReaderNumberOrNumbers::Array(vec![10.0, 20.0]))
    );
    assert_eq!(
        layout.column_widths,
        Some(ReaderNumberOrNumbers::Number(80.0))
    );
    let features = reader_features(&doc);
    assert!(
        features.contains(&PdfFeature::StandardStructureAttributes),
        "{features:?}"
    );
    assert!(
        features.contains(&PdfFeature::LayoutColumnAttributes),
        "{features:?}"
    );
    assert!(
        !features.contains(&PdfFeature::LayoutExtendedAttributes),
        "{features:?}"
    );
}

#[test]
fn layout_defaults_are_not_materialised() {
    let element = StructureElement::new("P")
        .with_attribute(StructureAttribute::layout(LayoutAttributes::new()))
        .with_marked_content(0, 0);
    let mut b = builder_with(element, PdfVersion::V1_4);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let layout = tree
        .standard_attributes(0)
        .expect("standard attributes")
        .layout
        .expect("Layout attributes");
    assert_eq!(layout.dict.0.len(), 1, "只应写出 /O：{:?}", layout.dict);
    assert_eq!(
        layout,
        ReaderLayoutAttributes {
            dict: layout.dict.clone(),
            ..ReaderLayoutAttributes::default()
        }
    );
    // 空 Layout 属性对象没有可识别的布局条目，扫描器不登记标准属性。
    let features = reader_features(&doc);
    assert!(
        !features.contains(&PdfFeature::StandardStructureAttributes),
        "{features:?}"
    );
}

#[test]
fn layout_shape_violations_fail_before_writing_bytes() {
    let cases = [
        (
            Dict::new().insert("BackgroundColor", number_array([2.0, 0.0, 0.0])),
            "BackgroundColor entry 0 must be a number in the range 0.0 to 1.0",
        ),
        (
            Dict::new().insert(
                "BorderStyle",
                value_array(vec![
                    Value::Name("Solid".into()),
                    Value::Name("Solid".into()),
                    Value::Name("Solid".into()),
                ]),
            ),
            "BorderStyle must be a name or an array of four names",
        ),
        (
            Dict::new().insert(
                "Padding",
                value_array(vec![
                    Value::Integer(1),
                    Value::Integer(2),
                    Value::Integer(3),
                ]),
            ),
            "Padding must be a number or an array of four numbers",
        ),
        (
            Dict::new().insert("ColumnCount", Value::Integer(0)),
            "ColumnCount must be an integer of at least 1",
        ),
        (
            Dict::new().insert("GlyphOrientationVertical", Value::Float(45.0)),
            "GlyphOrientationVertical must be a multiple of 90 between -180 and 360",
        ),
        (
            Dict::new().insert("TextDecorationThickness", Value::Float(-1.0)),
            "TextDecorationThickness must be a non-negative number",
        ),
        (
            Dict::new().insert("BorderThickness", Value::Float(f64::NAN)),
            "BorderThickness must be a finite non-negative number",
        ),
    ];
    for (entries, expected) in cases {
        let element = StructureElement::new("P")
            .with_attribute(raw_layout(entries))
            .with_marked_content(0, 0);
        assert_invalid_element(element, expected);
    }
}

#[test]
fn layout_applicability_violations_fail_before_writing_bytes() {
    // SpaceBefore 只能用于 BLSE 或显式非 Inline 放置的 ILSE。
    let element = StructureElement::new("Span")
        .with_attribute(StructureAttribute::layout(
            LayoutAttributes::new()
                .with_placement(Placement::Inline)
                .with_space_before(4.0),
        ))
        .with_marked_content(0, 0);
    assert_invalid_element(
        element,
        "SpaceBefore applies only to block-level structure elements or inline-level structure elements with a non-Inline /Placement (resolved type Span)",
    );

    // Width/Height 只用于插图、表格与表格单元；经 /RoleMap 解析后判定。
    let element = StructureElement::new("P")
        .with_attribute(StructureAttribute::layout(
            LayoutAttributes::new().with_width(NumberOrAuto::Number(10.0)),
        ))
        .with_marked_content(0, 0);
    assert_invalid_element(
        element,
        "Width applies only to Figure or Formula or Form or Table or TH or TD structure elements (resolved type P)",
    );

    let tree = StructureTree::new()
        .with_role("MySpan", "Span")
        .with_element(
            StructureElement::new("MySpan")
                .with_attribute(StructureAttribute::layout(
                    LayoutAttributes::new().with_width(NumberOrAuto::Number(10.0)),
                ))
                .with_marked_content(0, 0),
        );
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree);
    assert_invalid(
        &mut b,
        "Width applies only to Figure or Formula or Form or Table or TH or TD structure elements (resolved type Span)",
    );

    // ColumnCount 只用于 Art/Sect/Div。
    let element = StructureElement::new("P")
        .with_attribute(StructureAttribute::layout(
            LayoutAttributes::new().with_column_count(2),
        ))
        .with_marked_content(0, 0);
    assert_invalid_element(
        element,
        "ColumnCount applies only to Art or Sect or Div structure elements (resolved type P)",
    );
}

#[test]
fn layout_version_gates_are_zero_byte() {
    let mut b = builder_with(
        StructureElement::new("P")
            .with_attribute(StructureAttribute::layout(
                LayoutAttributes::new().with_space_before(6.0),
            ))
            .with_marked_content(0, 0),
        PdfVersion::V1_3,
    );
    assert_version_error(&mut b, "standard structure attributes require PDF 1.4");

    let mut b = builder_with(
        StructureElement::new("P")
            .with_attribute(StructureAttribute::layout(
                LayoutAttributes::new().with_background_color([1.0, 1.0, 1.0]),
            ))
            .with_marked_content(0, 0),
        PdfVersion::V1_4,
    );
    assert_version_error(&mut b, "extended Layout attributes require PDF 1.5");

    let mut b = builder_with(
        StructureElement::new("Div")
            .with_attribute(StructureAttribute::layout(
                LayoutAttributes::new().with_column_count(2),
            ))
            .with_marked_content(0, 0),
        PdfVersion::V1_5,
    );
    assert_version_error(&mut b, "Layout column attributes require PDF 1.6");

    // PDF 1.4 接受基础条目并零失败写出。
    let mut b = builder_with(
        StructureElement::new("P")
            .with_attribute(StructureAttribute::layout(
                LayoutAttributes::new().with_space_before(6.0),
            ))
            .with_marked_content(0, 0),
        PdfVersion::V1_4,
    );
    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let layout = tree
        .standard_attributes(0)
        .expect("standard attributes")
        .layout
        .expect("Layout attributes");
    assert_eq!(layout.space_before, Some(6.0));
    let features = reader_features(&doc);
    assert!(
        features.contains(&PdfFeature::StandardStructureAttributes),
        "{features:?}"
    );
    assert!(
        !features.contains(&PdfFeature::LayoutExtendedAttributes),
        "{features:?}"
    );
}

#[test]
fn layout_class_map_is_shape_checked_without_applicability() {
    // /ClassMap 可被任意类型复用：P 自身不允许 Width，但属性类只查形态。
    let tree = StructureTree::new()
        .with_class(
            "wide",
            StructureAttribute::layout(
                LayoutAttributes::new()
                    .with_width(NumberOrAuto::Number(50.0))
                    .with_text_align(TextAlign::Center),
            )
            .value,
        )
        .with_element(
            StructureElement::new("P")
                .with_class("wide")
                .with_marked_content(0, 0),
        );
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree);
    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let layout = tree
        .standard_attributes(0)
        .expect("standard attributes")
        .layout
        .expect("Layout attributes");
    assert_eq!(layout.width, Some(ReaderNumberOrAuto::Number(50.0)));
    assert_eq!(layout.text_align, Some(ReaderTextAlign::Center));

    // 形态错误即使类未被使用也会在分配对象号前失败。
    let tree = StructureTree::new()
        .with_class(
            "bad",
            raw_layout(Dict::new().insert(
                "BorderThickness",
                value_array(vec![Value::Integer(1), Value::Integer(2)]),
            ))
            .value,
        )
        .with_element(StructureElement::new("P").with_marked_content(0, 0));
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree);
    assert_invalid(
        &mut b,
        "BorderThickness must be a number or an array of four numbers",
    );
}
