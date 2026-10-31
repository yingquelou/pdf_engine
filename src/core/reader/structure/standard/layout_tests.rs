//! Layout 标准属性读端测试（ISO 32000-1 14.8.5.4 Table 342–346）。

use crate::core::reader::structure::{
    BlockAlign, BorderColor, BorderStyle, BorderStyles, BorderThickness, GlyphOrientationVertical,
    InlineAlign, LayoutAttributes, LineHeight, NumberOrAuto, NumberOrFour, NumberOrNumbers,
    Placement, RubyAlign, RubyPosition, StandardAttributes, TextAlign, TextDecorationType,
    WritingMode,
};
use crate::core::types::{Dict, Document, Value};

use super::tests::{
    arr, assert_malformed, attribute_document, document_with_elements, document_with_root, element,
    insert, name, struct_root,
};

/// `/O /Layout` 加上给定条目。
fn layout_document(entries: Dict) -> Document {
    let mut attribute = Dict::new().insert("O", name("Layout"));
    for (key, value) in entries.0 {
        attribute = attribute.insert(key, value);
    }
    attribute_document(attribute)
}

fn standard(doc: &Document) -> StandardAttributes {
    doc.structure_tree()
        .expect("structure tree")
        .expect("tree present")
        .standard_attributes(0)
        .expect("standard attributes")
}

fn layout(doc: &Document) -> LayoutAttributes {
    standard(doc).layout.expect("Layout owner")
}

fn rgb(red: f64, green: f64, blue: f64) -> Value {
    arr(vec![
        Value::Float(red),
        Value::Float(green),
        Value::Float(blue),
    ])
}

fn ints(values: [i64; 3]) -> Value {
    arr(values.into_iter().map(Value::Integer).collect())
}

#[test]
fn layout_attributes_parse_general_entries_and_defaults() {
    let doc = layout_document(
        Dict::new()
            .insert("Placement", name("Before"))
            .insert("WritingMode", name("TbRl"))
            .insert("BackgroundColor", ints([1, 0, 0]))
            .insert("BorderColor", rgb(0.1, 0.2, 0.3))
            .insert("BorderStyle", name("Dashed"))
            .insert("BorderThickness", Value::Integer(2))
            .insert("Color", ints([0, 0, 1]))
            .insert(
                "Padding",
                arr(vec![
                    Value::Integer(1),
                    Value::Integer(2),
                    Value::Integer(3),
                    Value::Integer(4),
                ]),
            ),
    );
    let attributes = layout(&doc);
    assert_eq!(attributes.placement, Some(Placement::Before));
    assert_eq!(attributes.writing_mode, Some(WritingMode::TbRl));
    assert_eq!(attributes.background_color, Some([1.0, 0.0, 0.0]));
    assert_eq!(
        attributes.border_color,
        Some(BorderColor::Uniform([0.1, 0.2, 0.3]))
    );
    assert_eq!(
        attributes.border_style,
        Some(BorderStyles::Uniform(BorderStyle::Dashed))
    );
    assert_eq!(
        attributes.border_thickness,
        Some(BorderThickness::Uniform(2.0))
    );
    assert_eq!(attributes.color, Some([0.0, 0.0, 1.0]));
    assert_eq!(
        attributes.padding,
        Some(NumberOrFour::Edges([1.0, 2.0, 3.0, 4.0]))
    );
    assert_eq!(
        attributes.dict.get("O").and_then(Value::as_name),
        Some("Layout")
    );
    assert!(standard(&doc).other.is_empty());

    // 只带 `/O /Layout`：类型化字段全部未指定，不物化规范缺省值。
    let defaults = layout_document(Dict::new());
    let attributes = layout(&defaults);
    assert_eq!(attributes.placement, None);
    assert_eq!(attributes.writing_mode, None);
    assert_eq!(attributes.background_color, None);
    assert_eq!(attributes.border_color, None);
    assert_eq!(attributes.border_style, None);
    assert_eq!(attributes.border_thickness, None);
    assert_eq!(attributes.color, None);
    assert_eq!(attributes.padding, None);
    assert_eq!(attributes.space_before, None);
    assert_eq!(attributes.text_align, None);
    assert_eq!(attributes.width, None);
    assert_eq!(attributes.line_height, None);
    assert_eq!(attributes.column_count, None);
    assert!(standard(&defaults).layout.is_some());
}

#[test]
fn layout_attributes_parse_block_and_table_entries() {
    let doc = layout_document(
        Dict::new()
            .insert("SpaceBefore", Value::Float(12.5))
            .insert("SpaceAfter", Value::Integer(2))
            .insert("StartIndent", Value::Integer(-3))
            .insert("EndIndent", Value::Integer(4))
            .insert("TextIndent", Value::Integer(5))
            .insert("TextAlign", name("Justify"))
            .insert(
                "BBox",
                arr(vec![
                    Value::Integer(0),
                    Value::Integer(0),
                    Value::Integer(100),
                    Value::Integer(50),
                ]),
            )
            .insert("Width", name("Auto"))
            .insert("Height", Value::Float(12.5))
            .insert("BlockAlign", name("Middle"))
            .insert("InlineAlign", name("End"))
            .insert(
                "TBorderStyle",
                arr(vec![
                    name("Solid"),
                    Value::Null,
                    name("Dotted"),
                    Value::Null,
                ]),
            )
            .insert(
                "TPadding",
                arr(vec![
                    Value::Integer(1),
                    Value::Integer(2),
                    Value::Integer(3),
                    Value::Integer(4),
                ]),
            ),
    );
    let attributes = layout(&doc);
    assert_eq!(attributes.space_before, Some(12.5));
    assert_eq!(attributes.space_after, Some(2.0));
    assert_eq!(attributes.start_indent, Some(-3.0));
    assert_eq!(attributes.end_indent, Some(4.0));
    assert_eq!(attributes.text_indent, Some(5.0));
    assert_eq!(attributes.text_align, Some(TextAlign::Justify));
    assert_eq!(attributes.bbox, Some([0.0, 0.0, 100.0, 50.0]));
    assert_eq!(attributes.width, Some(NumberOrAuto::Auto));
    assert_eq!(attributes.height, Some(NumberOrAuto::Number(12.5)));
    assert_eq!(attributes.block_align, Some(BlockAlign::Middle));
    assert_eq!(attributes.inline_align, Some(InlineAlign::End));
    assert_eq!(
        attributes.t_border_style,
        Some(BorderStyles::Edges([
            Some(BorderStyle::Solid),
            None,
            Some(BorderStyle::Dotted),
            None,
        ]))
    );
    assert_eq!(
        attributes.t_padding,
        Some(NumberOrFour::Edges([1.0, 2.0, 3.0, 4.0]))
    );
}

#[test]
fn layout_attributes_parse_inline_and_column_entries() {
    let doc = layout_document(
        Dict::new()
            .insert("BaselineShift", Value::Integer(-2))
            .insert("LineHeight", name("Normal"))
            .insert("TextDecorationColor", ints([1, 0, 0]))
            .insert("TextDecorationThickness", Value::Float(0.5))
            .insert("TextDecorationType", name("Underline"))
            .insert("RubyAlign", name("Distribute"))
            .insert("RubyPosition", name("Warichu"))
            .insert("GlyphOrientationVertical", Value::Integer(90))
            .insert("ColumnCount", Value::Integer(2))
            .insert(
                "ColumnGap",
                arr(vec![Value::Integer(10), Value::Integer(20)]),
            )
            .insert("ColumnWidths", Value::Integer(100)),
    );
    let attributes = layout(&doc);
    assert_eq!(attributes.baseline_shift, Some(-2.0));
    assert_eq!(attributes.line_height, Some(LineHeight::Normal));
    assert_eq!(attributes.text_decoration_color, Some([1.0, 0.0, 0.0]));
    assert_eq!(attributes.text_decoration_thickness, Some(0.5));
    assert_eq!(
        attributes.text_decoration_type,
        Some(TextDecorationType::Underline)
    );
    assert_eq!(attributes.ruby_align, Some(RubyAlign::Distribute));
    assert_eq!(attributes.ruby_position, Some(RubyPosition::Warichu));
    assert_eq!(
        attributes.glyph_orientation_vertical,
        Some(GlyphOrientationVertical::Angle(90.0))
    );
    assert_eq!(attributes.column_count, Some(2));
    assert_eq!(
        attributes.column_gap,
        Some(NumberOrNumbers::Array(vec![10.0, 20.0]))
    );
    assert_eq!(
        attributes.column_widths,
        Some(NumberOrNumbers::Number(100.0))
    );
}

#[test]
fn layout_attributes_parse_border_single_and_four_entry_forms() {
    let four_styles = Dict::new()
        .insert("O", name("Layout"))
        .insert(
            "BorderStyle",
            arr(vec![
                name("Solid"),
                Value::Null,
                name("Hidden"),
                Value::Null,
            ]),
        )
        .insert(
            "BorderColor",
            arr(vec![
                rgb(0.0, 0.0, 0.0),
                Value::Null,
                rgb(1.0, 1.0, 1.0),
                Value::Null,
            ]),
        )
        .insert(
            "BorderThickness",
            arr(vec![
                Value::Integer(1),
                Value::Null,
                Value::Integer(2),
                Value::Null,
            ]),
        );
    let single = Dict::new()
        .insert("O", name("Layout"))
        .insert("BorderStyle", name("Groove"))
        .insert("BorderColor", rgb(0.25, 0.5, 0.75))
        .insert("BorderThickness", Value::Float(1.5));
    let doc = document_with_elements(vec![
        (5, element("TD").insert("A", Value::Dict(four_styles))),
        (6, element("TD").insert("A", Value::Dict(single))),
    ]);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let edges = tree
        .standard_attributes(0)
        .expect("layout")
        .layout
        .expect("Layout owner");
    assert_eq!(
        edges.border_style,
        Some(BorderStyles::Edges([
            Some(BorderStyle::Solid),
            None,
            Some(BorderStyle::Hidden),
            None,
        ]))
    );
    assert_eq!(
        edges.border_color,
        Some(BorderColor::Edges([
            Some([0.0, 0.0, 0.0]),
            None,
            Some([1.0, 1.0, 1.0]),
            None,
        ]))
    );
    assert_eq!(
        edges.border_thickness,
        Some(BorderThickness::Edges([Some(1.0), None, Some(2.0), None]))
    );

    let uniform = tree
        .standard_attributes(1)
        .expect("layout")
        .layout
        .expect("Layout owner");
    assert_eq!(
        uniform.border_style,
        Some(BorderStyles::Uniform(BorderStyle::Groove))
    );
    assert_eq!(
        uniform.border_color,
        Some(BorderColor::Uniform([0.25, 0.5, 0.75]))
    );
    assert_eq!(
        uniform.border_thickness,
        Some(BorderThickness::Uniform(1.5))
    );
}

#[test]
fn layout_attributes_parse_unknown_enum_names() {
    let doc = layout_document(
        Dict::new()
            .insert("Placement", name("Floating"))
            .insert("WritingMode", name("Xx"))
            .insert("BorderStyle", name("Wavy"))
            .insert("TextAlign", name("Middle"))
            .insert("BlockAlign", name("Top"))
            .insert("InlineAlign", name("Left"))
            .insert("LineHeight", name("Tall"))
            .insert("TextDecorationType", name("Wavy"))
            .insert("RubyAlign", name("Left"))
            .insert("RubyPosition", name("Top"))
            .insert("GlyphOrientationVertical", name("45"))
            .insert("Width", name("Fixed")),
    );
    let attributes = layout(&doc);
    assert_eq!(
        attributes.placement,
        Some(Placement::Other("Floating".into()))
    );
    assert_eq!(
        attributes.writing_mode,
        Some(WritingMode::Other("Xx".into()))
    );
    assert_eq!(
        attributes.border_style,
        Some(BorderStyles::Uniform(BorderStyle::Other("Wavy".into())))
    );
    assert_eq!(
        attributes.text_align,
        Some(TextAlign::Other("Middle".into()))
    );
    assert_eq!(
        attributes.block_align,
        Some(BlockAlign::Other("Top".into()))
    );
    assert_eq!(
        attributes.inline_align,
        Some(InlineAlign::Other("Left".into()))
    );
    assert_eq!(
        attributes.line_height,
        Some(LineHeight::Other("Tall".into()))
    );
    assert_eq!(
        attributes.text_decoration_type,
        Some(TextDecorationType::Other("Wavy".into()))
    );
    assert_eq!(attributes.ruby_align, Some(RubyAlign::Other("Left".into())));
    assert_eq!(
        attributes.ruby_position,
        Some(RubyPosition::Other("Top".into()))
    );
    assert_eq!(
        attributes.glyph_orientation_vertical,
        Some(GlyphOrientationVertical::Other("45".into()))
    );
    assert_eq!(attributes.width, Some(NumberOrAuto::Other("Fixed".into())));
}

#[test]
fn malformed_layout_attributes_are_typed_errors() {
    let doc = layout_document(Dict::new().insert("Placement", Value::Integer(1)));
    assert_malformed(&doc, "structure element 0 /Placement must be a name");

    let doc = layout_document(Dict::new().insert(
        "BackgroundColor",
        arr(vec![Value::Integer(1), Value::Integer(0)]),
    ));
    assert_malformed(
        &doc,
        "structure element 0 /BackgroundColor must be an array of three numbers",
    );

    let doc = layout_document(Dict::new().insert("BackgroundColor", ints([2, 0, 0])));
    assert_malformed(
        &doc,
        "structure element 0 /BackgroundColor entry 0 must be a number in the range 0.0 to 1.0",
    );

    let doc = layout_document(Dict::new().insert(
        "BorderColor",
        arr(vec![
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(0),
        ]),
    ));
    assert_malformed(
        &doc,
        "structure element 0 /BorderColor must be an array of three numbers or an array of four colour arrays",
    );

    let doc = layout_document(Dict::new().insert(
        "BorderStyle",
        arr(vec![name("Solid"), name("Solid"), name("Solid")]),
    ));
    assert_malformed(
        &doc,
        "structure element 0 /BorderStyle must be a name or an array of four names",
    );

    let doc = layout_document(Dict::new().insert(
        "BorderThickness",
        arr(vec![
            Value::String("thick".into()),
            Value::Null,
            Value::Null,
            Value::Null,
        ]),
    ));
    assert_malformed(
        &doc,
        "structure element 0 /BorderThickness entry 0 must be a non-negative number or null",
    );

    let doc = layout_document(Dict::new().insert("BorderThickness", Value::Integer(-1)));
    assert_malformed(
        &doc,
        "structure element 0 /BorderThickness must be non-negative",
    );

    let doc = layout_document(Dict::new().insert(
        "Padding",
        arr(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]),
    ));
    assert_malformed(
        &doc,
        "structure element 0 /Padding must be a number or an array of four numbers",
    );

    let doc = layout_document(Dict::new().insert(
        "BBox",
        arr(vec![
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(1),
        ]),
    ));
    assert_malformed(
        &doc,
        "structure element 0 /BBox must be an array of four numbers",
    );

    for count in [Value::Integer(0), Value::Float(2.0)] {
        let doc = layout_document(Dict::new().insert("ColumnCount", count));
        assert_malformed(
            &doc,
            "structure element 0 /ColumnCount must be an integer of at least 1",
        );
    }

    let doc = layout_document(Dict::new().insert("ColumnGap", arr(Vec::new())));
    assert_malformed(&doc, "structure element 0 /ColumnGap must not be empty");

    let doc = layout_document(Dict::new().insert("ColumnWidths", arr(vec![name("wide")])));
    assert_malformed(
        &doc,
        "structure element 0 /ColumnWidths entry 0 must be a finite number",
    );

    let doc = layout_document(Dict::new().insert("LineHeight", Value::Bool(true)));
    assert_malformed(
        &doc,
        "structure element 0 /LineHeight must be a number or a name",
    );

    let doc = layout_document(Dict::new().insert("GlyphOrientationVertical", Value::Integer(45)));
    assert_malformed(
        &doc,
        "structure element 0 /GlyphOrientationVertical must be a multiple of 90 between -180 and 360",
    );

    let doc = layout_document(Dict::new().insert("GlyphOrientationVertical", Value::Bool(true)));
    assert_malformed(
        &doc,
        "structure element 0 /GlyphOrientationVertical must be a name or a number",
    );

    let doc = layout_document(Dict::new().insert("TextDecorationThickness", Value::Integer(-1)));
    assert_malformed(
        &doc,
        "structure element 0 /TextDecorationThickness must be non-negative",
    );

    let doc = layout_document(Dict::new().insert("Width", Value::Bool(true)));
    assert_malformed(
        &doc,
        "structure element 0 /Width must be a number or a name",
    );
}

#[test]
fn layout_attributes_merge_class_and_element_objects() {
    let class_map = Dict::new().insert(
        "Body",
        Value::Dict(
            Dict::new()
                .insert("O", name("Layout"))
                .insert("SpaceBefore", Value::Integer(4))
                .insert("BorderStyle", name("Solid")),
        ),
    );
    let element = element("P").insert("C", name("Body")).insert(
        "A",
        Value::Dict(
            Dict::new()
                .insert("O", name("Layout"))
                .insert("StartIndent", Value::Integer(8)),
        ),
    );
    let doc = document_with_root(
        struct_root(Dict::new())
            .insert("ClassMap", Value::Dict(class_map))
            .insert("K", arr(vec![Value::Ref(5, 0)])),
    );
    insert(&doc, 5, Value::Dict(element));
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let attributes = tree
        .standard_attributes(0)
        .expect("standard attributes")
        .layout
        .expect("Layout owner");
    assert_eq!(attributes.space_before, Some(4.0), "类的 /SpaceBefore 保留");
    assert_eq!(
        attributes.start_indent,
        Some(8.0),
        "元素的 /StartIndent 合并"
    );
    assert_eq!(
        attributes.border_style,
        Some(BorderStyles::Uniform(BorderStyle::Solid))
    );
    assert_eq!(
        attributes
            .dict
            .get("SpaceBefore")
            .and_then(Value::as_integer),
        Some(4)
    );
    assert_eq!(
        attributes
            .dict
            .get("StartIndent")
            .and_then(Value::as_integer),
        Some(8)
    );
}
