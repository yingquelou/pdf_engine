//! `/Differences` 增量更新字节的 §7.5.6 对象重放单元测试。

use crate::core::reader::fdf::embedded::replay::replay_bytes;
use crate::core::types::Value;

#[test]
fn replay_empty_bytes_yields_empty_entries() {
    let result = replay_bytes(b"");
    assert!(result.is_empty());
    assert_eq!(result.effective_count(), 0);
    assert!(!result.is_incremental());
    assert!(result.issues.is_empty());
}

#[test]
fn replay_single_indirect_object() {
    let bytes = b"1 0 obj\n<< /Type /Catalog >>\nendobj\n";
    let result = replay_bytes(bytes);
    assert_eq!(result.len(), 1);
    assert_eq!(result.entries[0].num, 1);
    assert_eq!(result.entries[0].generation, 0);
    assert!(matches!(result.entries[0].value, Value::Dict(_)));
    assert!(!result.is_incremental());
    assert!(result.issues.is_empty());
}

#[test]
fn replay_multiple_objects_in_file_order() {
    let bytes = b"1 0 obj\n42\nendobj\n2 0 obj\n(hello)\nendobj\n3 0 obj\n[1 2 3]\nendobj\n";
    let result = replay_bytes(bytes);
    assert_eq!(result.len(), 3);
    assert_eq!(result.entries[0].num, 1);
    assert_eq!(result.entries[1].num, 2);
    assert_eq!(result.entries[2].num, 3);
    assert!(matches!(result.entries[0].value, Value::Integer(42)));
    assert!(matches!(&result.entries[1].value, Value::String(s) if s == "hello"));
    assert!(matches!(&result.entries[2].value, Value::Array(_)));
}

#[test]
fn replay_same_object_multiple_copies_latest_wins() {
    let bytes = b"1 0 obj\n100\nendobj\n1 0 obj\n200\nendobj\n1 0 obj\n300\nendobj\n";
    let result = replay_bytes(bytes);
    assert_eq!(result.len(), 3, "all copies preserved in file order");
    assert_eq!(result.effective_count(), 1, "only one unique identifier");
    let latest = result.latest(1, 0).expect("should find latest");
    assert!(matches!(latest.value, Value::Integer(300)));
    let copies = result.copies(1, 0);
    assert_eq!(copies.len(), 3);
    assert!(matches!(copies[0].value, Value::Integer(100)));
    assert!(matches!(copies[2].value, Value::Integer(300)));
}

#[test]
fn replay_detects_incremental_markers() {
    let bytes = b"1 0 obj\n42\nendobj\nxref\n0 2\ntrailer\n<< /Size 2 >>\nstartxref\n0\n%%EOF\n";
    let result = replay_bytes(bytes);
    assert_eq!(result.len(), 1);
    assert!(result.is_incremental());
    assert_eq!(result.xref_sections, 1);
    assert_eq!(result.trailers, 1);
    assert_eq!(result.startxrefs, 1);
    assert_eq!(result.eof_markers, 1);
}

#[test]
fn replay_malformed_object_stops_with_diagnostic() {
    let bytes = b"1 0 obj\n42\nendobj\ngarbage that is not a valid object\n";
    let result = replay_bytes(bytes);
    assert_eq!(result.len(), 1, "only the first valid object replayed");
    assert!(!result.issues.is_empty(), "malformed input recorded as diagnostic");
    assert!(result.issues[0].contains("byte offset"));
}

#[test]
fn replay_xref_without_trailer_stops() {
    let bytes = b"1 0 obj\n42\nendobj\nxref\n0 1\n";
    let result = replay_bytes(bytes);
    assert_eq!(result.len(), 1);
    assert!(!result.issues.is_empty());
    assert!(result.issues[0].contains("not followed by a trailer"));
}

#[test]
fn replay_byte_offsets_are_correct() {
    let bytes = b"1 0 obj\n42\nendobj\n2 0 obj\n(hello)\nendobj\n";
    let result = replay_bytes(bytes);
    assert_eq!(result.len(), 2);
    assert_eq!(result.entries[0].byte_offset, 0);
    assert!(result.entries[1].byte_offset > 0);
}
