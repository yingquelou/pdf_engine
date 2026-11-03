//! 文档级与文件级测试：从临时文件解析完整 PDF、全文扫描回退。

use super::support::*;
use super::*;
use crate::core::reader::ObjectStreamViolation;
use crate::core::types::{PdfVersion, StreamData, Value};

#[test]
fn parse_document_from_file() {
    // 构造最小 PDF 写入临时文件，用 Parser 解析。
    let raw = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\nxref\n0 3\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \ntrailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n114\n%%EOF\n";
    let (f, _path) = write_tmp(raw, "minimal");
    let mut p = Parser::new(f).expect("parser");
    let doc = p.parse_document().expect("parse");
    assert_eq!(doc.source_version(), PdfVersion::V1_4);
    assert_eq!(doc.trailer().root(), Some((1, 0)));
    // 懒加载：打开后缓存为空
    assert_eq!(doc.cached_object_count(), 0);
    // 触发懒加载
    let cat = doc.get_object(1).expect("parse").expect("catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    assert_eq!(doc.cached_object_count(), 1);
}

#[test]
fn full_scan_finds_objects() {
    // 无 xref 关键字，强制全文扫描回退。
    let raw = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n2 0 obj\n<< /Type /Pages >>\nendobj\ntrailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n";
    let mut src = slice_source(raw);
    let (xref, trailer) = scan_all_objects(&mut src).expect("scan");
    assert!(xref.get(1).is_some());
    assert_eq!(trailer.root(), Some((1, 0)));
}

// ── 7.5.7：对象流内的流对象（违例文档） ──────────────────────────────
//
// 规范原文（docs/PDF32000_2008.pdf，pdftotext -raw 第 2240–2241 行）：
// 「The following objects shall not be stored in an object stream:
//  ▪ Stream objects」。这类文件是违例文档，读端立场是「宽进 + 类型化诊断」：
// 正文不得静默丢弃，违例必须可枚举，且绝不 panic。

/// 违例文档里被打包的流对象：读端还原成 `Value::Stream` + `Owned` 正文
/// （对象流载荷是解码后的内存副本，其偏移在源文件里不可寻址，故不能用
/// `Ref`），并报告类型化违例与 warning。
#[test]
fn obj_stream_inline_stream_is_recovered_as_owned_stream() {
    let (f, _path) = write_tmp(&objstm_inline_stream_pdf(11), "objstm-inline");
    let mut p = Parser::new(f).expect("parser");
    let doc = p.parse_document().expect("parse");

    let obj = doc
        .get_object(5)
        .expect("get_object must not error")
        .expect("inline stream object must resolve");
    let dict = match &obj.value {
        Value::Stream(d, StreamData::Owned(_)) => d,
        other => panic!("expected Stream with Owned body, got {other:?}"),
    };
    assert_eq!(obj.num, 5);
    assert_eq!(obj.generation, 0, "对象流内对象规范上 generation 为 0");
    assert_eq!(
        dict.get("Marker").and_then(|v| v.as_name()),
        Some("Inline"),
        "内联流字典键须原样保留"
    );
    assert_eq!(
        doc.stream_bytes(&obj.value).as_deref(),
        Some(&b"hello world"[..]),
        "内联流正文不得丢失"
    );

    assert_eq!(
        doc.object_stream_violations(),
        vec![ObjectStreamViolation::StreamObjectInObjectStream {
            object_number: 5,
            stream_number: 6,
            index: 0,
            recovered: true,
        }],
        "违例清单不符"
    );
    assert!(
        doc.warnings()
            .iter()
            .any(|w| w.contains("7.5.7 violation") && w.contains("object 5")),
        "warnings 通道不得哑：{:?}",
        doc.warnings()
    );
}

/// 内联 `stream` 尾部不可定位（`/Length` 声明越过载荷窗口）时：不得报错、
/// 不得 panic——对象退回普通字典保留，违例以 `recovered: false` 登记。
#[test]
fn obj_stream_inline_stream_with_bad_length_keeps_dict() {
    let (f, _path) = write_tmp(&objstm_inline_stream_pdf(99_999), "objstm-inline-bad");
    let mut p = Parser::new(f).expect("parser");
    let doc = p.parse_document().expect("parse");

    let obj = doc
        .get_object(5)
        .expect("不可定位的内联流尾部不得升级为错误")
        .expect("字典本体仍须可取");
    let dict = match &obj.value {
        Value::Dict(d) => d,
        other => panic!("expected the dictionary fallback, got {other:?}"),
    };
    assert_eq!(dict.get("Marker").and_then(|v| v.as_name()), Some("Inline"));

    assert_eq!(
        doc.object_stream_violations(),
        vec![ObjectStreamViolation::StreamObjectInObjectStream {
            object_number: 5,
            stream_number: 6,
            index: 0,
            recovered: false,
        }],
        "不可还原的违例也必须登记"
    );
}

/// 合规对象流（只打包字典）不得被误报——XRef 流 + ObjStm 线性化夹具。
#[test]
fn conforming_object_stream_reports_no_violations() {
    let (f, _path) = write_tmp(&build_linearized_xref_stream(b"hint"), "objstm-conforming");
    let mut p = Parser::new(f).expect("parser");
    let doc = p.parse_document().expect("parse");
    assert!(
        doc.object_stream_violations().is_empty(),
        "合规对象流不应报违例：{:?}",
        doc.object_stream_violations()
    );
    // 只读诊断不得污染缓存：扫描前后已缓存对象数一致。
    let before = doc.cached_object_count();
    assert!(
        doc.object_stream_violations().is_empty(),
        "二次扫描也应无违例"
    );
    assert_eq!(
        doc.cached_object_count(),
        before,
        "object_stream_violations 不得写缓存"
    );
}
