//! 集成测试：解析 → 生成 → 再解析的往返，以及 API 契约。

use aes::Aes128;
use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
use demo::pdf;
use demo::pdf::reader::Parser;
use demo::pdf::types::{
    Array, Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Trailer, Value, XrefKind,
};
use md5::{Digest, Md5};
use rc4::{
    Rc4, StreamCipher,
    consts::{U5, U10, U16},
};
use std::io::Write;

/// 把字节缓冲写成临时文件，用 Parser 解析为 Document。
/// 避免测试持有整个文件内存，与生产路径一致。
fn parse_buf(buf: &[u8]) -> Document {
    parse_buf_result(buf, None).unwrap()
}

fn parse_buf_with_password(buf: &[u8], password: &[u8]) -> Document {
    parse_buf_result(buf, Some(password)).unwrap()
}

fn parse_buf_result(
    buf: &[u8],
    password: Option<&[u8]>,
) -> Result<Document, demo::pdf::reader::ParseError> {
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
fn sample_doc() -> Document {
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

#[test]
fn roundtrip_preserves_structure() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    pdf::writer::write_document(&doc, &mut buf).expect("write");

    let doc2 = parse_buf(&buf);

    assert_eq!(doc2.trailer().root(), Some((1, 0)));
    let cat = doc2.get_object(1).expect("parse").expect("catalog present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    // Pages.Kids[0] == 3 0 R
    let pages = doc2.get_object(2).expect("parse").expect("pages present");
    let kids = pages
        .value
        .as_dict()
        .unwrap()
        .get("Kids")
        .and_then(|v| v.as_array())
        .unwrap();
    assert_eq!(kids.iter().next().unwrap().as_ref(), Some((3, 0)));
    // 流内容存在且非空（按需从文件读取，不常驻内存）
    let page = doc2.get_object(3).expect("parse").expect("page present");
    let stream_bytes = doc2.stream_bytes(&page.value).expect("stream bytes");
    assert!(!stream_bytes.is_empty());
}

#[test]
fn xref_offsets_are_valid() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    pdf::writer::write_document(&doc, &mut buf).expect("write");

    // 逐条目校验：
    // - 普通条目（type 1，按偏移定位）偏移处应能读到 "N G obj"；
    // - 对象流内条目（type 2，≥1.5 打包）没有文件偏移，改为验证按该条目
    //   能取回对象、且其对象流自身是可直接定位的流对象。
    let doc2 = parse_buf(&buf);
    for sub in &doc2.xref().0 {
        for (i, e) in sub.entries.iter().enumerate() {
            if !e.in_use() {
                continue;
            }
            let num = sub.start + i as i64;
            match &e.kind {
                XrefKind::Normal => {
                    let head = &buf[e.offset as usize..];
                    let line_end = head.iter().position(|&b| b == b'\n').unwrap_or(head.len());
                    let line = std::str::from_utf8(&head[..line_end]).unwrap_or("");
                    assert!(
                        line.starts_with(&format!("{num} {} obj", e.generation)),
                        "offset {:#x}: line = {:?}",
                        e.offset,
                        line
                    );
                }
                XrefKind::InObjStream { stream_num, .. } => {
                    let obj = doc2
                        .get_object(num)
                        .expect("parse")
                        .expect("packed object present");
                    assert_eq!(obj.num, num);
                    let stm = doc2
                        .get_object(*stream_num)
                        .expect("parse")
                        .expect("object stream present");
                    assert!(
                        matches!(stm.value, Value::Stream(_, _)),
                        "对象流自身应是可直接定位的流对象"
                    );
                }
                XrefKind::Free => {}
            }
        }
    }
}

/// 懒加载核心验证：打开 Document 后不应解析任何对象；
/// 只在 `get_object` 调用时才解析被请求的对象并缓存。
///
/// 默认 1.7 输出会把非流对象（Catalog、Pages）打包进一个对象流（ObjStm），
/// 因此取一个打包对象时会连带解析其**容器**对象流（对象 4），故计数为
/// 「被请求对象 + 其对象流」。其余无关对象（内容流对象 3）仍不被解析。
#[test]
fn lazy_load_only_requested_objects() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    pdf::writer::write_document(&doc, &mut buf).expect("write");

    let doc2 = parse_buf(&buf);

    // 打开后缓存应为空：未解析任何对象
    assert_eq!(
        doc2.cached_object_count(),
        0,
        "no objects should be parsed on open"
    );

    // 请求对象 2（打包在对象流内）：解析对象 2 及其对象流容器，共 2 个；
    // 对象 1/3 仍未被解析。
    let _pages = doc2.get_object(2).expect("parse").expect("obj 2 present");
    assert_eq!(
        doc2.cached_object_count(),
        2,
        "only the requested object and its object stream should be parsed"
    );

    // 再次请求对象 2：命中缓存，不新增解析
    let _pages2 = doc2.get_object(2).expect("parse").expect("obj 2 present");
    assert_eq!(
        doc2.cached_object_count(),
        2,
        "cache hit should not reparse"
    );

    // 请求对象 1（打包在同一对象流内）：对象流已缓存，只再解析对象 1，共 3 个
    let _cat = doc2.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(doc2.cached_object_count(), 3);

    // 内容流对象 3 至此仍未被解析（即便它在文件中且 xref 有可直接定位的条目）
    assert_eq!(doc2.cached_object_count(), 3);
}

/// 渲染指定页面的典型链路：沿 Root → Pages → Page，
/// 只解析这条链上的对象，不触及无关对象。
#[test]
fn render_one_page_walks_minimal_chain() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    pdf::writer::write_document(&doc, &mut buf).expect("write");

    let doc2 = parse_buf(&buf);

    // 渲染第 0 页的最小对象链
    let root_num = doc2.trailer().root().expect("root").0;
    let catalog = doc2.get_object(root_num).expect("parse").expect("catalog");
    let pages_ref = catalog
        .value
        .as_dict()
        .unwrap()
        .get("Pages")
        .and_then(|v| v.as_ref())
        .expect("Pages ref");
    let pages = doc2.get_object(pages_ref.0).expect("parse").expect("pages");
    let first_kid = pages
        .value
        .as_dict()
        .unwrap()
        .get("Kids")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .as_ref()
        .unwrap();
    let _page = doc2.get_object(first_kid.0).expect("parse").expect("page");

    // 默认 1.7 下 Catalog/Pages 打包在同一对象流（对象 4）内，故本链解析
    // 了「对象流容器 + Catalog + Pages + Page」共 4 个对象；已无其它无关对象
    // 被触及（文件内不再有第 5 个内容对象）。
    assert_eq!(
        doc2.cached_object_count(),
        4,
        "rendering one page should parse only Catalog/Pages/Page and their object stream"
    );
}

/// reader 读文件头声明版本，且声明的版本无论高低都能解析。
#[test]
fn reader_parses_declared_version() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    pdf::writer::write_document(&doc, &mut buf).expect("write");

    let doc2 = parse_buf(&buf);
    // writer 默认写 1.7（输出 XRef 流 + 对象流）
    assert_eq!(doc2.source_version(), PdfVersion::V1_7);
    // 1.7 支持 XRef 流与对象流，按内容解析，无回退/宽读 warning
    assert!(
        doc2.warnings().is_empty(),
        "1.7 xref stream/object stream should produce no warnings: {:?}",
        doc2.warnings()
    );
}

/// writer 不指定版本时用自身 default（1.7），**不再继承** doc.source_version。
/// （架构修复：读端版本与写端版本彻底解耦。）
#[test]
fn writer_defaults_to_1_7_not_doc_source_version() {
    let mut doc = sample_doc();
    // 设 source_version 为低版本（1.3），但 writer 应忽略
    doc.source_version = PdfVersion::V1_3;
    let mut buf = Vec::new();
    pdf::writer::write_document(&doc, &mut buf).expect("write");
    // 头部应是 "%PDF-1.7" 而非 "%PDF-1.3"
    assert!(
        buf.starts_with(b"%PDF-1.7\n"),
        "writer 应输出自身 default 1.7，而非 doc.source_version；head was: {:?}",
        &buf[..10.min(buf.len())]
    );
    // 回读版本应为 writer 输出的 1.7
    let doc2 = parse_buf(&buf);
    assert_eq!(doc2.source_version(), PdfVersion::V1_7);
}

/// writer 拒绝不支持的版本（2.0）——通过 with_version 显式指定。
/// （架构修复：writer 不再查看 doc.source_version，
/// 版本校验只针对 writer 自身的 with_version 设置。）
#[test]
fn writer_rejects_unsupported_version() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    let e = pdf::writer::DocumentWriter::new()
        .with_version(PdfVersion::V2_0)
        .build(&doc, &mut buf)
        .unwrap_err();
    assert!(matches!(e, pdf::writer::WriteError::UnsupportedVersion(_)));
}

/// xref 损坏时回退到全文扫描，仍能取到对象。
/// 显式用 with_version 1.4 让 writer 输出经典 `xref` 表形态。
#[test]
fn damaged_xref_falls_back_to_full_scan() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    pdf::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("write");

    // 把 "xref" 关键字改成 "XXXX"，强制经典表与 XRef 流策略均不适用，
    // 链回退到全文扫描。
    let mut corrupted = buf.clone();
    if let Some(pos) = corrupted.windows(4).position(|w| w == b"xref") {
        corrupted[pos..pos + 4].copy_from_slice(b"XXXX");
    }

    let doc2 = parse_buf(&corrupted);
    // 全文扫描应能找到对象 1（Catalog）——这是回退生效的核心证据。
    let cat = doc2.get_object(1).expect("parse").expect("catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
}

/// 文件无有效 %PDF 头时，版本回退为 1.0 并 warning。
#[test]
fn missing_header_defaults_version_with_warning() {
    // 构造一个没有 %PDF 头的 PDF（仅对象 + xref + trailer + startxref）
    let doc = sample_doc();
    let mut buf = Vec::new();
    pdf::writer::write_document(&doc, &mut buf).expect("write");
    // 剥掉头部 "%PDF-x.y\n%....\n"
    let stripped = strip_pdf_header(&buf);

    let doc2 = parse_buf(&stripped);
    assert_eq!(doc2.source_version(), PdfVersion::V1_0);
    assert!(
        doc2.warnings().iter().any(|w| w.contains("header")),
        "expected header warning: {:?}",
        doc2.warnings()
    );
}

/// 跳过文件头部的 `%PDF-x.y\n%<4 bytes>\n`（共 14 字节左右）。
fn strip_pdf_header(buf: &[u8]) -> Vec<u8> {
    // 找到第二个换行后的位置
    let mut nl_count = 0;
    for (i, &b) in buf.iter().enumerate() {
        if b == b'\n' {
            nl_count += 1;
            if nl_count == 2 {
                return buf[i + 1..].to_vec();
            }
        }
    }
    buf.to_vec()
}

/// DocumentWriter Builder 可指定版本（等价于 writer 的目标版本设定）。
#[test]
fn writer_builder_with_version() {
    let doc = sample_doc(); // 默认 1.7
    let mut buf = Vec::new();
    pdf::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_3)
        .build(&doc, &mut buf)
        .expect("build");
    assert!(buf.starts_with(b"%PDF-1.3"), "head: {:?}", &buf[..9]);

    // 回读版本应为 1.3（来自写入头部，而非 doc.version）
    let doc2 = parse_buf(&buf);
    assert_eq!(doc2.source_version(), PdfVersion::V1_3);
}

/// XRef 流 /W 含 0 宽字段时按规范取缺省值：type（w1=0）缺省为 1
/// （in-use）。修复前 type 恒为 0（free），条目不可用。
#[test]
fn xref_stream_zero_type_width_defaults_in_use() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    // XRef 流对象 3：/W [0 2 2]，type 字段宽度为 0（缺省 1）。
    // 每行 4 字节：offset(2) + generation(2)。
    let off_stm = buf.len();
    let mut stream: Vec<u8> = Vec::new();
    stream.extend_from_slice(&(off1 as u16).to_be_bytes());
    stream.extend_from_slice(&0u16.to_be_bytes());
    stream.extend_from_slice(&(off2 as u16).to_be_bytes());
    stream.extend_from_slice(&0u16.to_be_bytes());
    let hdr = format!(
        "3 0 obj\n<< /Type /XRef /Size 3 /Index [1 2] /W [0 2 2] /Length {} /Root 1 0 R >>\nstream\n",
        stream.len()
    );
    buf.extend_from_slice(hdr.as_bytes());
    buf.extend_from_slice(&stream);
    buf.extend_from_slice(b"\nendstream\nendobj\nstartxref\n");
    buf.extend_from_slice(format!("{off_stm}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");

    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    // 缺省 type=1 → 对象 1、2 为 in-use，能按偏移取到。
    let cat = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    let pages = doc.get_object(2).expect("parse").expect("obj 2 present");
    assert_eq!(
        pages
            .value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Pages")
    );
}

/// 混合引用文件：经典 xref 表 + trailer 含 /XRefStm 指向补充 XRef 流。
/// 流中对象 3 的条目对经典表不可见，应与经典表合并后可见。
#[test]
fn hybrid_reference_xref_stm_merges_entries() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let off3 = buf.len();
    buf.extend_from_slice(
        b"3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    );
    // 经典 xref 表：仅覆盖对象 0..2，对象 3 不在其中。
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    // 补充 XRef 流对象 4：只给出对象 3 的条目，/W [1 2 1]。
    let off_stm = buf.len();
    let mut stream: Vec<u8> = Vec::new();
    stream.push(1u8); // type = 1（in-use）
    stream.extend_from_slice(&(off3 as u16).to_be_bytes());
    stream.push(0u8); // generation = 0
    let hdr = format!(
        "4 0 obj\n<< /Type /XRef /Size 5 /Index [3 1] /W [1 2 1] /Length {} /Root 1 0 R >>\nstream\n",
        stream.len()
    );
    buf.extend_from_slice(hdr.as_bytes());
    buf.extend_from_slice(&stream);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    // 经典 trailer 同时含 /XRefStm。
    buf.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R /XRefStm {off_stm} >>\n").as_bytes(),
    );
    buf.extend_from_slice(b"startxref\n");
    buf.extend_from_slice(format!("{off_xref}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");

    let doc = parse_buf(&buf);
    // 对象 3 只存在于 XRef 流，须由 /XRefStm 合并后才可定位。
    let font = doc
        .get_object(3)
        .expect("parse")
        .expect("obj 3 present via /XRefStm");
    assert_eq!(
        font.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Font")
    );
    // 经典表条目仍可见。
    assert!(doc.get_object(1).expect("parse").is_some());
}

/// Catalog /Version 覆盖：头部声明 1.4，根 Catalog 含 /Version /1.5，
/// 有效版本取两者较大者 1.5。
#[test]
fn catalog_version_overrides_header_version() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Version /1.5 >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    buf.extend_from_slice(b"startxref\n");
    buf.extend_from_slice(format!("{off_xref}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");

    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
}

/// 读端宽读 2.0：`%PDF-2.0` 文件不应被当作「无有效头部」而回退 1.0，
/// 应标记为 2.0 且对象可正常取出。
#[test]
fn reader_accepts_pdf_2_0_header() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-2.0\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    buf.extend_from_slice(b"startxref\n");
    buf.extend_from_slice(format!("{off_xref}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");

    let doc = parse_buf(&buf);
    // 识别为 2.0（而非回退 1.0）
    assert_eq!(doc.source_version(), PdfVersion::V2_0);
    // 不得产生「无有效头部」的 warning
    assert!(
        !doc.warnings().iter().any(|w| w.contains("header")),
        "2.0 header must not be treated as missing: {:?}",
        doc.warnings()
    );
    // 对象仍可按 xref 正常取出
    let cat = doc.get_object(1).expect("parse").expect("catalog present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    let pages = doc.get_object(2).expect("parse").expect("pages present");
    assert_eq!(
        pages
            .value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Pages")
    );
}

/// 生成一条 XRef 流条目（`/W [1 2 1]`：1 字节 type + 2 字节字段 + 1 字节字段）。
fn xref_row(t: u8, f2: u16, f3: u8) -> Vec<u8> {
    let mut v = vec![t];
    v.extend_from_slice(&f2.to_be_bytes());
    v.push(f3);
    v
}

/// 追加一个 XRef 流对象并以 `startxref` + `%%EOF` 收尾（`/W [1 2 1]`、
/// `/Index [0 index_size]`、`/Size index_size`、`/Root 1 0 R`）。
///
/// `num` 为 XRef 流自身对象号；`off` 为其文件偏移——调用方须在构造条目
/// **之前**以 `buf.len()` 快照（条目可能引用该偏移）；`dict_extra` 追加到
/// 流字典（如 `/Filter`、`/DecodeParms`）。
fn append_xref_stream(
    buf: &mut Vec<u8>,
    num: i64,
    off: usize,
    index_size: usize,
    dict_extra: &str,
    entries: &[u8],
) {
    let hdr = format!(
        "{num} 0 obj\n<< /Type /XRef /Size {index_size} /Index [0 {index_size}] \
         /W [1 2 1]{dict_extra} /Length {} /Root 1 0 R >>\nstream\n",
        entries.len()
    );
    buf.extend_from_slice(hdr.as_bytes());
    buf.extend_from_slice(entries);
    buf.extend_from_slice(b"\nendstream\nendobj\nstartxref\n");
    buf.extend_from_slice(format!("{off}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");
}

/// 构造一个含 XRef 流（type 1 顶层对象 + type 2 指向对象流）的 PDF：
/// 对象 1 为 Catalog（引用对象 5）；对象 6 为对象流（ObjStm，内含对象
/// 5）；对象 7 为 XRef 流。
///
/// - `filter`：对象流的 `/Filter` 名（空串 = 未压缩）；
/// - `compress`：为真时用 zlib 真压缩对象流字节（需配 `/FlateDecode`）。
fn obj_stm_pdf(filter: &str, compress: bool) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Marker 5 0 R >>\nendobj\n");
    // 对象 6：对象流，头部为「对象号 相对偏移」对，随后是对象本体。
    let inner = b"<< /Marker /InObjStm >>";
    let mut stm: Vec<u8> = Vec::new();
    stm.extend_from_slice(b"5 0 "); // 对象 5，相对偏移 0
    stm.extend_from_slice(inner);
    let first = 4usize; // stm 中对象本体的起始偏移
    let payload = if compress { zlib(&stm) } else { stm };
    let off6 = buf.len();
    let filter_hdr = if filter.is_empty() {
        String::new()
    } else {
        format!(" /Filter /{filter}")
    };
    let hdr6 = format!(
        "6 0 obj\n<< /Type /ObjStm /N 1 /First {first}{filter_hdr} /Length {} >>\nstream\n",
        payload.len()
    );
    buf.extend_from_slice(hdr6.as_bytes());
    buf.extend_from_slice(&payload);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    // 对象 7：XRef 流，/W [1 2 1]，/Index [0 8]（条目覆盖对象 0..=7）。
    let off7 = buf.len();
    let mut xs: Vec<u8> = Vec::new();
    xs.extend_from_slice(&xref_row(0, 0, 255)); // 0：自由条目
    xs.extend_from_slice(&xref_row(1, off1 as u16, 0)); // 1：Catalog
    xs.extend_from_slice(&xref_row(0, 0, 0)); // 2
    xs.extend_from_slice(&xref_row(0, 0, 0)); // 3
    xs.extend_from_slice(&xref_row(0, 0, 0)); // 4
    xs.extend_from_slice(&xref_row(2, 6, 0)); // 5：位于对象流 6 内，索引 0（type 2）
    xs.extend_from_slice(&xref_row(1, off6 as u16, 0)); // 6：对象流本身
    xs.extend_from_slice(&xref_row(1, off7 as u16, 0)); // 7：XRef 流本身
    append_xref_stream(&mut buf, 7, off7, 8, "", &xs);
    buf
}

/// 未压缩对象流（ObjStm）正常路径：XRef 流 type 2 条目指向对象流内
/// 的对象，`get_object` 应能解析并取值正确。
#[test]
fn uncompressed_obj_stm_resolves_inner_object() {
    let buf = obj_stm_pdf("", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);

    // 顶层对象（type 1）仍可正常取出，并正确引用对象 5。
    let cat = doc.get_object(1).expect("parse").expect("catalog present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_ref()),
        Some((5, 0))
    );

    // 对象 5 位于对象流内，type 2 条目应驱动对象流解析。
    let inner = doc
        .get_object(5)
        .expect("parse")
        .expect("object inside object stream");
    assert_eq!(inner.num, 5);
    assert_eq!(inner.generation, 0);
    assert_eq!(
        inner
            .value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("InObjStm")
    );
    // 再次请求命中缓存，不重复解析。
    assert!(doc.get_object(5).expect("parse").is_some());

    assert!(
        doc.warnings().is_empty(),
        "unexpected warnings: {:?}",
        doc.warnings()
    );
}

/// 增量更新 + `/Prev` 链：初始修订定义对象 1=100、对象 2；尾追修订
/// 只更新对象 1=200（其 xref 仅含对象 1），trailer `/Prev` 指向初始 xref。
/// 期望：`get_object(1)` 得最新的 200（最新修订优先），对象 2 仍由历史
/// 修订补充可见（最新修订未提及）。
#[test]
fn incremental_update_prev_chain_merges_history() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n100\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Marker /History >>\nendobj\n");
    // 初始修订：经典 xref 表，覆盖对象 0..=2。
    let off_xref1 = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{off_xref1}\n%%EOF\n").as_bytes(),
    );
    // 尾追修订：更新对象 1=200，xref 仅含对象 1，trailer /Prev 指向初始 xref。
    let off1b = buf.len();
    buf.extend_from_slice(b"1 0 obj\n200\nendobj\n");
    let off_xref2 = buf.len();
    buf.extend_from_slice(b"xref\n1 1\n");
    buf.extend_from_slice(format!("{off1b:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 3 /Root 1 0 R /Prev {off_xref1} >>\nstartxref\n{off_xref2}\n%%EOF\n"
        )
        .as_bytes(),
    );

    let doc = parse_buf(&buf);
    // 最新修订更新了对象 1 → 取最新值 200（历史 100 不覆盖）。
    let o1 = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(o1.value, Value::Integer(200));
    // 最新修订未提及对象 2 → 由历史修订提供。
    let o2 = doc
        .get_object(2)
        .expect("parse")
        .expect("obj 2 via history");
    assert_eq!(
        o2.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("History")
    );
}

/// 增量更新的删除语义：尾追修订把对象 1 标为自由条目（`f`），其 xref
/// 仅含该自由条目、trailer `/Prev` 指向初始 xref。期望 `get_object(1)`
/// 返回 `None`——**不得**回退到历史修订里的旧值 100。
#[test]
fn incremental_update_free_entry_does_not_fall_back() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n100\nendobj\n");
    // 初始修订：经典 xref 表，覆盖对象 0..=1，对象 1 在用。
    let off_xref1 = buf.len();
    buf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 >>\nstartxref\n{off_xref1}\n%%EOF\n").as_bytes(),
    );
    // 尾追修订：对象 1 标为 free（删除），xref 仅含该 free 条目。
    let off_xref2 = buf.len();
    buf.extend_from_slice(b"xref\n1 1\n0000000000 00001 f \n");
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 /Prev {off_xref1} >>\nstartxref\n{off_xref2}\n%%EOF\n")
            .as_bytes(),
    );

    let doc = parse_buf(&buf);
    // 最新修订删除对象 1 → 必须返回 None（free 条目也是「首个命中」）。
    assert!(
        doc.get_object(1)
            .expect("get_object must not error")
            .is_none(),
        "free entry in newest revision must not fall back to history"
    );
}

/// `/Prev` 回溯到历史 **XRef 流**：初始修订的 xref 是 XRef 流对象 3，
/// 尾追修订用经典 xref 表更新对象 1=200，trailer `/Prev` 指向该 XRef 流。
/// 期望历史 XRef 流被回溯解析，其提供的对象 2 可见，且最新修订优先。
#[test]
fn incremental_update_prev_to_xref_stream() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n100\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Marker /History >>\nendobj\n");
    // 初始修订的 xref 为 XRef 流对象 3，覆盖对象 0..=3。
    let off_stm = buf.len();
    let mut xs: Vec<u8> = Vec::new();
    let mut push = |t: u8, f2: u16, f3: u8| {
        xs.push(t);
        xs.extend_from_slice(&f2.to_be_bytes());
        xs.push(f3);
    };
    push(0, 0, 255); // 0：自由条目
    push(1, off1 as u16, 0); // 1
    push(1, off2 as u16, 0); // 2
    push(1, off_stm as u16, 0); // 3：XRef 流自身
    let hdr = format!(
        "3 0 obj\n<< /Type /XRef /Size 4 /Index [0 4] /W [1 2 1] /Length {} /Root 1 0 R >>\nstream\n",
        xs.len()
    );
    buf.extend_from_slice(hdr.as_bytes());
    buf.extend_from_slice(&xs);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    // 尾追修订：更新对象 1=200 的经典 xref 表，/Prev 指向历史 XRef 流。
    let off1b = buf.len();
    buf.extend_from_slice(b"1 0 obj\n200\nendobj\n");
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n1 1\n");
    buf.extend_from_slice(format!("{off1b:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 4 /Root 1 0 R /Prev {off_stm} >>\nstartxref\n{off_xref}\n%%EOF\n"
        )
        .as_bytes(),
    );

    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    // 最新修订优先：对象 1 = 200。
    let o1 = doc.get_object(1).expect("parse").expect("obj 1");
    assert_eq!(o1.value, Value::Integer(200));
    // /Prev 回溯到历史 XRef 流：对象 2 仅由历史 xref 提供。
    let o2 = doc
        .get_object(2)
        .expect("parse")
        .expect("obj 2 via history xref stream");
    assert_eq!(
        o2.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("History")
    );
}

/// 用 zlib 压缩数据（等价于 PDF 的 `/FlateDecode` 编码）。
fn zlib(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

/// 按 `(码值, 码宽)` 序列做 MSB-first 位打包（测试用 LZW 码流构造）。
fn pack_lzw_codes(codes: &[(u32, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut pending = 0u32;
    for &(code, width) in codes {
        acc = (acc << width) | code;
        pending += width;
        while pending >= 8 {
            pending -= 8;
            out.push((acc >> pending) as u8);
        }
    }
    if pending > 0 {
        out.push(((acc << (8 - pending)) & 0xff) as u8);
    }
    out
}

/// 用「CLEAR + 9 位字面量 + EOD」编码小段数据（不依赖字典）。
///
/// 仅用于测试构造 LZW 压缩流；数据须足够短（码数不足以触发 9→10 位切换），
/// 因此不覆盖码宽增长（由 `filters.rs` 单元测试覆盖）。
fn lzw_encode_literals(data: &[u8]) -> Vec<u8> {
    let mut codes: Vec<(u32, u32)> = vec![(256, 9)];
    codes.extend(data.iter().map(|&b| (b as u32, 9)));
    codes.push((257, 9));
    pack_lzw_codes(&codes)
}

/// 构造「头部声明 `header` + 实际用 XRef 流（对象 3）」的 PDF：
/// 对象 1=Catalog、2=Pages，对象 3=XRef 流 `/W [1 2 1] /Index [0 3]`。
///
/// - `filter`：XRef 流的 `/Filter` 名（空串 = 未压缩）；
/// - `png_up`：为真时按 PNG Up（Predictor 12，Columns=4）预编码条目后再压缩。
fn xref_stream_pdf(header: &str, filter: &str, png_up: bool) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(header.as_bytes());
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    // 原始条目：每行 type(1)+offset(2)+gen(1) = 4 字节
    let mut xs: Vec<u8> = Vec::new();
    xs.extend_from_slice(&xref_row(0, 0, 255)); // 0：自由条目
    xs.extend_from_slice(&xref_row(1, off1 as u16, 0)); // 1：Catalog
    xs.extend_from_slice(&xref_row(1, off2 as u16, 0)); // 2：Pages
    // PNG Up 预编码（按行做 enc[i] = raw[i] - raw_prev[i]）
    let columns = 4usize;
    let encoded = if png_up {
        let mut e = Vec::new();
        for r in 0..(xs.len() / columns) {
            for c in 0..columns {
                let cur = xs[r * columns + c];
                let up = if r == 0 { 0 } else { xs[(r - 1) * columns + c] };
                e.push(cur.wrapping_sub(up));
            }
        }
        e
    } else {
        xs.clone()
    };
    let stream_bytes = match filter {
        "" => encoded,
        "LZWDecode" => lzw_encode_literals(&encoded),
        _ => zlib(&encoded),
    };
    let off_stm = buf.len();
    let filter_hdr = if filter.is_empty() {
        String::new()
    } else {
        format!(" /Filter /{filter}")
    };
    let parms_hdr = if png_up {
        format!(" /DecodeParms << /Predictor 12 /Columns {columns} >>")
    } else {
        String::new()
    };
    append_xref_stream(
        &mut buf,
        3,
        off_stm,
        3,
        &format!("{filter_hdr}{parms_hdr}"),
        &stream_bytes,
    );
    buf
}

/// 断言文档能取出 Catalog（对象 1）与 Pages（对象 2）。
fn assert_catalog_and_pages(doc: &Document) {
    let cat = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    let pages = doc.get_object(2).expect("parse").expect("obj 2 present");
    assert_eq!(
        pages
            .value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Pages")
    );
}

/// 去版本门控：头部声明 `%PDF-1.3`，但实际用 XRef 流（未压缩）。
/// 修复前受声明版本门控会跳过 XRef 流策略、退化到全文扫描；修复后
/// 应按内容解析出对象，并留一条「宽读」说明性 warning。
#[test]
fn xref_stream_parsed_despite_low_declared_version() {
    let buf = xref_stream_pdf("%PDF-1.3\n", "", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_3);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings()
            .iter()
            .any(|w| w.contains("wide read") && w.contains("1.3")),
        "expected a wide-read note for declared 1.3, got: {:?}",
        doc.warnings()
    );
}

/// 压缩 XRef 流：`/Filter /FlateDecode` 的 XRef 流应能解码并解析出对象。
#[test]
fn flate_compressed_xref_stream_is_readable() {
    let buf = xref_stream_pdf("%PDF-1.5\n", "FlateDecode", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings().is_empty(),
        "flate xref stream should parse cleanly: {:?}",
        doc.warnings()
    );
}

/// 带 Predictor 的 XRef 流：`/FlateDecode` + `/DecodeParms << /Predictor 12
/// /Columns 4 >>`（PNG Up）。这是真实 XRef 流的常见形态。
#[test]
fn flate_xref_stream_with_png_up_predictor_is_readable() {
    let buf = xref_stream_pdf("%PDF-1.5\n", "FlateDecode", true);
    let doc = parse_buf(&buf);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings().is_empty(),
        "predictor xref stream should parse cleanly: {:?}",
        doc.warnings()
    );
}

/// LZW 压缩的 XRef 流：`/Filter /LZWDecode` 应解码并解析出对象，
/// 且不产生「unsupported filter」类 warning。
#[test]
fn lzw_xref_stream_is_readable() {
    let buf = xref_stream_pdf("%PDF-1.5\n", "LZWDecode", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings().is_empty(),
        "lzw xref stream should parse cleanly: {:?}",
        doc.warnings()
    );
}

/// LZW 压缩 + PNG Up Predictor 的 XRef 流：验证「先 LZW 解码、后预测器」
/// 的串联顺序在集成路径上成立。
#[test]
fn lzw_xref_stream_with_png_up_predictor_is_readable() {
    let buf = xref_stream_pdf("%PDF-1.5\n", "LZWDecode", true);
    let doc = parse_buf(&buf);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings().is_empty(),
        "lzw predictor xref stream should parse cleanly: {:?}",
        doc.warnings()
    );
}

/// 压缩对象流（`/Filter /FlateDecode`，真压缩）：对象流内对象应可解码取出
/// （修复前记 warning + 返回 None）。
#[test]
fn flate_compressed_obj_stm_resolves_inner_object() {
    let buf = obj_stm_pdf("FlateDecode", true);
    let doc = parse_buf(&buf);
    let inner = doc
        .get_object(5)
        .expect("parse")
        .expect("object inside flate-compressed object stream");
    assert_eq!(inner.num, 5);
    assert_eq!(
        inner
            .value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("InObjStm")
    );
    assert!(
        doc.warnings().is_empty(),
        "flate object stream should decode cleanly: {:?}",
        doc.warnings()
    );
}

/// 不可解码过滤器（`/Filter /DCTDecode`）的对象流：不得 panic，
/// `get_object` 返回 None 且 warnings 有说明。
#[test]
fn undecodable_filter_obj_stm_fails_gracefully() {
    let buf = obj_stm_pdf("DCTDecode", false);
    let doc = parse_buf(&buf);
    let got = doc.get_object(5).expect("get_object must not error");
    assert!(
        got.is_none(),
        "undecodable object stream should not resolve"
    );
    assert!(
        doc.warnings()
            .iter()
            .any(|w| w.contains("DCTDecode") && w.contains("compressed")),
        "expected an explanatory warning about DCTDecode, got: {:?}",
        doc.warnings()
    );
}

/// 用指定目标版本写出文档（供内容-版本校验类测试复用）。
fn try_write(doc: &Document, version: PdfVersion) -> Result<Vec<u8>, pdf::writer::WriteError> {
    let mut buf = Vec::new();
    pdf::writer::DocumentWriter::new()
        .with_version(version)
        .build(doc, &mut buf)?;
    Ok(buf)
}

// ── 问题 7：写端 Name `#xx` 转义（严写） ──────────────────────────

/// Name 转义回读（roundtrip）：含空格、分隔符、`#` 与 Latin-1 非 ASCII 的
/// Name（既作字典键又作 Name 值）写出后应能被 reader 精确读回。
///
/// 分别在 1.4（经典 xref，对象顶层写出）与 1.5（XRef 流 + 对象流打包）
/// 两种输出形态下验证，保证转义不受布局影响。
#[test]
fn name_escaping_roundtrips() {
    // 值含：空格 / 圆括号 / `#` / `%` / Latin-1 非 ASCII（é=U+00E9、ÿ=U+00FF）
    // 键含：圆括号 / `#`
    let key = "K(1)#x";
    let val = "a b(c)#d%é/ÿ";
    let expected_escape = "/a#20b#28c#29#23d#25#E9#2F#FF";
    let expected_key_escape = "/K#281#29#23x";

    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert(key, Value::Name(val.into())),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };

    for version in [PdfVersion::V1_4, PdfVersion::V1_5] {
        let buf = try_write(&doc, version).expect("write");
        let out = String::from_utf8_lossy(&buf);
        assert!(
            out.contains(expected_escape),
            "version {version}: 转义后的 Name 值应出现在输出中"
        );
        assert!(
            out.contains(expected_key_escape),
            "version {version}: 转义后的字典键应出现在输出中"
        );

        let doc2 = parse_buf(&buf);
        let cat = doc2.get_object(1).expect("parse").expect("catalog");
        let d = cat.value.as_dict().unwrap();
        assert_eq!(
            d.get(key).and_then(|v| v.as_name()),
            Some(val),
            "version {version}: Name 值应精确回读"
        );
    }
}

// ── 问题 3：≥1.5 输出 XRef 流 + 对象流（优先较新特性） ─────────────

/// 目标版本 1.5：输出 XRef 流，不再有经典 `xref` 段；头部 `%PDF-1.5`，
/// `startxref` 指向 XRef 流对象；回读能取到对象。
#[test]
fn xref_stream_written_at_1_5() {
    let doc = sample_doc();
    let buf = try_write(&doc, PdfVersion::V1_5).expect("write");
    let out = String::from_utf8_lossy(&buf);

    assert!(out.contains("%PDF-1.5"), "头部应为 1.5");
    assert!(out.contains("/Type /XRef"), "应输出 XRef 流对象");
    assert!(!out.contains("\nxref\n"), "1.5 不应再有经典 xref 表：{out}");
    assert!(out.contains("%%EOF"));

    // startxref 指向的对象应是 XRef 流自身（以数字开头且含 /Type /XRef）。
    let sx = out.rfind("startxref").expect("startxref 关键字");
    let off: usize = out[sx + "startxref".len()..]
        .trim_start()
        .split(['\n', '\r'])
        .next()
        .unwrap()
        .trim()
        .parse()
        .expect("startxref 偏移应可解析");
    let seg = String::from_utf8_lossy(&buf[off..(off + 96).min(buf.len())]);
    assert!(
        seg.starts_with(|c: char| c.is_ascii_digit()),
        "startxref 应指向间接对象：{seg}"
    );
    assert!(
        seg.contains("/Type /XRef"),
        "startxref 应指向 XRef 流：{seg}"
    );

    // 回读：内容流对象（3）作为顶层对象仍可直接定位并取到字节。
    let doc2 = parse_buf(&buf);
    assert_eq!(doc2.source_version(), PdfVersion::V1_5);
    let cat = doc2.get_object(1).expect("parse").expect("catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    assert!(matches!(
        doc2.xref().get(3).map(|e| &e.kind),
        Some(XrefKind::Normal)
    ));
}

/// 目标版本 1.5：非流对象（Catalog/Pages）打包进 ObjStm（type 2 条目），
/// 流对象仍是顶层对象（type 1 条目）；回读能取到打包对象与流对象。
#[test]
fn object_stream_packs_non_stream_objects_at_1_5() {
    let doc = sample_doc();
    let buf = try_write(&doc, PdfVersion::V1_5).expect("write");
    let out = String::from_utf8_lossy(&buf);

    assert!(out.contains("/Type /ObjStm"), "应输出对象流：{out}");
    assert!(out.contains("/N 2"), "对象流应打包 2 个非流对象");
    assert!(out.contains("/First "), "对象流应含 /First");
    // 流对象（内容流）仍在顶层，未被塞进对象流。
    assert!(
        !out[..out.find("/Type /ObjStm").unwrap()].contains("endobj\n1 0 obj"),
        "对象 1 不应作为顶层对象单独写出"
    );

    let doc2 = parse_buf(&buf);
    // 打包对象：Catalog(1)、Pages(2) → type 2（对象流内）。
    for num in [1i64, 2] {
        match doc2.xref().get(num).map(|e| &e.kind) {
            Some(XrefKind::InObjStream { stream_num, index }) => {
                assert!(*stream_num > 0, "对象流对象号应为正");
                assert!(*index >= 0);
            }
            other => panic!("object {num} should be in an object stream, got {other:?}"),
        }
        assert!(doc2.get_object(num).expect("parse").is_some());
    }
    let pages = doc2.get_object(2).expect("parse").expect("pages");
    assert_eq!(
        pages
            .value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Pages")
    );
    // 流对象（内容流，对象 3）仍是顶层对象（type 1）且字节可取。
    let page = doc2.get_object(3).expect("parse").expect("content stream");
    assert!(matches!(page.value, Value::Stream(_, _)));
    assert!(!doc2.stream_bytes(&page.value).expect("bytes").is_empty());
}

/// 目标版本 1.4：保持经典 xref 表 + trailer，不输出 XRef 流/对象流。
#[test]
fn classic_xref_kept_below_1_5() {
    let doc = sample_doc();
    let buf = try_write(&doc, PdfVersion::V1_4).expect("write");
    let out = String::from_utf8_lossy(&buf);

    assert!(out.contains("%PDF-1.4"));
    assert!(out.contains("\nxref\n"), "1.4 应输出经典 xref 表");
    assert!(out.contains("trailer"), "1.4 应输出 trailer");
    assert!(!out.contains("/Type /XRef"), "1.4 不应输出 XRef 流");
    assert!(!out.contains("/Type /ObjStm"), "1.4 不应输出对象流");
    // 所有对象均顶层写出。
    assert!(out.contains("1 0 obj"));
    assert!(out.contains("3 0 obj"));

    let doc2 = parse_buf(&buf);
    assert_eq!(doc2.source_version(), PdfVersion::V1_4);
    assert!(doc2.get_object(1).expect("parse").is_some());
}

// ── 问题 2：写端内容-版本一致性校验（严出） ──────────────────────

/// 构造含单个指定 `/Filter` 流的文档：对象 1 = Catalog，对象 2 = 该过滤器流。
fn filter_stream_doc(filter: &str) -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Stream(
            Dict::new().insert("Filter", Value::Name(filter.into())),
            StreamData::Owned(b"\x00\x01".to_vec()),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

/// 内容含 `/Filter /JPXDecode`（需 1.5）：目标 1.4 → UnsupportedVersion
/// （信息含特性名与最低版本 1.5）；目标 1.5 → 成功。
#[test]
fn content_version_check_rejects_jpx_below_1_5() {
    let doc = filter_stream_doc("JPXDecode");
    let mut buf = Vec::new();
    let e = pdf::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect_err("1.4 目标下 JPXDecode 应被拒");
    let msg = match e {
        pdf::writer::WriteError::UnsupportedVersion(m) => m,
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    };
    assert!(msg.contains("JPXDecode"), "信息应含特性名：{msg}");
    assert!(msg.contains("1.5"), "信息应含最低版本 1.5：{msg}");

    // 目标 1.5：通过，且 writer 自身选择的 XRef 流/对象流不会被误判为越版。
    let buf2 = try_write(&doc, PdfVersion::V1_5).expect("1.5 应成功");
    let out = String::from_utf8_lossy(&buf2);
    assert!(out.contains("%PDF-1.5"));
    assert!(out.contains("/Type /XRef"));
}

/// 内容-版本校验覆盖其余特性：JBIG2Decode(1.4)、透明度（/SMask 与
/// ExtGState 的 /ca）、可选内容（Catalog /OCProperties，1.5）、
/// 加密（trailer /Encrypt，1.1）。
#[test]
fn content_feature_checks_cover_jbig2_transparency_oc_and_encryption() {
    // JBIG2Decode 需 1.4：1.3 拒绝，1.4 通过。
    let jbig2 = filter_stream_doc("JBIG2Decode");
    assert!(matches!(
        try_write(&jbig2, PdfVersion::V1_3),
        Err(pdf::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&jbig2, PdfVersion::V1_4).is_ok());

    // 透明度 /SMask 需 1.4：1.3 拒绝，1.4 通过。
    let smask = sample_doc();
    smask.set_object(Object::new(
        4,
        0,
        Value::Dict(Dict::new().insert("SMask", Value::Ref(3, 0))),
    ));
    assert!(matches!(
        try_write(&smask, PdfVersion::V1_3),
        Err(pdf::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&smask, PdfVersion::V1_4).is_ok());

    // ExtGState 的 /ca（软掩码/混合）同上。
    let gstate = sample_doc();
    gstate.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("ExtGState".into()))
                .insert("ca", Value::Float(0.5)),
        ),
    ));
    assert!(matches!(
        try_write(&gstate, PdfVersion::V1_3),
        Err(pdf::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&gstate, PdfVersion::V1_4).is_ok());

    // Catalog /OCProperties 需 1.5：1.4 拒绝，1.5 通过。
    let oc = sample_doc();
    oc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert("OCProperties", Value::Dict(Dict::new())),
        ),
    ));
    assert!(matches!(
        try_write(&oc, PdfVersion::V1_4),
        Err(pdf::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&oc, PdfVersion::V1_5).is_ok());

    // trailer /Encrypt 需 1.1：1.0 拒绝，1.1 通过。
    let mut enc = sample_doc();
    let enc_dict = enc
        .trailer
        .dict
        .clone()
        .insert("Encrypt", Value::Dict(Dict::new()));
    enc.trailer = Trailer { dict: enc_dict };
    assert!(matches!(
        try_write(&enc, PdfVersion::V1_0),
        Err(pdf::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&enc, PdfVersion::V1_1).is_ok());
}

// ── 架构修复新增测试 ──────────────────────────────────────────────

/// 读含 ObjStm + XRef 流的 1.5 PDF → writer 以 1.3 写出 → 回读断言：
/// 所有对象可见（load_all_objects 预解包后顶层写出）、输出经典 xref 表、
/// 无 ObjStm / XRef 流残留（结构性特性自动降级）。
#[test]
fn obj_stm_xref_stream_automatically_downgraded_below_1_5() {
    // 1.5 PDF：含对象流（对象 5 打包在对象流 6 里）+ XRef 流
    let buf = obj_stm_pdf("", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);

    // Features 初始：XRef 流来自 trailer.dict（懒加载就能扫到）；
    // ObjectStream 需要解析对象流对象本身才会被扫到（懒加载路径不会触发）。
    // 先调 load_all_objects 预解包全部对象 → 触发 ObjStm 字典扫描。
    doc.load_all_objects().expect("预加载全部对象");

    let feats = doc.features.borrow();
    assert!(feats.contains(PdfFeature::XRefStream), "应检测到 XRef 流");
    assert!(feats.contains(PdfFeature::ObjectStream), "应检测到对象流");
    drop(feats);

    // 以 1.3 写出 → 策略选择 PlainObjectLayout + ClassicXrefSerializer
    let mut out = Vec::new();
    pdf::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_3)
        .build(&doc, &mut out)
        .expect("1.3 目标下结构性特性应自动降级，不应报错");

    // 输出应是经典 xref 表形态，1.3 头
    let text = String::from_utf8_lossy(&out);
    eprintln!("=== writer 输出头 ===\n{}", &text[..300.min(text.len())]);
    eprintln!("=== 尾部 ===\n{}", &text[text.len().saturating_sub(300)..]);
    assert!(
        text.contains("%PDF-1.3"),
        "输出应为 1.3 头：{}",
        &text[..10.min(text.len())]
    );
    assert!(text.contains("xref\n"), "应含经典 xref 关键字");
    assert!(!text.contains("/Type /XRef"), "降级后不应有 XRef 流");
    assert!(!text.contains("/Type /ObjStm"), "降级后不应有对象流");

    // 回读：所有对象应直接可见（对象流内的对象已解包为顶层）
    let doc2 = parse_buf(&out);
    assert_eq!(doc2.source_version(), PdfVersion::V1_3);
    let cat = doc2.get_object(1).expect("1.3 回读").expect("Catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    // 对象 5 原本在对象流里，降级后应作为顶层对象可直接取
    let inner = doc2
        .get_object(5)
        .expect("1.3 回读")
        .expect("对象流内对象应在降级后可见");
    assert_eq!(
        inner
            .value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("InObjStm")
    );
}

/// writer 默认版本（1.7）与 doc.source_version 彻底解耦：
/// 不指定 with_version 时输出 1.7，不受 doc.source_version 影响。
#[test]
fn writer_default_target_version_is_1_7_independent_of_source() {
    // 构造一个 source_version=1.3 的写入端 Document
    let mut doc = sample_doc();
    doc.source_version = PdfVersion::V1_3;
    assert_eq!(doc.source_version(), PdfVersion::V1_3);

    // write_document 不传 version → 输出 writer default 1.7
    let mut buf = Vec::new();
    pdf::writer::write_document(&doc, &mut buf).expect("write");
    let text = String::from_utf8_lossy(&buf);
    assert!(text.contains("%PDF-1.7"), "默认输出应为 1.7 而非 1.3");

    // 显式传 1.5 仍然可用
    let mut buf2 = Vec::new();
    pdf::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_5)
        .build(&doc, &mut buf2)
        .expect("write with_version");
    let text2 = String::from_utf8_lossy(&buf2);
    assert!(text2.contains("%PDF-1.5"));
}

/// FeatureProfile 一致性：含 JPXDecode 时
/// doc.features.contains(JPXDecode)=true，且 validate_content_version
/// 在目标 1.4 下拒绝（不可自动降级的内容特性）。
#[test]
fn feature_profile_matches_content_validation() {
    let doc = filter_stream_doc("JPXDecode");

    // FeatureProfile 应含 JPXDecode（filter_stream_doc 里 set_object 时
    // 会调 scan_value_features，把 JPXDecode 加进 features）
    let feats = doc.features.borrow();
    assert!(
        feats.contains(PdfFeature::JPXDecode),
        "set_object 应自动填充 JPXDecode 到 FeatureProfile"
    );
    drop(feats);

    // 目标 1.4 应因 JPXDecode 被拒（不可自动降级）
    let mut buf = Vec::new();
    let e = pdf::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect_err("1.4 应拒绝 JPXDecode");
    match e {
        pdf::writer::WriteError::UnsupportedVersion(msg) => {
            assert!(msg.contains("JPXDecode"));
            assert!(msg.contains("1.5"));
        }
        other => panic!("预期 UnsupportedVersion，得到 {other:?}"),
    }

    // 目标 1.5 通过，不报错
    pdf::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_5)
        .build(&doc, &mut Vec::new())
        .expect("1.5 应接受 JPXDecode");
}

/// 读端解压后的流字典 /Filter /FlateDecode 应被 writer 自动移除，
/// 输出的流字典不含压缩过滤器、/Length 对应未压缩字节长度。
///
/// 这验证「宽进（读端解压）→ 严出（writer 按目标版本重新表达）」
/// 的关键协调逻辑。
#[test]
fn writer_strips_compression_filter_after_reader_decompression() {
    // 构造一个手动 Flate 压缩的流，手动模拟「读端解压后 Dict 仍保留 Filter」场景
    // （测试用手动压缩/解压绕过 reader，因为 sample_doc / filter_stream_doc
    // 里的 Owned 流本就未被压缩）
    let flate_bytes = {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        let raw = b"BT 72 720 Td (compressed content) Tj ET";
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(raw).unwrap();
        enc.finish().unwrap()
    };

    // 模拟读端解压后的状态：Owned 存解压后字节，但 dict 仍保留 /Filter /FlateDecode
    // （实际读端会做这个，此处手动构造验证 writer 的修正逻辑）
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Stream(
            Dict::new().insert("Filter", Value::Name("FlateDecode".into())),
            StreamData::Owned(b"BT 72 720 Td (compressed content) Tj ET".to_vec()),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };

    // writer 应自动去掉 /Filter /FlateDecode，输出未压缩内容
    let mut out = Vec::new();
    pdf::writer::write_document(&doc, &mut out).expect("write");
    let text = String::from_utf8_lossy(&out);

    // 输出流字典不应再有 /Filter /FlateDecode
    let no_filter = !text.contains("FlateDecode");
    assert!(
        no_filter,
        "writer 应移除 /Filter /FlateDecode（字节已解压）"
    );

    // 输出流字节应是未压缩的（因为 Owned 里存的就是解压后字节）
    assert!(
        text.contains("BT 72 720 Td (compressed content) Tj ET"),
        "输出流应直接包含未压缩的 PDF 操作符"
    );

    // 回读：内容应完全一致
    let doc2 = parse_buf(&out);
    let stream_obj = doc2.get_object(2).expect("回读").expect("流对象");
    let bytes = doc2.stream_bytes(&stream_obj.value).expect("流字节");
    assert_eq!(
        bytes, b"BT 72 720 Td (compressed content) Tj ET",
        "回读的流字节应等于未压缩内容"
    );

    // 确认：flate_bytes 变量在这个场景里没用了，但保留它展示「原始压缩→解压→输出未压缩」的链路
    let _ = flate_bytes;
}

/// 回归（#6）：解析含 ObjStm + XRef 流的 1.5 PDF 后以**同版本 1.5** 再写出，
/// writer 自身生成的对象流 / XRef 流不得与源文件残留的结构性对象叠加。
///
/// 修复前：源里的 `/Type /ObjStm`、`/Type /XRef` 容器对象被当作普通顶层
/// 对象保留，导致输出出现 2×ObjStm、2×XRef，体积显著膨胀。
#[test]
fn rewrite_at_same_version_has_single_structural_objects() {
    let buf = obj_stm_pdf("", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);

    let out = try_write(&doc, PdfVersion::V1_5).expect("1.5 再写出");
    let text = String::from_utf8_lossy(&out);

    assert_eq!(
        text.matches("/Type /ObjStm").count(),
        1,
        "同版本再写出应恰有 1 个对象流，不得叠加源残留：\n{text}"
    );
    assert_eq!(
        text.matches("/Type /XRef").count(),
        1,
        "同版本再写出应恰有 1 个 XRef 流，不得叠加源残留：\n{text}"
    );
}

/// 回归（#10）：写端的过滤器处理与 `filters.rs::canonical_filter`（唯一事实
/// 源）对齐——读端**已解压**的过滤器（FlateDecode/LZWDecode/CCITTFaxDecode/
/// DCTDecode，含缩写 Fl/LZW/CCF/DCT）应被剥离；读端**不解压**的过滤器（如
/// JBIG2Decode，含缩写）必须原样保留，否则会把压缩态字节当明文写出。
#[test]
fn writer_filter_handling_matches_reader_decoding() {
    let mk = |filter: &str| {
        let mut doc = Document::new();
        doc.set_object(Object::new(
            1,
            0,
            Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
        ));
        doc.set_object(Object::new(
            2,
            0,
            Value::Stream(
                Dict::new().insert("Filter", Value::Name(filter.into())),
                StreamData::Owned(b"\x00\x01\x02".to_vec()),
            ),
        ));
        doc.trailer = Trailer {
            dict: Dict::new().insert("Root", Value::Ref(1, 0)),
        };
        doc
    };

    // 读端已解压：FlateDecode/LZWDecode/CCITTFaxDecode/DCTDecode（及缩写
    // Fl/LZW/CCF/DCT）应被剥离——本就已是明文字节。
    for f in [
        "FlateDecode",
        "Fl",
        "LZWDecode",
        "LZW",
        "CCITTFaxDecode",
        "CCF",
        "DCTDecode",
        "DCT",
    ] {
        let out = try_write(&mk(f), PdfVersion::V1_4).expect("write");
        let doc2 = parse_buf(&out);
        let s = doc2.get_object(2).expect("parse").expect("stream");
        let kept = s.value.as_dict().and_then(|d| d.get("Filter"));
        assert!(
            kept.is_none(),
            "读端已解压的过滤器应被剥离（{f}），实际: {kept:?}"
        );
    }

    // 读端不解压：JBIG2Decode（仍未实现）应原样保留。
    for f in ["JBIG2Decode"] {
        let out = try_write(&mk(f), PdfVersion::V1_4).expect("write");
        let doc2 = parse_buf(&out);
        let s = doc2.get_object(2).expect("parse").expect("stream");
        let kept = s
            .value
            .as_dict()
            .and_then(|d| d.get("Filter"))
            .and_then(|v| v.as_name());
        assert_eq!(kept, Some(f), "读端不解压的过滤器应原样保留（{f}）");
    }
}

fn rc4_5(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U5>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

fn rc4_10(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U10>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

fn rc4_16(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U16>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn encrypt_r2_fixture() -> Vec<u8> {
    const PADDING: [u8; 32] = [
        0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01,
        0x08, 0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53,
        0x69, 0x7a,
    ];
    let pad = |password: &[u8]| {
        let mut result = password[..password.len().min(32)].to_vec();
        result.extend_from_slice(&PADDING[..32 - result.len()]);
        result
    };
    let id = b"pdf-security-test";
    let user_pad = pad(b"user-password");
    let owner_pad = pad(b"owner-password");
    let owner_key = Md5::digest(&owner_pad);
    let owner_entry = rc4_5(&owner_key[..5], &user_pad);
    let permissions = -4i32;
    let mut key_material = user_pad;
    key_material.extend_from_slice(&owner_entry);
    key_material.extend_from_slice(&permissions.to_le_bytes());
    key_material.extend_from_slice(id);
    let file_key = Md5::digest(key_material);
    let user_entry = rc4_5(&file_key[..5], &PADDING);

    let mut object_key_material = file_key[..5].to_vec();
    object_key_material.extend_from_slice(&[2, 0, 0, 0, 0]);
    let object_key = Md5::digest(object_key_material);
    let encrypted_label = rc4_10(&object_key[..10], b"decrypted label");
    let encrypted_stream = rc4_10(&object_key[..10], b"decrypted stream");

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(
        format!(
            "2 0 obj\n<< /Label <{}> /Length {} >>\nstream\n",
            hex_bytes(&encrypted_label),
            encrypted_stream.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&encrypted_stream);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    let off3 = buf.len();
    buf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Filter /Standard /V 1 /R 2 /O <{}> /U <{}> /P {} >>\nendobj\n",
            hex_bytes(&owner_entry),
            hex_bytes(&user_entry),
            permissions
        )
        .as_bytes(),
    );
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off3:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 4 /Root 1 0 R /Encrypt 3 0 R /ID [<{}> <{}>] >>\nstartxref\n{off_xref}\n%%EOF\n",
            hex_bytes(id),
            hex_bytes(id)
        )
        .as_bytes(),
    );
    buf
}

/// ISO 32000-1 Standard Security Handler R2 authenticates user and owner
/// passwords, decrypts object strings and streams, and refuses unsafe writes.
#[test]
fn standard_security_r2_decrypts_strings_and_streams() {
    let buf = encrypt_r2_fixture();
    let doc = parse_buf_with_password(&buf, b"user-password");
    assert!(doc.is_encrypted());
    let s = doc.get_object(2).expect("parse").expect("stream present");
    let label = s.value.as_dict().unwrap().get("Label").unwrap();
    assert_eq!(label, &Value::HexString(hex_bytes(b"decrypted label")));
    assert_eq!(doc.try_stream_bytes(&s.value).unwrap(), b"decrypted stream");
    assert_eq!(
        doc.decode_stream_bytes(&s.value).unwrap(),
        b"decrypted stream"
    );

    let owner_doc = parse_buf_with_password(&buf, b"owner-password");
    let owner_stream = owner_doc.get_object(2).unwrap().unwrap();
    assert_eq!(
        owner_doc.try_stream_bytes(&owner_stream.value).unwrap(),
        b"decrypted stream"
    );
    assert!(parse_buf_result(&buf, Some(b"wrong-password")).is_err());

    let error = demo::pdf::writer::DocumentWriter::new()
        .build(&doc, &mut Vec::new())
        .unwrap_err();
    assert!(matches!(
        error,
        demo::pdf::writer::WriteError::EncryptedDocument
    ));

    // 加密特性被记录
    let feats = doc.features.borrow();
    assert!(
        feats.contains(PdfFeature::EncryptionRC4),
        "trailer /Encrypt 应记录 EncryptionRC4"
    );
    drop(feats);
}

#[test]
fn standard_security_r4_decrypts_explicit_crypt_filter() {
    let buf = encrypt_r4_fixture();
    let doc = parse_buf_with_password(&buf, b"user-password");
    let stream = doc.get_object(2).unwrap().unwrap();
    let label = stream.value.as_dict().unwrap().get("Label").unwrap();
    assert_eq!(label, &Value::HexString(hex_bytes(b"R4 encrypted label")));
    assert_eq!(
        doc.try_stream_bytes(&stream.value).unwrap(),
        b"R4 crypt filter stream"
    );
    assert_eq!(
        doc.decode_stream_bytes(&stream.value).unwrap(),
        b"R4 crypt filter stream"
    );
    assert!(parse_buf_result(&buf, Some(b"wrong-password")).is_err());
}

#[test]
fn crypt_identity_filter_is_supported_without_encryption() {
    let doc = Document::new();
    let stream = Value::Stream(
        Dict::new()
            .insert("Filter", Value::Name("Crypt".into()))
            .insert(
                "DP",
                Value::Dict(Dict::new().insert("Name", Value::Name("Identity".into()))),
            ),
        StreamData::Owned(b"identity stream".to_vec()),
    );
    assert_eq!(
        doc.decode_stream_bytes(&stream).unwrap(),
        b"identity stream"
    );

    let non_identity = Value::Stream(
        Dict::new()
            .insert("Filter", Value::Name("Crypt".into()))
            .insert(
                "DecodeParms",
                Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
            ),
        StreamData::Owned(b"ciphertext".to_vec()),
    );
    assert!(doc.decode_stream_bytes(&non_identity).is_err());

    let invalid_params = Value::Stream(
        Dict::new()
            .insert(
                "Filter",
                Value::Array(Array(vec![
                    Value::Name("Crypt".into()),
                    Value::Name("FlateDecode".into()),
                ])),
            )
            .insert(
                "DecodeParms",
                Value::Dict(Dict::new().insert("Name", Value::Name("Identity".into()))),
            ),
        StreamData::Owned(Vec::new()),
    );
    assert!(doc.decode_stream_bytes(&invalid_params).is_err());
}

#[test]
fn type1_font_resolves_indirect_widths_and_descriptor_metrics() {
    let doc = Document::new();
    doc.set_object(Object::new(
        10,
        0,
        Value::Array(Array(vec![Value::Integer(500), Value::Float(611.5)])),
    ));
    doc.set_object(Object::new(
        11,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontName", Value::Name("ExampleSans".into()))
                .insert("Flags", Value::Integer(32))
                .insert(
                    "FontBBox",
                    Value::Array(Array(vec![
                        Value::Integer(-20),
                        Value::Integer(-200),
                        Value::Integer(1000),
                        Value::Integer(900),
                    ])),
                )
                .insert("Ascent", Value::Integer(750))
                .insert("Descent", Value::Integer(-200))
                .insert("MissingWidth", Value::Integer(400)),
        ),
    ));
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("FirstChar", Value::Integer(65))
            .insert("LastChar", Value::Integer(66))
            .insert("Widths", Value::Ref(10, 0))
            .insert("FontDescriptor", Value::Ref(11, 0)),
    );

    let font = doc.type1_font(&font).unwrap().unwrap();
    assert_eq!(font.base_font, "ExampleSans");
    assert_eq!(font.width_for(65), Some(500.0));
    assert_eq!(font.width_for(66), Some(611.5));
    assert_eq!(font.width_for(64), Some(400.0));
    assert_eq!(font.width_for(67), Some(400.0));
    let descriptor = font.descriptor.unwrap();
    assert_eq!(descriptor.font_name.as_deref(), Some("ExampleSans"));
    assert_eq!(descriptor.flags, Some(32));
    assert_eq!(descriptor.font_bbox, Some([-20.0, -200.0, 1000.0, 900.0]));
    assert_eq!(descriptor.ascent, Some(750.0));
    assert_eq!(descriptor.descent, Some(-200.0));
    assert_eq!(descriptor.missing_width, Some(400.0));

    let incomplete = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("CustomFont".into())),
    );
    assert!(doc.type1_font(&incomplete).is_err());

    let standard_14 = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert!(
        doc.type1_font(&standard_14)
            .unwrap()
            .unwrap()
            .width_for(65)
            .is_none()
    );

    let unsupported = Value::Dict(Dict::new().insert("Subtype", Value::Name("TrueType".into())));
    assert!(doc.type1_font(&unsupported).unwrap().is_none());
}

#[test]
fn indirect_stream_length_handles_embedded_endstream_and_cycles() {
    let payload = b"before endstream after";
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let offset1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let offset2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Length 3 0 R >>\nstream\n");
    pdf.extend_from_slice(payload);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let offset3 = pdf.len();
    pdf.extend_from_slice(format!("3 0 obj\n{}\nendobj\n", payload.len()).as_bytes());
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{offset1:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(format!("{offset2:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(format!("{offset3:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );

    let document = parse_buf(&pdf);
    let stream = document.get_object(2).unwrap().unwrap();
    assert_eq!(document.try_stream_bytes(&stream.value).unwrap(), payload);

    let mut cyclic = b"%PDF-1.7\n".to_vec();
    let offset1 = cyclic.len();
    cyclic.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let offset2 = cyclic.len();
    cyclic.extend_from_slice(b"2 0 obj\n<< /Length 2 0 R >>\nstream\ncycle\nendstream\nendobj\n");
    let xref_offset = cyclic.len();
    cyclic.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    cyclic.extend_from_slice(format!("{offset1:010} 00000 n \n").as_bytes());
    cyclic.extend_from_slice(format!("{offset2:010} 00000 n \n").as_bytes());
    cyclic.extend_from_slice(
        format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    let document = parse_buf(&cyclic);
    let error = document.get_object(2).unwrap_err();
    assert!(error.message.contains("cyclic indirect stream /Length"));
}

fn encrypt_r4_fixture() -> Vec<u8> {
    const PADDING: [u8; 32] = [
        0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01,
        0x08, 0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53,
        0x69, 0x7a,
    ];
    let pad = |password: &[u8]| {
        let mut result = password[..password.len().min(32)].to_vec();
        result.extend_from_slice(&PADDING[..32 - result.len()]);
        result
    };
    let id = b"pdf-r4-security-test";
    let user_password = pad(b"user-password");
    let owner_password = pad(b"owner-password");
    let mut owner_digest = Md5::digest(&owner_password).to_vec();
    for _ in 0..50 {
        owner_digest = Md5::digest(&owner_digest[..16]).to_vec();
    }
    let mut owner_entry = rc4_16(&owner_digest[..16], &user_password);
    for iteration in 1..20u8 {
        let key: Vec<u8> = owner_digest[..16]
            .iter()
            .map(|byte| byte ^ iteration)
            .collect();
        owner_entry = rc4_16(&key, &owner_entry);
    }

    let permissions = -4i32;
    let mut key_material = user_password;
    key_material.extend_from_slice(&owner_entry);
    key_material.extend_from_slice(&permissions.to_le_bytes());
    key_material.extend_from_slice(id);
    key_material.extend_from_slice(&[0xff; 4]);
    let mut file_key = Md5::digest(key_material).to_vec();
    for _ in 0..50 {
        file_key = Md5::digest(&file_key[..16]).to_vec();
    }
    let mut user_material = PADDING.to_vec();
    user_material.extend_from_slice(id);
    let digest = Md5::digest(user_material);
    let mut user_entry = rc4_16(&file_key[..16], &digest);
    for iteration in 1..20u8 {
        let key: Vec<u8> = file_key[..16].iter().map(|byte| byte ^ iteration).collect();
        user_entry = rc4_16(&key, &user_entry);
    }
    user_entry.extend_from_slice(&[0; 16]);

    let mut object_key_material = file_key[..16].to_vec();
    object_key_material.extend_from_slice(&[2, 0, 0, 0, 0]);
    object_key_material.extend_from_slice(b"sAlT");
    let object_key = Md5::digest(object_key_material);
    let iv = [0x73; 16];
    let plaintext = b"R4 crypt filter stream";
    let encrypted_label = encrypt_aesv2(b"R4 encrypted label", &object_key[..16], [0x29; 16]);
    let encrypted_stream = encrypt_aesv2(plaintext, &object_key[..16], iv);
    let mut buf = b"%PDF-1.7\n".to_vec();
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(
        format!(
            "2 0 obj\n<< /Label <{}> /Filter /Crypt /DecodeParms << /Name /StdCF >> /Length {} >>\nstream\n",
            hex_bytes(&encrypted_label),
            encrypted_stream.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&encrypted_stream);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    let off3 = buf.len();
    buf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Filter /Standard /V 4 /R 4 /Length 128 /O <{}> /U <{}> /P {} /EncryptMetadata false /CF << /StdCF << /CFM /AESV2 /Length 16 >> >> /StmF /StdCF /StrF /StdCF >>\nendobj\n",
            hex_bytes(&owner_entry),
            hex_bytes(&user_entry),
            permissions
        )
        .as_bytes(),
    );
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off3:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 4 /Root 1 0 R /Encrypt 3 0 R /ID [<{}> <{}>] >>\nstartxref\n{off_xref}\n%%EOF\n",
            hex_bytes(id),
            hex_bytes(id)
        )
        .as_bytes(),
    );
    buf
}

fn encrypt_aesv2(plaintext: &[u8], key: &[u8], iv: [u8; 16]) -> Vec<u8> {
    let padding = 16 - plaintext.len() % 16;
    let mut ciphertext = plaintext.to_vec();
    ciphertext.extend(std::iter::repeat_n(padding as u8, padding));
    let cipher = Aes128::new_from_slice(key).unwrap();
    let mut previous = iv;
    for chunk in ciphertext.chunks_mut(16) {
        for (byte, prior) in chunk.iter_mut().zip(previous) {
            *byte ^= prior;
        }
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
        previous.copy_from_slice(chunk);
    }
    [iv.as_slice(), ciphertext.as_slice()].concat()
}

/// 环形 `/Prev` 链（xref A 的 `/Prev` 指向 B，B 又指回 A）：解析必须
/// 终止（防死循环护栏生效）、记 revisited warning，且链上对象仍可解析。
#[test]
fn circular_prev_chain_terminates_with_warning() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Marker /InRing >>\nendobj\n");
    let off_xref_a = buf.len();

    // 修订 A：覆盖对象 1，/Prev 指向修订 B（B 在文件更后处）。/Prev 用
    // 固定 10 位宽度占位，使 A 段长度与占位值无关，可一次算出 B 的偏移。
    let section_a = |prev: u64| {
        let mut s = String::from("xref\n0 2\n0000000000 65535 f \n");
        s.push_str(&format!("{off1:010} 00000 n \n"));
        s.push_str(&format!(
            "trailer\n<< /Size 2 /Root 1 0 R /Prev {prev:010} >>\nstartxref\n{off_xref_a}\n%%EOF\n"
        ));
        s
    };
    let off_xref_b = off_xref_a + section_a(0).len();
    buf.extend_from_slice(section_a(off_xref_b as u64).as_bytes());

    // 修订 B：/Prev 指回修订 A，构成 A→B→A 环；startxref 指向 B（最新修订）。
    buf.extend_from_slice(b"xref\n0 1\n0000000000 65535 f \n");
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 /Root 1 0 R /Prev {off_xref_a} >>\nstartxref\n{off_xref_b}\n%%EOF\n")
            .as_bytes(),
    );

    let doc = parse_buf(&buf);
    assert!(
        doc.warnings().iter().any(|w| w.contains("revisited")),
        "环形 /Prev 应记 revisited warning：{:?}",
        doc.warnings()
    );
    let o1 = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(
        o1.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("InRing")
    );
}

/// 写端对 `StreamData::Ref`（读端未解压的流，如 DCTDecode 图像）按原样
/// 透传：`/Filter` 保留、流字节不变（不得按 Owned 流的剥离规则误删过滤器）。
#[test]
fn ref_stream_passthrough_keeps_filter_and_bytes() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(
        b"2 0 obj\n<< /Filter /DCTDecode /Length 4 >>\nstream\n\xFF\xD9\xAB\xCD\nendstream\nendobj\n",
    );
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{off_xref}\n%%EOF\n").as_bytes(),
    );

    let doc = parse_buf(&buf);
    let out = try_write(&doc, PdfVersion::V1_4).expect("write");
    let doc2 = parse_buf(&out);
    let s = doc2.get_object(2).expect("parse").expect("stream present");
    let kept = s
        .value
        .as_dict()
        .and_then(|d| d.get("Filter"))
        .and_then(|v| v.as_name());
    assert_eq!(kept, Some("DCTDecode"), "Ref 流的过滤器应原样保留");
    assert_eq!(
        doc2.stream_bytes(&s.value).expect("stream bytes"),
        [0xFF, 0xD9, 0xAB, 0xCD],
        "Ref 流字节应原样透传"
    );
}

/// xref 条目的 generation 与对象头的 generation 不一致时：以对象头为准
/// （xref gen 仅为建议值），对象仍可解析并进入缓存。
#[test]
fn xref_generation_mismatch_defers_to_object_header() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Marker /Gen0 >>\nendobj\n");
    let off_xref = buf.len();
    // 条目声明 gen=1，但对象头是 `1 0 obj`（gen=0）——刻意不一致。
    buf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00001 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{off_xref}\n%%EOF\n").as_bytes(),
    );

    let doc = parse_buf(&buf);
    let o = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(o.generation, 0, "对象头的 generation 应优先于 xref 条目");
    assert_eq!(
        o.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("Gen0")
    );
    assert_eq!(doc.cached_object_count(), 1, "解析结果应进入缓存");
}
