//! `/ObjStm` 内联流对象的往返（7.5.7：对象流内**不得**存流对象）。
//!
//! 本主题针对一个「违例但可读」的输入：对象流（`/Type /ObjStm`）的载荷里，
//! 某个被打包对象本身又带 `stream … endstream` 正文。规范（ISO 32000-1
//! 7.5.7）明确禁止把流对象放进对象流，因此这类文件是**违例文档**；本测试
//! 锁定 crate 的处理立场：读端把它还原成一个真正的 `Value::Stream` 并给出
//! 类型化诊断，写端把它作为顶层对象写出、绝不重新打包进对象流——内容不得
//! 静默丢失。

use super::*;
use pdf_engine::core::reader::ObjectStreamViolation;

/// XRef 流一行的 `[1 2 1]` 编码（type / 字段2 / 字段3）。
fn xref_row(t: u8, f2: u16, f3: u8) -> Vec<u8> {
    let mut v = vec![t];
    v.extend_from_slice(&f2.to_be_bytes());
    v.push(f3);
    v
}

/// 构造一个「对象流内嵌流对象」的 1.5 PDF。
///
/// 物理布局：
/// - 对象 1 = Catalog（`/Marker 5 0 R`，顶层普通对象）；
/// - 对象 6 = `/Type /ObjStm`，内含对象 5；
/// - 对象 5 的本体是 `<< /Marker /Inline /Length 11 >> stream … endstream`，
///   即**被打包的对象自己是个流**（违例）；
/// - 对象 7 = XRef 流（`/W [1 2 1]`，对象 5 为 type 2 条目）。
///
/// `compressed` 决定对象流容器是否 `/FlateDecode` 真压缩。
fn objstm_with_inline_stream(compressed: bool) -> Vec<u8> {
    // 对象 5 在对象流载荷内的本体（违例的内联流）。
    let inner: &[u8] = b"<< /Marker /Inline /Length 11 >>\nstream\nhello world\nendstream";

    // 载荷头部：N 对「对象号 相对偏移」——这里是「5 0 」，4 字节。
    let header = b"5 0 ";
    let first = header.len();
    let mut stm: Vec<u8> = header.to_vec();
    stm.extend_from_slice(inner);

    let payload = if compressed { zlib(&stm) } else { stm };

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Marker 5 0 R >>\nendobj\n");

    // 对象 6：对象流容器。
    let off6 = buf.len();
    let filter = if compressed {
        " /Filter /FlateDecode"
    } else {
        ""
    };
    buf.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /ObjStm /N 1 /First {first}{filter} /Length {} >>\nstream\n",
            payload.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&payload);
    buf.extend_from_slice(b"\nendstream\nendobj\n");

    // 对象 7：XRef 流（条目覆盖对象 0..=7）。
    let off7 = buf.len();
    let mut xs: Vec<u8> = Vec::new();
    xs.extend_from_slice(&xref_row(0, 0, 255)); // 0：free
    xs.extend_from_slice(&xref_row(1, off1 as u16, 0)); // 1：Catalog
    for _ in 2..5 {
        xs.extend_from_slice(&xref_row(0, 0, 255)); // 2..=4：free
    }
    xs.extend_from_slice(&xref_row(2, 6, 0)); // 5：位于 ObjStm 6 内，索引 0
    xs.extend_from_slice(&xref_row(1, off6 as u16, 0)); // 6：ObjStm
    xs.extend_from_slice(&xref_row(1, off7 as u16, 0)); // 7：XRef 流自身
    buf.extend_from_slice(
        format!(
            "7 0 obj\n<< /Type /XRef /Size 8 /Index [0 8] /W [1 2 1] /Length {} /Root 1 0 R >>\nstream\n",
            xs.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&xs);
    buf.extend_from_slice(b"\nendstream\nendobj\nstartxref\n");
    buf.extend_from_slice(format!("{off7}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");
    buf
}

/// 取对象 5 并断言它是内容完好的流。
fn assert_inline_stream_preserved(doc: &Document, stage: &str) {
    let obj = doc
        .get_object(5)
        .unwrap_or_else(|e| panic!("{stage}: get_object(5) 失败：{e}"))
        .unwrap_or_else(|| panic!("{stage}: 对象 5（对象流内的流对象）未被解包"));
    let dict = match &obj.value {
        Value::Stream(d, _) => d,
        other => panic!("{stage}: 期望 Value::Stream，实际为 {other:?}"),
    };
    assert_eq!(
        dict.get("Marker").and_then(|v| v.as_name()),
        Some("Inline"),
        "{stage}: 内联流字典键丢失"
    );
    assert_eq!(
        doc.stream_bytes(&obj.value).as_deref(),
        Some(&b"hello world"[..]),
        "{stage}: 内联流正文字节丢失"
    );
}

/// 违例输入：读端把对象流内的流对象还原成真正的 `Value::Stream`（正文不丢），
/// 并给出类型化诊断而非静默转成字典。
#[test]
fn objstm_inline_stream_is_read_as_stream() {
    let doc = parse_buf(&objstm_with_inline_stream(false));
    assert_inline_stream_preserved(&doc, "读端");

    let issues = doc.object_stream_violations();
    assert_eq!(
        issues,
        vec![ObjectStreamViolation::StreamObjectInObjectStream {
            object_number: 5,
            stream_number: 6,
            index: 0,
            recovered: true,
        }],
        "违例诊断不符：{issues:?}"
    );
    // warnings 通道同样不能哑。
    assert!(
        doc.warnings()
            .iter()
            .any(|w| w.contains("7.5.7 violation") && w.contains("object 5")),
        "warning 缺失：{:?}",
        doc.warnings()
    );
}

/// 完整往返：builder 写 → parser 重读 → 断言。对象流内的流对象经
/// `DocumentWriter`（默认 1.7）后仍是内容一致的流。
#[test]
fn objstm_inline_stream_survives_write_reparse() {
    let doc = parse_buf(&objstm_with_inline_stream(false));
    assert_inline_stream_preserved(&doc, "往返前");

    let mut out = Vec::new();
    core::writer::DocumentWriter::new()
        .build(&doc, &mut out)
        .expect("写出应成功（内联流对象转为顶层对象，不静默丢弃）");

    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("hello world"), "输出缺少内联流正文：{text}");

    let doc2 = parse_buf(&out);
    assert_inline_stream_preserved(&doc2, "重读后");
    // writer 已把该流对象放到顶层，重写后的文档不应再报告违例。
    assert!(
        doc2.object_stream_violations().is_empty(),
        "重写后的文档不应再含对象流内联流：{:?}",
        doc2.object_stream_violations()
    );
}

/// 压缩对象流（`/FlateDecode`）内的违例流对象同样要保住正文。
#[test]
fn objstm_inline_stream_survives_write_reparse_when_objstm_flated() {
    let doc = parse_buf(&objstm_with_inline_stream(true));
    assert_inline_stream_preserved(&doc, "压缩往返前");

    let mut out = Vec::new();
    core::writer::DocumentWriter::new()
        .build(&doc, &mut out)
        .expect("写出应成功");
    let doc2 = parse_buf(&out);
    assert_inline_stream_preserved(&doc2, "压缩重读后");
}

/// 1.4 目标版本（朴素布局，无对象流）：内联流对象仍是顶层流对象。
#[test]
fn objstm_inline_stream_written_plain_for_1_4() {
    let doc = parse_buf(&objstm_with_inline_stream(false));
    let out = try_write(&doc, PdfVersion::V1_4).expect("1.4 写出成功");
    let text = String::from_utf8_lossy(&out);
    assert!(!text.contains("/Type /ObjStm"), "1.4 不应有对象流：{text}");
    let doc2 = parse_buf(&out);
    assert_inline_stream_preserved(&doc2, "1.4 重读后");
}
