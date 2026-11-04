//! `/Differences` 增量更新字节 §7.5.6 对象重放的集成往返测试。

use pdf_engine::core::types::Value;

use super::support::parse_buf;

/// 拼一个带 `/Differences` 流的 FDF 文件（增量更新字节 = 一个简单对象）。
fn fdf_with_differences_stream(object_bytes: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"%FDF-1.4\n");

    // Object 1: Catalog with /FDF pointer
    let catalog_offset = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /FDF 2 0 R >>\nendobj\n");

    // Object 2: FDF document dictionary with /Differences pointer
    let fdf_dict_offset = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Differences 3 0 R >>\nendobj\n");

    // Object 3: Stream containing the incremental update bytes
    let stream_offset = buf.len();
    let stream_header = format!("3 0 obj\n<< /Length {} >>\nstream\n", object_bytes.len());
    buf.extend_from_slice(stream_header.as_bytes());
    buf.extend_from_slice(object_bytes.as_bytes());
    buf.extend_from_slice(b"\nendstream\nendobj\n");

    // Xref table
    let xref_offset = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n");
    buf.extend_from_slice(b"0000000000 65535 f \n");
    buf.extend_from_slice(format!("{:010} 00000 n \n", catalog_offset).as_bytes());
    buf.extend_from_slice(format!("{:010} 00000 n \n", fdf_dict_offset).as_bytes());
    buf.extend_from_slice(format!("{:010} 00000 n \n", stream_offset).as_bytes());
    buf.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    buf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());

    buf
}

#[test]
fn differences_replay_exposes_single_object() {
    let object_bytes = "2 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n";
    let fdf_bytes = fdf_with_differences_stream(object_bytes);
    let document = parse_buf(&fdf_bytes);

    let differences = document
        .fdf_differences()
        .expect("ok")
        .expect("should have /Differences");

    assert!(differences.is_stream(), "should be a stream");

    let replay = differences.replay(&document);
    assert_eq!(replay.len(), 1, "should replay one object");
    assert_eq!(replay.entries[0].num, 2);
    assert_eq!(replay.entries[0].generation, 0);
    assert!(matches!(replay.entries[0].value, Value::Dict(_)));
    assert!(!replay.is_incremental(), "no xref/trailer markers in this stream");
    assert!(replay.issues.is_empty());
}

#[test]
fn differences_replay_handles_multiple_objects() {
    let object_bytes = "10 0 obj\n42\nendobj\n11 0 obj\n(hello)\nendobj\n";
    let fdf_bytes = fdf_with_differences_stream(object_bytes);
    let document = parse_buf(&fdf_bytes);
    
    let differences = document
        .fdf_differences()
        .expect("ok")
        .expect("should have /Differences");
    
    let replay = differences.replay(&document);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.entries[0].num, 10);
    assert_eq!(replay.entries[1].num, 11);
    assert!(matches!(replay.entries[0].value, Value::Integer(42)));
    assert!(matches!(&replay.entries[1].value, Value::String(s) if s == "hello"));
}

#[test]
fn differences_replay_latest_copy_wins() {
    let object_bytes = "5 0 obj\n100\nendobj\n5 0 obj\n200\nendobj\n5 0 obj\n300\nendobj\n";
    let fdf_bytes = fdf_with_differences_stream(object_bytes);
    let document = parse_buf(&fdf_bytes);
    
    let differences = document
        .fdf_differences()
        .expect("ok")
        .expect("should have /Differences");
    
    let replay = differences.replay(&document);
    assert_eq!(replay.len(), 3, "all copies preserved");
    assert_eq!(replay.effective_count(), 1, "one unique identifier");
    
    let latest = replay.latest(5, 0).expect("should find latest");
    assert!(matches!(latest.value, Value::Integer(300)));
    
    let copies = replay.copies(5, 0);
    assert_eq!(copies.len(), 3);
}

#[test]
fn differences_replay_detects_incremental_markers() {
    let object_bytes = "1 0 obj\n42\nendobj\nxref\n0 2\n0000000000 65535 f \n0000000010 00000 n \ntrailer\n<< /Size 2 >>\nstartxref\n100\n%%EOF\n";
    let fdf_bytes = fdf_with_differences_stream(object_bytes);
    let document = parse_buf(&fdf_bytes);
    
    let differences = document
        .fdf_differences()
        .expect("ok")
        .expect("should have /Differences");
    
    let replay = differences.replay(&document);
    assert_eq!(replay.len(), 1);
    assert!(replay.is_incremental());
    assert_eq!(replay.xref_sections, 1);
    assert_eq!(replay.trailers, 1);
    assert_eq!(replay.startxrefs, 1);
    assert_eq!(replay.eof_markers, 1);
}

#[test]
fn differences_replay_malformed_input_degrades_gracefully() {
    let object_bytes = "1 0 obj\n42\nendobj\ngarbage not a valid object\n";
    let fdf_bytes = fdf_with_differences_stream(object_bytes);
    let document = parse_buf(&fdf_bytes);
    
    let differences = document
        .fdf_differences()
        .expect("ok")
        .expect("should have /Differences");
    
    let replay = differences.replay(&document);
    assert_eq!(replay.len(), 1, "only valid object replayed");
    assert!(!replay.issues.is_empty(), "malformed input recorded");
    assert!(replay.issues[0].contains("byte offset"));
}

#[test]
fn differences_replay_empty_stream_yields_empty() {
    let object_bytes = "";
    let fdf_bytes = fdf_with_differences_stream(object_bytes);
    let document = parse_buf(&fdf_bytes);
    
    let differences = document
        .fdf_differences()
        .expect("ok")
        .expect("should have /Differences");
    
    let replay = differences.replay(&document);
    assert!(replay.is_empty());
    assert_eq!(replay.effective_count(), 0);
}
