//! 文本抽取器对属性表**间接引用操作数**（`/Span 5 0 R BDC`）的互操作容错
//! 往返：写端 [`ContentStream::begin_marked_content_reference`] 产出该形态
//! → `writer::write_document` 落盘 → `Parser` 重解析 → `Document::extract_page_text`
//! 断言 `/ActualText` 替换生效、OFF 组文本不出现、ON 组文本出现。
//!
//! 规范口径（`pdftotext -raw docs/PDF32000_2008.pdf -` 的输出行号）：
//! - Table 320 的 `BDC` 行 L29073–29078 给出前缀序 `tag properties BDC`，且
//!   `properties` **shall** 是内联字典或当前资源 `/Properties` 里的名称；
//! - 14.6.2 L29087–29091 要求「值中含间接引用的属性表字典 shall 定义为命名
//!   资源并按名称引用」。
//!
//! 所以把整张属性表写成 `5 0 R` 操作数**不是 ISO 32000-1 授权的写法**；本文件
//! 往返的是生产端实态（本 crate 写端也这么产），按**互操作容错**实现，不构成
//! 合规声明，也未新增 `PdfFeature` 版本门禁。
//!
//! 写端只写 `obj gen R`、不分配间接对象（对象号由调用方登记，见
//! `ContentStream::begin_marked_content_reference` 文档），因此本页用
//! `Document::set_object` 手工登记对象号后走 `writer::write_document`；
//! `PdfBuilder` 目前没有将引用形态内容写到已知对象号上的通道（`builder/**`
//! 在本轮写集之外）。解引用口径与已落地的声明式读端
//! （`Document::page_marked_content`）一致，本文件末尾逐项交叉核对。

use super::*;
use pdf_engine::core::content::ContentStream;
use pdf_engine::core::types::{Document, Trailer};
use pdf_engine::core::writer;

/// 属性表（只带 `/ActualText`）。
fn actual_text_property_dict() -> Value {
    Value::Dict(Dict::new().insert(
        "ActualText",
        Value::String("replacement via reference".into()),
    ))
}

/// 属性表：`/ActualText` + `/OC` 指向可见 OCG。
fn visible_list_property_dict() -> Value {
    Value::Dict(
        Dict::new()
            .insert(
                "ActualText",
                Value::String("replaced via visible list".into()),
            )
            .insert("OC", Value::Ref(7, 0)),
    )
}

/// 属性表：`/OC` 指向关闭的 OCG（无替换文本）。
fn hidden_list_property_dict() -> Value {
    Value::Dict(Dict::new().insert("OC", Value::Ref(6, 0)))
}

fn ocg(name: &str) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("OCG".into()))
            .insert("Name", Value::String(name.into())),
    )
}

/// 五个引用形态区段 + 一段无标记文本的页面内容。
fn reference_marked_page_content() -> Vec<u8> {
    let mut stream = ContentStream::new();
    stream
        .begin_marked_content_reference("Actual", 5, 0)
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("original replaced")
        .end_text()
        .end_marked_content();
    stream
        .begin_marked_content_reference("Hidden", 6, 0)
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("hidden bare ocg")
        .end_text()
        .end_marked_content();
    stream
        .begin_marked_content_reference("Shown", 7, 0)
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("visible bare ocg")
        .end_text()
        .end_marked_content();
    stream
        .begin_marked_content_reference("HiddenList", 8, 0)
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("hidden via oc list")
        .end_text()
        .end_marked_content();
    stream
        .begin_marked_content_reference("VisibleList", 9, 0)
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("ignored")
        .end_text()
        .end_marked_content();
    stream
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("unmarked tail")
        .end_text();
    stream.to_bytes()
}

/// 单页文档：对象 5–9 是引用落点（对象 99 故意不存在），默认配置里
/// OCG 6 为 OFF、OCG 7 为 ON。
fn reference_property_page_document(content: Vec<u8>, extra_property: Value) -> Document {
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
                            .insert(
                                "OCGs",
                                Value::Array(
                                    Array::new().push(Value::Ref(6, 0)).push(Value::Ref(7, 0)),
                                ),
                            )
                            .insert(
                                "D",
                                Value::Dict(
                                    Dict::new()
                                        .insert("BaseState", Value::Name("ON".into()))
                                        .insert(
                                            "ON",
                                            Value::Array(Array::new().push(Value::Ref(7, 0))),
                                        )
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
                        Value::Dict(Dict::new().insert("F1", Value::Ref(10, 0))),
                    )),
                ),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(content)),
    ));
    doc.set_object(Object::new(5, 0, actual_text_property_dict()));
    doc.set_object(Object::new(6, 0, ocg("Hidden group")));
    doc.set_object(Object::new(7, 0, ocg("Visible group")));
    doc.set_object(Object::new(8, 0, hidden_list_property_dict()));
    doc.set_object(Object::new(9, 0, extra_property));
    doc.set_object(Object::new(
        10,
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
            .insert("Size", Value::Integer(11)),
    };
    doc
}

fn written_page_bytes(content: Vec<u8>, extra_property: Value) -> Vec<u8> {
    let document = reference_property_page_document(content, extra_property);
    let mut buf = Vec::new();
    writer::write_document(&document, &mut buf).expect("写出带引用形态属性表的页面");
    buf
}

/// 主链路：写端产出 `/tag obj 0 R BDC` → 落盘 → 重解析 → 抽取文本时
/// `/ActualText` 替换生效、OFF 组（裸 OCG 与 `/OC` 属性表两条落点）不出现、
/// ON 组出现、无标记文本保留。
#[test]
fn extractor_applies_actual_text_and_oc_visibility_from_referenced_property_lists() {
    let content = reference_marked_page_content();
    assert!(
        content.starts_with(b"/Actual 5 0 R BDC\n"),
        "写端应输出前缀序 `/tag obj gen R BDC`：{:?}",
        String::from_utf8_lossy(&content)
    );

    let buf = written_page_bytes(content, visible_list_property_dict());
    let raw = String::from_utf8_lossy(&buf).into_owned();
    assert!(
        raw.contains("/Actual 5 0 R BDC") && raw.contains("/HiddenList 8 0 R BDC"),
        "落盘字节里应保留间接引用操作数：{raw}"
    );

    let reparsed = parse_buf(&buf);
    let text = reparsed
        .try_extract_page_text(0)
        .expect("抽取整页文本不因引用形态报错");

    assert!(
        text.contains("replacement via reference"),
        "引用指向的属性表里的 /ActualText 要参与替换：{text}"
    );
    assert!(
        !text.contains("original replaced"),
        "被替换的原文不再出现：{text}"
    );
    assert!(
        !text.contains("hidden bare ocg"),
        "引用直接命中默认 OFF 的裸 OCG 时该区段隐藏：{text}"
    );
    assert!(
        !text.contains("hidden via oc list"),
        "引用命中的属性表带 /OC 指向 OFF 组时同样隐藏：{text}"
    );
    assert!(
        text.contains("visible bare ocg"),
        "引用命中默认 ON 的 OCG 时文本保留：{text}"
    );
    assert!(
        text.contains("replaced via visible list") && !text.contains("ignored"),
        "可见属性表的 /ActualText 也要替换：{text}"
    );
    assert!(
        text.contains("unmarked tail"),
        "区段外的文本不受影响：{text}"
    );
    assert_eq!(
        text, "replacement via referencevisible bare ocgreplaced via visible listunmarked tail",
        "抽取结果逐字可预期（本 crate 的抽取器不在 BT/ET 间插分隔符）"
    );
}

/// 交叉核对：抽取器与已落地的声明式读端（`page_marked_content`）对同一页
/// 每个引用区段给出一致的可见性与替换文本口径。
#[test]
fn referenced_property_lists_extract_identically_to_the_declarative_reader() {
    let buf = written_page_bytes(
        reference_marked_page_content(),
        visible_list_property_dict(),
    );
    let reparsed = parse_buf(&buf);
    let sections = reparsed
        .page_marked_content(0)
        .expect("引用形态区段全部配对");
    assert_eq!(sections.len(), 5, "{sections:?}");

    let hidden_by_reader: Vec<&str> = sections
        .iter()
        .filter(|section| section.visibility == Some(false))
        .map(|section| section.tag.as_str())
        .collect();
    assert_eq!(hidden_by_reader, ["Hidden", "HiddenList"]);

    let replacements: Vec<(&str, String)> = sections
        .iter()
        .filter_map(|section| {
            section
                .actual_text()
                .map(|text| (section.tag.as_str(), text))
        })
        .collect();
    assert_eq!(
        replacements,
        [
            ("Actual", "replacement via reference".to_owned()),
            ("VisibleList", "replaced via visible list".to_owned()),
        ]
    );

    // 声明式读端判为隐藏的区段，抽取器必须一个字都不输出；判为可见且带
    // /ActualText 的区段，抽取器必须输出替换串。
    let text = reparsed.extract_page_text(0).expect("抽取文本");
    for section in &sections {
        if section.visibility == Some(false) {
            assert!(
                !text.contains(&shown_text_of(&section.tag)),
                "声明式读端隐藏而抽取器仍输出了 {tag} 的内容",
                tag = section.tag
            );
        }
        if let Some(replacement) = section.actual_text() {
            assert!(
                text.contains(&replacement),
                "声明式读端的 /ActualText 未参与抽取：{text}"
            );
        }
    }
}

/// 区段标签对应的显示串（与 [`reference_marked_page_content`] 一致）。
fn shown_text_of(tag: &str) -> String {
    match tag {
        "Actual" => "original replaced",
        "Hidden" => "hidden bare ocg",
        "Shown" => "visible bare ocg",
        "HiddenList" => "hidden via oc list",
        "VisibleList" => "ignored",
        _ => "",
    }
    .to_owned()
}

/// 隐藏的引用区段：`/OC` 指向默认 OFF 的 OCG 且同时带 `/ActualText` 时，
/// 抽取整页得到空串——隐藏优先，替换文本不把内容救回来。
///
/// 这条口径正是已落地的 `tests/interactive/marked_content_reference.rs` 里
/// 「抽取器尚无引用形态，故断言原文照出（`Some("shown")`）」那条断言需要
/// 改成 `Some("")` 的原因：该页的属性表同时带 `/OC` 指向 OFF 组与
/// `/ActualText`，声明式读端早就给出 `visibility == Some(false)`。
#[test]
fn hidden_referenced_property_list_yields_no_page_text_at_all() {
    let mut stream = ContentStream::new();
    stream
        .begin_marked_content_reference("Span", 9, 0)
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("shown")
        .end_text()
        .end_marked_content();
    let hidden_with_actual_text = Value::Dict(Dict::new().insert("OC", Value::Ref(6, 0)).insert(
        "ActualText",
        Value::String("replacement via reference".into()),
    ));

    let buf = written_page_bytes(stream.to_bytes(), hidden_with_actual_text);
    let reparsed = parse_buf(&buf);
    let section = reparsed
        .page_marked_content(0)
        .expect("引用形态区段配对")
        .pop()
        .expect("一条区段");
    assert_eq!(section.visibility, Some(false));
    assert_eq!(
        section.actual_text().as_deref(),
        Some("replacement via reference")
    );
    assert_eq!(
        reparsed.extract_page_text(0).as_deref(),
        Some(""),
        "声明式读端判隐藏的区段，抽取器不得发出替换文本"
    );
}

/// 降级链路：引用指向不存在的对象与非字典对象时不报错，文本原样抽出
/// （`/ActualText` 无来源、可见性按可见处理）。
///
/// 反面样本用超出 `Size` 的对象号 99：本 crate 写出的文件里，落在
/// `1 ..= Size` 的空号会被 writer 的对象流/交叉引用流复用，那样测的就是
/// 「引用命中」而不是降级。
#[test]
fn dangling_reference_property_operand_still_extracts_the_marked_text() {
    let mut stream = ContentStream::new();
    stream
        .begin_marked_content_reference("Dangling", 99, 0)
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("dangling kept")
        .end_text()
        .end_marked_content();
    stream
        .begin_marked_content_reference("NotADict", 6, 0)
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("ocg without actual text")
        .end_text()
        .end_marked_content();

    // 对象 6 是 OFF 的裸 OCG：无属性表可取，但可见性照判 → 隐藏。
    let buf = written_page_bytes(stream.to_bytes(), Value::Integer(7));
    let reparsed = parse_buf(&buf);
    let text = reparsed.try_extract_page_text(0).expect("悬空引用不是错误");
    assert!(
        text.contains("dangling kept"),
        "对象不存在时按「无属性、可见」降级：{text}"
    );
    assert!(
        !text.contains("ocg without actual text"),
        "命中 OFF 裸 OCG 的区段仍然隐藏：{text}"
    );
}

/// 标记内容点 `DP` 与 `BMC`/`EMC` 帧的继承语义不受折叠影响：内层 `BMC`
/// 继承外层引用属性表推出的隐藏状态。
#[test]
fn nested_plain_marked_section_inherits_hidden_state_from_referenced_group() {
    let mut stream = ContentStream::new();
    stream
        .begin_marked_content_reference("Outer", 6, 0)
        .marked_content_point("Inner")
        .begin_marked_content("Nested")
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("gone nested")
        .end_text()
        .end_marked_content()
        .end_marked_content();
    stream
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("outside")
        .end_text();

    let buf = written_page_bytes(stream.to_bytes(), actual_text_property_dict());
    let reparsed = parse_buf(&buf);
    assert_eq!(
        reparsed.extract_page_text(0).as_deref(),
        Some("outside"),
        "外层引用命中 OFF 组时，内层 BMC 继承 hidden"
    );
}
