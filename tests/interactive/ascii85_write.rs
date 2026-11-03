//! ASCII85 串字面量 `<~ ... ~>` 的**写出**证据：写 → 用本库读端回解析 →
//! 取回同一个 `Value::Ascii85String`，以及非法 body 的**类型化拒绝**。
//!
//! 口径（`Value::Ascii85String` 存的是 `<~` 与 `~>` 之间的**编码文本**，
//! 不是解码后的字节）：
//!
//! - 读端（`core::reader::lexer::read_ascii85`）逐字收集到**第一个** `~>`
//!   为止、不转义；写端同样逐字包裹。于是 body 里出现 `~>` 时，写出的文件
//!   回读会得到一个**更短**的串——静默损坏。本文件证明写端在可失败的写出
//!   入口（`write_object` / `write_document` / `PdfBuilder::build`）用
//!   [`WriteError::InvalidObject`] 拒绝这类 body，且失败零字节。
//! - 字符口径来自 ISO 32000-1 **§7.4.3 ASCII85Decode Filter**
//!   （`docs/PDF32000_2008.pdf`，`pdftotext -raw` 行号 raw L1311-1313）：
//!   字母表是 `!`(21h)–`u`(75h) 加 `z`(7Ah)，`~>`(7Eh)(3Eh) 是 EOD 标记，
//!   白空格一律忽略，其他字符与不可能的组合报错。
//!   §7.3.4（raw L815-822）只定义 `( )` 与 `< >` 两种串形态，**§7.3.5
//!   （raw L911）是 Name Objects**——`<~ ~>` 本身不是 ISO 32000-1 语法，
//!   故 body 的合法性只能按 ASCII85 过滤器口径判定。
//! - 白空格集合取 §7.2.2 Table 1（raw L738-746）：NUL HT LF FF CR SP。
//!
//! 要存任意字节请用 `Value::String` / `Value::HexString`；二进制负载走流
//! 对象或嵌入文件路径。本库不提供 base-85 编码器。

use super::*;
use pdf_engine::core::builder::{PageSize, PdfBuilder};
use pdf_engine::core::types::{Document, PdfVersion, Trailer};
use pdf_engine::core::writer::{self, WriteError};

/// 最小文档：1 号对象是目录字典，`payloads` 作为若干条目挂进去。
fn doc_with_catalog(entries: &[(&str, Value)]) -> Document {
    let mut dict = Dict::new().insert("Type", Value::Name("Catalog".into()));
    for (key, value) in entries {
        dict = dict.insert(*key, value.clone());
    }
    let mut doc = Document::new();
    doc.set_object(Object::new(1, 0, Value::Dict(dict)));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc.xref = pdf_engine::core::types::Xref::new();
    doc.startxref = pdf_engine::core::types::StartXref(0);
    doc
}

/// 写出（默认 1.7：非流对象打包进对象流）并回读目录字典。
fn write_then_catalog(doc: &Document) -> Dict {
    let mut buf = Vec::new();
    writer::write_document(doc, &mut buf).expect("legal ascii85 bodies must write");
    let reparsed = parse_buf(&buf);
    reparsed.load_all_objects().expect("load objects");
    let (root, _generation) = reparsed.trailer().root().expect("/Root");
    reparsed
        .get_object(root)
        .expect("catalog lookup")
        .expect("catalog object")
        .value
        .as_dict()
        .expect("catalog dict")
        .clone()
}

/// 合法 body 经 `write_document`（1.7，打包进对象流）写出后，读端取回
/// **同一个值**：字节对称，未被截断。
#[test]
fn ascii85_string_written_by_the_writer_reparses_to_the_same_value() {
    let bodies = [
        "9jqo^",
        "z",
        "9jqo^z9jqo^",
        "9jqo^\n 9jqo^\t\r\u{0c}\u{0} ",
        "",
        "a<b>c",
    ];
    let entries: Vec<(String, Value)> = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| (format!("S{index}"), Value::Ascii85String((*body).into())))
        .collect();
    let mut dict = Dict::new().insert("Type", Value::Name("Catalog".into()));
    for (key, value) in &entries {
        dict = dict.insert(key.as_str(), value.clone());
    }
    let mut doc = Document::new();
    doc.set_object(Object::new(1, 0, Value::Dict(dict)));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc.xref = pdf_engine::core::types::Xref::new();
    doc.startxref = pdf_engine::core::types::StartXref(0);

    let catalog = write_then_catalog(&doc);
    for (key, value) in &entries {
        assert_eq!(
            catalog.get(key.as_str()),
            Some(value),
            "{key} 必须逐字回读为同一个 Ascii85String"
        );
    }
}

/// 嵌套形态（数组条目 + 流对象字典条目）同样字节对称回读，且文件里能看到
/// `<~ ... ~>` 原样包裹（不转义、不剥离）。
#[test]
fn ascii85_string_nested_bodies_reparses_verbatim_and_are_wrapped_unescaped() {
    let list = Value::Array(Array::new().push(Value::Ascii85String("9jqo^".into())));
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("List", list.clone())
                .insert("Sig", Value::Ref(2, 0)),
        ),
    ));
    // 流对象只能是顶层对象（规范禁止打包进对象流），其字典条目同样受门禁。
    doc.set_object(Object::new(
        2,
        0,
        Value::Stream(
            Dict::new()
                .insert("Sig", Value::Ascii85String("z".into()))
                .insert("Length", Value::Integer(3)),
            StreamData::Owned(b"abc".to_vec()),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc.xref = pdf_engine::core::types::Xref::new();
    doc.startxref = pdf_engine::core::types::StartXref(0);

    let mut buf = Vec::new();
    writer::write_document(&doc, &mut buf).expect("legal nested bodies");
    let text = String::from_utf8_lossy(&buf).to_string();
    assert!(text.contains("<~9jqo^~>"), "数组条目须原样包裹：{text}");
    assert!(text.contains("<~z~>"), "流字典条目须原样包裹：{text}");

    let reparsed = parse_buf(&buf);
    reparsed.load_all_objects().expect("load objects");
    let catalog = reparsed
        .get_object(1)
        .expect("catalog lookup")
        .expect("catalog object")
        .value
        .as_dict()
        .expect("catalog dict")
        .clone();
    assert_eq!(catalog.get("List"), Some(&list), "数组里的 body 须逐字回读");
    let stream_object = reparsed
        .get_object(2)
        .expect("stream lookup")
        .expect("stream object");
    let stream_dict = stream_object.value.as_dict().expect("stream dict");
    assert_eq!(
        stream_dict.get("Sig"),
        Some(&Value::Ascii85String("z".into())),
        "流字典里的 body 须逐字回读"
    );
}

/// 经典 xref 表布局（1.4，逐对象顶层写出）同样字节对称。
#[test]
fn ascii85_string_reparses_under_the_classic_xref_layout() {
    let doc = doc_with_catalog(&[("Payload", Value::Ascii85String("9jqo^\n9jqo^".into()))]);
    let mut buf = Vec::new();
    writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("legal body at 1.4");
    assert!(String::from_utf8_lossy(&buf).contains("<~9jqo^\n9jqo^~>"));
    assert_eq!(
        write_then_catalog_at_1_4(&buf),
        Value::Ascii85String("9jqo^\n9jqo^".into())
    );
}

/// 1.4 产物的目录条目回读（`write_then_catalog` 用默认版本，故单独给出）。
fn write_then_catalog_at_1_4(buf: &[u8]) -> Value {
    let reparsed = parse_buf(buf);
    reparsed.load_all_objects().expect("load objects");
    let (root, _) = reparsed.trailer().root().expect("/Root");
    reparsed
        .get_object(root)
        .expect("lookup")
        .expect("catalog")
        .value
        .as_dict()
        .expect("catalog dict")
        .get("Payload")
        .cloned()
        .expect("/Payload")
}

/// body 含终止符 `~>`：**类型化拒绝而非静默截断的往返**。
///
/// 若放行，`write_value` 产出 `<~a~>b~>`，读端按「第一个 `~>`」收尾得到
/// `Ascii85String("a")`——文件能正常解析、值却是错的。这里断言写端根本不
/// 产出字节：`write_document` 返回 [`WriteError::InvalidObject`]，缓冲为空，
/// 因此不存在可被截断回读的产物。
#[test]
fn ascii85_string_body_with_the_terminator_is_typed_rejected_with_zero_bytes() {
    for version in [PdfVersion::V1_4, PdfVersion::V1_7] {
        let doc = doc_with_catalog(&[("Payload", Value::Ascii85String("a~>b".into()))]);
        let mut buf = Vec::new();
        let error = writer::DocumentWriter::new()
            .with_version(version)
            .build(&doc, &mut buf)
            .err()
            .unwrap_or_else(|| panic!("版本 {version} 必须拒绝含 EOD 的 body"));
        match error {
            WriteError::InvalidObject(message) => {
                assert!(
                    message.contains("~>") && message.contains("truncated"),
                    "拒绝理由须点名 EOD 截断：{message}"
                );
                assert!(message.contains("object 1"), "须给出对象号：{message}");
            }
            other => panic!("expected InvalidObject, got {other}"),
        }
        assert!(buf.is_empty(), "拒绝必须零字节：版本 {version}");
    }
}

/// 字母表外字符、单独 `~`、组中间的 `z`：同一类型化错误、同样零字节。
#[test]
fn ascii85_string_illegal_payload_forms_are_typed_rejected() {
    for (body, expected) in [
        ("9jqo*v", "outside the ASCII85 alphabet"),
        ("9jqo\u{7f}", "outside the ASCII85 alphabet"),
        ("9jqo^~", "TILDE"),
        ("9jqo^!z", "partial group"),
    ] {
        let doc = doc_with_catalog(&[("Payload", Value::Ascii85String(body.into()))]);
        let mut buf = Vec::new();
        let error = writer::write_document(&doc, &mut buf)
            .err()
            .unwrap_or_else(|| panic!("body {body:?} 必须被拒"));
        match error {
            WriteError::InvalidObject(message) => assert!(
                message.contains(expected),
                "body {body:?}: 期望理由含 {expected:?}，实得 {message}"
            ),
            other => panic!("body {body:?}: expected InvalidObject, got {other}"),
        }
        assert!(buf.is_empty(), "body {body:?}: 拒绝必须零字节");
    }
}

/// 装配期通道（`PdfBuilder::build`）与序列化咽喉点同一理由：非法 body 在
/// 任何字节写出之前被拒。取值通道用 14.7.5.3 的用户属性 `/V`（Table 329
/// 里它是任意值），落在装配好的文档对象里。
#[test]
fn ascii85_string_illegal_body_is_refused_by_the_builder_assembly_preflight() {
    use pdf_engine::core::builder::{
        StructureAttribute, StructureElement, StructureMarkInfo, StructureTree, UserProperty,
    };

    let tree_with = |body: &str| {
        StructureTree::new().with_element(StructureElement::new("P").with_attribute(
            StructureAttribute::user_properties(vec![UserProperty::new(
                "Payload",
                Value::Ascii85String(body.into()),
            )]),
        ))
    };
    let builder_with = |body: &str| {
        let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
        builder.page(PageSize::A4);
        builder.set_structure_tree(tree_with(body));
        builder.set_mark_info(
            StructureMarkInfo::new()
                .with_marked(true)
                .with_user_properties(true),
        );
        builder
    };

    let mut builder = builder_with("a~>b");
    let mut buf = Vec::new();
    let error = builder
        .build(&mut buf)
        .expect_err("装配期必须拒绝含 EOD 的 ascii85 body");
    match error {
        WriteError::InvalidObject(message) => assert!(
            message.contains("~>") && message.contains("truncated"),
            "装配期理由须与写出期一致：{message}"
        ),
        other => panic!("expected InvalidObject, got {other}"),
    }
    assert!(buf.is_empty(), "装配期拒绝必须零字节");

    // 合法 body 经同一通道写出并留在产物字节里。
    let mut builder = builder_with("9jqo^");
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("合法 body 可通过装配期");
    assert!(String::from_utf8_lossy(&buf).contains("<~9jqo^~>"));
}

/// `write_object` 的 trailer 顺带核验：经典表用无错 channel 的 `write_value`
/// 输出 trailer，故违规 body 在逐对象咽喉点同样被类型化拒绝。
#[test]
fn ascii85_string_illegal_body_in_the_trailer_is_typed_rejected() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Payload", Value::Ascii85String("a~>b".into())),
    };
    doc.xref = pdf_engine::core::types::Xref::new();
    doc.startxref = pdf_engine::core::types::StartXref(0);

    let mut buf = Vec::new();
    let error = writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect_err("trailer 里的非法 body 必须被拒");
    match error {
        WriteError::InvalidObject(message) => assert!(
            message.contains("trailer /Payload") && message.contains("~>"),
            "须指明 trailer 条目：{message}"
        ),
        other => panic!("expected InvalidObject, got {other}"),
    }
    assert!(buf.is_empty(), "拒绝必须零字节");
}
