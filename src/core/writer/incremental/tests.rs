//! 增量更新写出测试（7.5.6）：复制源字节、追加修订、回读验证 /Prev 链。

use crate::core::builder::FileIdentifier;
use crate::core::reader::Parser;
use crate::core::types::{Dict, Document, Object, PdfVersion, Trailer, Value, XrefKind};
use crate::core::writer::{DocumentWriter, WriteError};
use std::io::Write as _;

/// 与 `writer/tests.rs` 相同策略：临时文件落在 target/test-tmp/ 下。
fn temp_path(tag: &str) -> std::path::PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!(
        "demo-incr-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

fn parse_bytes(bytes: &[u8], tag: &str) -> Document {
    let path = temp_path(tag);
    {
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
    }
    let file = std::fs::File::open(&path).unwrap();
    let mut p = Parser::new(file).unwrap();
    p.parse_document().unwrap()
}

/// 基线文档：Catalog（1）+ 载荷字典（2），trailer 带自定义键 `/Seed`。
fn base_bytes(version: Option<PdfVersion>) -> Vec<u8> {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert("Note", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(Dict::new().insert("Payload", Value::Integer(42))),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Seed", Value::Integer(7)),
    };
    let mut buf = Vec::new();
    let mut writer = DocumentWriter::new();
    if let Some(v) = version {
        writer = writer.with_version(v);
    }
    writer.build(&doc, &mut buf).unwrap();
    buf
}

fn incremental_bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    DocumentWriter::new()
        .build_incremental(doc, &mut out)
        .expect("incremental build should succeed");
    out
}

#[test]
fn incremental_stream_section_chains_prev_and_keeps_history() {
    let base = base_bytes(None);
    let doc = parse_bytes(&base, "chain");
    let original_start = doc.startxref.0;
    doc.set_object(Object::new(
        9,
        0,
        Value::Dict(Dict::new().insert("Added", Value::Integer(1))),
    ));
    let bytes = incremental_bytes(&doc);
    assert_eq!(
        &bytes[..base.len()],
        &base[..],
        "增量输出应以源文件字节为前缀"
    );
    let revised = parse_bytes(&bytes, "chain2");
    assert!(revised.startxref.0 > original_start);
    assert_eq!(
        revised.trailer.dict.get("Prev"),
        Some(&Value::Integer(original_start))
    );
    // trailer 合并：非结构键 /Seed 沿用，/Root 保留。
    assert_eq!(revised.trailer.dict.get("Seed"), Some(&Value::Integer(7)));
    let added = revised
        .get_object(9)
        .unwrap()
        .expect("added object visible");
    assert_eq!(
        added.value.as_dict().unwrap().get("Added"),
        Some(&Value::Integer(1))
    );
    // 未触碰的历史对象沿 /Prev 链回退解析。
    let catalog = revised
        .get_object(1)
        .unwrap()
        .expect("catalog from history");
    assert_eq!(
        catalog
            .value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(Value::as_name),
        Some("Catalog")
    );
}

#[test]
fn incremental_overrides_object_with_bumped_generation() {
    let doc = parse_bytes(&base_bytes(Some(PdfVersion::V1_4)), "ovr");
    let payload = Value::Dict(Dict::new().insert("Payload", Value::Integer(43)));
    doc.set_object(Object::new(2, 0, payload.clone()));
    let bytes = incremental_bytes(&doc);
    let revised = parse_bytes(&bytes, "ovr2");
    let obj = revised.get_object(2).unwrap().expect("override");
    assert_eq!(obj.generation, 1, "在用条目应升一代");
    assert_eq!(obj.value, payload);
    assert!(matches!(
        revised.xref.get(2).unwrap().kind,
        XrefKind::Normal
    ));

    // 第二次修订在同一号上再升一代。
    let payload2 = Value::Dict(Dict::new().insert("Payload", Value::Integer(44)));
    revised.set_object(Object::new(2, 0, payload2.clone()));
    let bytes2 = incremental_bytes(&revised);
    let revised2 = parse_bytes(&bytes2, "ovr3");
    let obj2 = revised2.get_object(2).unwrap().expect("second override");
    assert_eq!(obj2.generation, 2);
    assert_eq!(obj2.value, payload2);
}

#[test]
fn bumped_generation_still_resolves_old_generation_references() {
    // 对象 1 以 `2 0 R` 引用对象 2；增量更新后 2 升为 1 代。
    // 引用解析按对象号取最新修订（与 get_object 语义一致），旧代号
    // 引用必须仍可解析，否则增量更新会切断全部入边。
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Payload", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(Dict::new().insert("Payload", Value::Integer(42))),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    let mut base = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut base)
        .unwrap();

    let parsed = parse_bytes(&base, "genref");
    parsed.set_object(Object::new(
        2,
        0,
        Value::Dict(Dict::new().insert("Payload", Value::Integer(99))),
    ));
    let bytes = incremental_bytes(&parsed);
    let revised = parse_bytes(&bytes, "genref2");
    assert_eq!(
        revised.get_object(2).unwrap().expect("bumped").generation,
        1
    );
    let catalog = revised.get_object(1).unwrap().expect("catalog").value;
    let entry = catalog
        .as_dict()
        .unwrap()
        .get("Payload")
        .cloned()
        .expect("/Payload reference");
    let resolved = crate::core::reader::navigation::resolve(&revised, &entry)
        .expect("old-generation reference must resolve to the latest revision");
    assert_eq!(
        resolved.as_dict().unwrap().get("Payload"),
        Some(&Value::Integer(99))
    );
}

#[test]
fn incremental_free_entry_hides_deleted_object() {
    let doc = parse_bytes(&base_bytes(Some(PdfVersion::V1_4)), "del");
    assert!(doc.get_object(2).unwrap().is_some());
    doc.delete_object(2);
    let bytes = incremental_bytes(&doc);
    let revised = parse_bytes(&bytes, "del2");
    assert!(
        revised.get_object(2).unwrap().is_none(),
        "自由条目应遮蔽历史在用条目"
    );
    let entry = revised.xref.get(2).expect("free entry present");
    assert!(matches!(entry.kind, XrefKind::Free));
    assert_eq!(entry.generation, 1);
    // 同文档其余对象不受影响。
    assert!(revised.get_object(1).unwrap().is_some());
}

#[test]
fn incremental_classic_table_merges_history_entries() {
    let doc = parse_bytes(&base_bytes(Some(PdfVersion::V1_4)), "classic");
    let original_start = doc.startxref.0;
    doc.set_object(Object::new(
        9,
        0,
        Value::Dict(Dict::new().insert("Added", Value::Integer(1))),
    ));
    let bytes = incremental_bytes(&doc);
    let out = String::from_utf8_lossy(&bytes);
    assert_eq!(
        out.matches("\nxref\n").count(),
        2,
        "增量段应是第二段经典 xref 全表"
    );
    let revised = parse_bytes(&bytes, "classic2");
    // 全表合并：未变更对象 2 由复制的历史条目直接定位。
    let payload = revised.get_object(2).unwrap().expect("history entry");
    assert_eq!(
        payload.value.as_dict().unwrap().get("Payload"),
        Some(&Value::Integer(42))
    );
    assert!(revised.get_object(9).unwrap().is_some());
    assert_eq!(
        revised.trailer.dict.get("Prev"),
        Some(&Value::Integer(original_start))
    );
    // 经典 trailer 不得携带 XRef 流结构键。
    assert!(revised.trailer.dict.get("W").is_none());
    assert!(revised.trailer.dict.get("Index").is_none());
}

#[test]
fn incremental_updates_file_identifier_permanent_kept() {
    let id = FileIdentifier::generate(b"seed-material");
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("ID", id.to_value()),
    };
    let mut base = Vec::new();
    DocumentWriter::new().build(&doc, &mut base).unwrap();
    let parsed = parse_bytes(&base, "id");
    let (perm0, upd0) = parsed.file_identifier().expect("base /ID");
    assert_eq!(perm0, id.permanent);

    parsed.set_object(Object::new(
        5,
        0,
        Value::Dict(Dict::new().insert("Mark", Value::Integer(1))),
    ));
    let bytes = incremental_bytes(&parsed);
    let revised = parse_bytes(&bytes, "id2");
    let (perm1, upd1) = revised.file_identifier().expect("updated /ID");
    assert!(id.permanent_matches(&perm1), "永久 ID 必须保持不变");
    assert_eq!(perm1, perm0);
    assert_ne!(upd1, upd0, "更新 ID 应随修订变化");
}

#[test]
fn incremental_requires_file_backed_document() {
    let doc = Document::new();
    let mut out = Vec::new();
    let error = DocumentWriter::new()
        .build_incremental(&doc, &mut out)
        .expect_err("in-memory document has no source bytes");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("requires a document parsed from a file"),
        "{error}"
    );
    assert!(out.is_empty(), "拒绝时不应写出任何字节");
}

#[test]
fn incremental_rejects_header_version_change() {
    let doc = parse_bytes(&base_bytes(None), "ver");
    let mut out = Vec::new();
    let error = DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build_incremental(&doc, &mut out)
        .expect_err("file header version cannot be rewritten by an update");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("cannot change the header version"),
        "{error}"
    );
    assert!(out.is_empty(), "拒绝时不应写出任何字节");
}

#[test]
fn incremental_rejects_modify_and_delete_conflict() {
    let doc = parse_bytes(&base_bytes(None), "conflict");
    // 先删后改：同一号同时出现在删除集与脏集，语义矛盾。
    doc.delete_object(2);
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(Dict::new().insert("Payload", Value::Integer(43))),
    ));
    let mut out = Vec::new();
    let error = DocumentWriter::new()
        .build_incremental(&doc, &mut out)
        .expect_err("object cannot be both modified and deleted");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("both modified and deleted"),
        "{error}"
    );
    assert!(out.is_empty(), "拒绝时不应写出任何字节");
}
