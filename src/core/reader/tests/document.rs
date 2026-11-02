//! 文档级与文件级测试：从临时文件解析完整 PDF、全文扫描回退。

use super::*;
use crate::core::types::PdfVersion;

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
