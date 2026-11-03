//! 7.3.4 扩展语法 `<~ ~>` ASCII85 字符串：写出 → 解析 的往返证据。
//!
//! `Value::Ascii85String` 此前只有写端（`writer/value.rs` 产出 `<~ ... ~>`）
//! 而没有读端路径，类型是死代码：本库自己写出的文件（尤其 FDF 文本串值）
//! 自己的读端解析不回来。这里以真实文件字节走完整 `Parser` 管线，证明该
//! 缺陷已闭掉，并固定与写端逐字对称的语义口径。
//!
//! 该形态是**非 ISO 32000-1 语法**（标准只定义 ASCII85 过滤器 7.4.3），
//! 按生产者实态提供互操作支持，不进 `PdfFeature`、无版本门禁。

use super::*;

/// 写端包裹方式核对：`Emit` 把 payload **原样**夹在 `<~` 与 `~>` 之间，
/// 不做任何转义，故读端也必须逐字保留才能对称。
#[test]
fn ascii85_string_emitter_wraps_the_payload_verbatim_between_the_delimiters() {
    let mut out = String::new();
    core::writer::write_value(&Value::Ascii85String("9jqo^".into()), &mut out);
    assert_eq!(out, "<~9jqo^~>");

    // 含空白/杂散 `~` 的 payload 同样一字不动地写出（写端不转义）。
    let mut out = String::new();
    core::writer::write_value(&Value::Ascii85String("9jqo^~ \n".into()), &mut out);
    assert_eq!(out, "<~9jqo^~ \n~>");
}

/// 单个间接对象形态的最小 PDF：`body` 由调用方给出（通常直接来自写端）。
fn one_object_pdf(body: &str) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let object_at = buf.len();
    buf.extend_from_slice(format!("1 0 obj\n{body}\nendobj\n").as_bytes());
    let xref_at = buf.len();
    buf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{object_at:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(b"trailer\n<< /Root 1 0 R /Size 2 >>\n");
    buf.extend_from_slice(format!("startxref\n{xref_at}\n%%EOF\n").as_bytes());
    buf
}

/// 核心往返证据：写端产出的字节喂给读端，解析回**同一个值**。
#[test]
fn ascii85_string_written_by_emitter_reparses_to_same_value() {
    let original = Value::Ascii85String("9jqo^".into());
    let mut emitted = String::new();
    core::writer::write_value(&original, &mut emitted);

    let doc = parse_buf(&one_object_pdf(&emitted));
    let obj = doc
        .get_object(1)
        .expect("parse")
        .expect("object 1 must resolve");
    assert_eq!(obj.value, original);
}

/// 字典条目形态的往返（FDF/签名字典里的实际用法）。
#[test]
fn ascii85_string_written_by_emitter_reparses_inside_a_dict_through_the_real_file_path() {
    let original = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Catalog".into()))
            .insert("S", Value::Ascii85String("9jqo^".into()))
            .insert("Empty", Value::Ascii85String(String::new())),
    );
    let mut emitted = String::new();
    core::writer::write_value(&original, &mut emitted);

    let doc = parse_buf(&one_object_pdf(&emitted));
    let obj = doc
        .get_object(1)
        .expect("parse")
        .expect("object 1 must resolve");
    assert_eq!(obj.value, original);
    assert_eq!(
        obj.value.as_dict().unwrap().get("S").unwrap().type_name(),
        "ascii85-string"
    );
}

/// 以 `~` 结尾的 payload：写端产出 `...~~>`，读端停在 `~>`、把杂散 `~`
/// 留在 payload 内，因此仍是字节对称往返。
#[test]
fn ascii85_string_payload_ending_in_a_stray_tilde_reparses_to_same_value() {
    let original = Value::Ascii85String("9jqo^~".into());
    let mut emitted = String::new();
    core::writer::write_value(&original, &mut emitted);
    assert_eq!(emitted, "<~9jqo^~~>");

    let doc = parse_buf(&one_object_pdf(&emitted));
    let obj = doc
        .get_object(1)
        .expect("parse")
        .expect("object 1 must resolve");
    assert_eq!(obj.value, original);
}

/// payload 的语义口径：存 ASCII85 **编码文本**，用既有过滤器公共入口
/// `filters::decode_stream` 还原字节（本车道不改 `filters/**`）。
#[test]
fn ascii85_string_reparsed_from_emitter_output_decodes_to_the_original_bytes() {
    let mut emitted = String::new();
    core::writer::write_value(&Value::Ascii85String("9jqo^".into()), &mut emitted);

    let doc = parse_buf(&one_object_pdf(&emitted));
    let obj = doc
        .get_object(1)
        .expect("parse")
        .expect("object 1 must resolve");
    let Value::Ascii85String(payload) = &obj.value else {
        panic!("expected ascii85-string, got {}", obj.value.type_name());
    };
    let dict = Dict::new().insert("Filter", Value::Name("ASCII85Decode".into()));
    assert_eq!(
        core::filters::decode_stream(&dict, payload.as_bytes()).expect("decode"),
        b"Man "
    );
}

/// 未闭合（没有 `~>`）：类型化错误从对象解析处上抛，不静默给出错值。
#[test]
fn ascii85_string_unterminated_object_reports_a_typed_error_instead_of_a_wrong_value() {
    let doc = parse_buf(&one_object_pdf("<< /S <~9jqo^ >>"));
    let e = doc
        .get_object(1)
        .expect_err("unterminated ascii85 string must not resolve to a value");
    assert!(
        e.message.contains("unterminated ascii85 string"),
        "expected a typed unterminated-ascii85 error, got: {e}"
    );
}

/// 已知边界（写端前提，非读端缺陷）：payload 里含 `~>` 序列时，写端
/// 逐字包裹产出的文件本身就有歧义——读端按「第一个 `~>` 结束」取
/// payload。这里钉住**观察到的行为**：不 panic、不无限循环、后续 token
/// 不被吞成静默成功值之外的形态。真正修法在写端（本车道禁改）。
#[test]
fn ascii85_string_payload_containing_the_terminator_sequence_terminates_at_the_first_one() {
    let mut emitted = String::new();
    core::writer::write_value(&Value::Ascii85String("a~>b".into()), &mut emitted);
    assert_eq!(emitted, "<~a~>b~>");

    let doc = parse_buf(&one_object_pdf(&emitted));
    let obj = doc
        .get_object(1)
        .expect("parse")
        .expect("object 1 must resolve");
    assert_eq!(obj.value, Value::Ascii85String("a".into()));
}

/// 文档级写出（`write_document`）也要把该形态带回来，覆盖真实产出路径。
#[test]
fn ascii85_string_survives_a_full_document_write_and_reparse() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Payload", Value::Ascii85String("9jqo^".into())),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(2)),
    };

    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write document");

    let doc2 = parse_buf(&buf);
    let obj = doc2
        .get_object(1)
        .expect("parse")
        .expect("catalog must resolve");
    assert_eq!(
        obj.value.as_dict().unwrap().get("Payload"),
        Some(&Value::Ascii85String("9jqo^".into()))
    );
}
