//! 真实 PDF fixture 回归测试（A6）。
//!
//! 验证 `tests/fixtures/` 中的真实 PDF 样本可被解析器正确处理，
//! 并对损坏输入返回类型化错误而非 panic。

use pdf_engine::core::reader::Parser;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn fixture_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    std::path::Path::new(&manifest_dir)
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn read_fixture(name: &str) -> Vec<u8> {
    let path = fixture_path(name);
    let mut file = File::open(&path).expect("fixture file exists");
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).expect("read fixture");
    buf
}

fn parse_buf(buf: &[u8]) -> Result<pdf_engine::core::types::Document, String> {
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("fixture-test-{}-{}.pdf", std::process::id(), seq));
    let mut f = File::create(&path).map_err(|e| e.to_string())?;
    f.write_all(buf).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    let file = File::open(&path).map_err(|e| e.to_string())?;
    let mut parser = Parser::new(file).map_err(|e| e.to_string())?;
    let doc = parser.parse_document().map_err(|e| format!("{:?}", e))?;
    let _ = std::fs::remove_file(&path);
    Ok(doc)
}

#[test]
fn minimal_document_parses_without_error() {
    let _doc = parse_buf(&read_fixture("minimal-document.pdf")).unwrap();
}

#[test]
fn pdflatex_4_pages_parses_without_error() {
    let _doc = parse_buf(&read_fixture("pdflatex-4-pages.pdf")).unwrap();
}

#[test]
fn truncated_minimal_document_returns_typed_error() {
    let buf = read_fixture("minimal-document.pdf");
    let truncated = &buf[..buf.len() / 2];
    let result = parse_buf(truncated);
    assert!(
        result.is_err(),
        "truncated PDF should return parse error, not panic"
    );
}

#[test]
fn truncated_pdflatex_4_pages_returns_typed_error() {
    let buf = read_fixture("pdflatex-4-pages.pdf");
    let truncated = &buf[..buf.len() / 2];
    let result = parse_buf(truncated);
    assert!(
        result.is_err(),
        "truncated PDF should return parse error, not panic"
    );
}

#[test]
fn corrupted_pdf_returns_typed_error_not_panic() {
    let mut buf = read_fixture("minimal-document.pdf");
    // Corrupt the middle of the file
    if buf.len() > 100 {
        for byte in &mut buf[50..100] {
            *byte = 0xFF;
        }
    }
    // Should either fail to parse or return an error during parsing, not panic
    let _ = parse_buf(&buf);
}

#[test]
fn pdflatex_4_pages_all_content_ops_parse() {
    use pdf_engine::core::reader::page_content::PageContentError;
    let doc = parse_buf(&read_fixture("pdflatex-4-pages.pdf")).unwrap();
    for page_index in 0..4 {
        let ops = doc
            .page_content_ops(page_index)
            .unwrap_or_else(|e| panic!("page {} failed: {:?}", page_index, e));
        assert!(!ops.is_empty(), "page {} parsed zero operators", page_index);
    }
    // Page 4 should return PageNotFound
    let result = doc.page_content_ops(4);
    assert!(matches!(result, Err(PageContentError::PageNotFound)));
}

#[test]
fn pdflatex_4_pages_state_machine_runs() {
    let doc = parse_buf(&read_fixture("pdflatex-4-pages.pdf")).unwrap();
    for page_index in 0..4 {
        let ops = doc.page_content_ops(page_index).expect("content ops parse");
        let trace = doc
            .page_content_state_trace(page_index)
            .unwrap_or_else(|e| panic!("page {} state machine failed: {:?}", page_index, e));
        assert_eq!(
            trace.len(),
            ops.len(),
            "page {} should snapshot once per operator",
            page_index
        );
        // Initial state follows Table 57 defaults until operators change it.
        assert_eq!(trace[0].line_width, 1.0);
    }
    let final_state = doc.page_content_state(0).expect("final state");
    assert!(final_state.ctm.iter().all(|v| v.is_finite()));
}

#[test]
fn pdflatex_4_pages_marked_content_pairs_with_spans() {
    let doc = parse_buf(&read_fixture("pdflatex-4-pages.pdf")).unwrap();
    for page_index in 0..4 {
        let ops = doc.page_content_ops(page_index).expect("content ops parse");
        let sections = doc
            .page_marked_content(page_index)
            .unwrap_or_else(|e| panic!("page {} marked content failed: {e}", page_index));
        for section in &sections {
            assert!(section.end <= ops.len(), "section /{} span", section.tag);
            if section.point {
                assert_eq!(section.end, section.start + 1);
            }
        }
        // Pairing is deterministic across repeat passes.
        assert_eq!(sections, doc.page_marked_content(page_index).unwrap());
    }
}

/// C5：真实 pdflatex 内容流经 `serialize_ops` → 重新解析应保持模型无损
/// （含未建模操作符透传与二进制文本的字面量/十六进制选择）。
#[test]
fn pdflatex_4_pages_content_model_serialization_roundtrips() {
    use pdf_engine::core::reader::content::{parse_content_stream, serialize_ops};
    let doc = parse_buf(&read_fixture("pdflatex-4-pages.pdf")).unwrap();
    for page_index in 0..4 {
        let ops = doc.page_content_ops(page_index).expect("content ops parse");
        let bytes = serialize_ops(&ops);
        let back = parse_content_stream(&bytes)
            .unwrap_or_else(|e| panic!("page {page_index} re-parse failed: {e}"));
        assert_eq!(
            ops, back,
            "page {page_index} model changed through roundtrip"
        );
    }
}

#[test]
fn xmp_metadata_fixture_parses_without_error() {
    let _doc = parse_buf(&read_fixture("xmp-metadata.pdf")).unwrap();
}

#[test]
fn xmp_metadata_fixture_exposes_metadata_streams_and_info() {
    let doc = parse_buf(&read_fixture("xmp-metadata.pdf")).unwrap();
    let xml = doc
        .xmp_metadata()
        .expect("fixture /Metadata stream should decode");
    assert!(
        xml.windows(7).any(|w| w == b"xmpmeta") || xml.windows(5).any(|w| w == b"<?xml"),
        "decoded bytes should look like XML"
    );
    assert!(
        doc.document_info().is_some(),
        "fixture trailer /Info should resolve",
    );
    assert!(
        doc.file_identifier().is_some(),
        "fixture trailer /ID should be two 16-byte strings",
    );
}
