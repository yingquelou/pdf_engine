//! 反序显示串读端测试（ISO 32000-1 14.8.2.3.3）。

use crate::core::reader::accessibility::{ReversedCharsDiagnostic, ReversedCharsSequence};
use crate::core::reader::content::MarkedSection;
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, StreamData, Trailer, Value};

const CATALOG: i64 = 1;
const PAGES: i64 = 2;
const PAGE: i64 = 3;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 单页文档：内容流为给定字节，无结构树。
fn page_document(content: &[u8]) -> Document {
    let mut doc = Document::new();
    insert(
        &doc,
        PAGES,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGE, 0)]))
                .insert("Count", Value::Integer(1)),
        ),
    );
    insert(
        &doc,
        PAGE,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(PAGES, 0))
                .insert("Resources", Value::Dict(Dict::new()))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
                ),
        ),
    );
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES, 0)),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

fn sequences(doc: &Document) -> Vec<ReversedCharsSequence> {
    doc.page_reversed_chars(0)
        .unwrap_or_else(|error| panic!("反序显示串还原应成功: {error}"))
}

fn text(value: &str) -> String {
    value.to_owned()
}

#[test]
fn reversed_chars_restore_natural_word_order() {
    // 规范 EXAMPLE 的形态：BMC 不带属性表，两个显示串逐串反转、串序不变。
    let doc =
        page_document(b"/ReversedChars BMC BT 72 700 Td ( olleH) Tj -200 0 Td (dlrow) Tj ET EMC");
    let sections = doc.page_marked_content(0).expect("marked content");
    assert_eq!(sections.len(), 1);
    assert!(sections[0].is_reversed_chars());

    let sequences = sequences(&doc);
    assert_eq!(sequences.len(), 1, "{sequences:?}");
    assert_eq!(
        sequences[0].strings,
        vec![text("Hello "), text("world")],
        "每个显示串逐串反转，串与串保持自然顺序"
    );
    assert!(sequences[0].diagnostics.is_empty());
    assert_eq!(sequences[0].text(), text("Hello world"));
}

#[test]
fn reversed_chars_bdc_property_list_recognized_by_tag() {
    // 写端 `begin_reversed_chars` 的 BDC 形态：识别只依据标签，属性表条目
    // 不参与判定。
    let doc = page_document(
        b"/ReversedChars << /ReversedChars true >> BDC BT 72 700 Td (olleH) Tj ET EMC",
    );
    let sequences = sequences(&doc);
    assert_eq!(sequences.len(), 1, "{sequences:?}");
    assert_eq!(sequences[0].strings, vec![text("Hello")]);
    assert!(sequences[0].diagnostics.is_empty());
}

#[test]
fn reversed_chars_interior_space_reported_as_diagnostic() {
    // 显示串内部的 U+0020 违反 shall：还原文本照常给出，违规以诊断呈现。
    let doc = page_document(b"/ReversedChars BMC BT 72 700 Td (dlrow olleH) Tj ET EMC");
    let sequences = sequences(&doc);
    assert_eq!(sequences.len(), 1, "{sequences:?}");
    assert_eq!(sequences[0].strings, vec![text("Hello world")]);
    assert_eq!(
        sequences[0].diagnostics,
        vec![ReversedCharsDiagnostic::InteriorSpace { string: 0 }]
    );
}

#[test]
fn reversed_chars_word_break_spaces_at_ends_are_conformant() {
    // 显示串首尾的分词空格（U+0020）合法：`" olleH "` 反转成 `" Hello "`，
    // 空格都在两端、没有内部空格。
    let doc = page_document(b"/ReversedChars BMC BT 72 700 Td ( olleH ) Tj ET EMC");
    let sequences = sequences(&doc);
    assert_eq!(sequences.len(), 1, "{sequences:?}");
    assert_eq!(sequences[0].strings, vec![text(" Hello ")]);
    assert!(
        sequences[0].diagnostics.is_empty(),
        "首尾分词空格合法: {:?}",
        sequences[0].diagnostics
    );
}

#[test]
fn reversed_chars_empty_show_string_reported_as_diagnostic() {
    let doc = page_document(b"/ReversedChars BMC BT 72 700 Td () Tj (olleH) Tj ET EMC");
    let sequences = sequences(&doc);
    assert_eq!(sequences.len(), 1, "{sequences:?}");
    assert_eq!(sequences[0].strings, vec![text(""), text("Hello")]);
    assert_eq!(
        sequences[0].diagnostics,
        vec![ReversedCharsDiagnostic::Empty { string: 0 }]
    );
}

#[test]
fn reversed_chars_non_printable_bytes_reported_as_diagnostic() {
    // 非可打印 ASCII 字节（如 UTF-16BE 字面串）：逐字节反转不保证字符级
    // 正确，以 NonPrintable 诊断呈现。
    let doc = page_document(b"/ReversedChars BMC BT 72 700 Td <FEFF0048> Tj ET EMC");
    let sequences = sequences(&doc);
    assert_eq!(sequences.len(), 1, "{sequences:?}");
    assert_eq!(
        sequences[0].diagnostics,
        vec![ReversedCharsDiagnostic::NonPrintable { string: 0 }]
    );
    // <FEFF0048> 字节反转为 48 00 FF FE，lossy 还原为 "H\u{0}\u{FFFD}\u{FFFD}"。
    assert!(sequences[0].strings[0].starts_with('H'));
}

#[test]
fn reversed_chars_tj_array_strings_reversed_individually() {
    // TJ 数组里的每个字符串元素单独反转；调整值（负数空隙）保持原位序。
    let doc = page_document(b"/ReversedChars BMC BT 72 700 Td [( olleH)-250(dlrow)] TJ ET EMC");
    let sequences = sequences(&doc);
    assert_eq!(sequences.len(), 1, "{sequences:?}");
    assert_eq!(
        sequences[0].strings,
        vec![text("Hello "), text("world")],
        "串内反转、串间不逆"
    );
    assert!(sequences[0].diagnostics.is_empty());
}

#[test]
fn reversed_chars_nested_section_strings_are_restored() {
    // 嵌套在 ReversedChars 序列内的显示串同样属于该序列。
    let doc = page_document(b"/ReversedChars BMC /Span BMC BT 72 700 Td (olleH) Tj ET EMC EMC");
    let sequences = sequences(&doc);
    assert_eq!(sequences.len(), 1, "{sequences:?}");
    let sections = doc.page_marked_content(0).expect("marked content");
    let outer = &sequences[0];
    // 输出按关闭顺序：内层 `Span` 先关，外层 `ReversedChars` 的下标在后。
    assert_eq!(
        outer.section,
        sections
            .iter()
            .position(|section| section.is_reversed_chars())
            .expect("ReversedChars 区段"),
        "还原结果应挂在 ReversedChars 区段上"
    );
    assert_eq!(outer.strings, vec![text("Hello")]);
    assert!(
        sections.iter().filter(|s| s.tag == "Span").count() == 1,
        "Span 内层区段存在: {sections:?}"
    );
}

#[test]
fn reversed_chars_points_and_other_tags_are_ignored() {
    let doc = page_document(b"/ReversedChars MP /P BMC BT 72 700 Td (olleH) Tj ET EMC");
    let sections = doc.page_marked_content(0).expect("marked content");
    let point: &MarkedSection = &sections[0];
    assert!(point.point && point.tag == "ReversedChars");
    let sequences = sequences(&doc);
    assert!(sequences.is_empty(), "点序列不包围显示串: {sequences:?}");
}

#[test]
fn reversed_chars_extractor_restores_natural_order() {
    // 文本提取路径同样还原：显示串反序存储，`extract_page_text` 读出自然
    // 顺序（无字体元数据时按 Latin-1 兜底解码）。串间接排（Td）会带换行，
    // 这里用同排两串聚焦反转本身。
    let doc = page_document(b"/ReversedChars BMC BT 72 700 Td ( olleH) Tj (dlrow) Tj ET EMC");
    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("Hello world"),
        "提取器应把 ReversedChars 序列的显示串还原成自然顺序"
    );
}

#[test]
fn reversed_chars_section_registers_feature_profile() {
    let doc = page_document(b"/ReversedChars BMC BT 72 700 Td (olleH) Tj ET EMC");
    doc.page_marked_content(0).expect("marked content");
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::ReversedCharacters),
        "读取标记内容时应登记 14.8.2.3.3 特性"
    );
}
