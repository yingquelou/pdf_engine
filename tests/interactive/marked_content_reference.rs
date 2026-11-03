//! 属性表的间接引用操作数（`/Tag 5 0 R BDC`）的写端 → 落盘 → 解析 →
//! 回读往返（ISO 32000-1 14.6.1/14.6.2）。
//!
//! 规范口径（`pdftotext -raw docs/PDF32000_2008.pdf -` 的输出行号）：
//! - L29005–29006：整族标记内容操作数（含 properties 操作数）自 **PDF 1.2** 起；
//! - Table 320 的 `BDC` 行 L29075–29078 只授权两种操作数形态——内联字典，或
//!   当前资源 `/Properties` 子字典中该名称对应的条目；L29073 给出前缀顺序
//!   `tag properties BDC`，所以间接引用形态写作 `/Tag 5 0 R BDC`（标签在前）；
//! - L29087–29091：属性表字典的值只要含间接引用，该字典 **shall** 定义为
//!   `/Properties` 命名资源并按名称作操作数。
//!
//! 因此本文件往返的是**生产者实态**（对端写出 `obj gen R` 作 properties）：
//! 写端 [`ContentStream::begin_marked_content_reference`] 原样序列化
//! `/Span 5 0 R BDC`（对象号由调用方给定，本层不分配间接对象），读端
//! [`pdf_engine::core::reader::content::Op`] 折叠出 `Value::Ref` 操作数，
//! `Document::page_marked_content` 再按对象号解引用并登记属性字典（含 `/OC`
//! 可见性、`/ActualText`、`/Alt`、`/Lang`）。解引用失败只降级为
//! `properties: None`，页面其余区段照常返回、绝不报错。
//!
//! 已知边界：文本抽取器（`src/core/reader/text/**`，本轮写集之外）自身的
//! `Operand` 模型没有引用形态，所以这一操作数形态的 `/ActualText` 仍不参与
//! 抽取替换——本文件把该行为断言为「原文照出」，而不是假装替换生效。

use super::*;
use pdf_engine::core::content::ContentStream;
use pdf_engine::core::types::{Document, Trailer};
use pdf_engine::core::writer;

/// 属性表字典：`/OC` 指向关闭的 OCG，另带 14.9.4 的三个替换文本键。
fn referenced_property_dict() -> Value {
    Value::Dict(
        Dict::new()
            .insert("OC", Value::Ref(6, 0))
            .insert(
                "ActualText",
                Value::String("replacement via reference".into()),
            )
            .insert("Alt", Value::String("alternate via reference".into()))
            .insert("Lang", Value::Name("en-GB".into())),
    )
}

/// `/Span 5 0 R BDC … EMC` 一整段带文本的标记内容（写端链式入口产出）。
fn reference_marked_content() -> Vec<u8> {
    let mut stream = ContentStream::new();
    stream
        .begin_marked_content_reference("Span", 5, 0)
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("shown")
        .end_text()
        .end_marked_content();
    stream.to_bytes()
}

/// 最小可用页面文档：对象 5 由调用方决定形态，对象 9 故意缺失。
fn reference_property_document(content: Vec<u8>, property_object: Value) -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert(
                    "OCProperties",
                    Value::Dict(
                        Dict::new()
                            .insert("OCGs", Value::Array(Array::new().push(Value::Ref(6, 0))))
                            .insert(
                                "D",
                                Value::Dict(
                                    Dict::new()
                                        .insert("BaseState", Value::Name("ON".into()))
                                        .insert(
                                            "OFF",
                                            Value::Array(Array::new().push(Value::Ref(6, 0))),
                                        ),
                                ),
                            ),
                    ),
                ),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Contents", Value::Ref(4, 0))
                .insert(
                    "Resources",
                    Value::Dict(Dict::new().insert(
                        "Font",
                        Value::Dict(Dict::new().insert("F1", Value::Ref(7, 0))),
                    )),
                ),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(content)),
    ));
    doc.set_object(Object::new(5, 0, property_object));
    doc.set_object(Object::new(
        6,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("OCG".into()))
                .insert("Name", Value::String("Hidden group".into())),
        ),
    ));
    doc.set_object(Object::new(
        7,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name("Helvetica".into()))
                .insert("Encoding", Value::Name("WinAnsiEncoding".into())),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(8)),
    };
    doc
}

/// 往返主链路：写端把属性表写成 `5 0 R`、落盘、重新解析后区段属性字典
/// 完整还原（含 `/OC` 可见性与 `/ActualText`/`/Alt`/`/Lang`）。
#[test]
fn indirect_reference_property_operand_roundtrips_through_a_written_page() {
    let content = reference_marked_content();
    assert!(
        content.starts_with(b"/Span 5 0 R BDC\n"),
        "写端应输出前缀序 `/Span 5 0 R BDC`：{:?}",
        String::from_utf8_lossy(&content)
    );

    let document = reference_property_document(content.clone(), referenced_property_dict());
    let mut buf = Vec::new();
    writer::write_document(&document, &mut buf).expect("写出带间接引用属性表的页面");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    assert!(
        raw.contains("/Span 5 0 R BDC"),
        "落盘字节里应保留间接引用操作数：{raw}"
    );

    let reparsed = parse_buf(&buf);
    let sections = reparsed
        .page_marked_content(0)
        .expect("区段配对，页面无需整体降级");
    assert_eq!(sections.len(), 1);
    let section = &sections[0];
    assert_eq!(section.tag, "Span");
    assert_eq!(
        section.actual_text().as_deref(),
        Some("replacement via reference"),
        "引用解引用后 /ActualText 要能回到区段上"
    );
    assert_eq!(
        section.alternate_text().as_deref(),
        Some("alternate via reference")
    );
    assert_eq!(section.language().as_deref(), Some("en-GB"));
    assert_eq!(
        section.visibility,
        Some(false),
        "/OC 指向默认配置 OFF 的 OCG"
    );
    assert_eq!(
        section
            .properties
            .as_ref()
            .and_then(|dict| dict.get("Type")),
        None,
        "解引用取回的是属性表字典本身，不是 OCG"
    );
    assert_eq!(
        reparsed.extract_page_text(0).as_deref(),
        Some("shown"),
        "文本抽取器（reader/text，本轮写集之外）尚无引用形态，\
         故 /ActualText 不参与替换：断言原文照出，留作后续增量"
    );
}

/// 降级链路：对象 5 不是字典、对象 99 不存在——两条区段都保留，
/// 只是属性表为空，页面其余内容照读。
#[test]
fn unusable_reference_operands_degrade_without_losing_page_sections() {
    let mut stream = ContentStream::new();
    stream
        .begin_marked_content_reference("Span", 5, 0)
        .end_marked_content()
        .begin_marked_content_reference("Note", 99, 0)
        .end_marked_content();
    let content = stream.to_bytes();
    assert_eq!(
        content, b"/Span 5 0 R BDC\nEMC\n/Note 99 0 R BDC\nEMC\n",
        "引用形态的操作数按前缀序原样写出"
    );

    let document = reference_property_document(content, Value::Integer(7));
    let mut buf = Vec::new();
    writer::write_document(&document, &mut buf).expect("写出降级样本");
    let reparsed = parse_buf(&buf);
    let sections = reparsed
        .page_marked_content(0)
        .expect("引用不可解不是错误，整页标记内容不作废");
    assert_eq!(sections.len(), 2);
    let tags: Vec<&str> = sections
        .iter()
        .map(|section| section.tag.as_str())
        .collect();
    assert_eq!(tags, ["Span", "Note"]);
    assert!(
        sections
            .iter()
            .all(|section| section.properties.is_none() && section.visibility.is_none()),
        "解引用失败只丢属性表，不影响区段本身：{:?}",
        sections
            .iter()
            .map(|section| (
                section.tag.as_str(),
                section.properties.clone(),
                section.visibility
            ))
            .collect::<Vec<_>>()
    );
    // 反面样本：`9 0 R` 在本 crate 写出的文件里恰好是重写后的交叉引用流，
    // 所以缺对象要用超出 `Size` 的对象号，否则测的是「引用命中」而不是降级。
    let mut dangling = ContentStream::new();
    dangling
        .begin_marked_content_reference("Span", 9, 0)
        .end_marked_content();
    let dangling_document =
        reference_property_document(dangling.to_bytes(), referenced_property_dict());
    let mut dangling_buf = Vec::new();
    writer::write_document(&dangling_document, &mut dangling_buf).expect("写出命中样本");
    let hits = parse_buf(&dangling_buf)
        .page_marked_content(0)
        .expect("区段配对")
        .pop()
        .expect("一条区段");
    assert_eq!(
        hits.properties
            .as_ref()
            .and_then(|dict| dict.get("Type"))
            .and_then(Value::as_name),
        Some("XRef"),
        "对象号被 writer 复用时按解引用结果登记：与命名资源的口径一致"
    );
}
