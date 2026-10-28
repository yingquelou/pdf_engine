//! 集成测试共享辅助。
//!
//! 该目录不是独立测试目标（`tests/support/mod.rs`），由各主题文件
//! `mod support;` 引入；每个主题目标各自编译一份，未用到的辅助函数在
//! 对应目标里视为死代码，故整模块允许 dead_code。

#![allow(dead_code)]

use pdf_engine::core::reader::Parser;
use pdf_engine::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

/// 把字节缓冲写成临时文件，用 Parser 解析为 Document。
/// 避免测试持有整个文件内存，与生产路径一致。
pub fn parse_buf(buf: &[u8]) -> Document {
    parse_buf_result(buf, None).unwrap()
}

pub fn parse_buf_with_password(buf: &[u8], password: &[u8]) -> Document {
    parse_buf_result(buf, Some(password)).unwrap()
}

pub fn parse_buf_result(
    buf: &[u8],
    password: Option<&[u8]>,
) -> Result<Document, pdf_engine::core::reader::ParseError> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    // 沙箱策略按镜像路径拦截：target 目录内的二进制禁止写系统 %TEMP%，
    // 临时文件统一落在构建目录 target/test-tmp/ 下。
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!(
        "demo-test-{}-{}-{}.pdf",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        seq
    ));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(buf).unwrap();
    f.sync_all().unwrap();
    drop(f);
    let file = std::fs::File::open(&path).unwrap();
    let mut p = Parser::new(file).unwrap();
    if let Some(password) = password {
        p = p.with_password(password);
    }
    let doc = p.parse_document();
    let _ = std::fs::remove_file(&path);
    doc
}

/// 构造一个最小但完整的 PDF 文档。
pub fn sample_doc() -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
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
        Value::Stream(
            Dict::new(),
            StreamData::Owned(b"BT 72 720 Td (hi) Tj ET".to_vec()),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };
    doc
}
