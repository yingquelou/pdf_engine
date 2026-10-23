//! writer 模块测试。

use super::*;
use crate::core::types::{Dict, Document, Object, StreamData, Value};

#[test]
fn serialize_simple_values() {
    let mut s = String::new();
    write_value(&Value::Integer(42), &mut s);
    assert_eq!(s, "42");
    s.clear();
    write_value(&Value::Name("Type".into()), &mut s);
    assert_eq!(s, "/Type");
    s.clear();
    write_value(&Value::Ref(3, 0), &mut s);
    assert_eq!(s, "3 0 R");
}

#[test]
fn serialize_dict_and_array() {
    let mut s = String::new();
    let d = Dict::new()
        .insert("Name", Value::Name("Catalog".into()))
        .insert("Pages", Value::Ref(2, 0));
    write_value(&Value::Dict(d), &mut s);
    assert!(s.contains("/Name /Catalog"));
    assert!(s.contains("/Pages 2 0 R"));

    s.clear();
    let a = crate::core::types::Array::new()
        .push(Value::Integer(1))
        .push(Value::Integer(2));
    write_value(&Value::Array(a), &mut s);
    assert_eq!(s, "[1 2]");
}

#[test]
fn write_object_stream() {
    let obj = Object::new(
        1,
        0,
        Value::Stream(
            Dict::new().insert("Length", Value::Integer(5)),
            StreamData::Owned(b"hello".to_vec()),
        ),
    );
    let doc = Document::from_objects(
        vec![obj],
        crate::core::types::Xref::new(),
        crate::core::types::Trailer::new(),
        crate::core::types::StartXref(0),
    );
    let obj = doc.get_object(1).unwrap().unwrap();
    let bytes = write_object(&obj, &doc).unwrap();
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("1 0 obj"));
    assert!(s.contains("stream"));
    assert!(s.contains("hello"));
    assert!(s.contains("endstream"));
    assert!(s.contains("endobj"));
}

#[test]
fn roundtrip_minimal_document() {
    use crate::core::reader::Parser;
    use crate::core::types::{Document, Object, StartXref, Trailer, Value, Xref};

    // 构造一个最小 PDF：1 个 Catalog 对象
    let mut doc = Document::new();
    let catalog = Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    );
    doc.set_object(catalog);
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc.xref = Xref::new();
    doc.startxref = StartXref(0);

    let mut buf = Vec::new();
    write_document(&doc, &mut buf).unwrap();
    let out = String::from_utf8_lossy(&buf);
    // 默认 1.7 ≥1.5：输出 XRef 流 + 对象流（无经典 xref 表/trailer）。
    assert!(out.contains("%PDF-1.7"));
    assert!(out.contains("/Type /XRef"));
    assert!(out.contains("/Type /ObjStm"));
    assert!(out.contains("/Root 1 0 R"));
    assert!(!out.contains("\nxref\n"), "不应再有经典 xref 表");
    assert!(out.contains("startxref"));
    assert!(out.contains("%%EOF"));

    // 回读验证（懒加载：get_object 触发解析）。
    // Parser 现为文件式：把 buf 写临时文件再打开。
    use std::io::Write as _;
    // 沙箱策略按镜像路径拦截：target 目录内的二进制禁止写系统 %TEMP%，
    // 临时文件统一落在构建目录 target/test-tmp/ 下；无法定位 target 时
    // 回退系统临时目录。
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    let tmp = dir.join(format!(
        "demo-writer-rt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    {
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(&buf).unwrap();
        f.flush().unwrap();
    }
    let file = std::fs::File::open(&tmp).unwrap();
    let mut p = Parser::new(file).unwrap();
    let doc2 = p.parse_document().unwrap();
    let cat = doc2.get_object(1).unwrap().expect("catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
}
