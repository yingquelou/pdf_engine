//! 混合引用写出单元测试：隐藏集计算与三段式字节形态（7.5.8.4）。

use super::*;
use crate::core::types::{Object, StartXref, Trailer, Xref};
use crate::core::writer::DocumentWriter;

fn name(v: &str) -> Value {
    Value::Name(v.into())
}

fn r(n: i64) -> Value {
    Value::Ref(n, 0)
}

/// 主测试夹具（对象号 1..8）：
/// - 1 目录：`Pages 2R`、`Outlines` **直接字典**值、`StructTreeRoot 5R`；
/// - 2 页树（可见）；5/6/7 结构树闭包（候选隐藏，5 恶意回指 1、6 回指 2）；
/// - 8 内容**流**对象（6 引用它）——v1 不隐藏流对象，留在主表登记 `n`。
fn fixture_doc() -> Document {
    let catalog = Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", r(2))
                .insert(
                    "Outlines",
                    Value::Dict(Dict::new().insert("Count", Value::Integer(0))),
                )
                .insert("StructTreeRoot", r(5)),
        ),
    );
    let pages = Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", Value::Array(Array::new()))
                .insert("Count", Value::Integer(0)),
        ),
    );
    let struct_root = Object::new(
        5,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("StructTreeRoot"))
                .insert("K", r(6))
                // 恶意回指目录：目录恒可见，不受影响。
                .insert("Parent", r(1)),
        ),
    );
    let element = Object::new(
        6,
        0,
        Value::Dict(
            Dict::new()
                .insert("P", r(5))
                .insert("Next", r(7))
                .insert("Pg", r(2))
                // 引用内容流：流对象 v1 不隐藏。
                .insert("C", r(8)),
        ),
    );
    let leaf = Object::new(7, 0, Value::Dict(Dict::new().insert("P", r(6))));
    let content = Object::new(
        8,
        0,
        Value::Stream(
            Dict::new().insert("Length", Value::Integer(6)),
            StreamData::Owned(b"BT  ET".to_vec()),
        ),
    );
    Document::from_objects(
        vec![catalog, pages, struct_root, element, leaf, content],
        Xref::new(),
        Trailer {
            dict: Dict::new()
                .insert("Root", r(1))
                .insert("Size", Value::Integer(9)),
        },
        StartXref(0),
    )
}

fn hidden_of(doc: &Document, policy: &HybridReferencePolicy) -> BTreeSet<i64> {
    compute_hidden(&doc.objects_for_write(), &doc.trailer, policy).expect("hidden set")
}

/// 主表条目行提取：定位 `xref\n0 <size>\n` 后按 20 字节定宽读回。
fn main_table(out: &[u8]) -> (i64, Vec<(i64, i64, char)>) {
    let pos = out
        .windows(5)
        .position(|w| w == b"xref\n")
        .expect("xref keyword");
    let body = pos + 5;
    let nl = body
        + out[body..]
            .iter()
            .position(|&b| b == b'\n')
            .expect("header line");
    let header = std::str::from_utf8(&out[body..nl]).expect("header ascii");
    let mut it = header.split_whitespace();
    let start: i64 = it.next().unwrap().parse().unwrap();
    let size: i64 = it.next().unwrap().parse().unwrap();
    assert_eq!(start, 0);
    let mut rows = Vec::with_capacity(size as usize);
    for i in 0..size {
        let line = &out[nl + 1 + (i as usize) * 20..nl + 1 + (i as usize + 1) * 20];
        let text = std::str::from_utf8(line).expect("table line ascii");
        let offset: i64 = text[0..10].parse().expect("offset");
        let generation: i64 = text[11..16].parse().expect("generation");
        let kind = text.chars().nth(17).expect("n/f");
        rows.push((offset, generation, kind));
    }
    (size, rows)
}

/// trailer 文本切片：第 `idx` 个 `trailer` 关键字之后到 `startxref` 之前。
fn trailer_text(out: &[u8], idx: usize) -> String {
    let mut from = 0usize;
    let mut pos = 0usize;
    for _ in 0..=idx {
        let found = out[from..]
            .windows(7)
            .position(|w| w == b"trailer")
            .map(|p| from + p)
            .expect("trailer keyword");
        pos = found;
        from = found + 7;
    }
    let end = out[pos..]
        .windows(9)
        .position(|w| w == b"startxref")
        .map(|p| pos + p)
        .expect("startxref after trailer");
    String::from_utf8_lossy(&out[pos..end]).into_owned()
}

#[test]
fn hidden_set_excludes_catalog_pages_and_stream_objects() {
    let doc = fixture_doc();
    let hidden = hidden_of(&doc, &HybridReferencePolicy::default());
    // 结构树闭包 5/6/7 隐藏；目录 1、页树 2 可见；流对象 8 v1 不隐藏；
    // Outlines 的直接字典值无法隐藏（跳过、不报错）。
    assert_eq!(hidden.into_iter().collect::<Vec<i64>>(), vec![5, 6, 7]);
}

#[test]
fn hidden_set_protects_root_target_and_trailer_info_object() {
    // 1 目录 {Pages 2R, Outlines 3R}；3 大纲根引用 2（页树，可见）与 9
    // （trailer /Info 目标）。隐藏集必须只剩 3。
    let catalog = Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", r(2))
                .insert("Outlines", r(3)),
        ),
    );
    let pages = Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", Value::Array(Array::new())),
        ),
    );
    let outline = Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Outlines"))
                .insert("First", r(2))
                .insert("Notes", r(9)),
        ),
    );
    let info = Object::new(
        9,
        0,
        Value::Dict(Dict::new().insert("Title", Value::String("t".into()))),
    );
    let doc = Document::from_objects(
        vec![catalog, pages, outline, info],
        Xref::new(),
        Trailer {
            dict: Dict::new()
                .insert("Root", r(1))
                .insert("Info", r(9))
                .insert("Size", Value::Integer(10)),
        },
        StartXref(0),
    );
    let hidden = hidden_of(&doc, &HybridReferencePolicy::default());
    assert_eq!(hidden.into_iter().collect::<Vec<i64>>(), vec![3]);
}

#[test]
fn explicit_catalog_keys_policy_only_hides_listed_key_closure() {
    // 夹具里改用显式键 Outlines（指向直接字典 → 跳过）之外的空闭合：
    // 显式键 StructTreeRoot 才隐藏 5/6/7；只列 Threads 时无隐藏 → 拒绝。
    let doc = fixture_doc();
    let hidden = hidden_of(
        &doc,
        &HybridReferencePolicy::catalog_keys(["StructTreeRoot"]),
    );
    assert_eq!(hidden.into_iter().collect::<Vec<i64>>(), vec![5, 6, 7]);
    let err = compute_hidden(
        &doc.objects_for_write(),
        &doc.trailer,
        &HybridReferencePolicy::catalog_keys(["Threads"]),
    )
    .expect_err("no closure for Threads");
    assert!(matches!(err, WriteError::InvalidObject(_)));
}

#[test]
fn main_table_marks_hidden_numbers_as_free_generation_65535() {
    let mut doc = fixture_doc();
    let mut out: Vec<u8> = Vec::new();
    DocumentWriter::new()
        .build_hybrid(&mut doc, &mut out)
        .unwrap();
    let (size, rows) = main_table(&out);
    // 最大对象号 8 → 对象流 9、交叉引用流 10、Size 11（覆盖流对象号）。
    assert_eq!(size, 11);
    assert_eq!(rows.len(), 11);
    for num in [5i64, 6, 7, 9, 10] {
        let (_, generation, kind) = rows[num as usize];
        assert_eq!(
            (generation, kind),
            (65535, 'f'),
            "对象 {num} 应为 65535 自由条目"
        );
    }
    // 可见对象 1/2 与流对象 8 按偏移登记 n 条目，且偏移确实指向对象头。
    for num in [1i64, 2, 8] {
        let (offset, generation, kind) = rows[num as usize];
        assert_eq!((generation, kind), (0, 'n'), "对象 {num} 应为 n 条目");
        let prefix = format!("{num} 0 obj");
        assert!(
            out[offset as usize..].starts_with(prefix.as_bytes()),
            "对象 {num} 的偏移应指向 `{prefix}`"
        );
    }
    // 从未分配的空洞号 3/4：沿用 `0000000000 00000 f` 形态。
    for num in [3i64, 4] {
        assert_eq!(rows[num as usize], (0, 0, 'f'));
    }
}

#[test]
fn main_table_free_chain_links_hidden_numbers_and_ends_at_zero() {
    let mut doc = fixture_doc();
    let mut out: Vec<u8> = Vec::new();
    DocumentWriter::new()
        .build_hybrid(&mut doc, &mut out)
        .unwrap();
    let (_, rows) = main_table(&out);
    // 自由链 5→6→7→9(对象流)→10(交叉引用流)→0，表头 0 指向链首 5。
    assert_eq!(rows[0].0, 5);
    for (from, to) in [(5i64, 6i64), (6, 7), (7, 9), (9, 10), (10, 0)] {
        assert_eq!(rows[from as usize].0, to, "自由链 {from} 应指向 {to}");
    }
}

#[test]
fn xref_stm_entry_only_appears_in_update_section_trailer() {
    let mut doc = fixture_doc();
    let mut out: Vec<u8> = Vec::new();
    DocumentWriter::new()
        .build_hybrid(&mut doc, &mut out)
        .unwrap();
    // 主 trailer（第一段）不含 /XRefStm（Table 19 shall）。
    let main_trailer = trailer_text(&out, 0);
    assert!(
        !main_trailer.contains("/XRefStm"),
        "主 trailer 不应含 /XRefStm: {main_trailer}"
    );
    assert!(main_trailer.contains("/Size 11"));
    assert!(main_trailer.contains("/Root 1 0 R"));
    // 更新段 trailer 含 /XRefStm 与 /Prev，且偏移分别指向流对象与主表。
    let update_trailer = trailer_text(&out, 1);
    let xref_stm: i64 = update_trailer
        .split("/XRefStm ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .expect("update trailer /XRefStm");
    assert!(
        out[xref_stm as usize..].starts_with(b"10 0 obj"),
        "/XRefStm 应指向交叉引用流对象 10"
    );
    let prev: i64 = update_trailer
        .split("/Prev ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .expect("update trailer /Prev");
    assert!(
        out[prev as usize..].starts_with(b"xref\n0 11"),
        "/Prev 应指向主经典表"
    );
}

#[test]
fn supplemental_xref_stream_covers_hidden_and_stream_numbers_only() {
    let mut doc = fixture_doc();
    let mut out: Vec<u8> = Vec::new();
    DocumentWriter::new()
        .build_hybrid(&mut doc, &mut out)
        .unwrap();
    // 交叉引用流字典：/Index 覆盖 [5..7] 与 [9..10] 两段；无 /Prev；/Size 同主表。
    let stm = out
        .windows(b"10 0 obj".len())
        .position(|w| w == b"10 0 obj")
        .expect("xref stream object");
    // 字典切片只取到该对象的 endobj，避免覆盖其后更新段 trailer 的 /Prev。
    let end = out[stm..]
        .windows(b"endobj".len())
        .position(|w| w == b"endobj")
        .map(|p| stm + p)
        .expect("endobj after xref stream object");
    let head = String::from_utf8_lossy(&out[stm..end]).into_owned();
    assert!(head.contains("/Type /XRef"));
    assert!(head.contains("/Size 11"));
    assert!(
        head.contains("/Index [5 3 9 2]"),
        "xref stream dict: {head}"
    );
    assert!(!head.contains("/Prev"), "混合引用的交叉引用流不写 /Prev");
}

#[test]
fn output_ends_with_empty_update_section_trailer_and_startxref() {
    let mut doc = fixture_doc();
    let mut out: Vec<u8> = Vec::new();
    DocumentWriter::new()
        .build_hybrid(&mut doc, &mut out)
        .unwrap();
    // 更新段为空经典表（规范明说更新表可以不含条目），文件以 startxref+%%EOF 收尾。
    assert!(
        out.windows(9).any(|w| w == b"xref\n0 0\n"),
        "应含空更新段 xref 0 0"
    );
    assert!(out.ends_with(b"%%EOF\n"));
    // 恰好两个 %%EOF（主段 + 更新段）。
    let mut count = 0usize;
    let mut from = 0usize;
    while let Some(p) = out[from..].windows(5).position(|w| w == b"%%EOF") {
        count += 1;
        from += p + 5;
    }
    assert_eq!(count, 2, "主段与更新段各一个 %%EOF");
}

#[test]
fn rejects_hybrid_build_when_nothing_can_be_hidden() {
    // 没有可选键闭包（Outlines 是可见页树）→ 拒绝且零字节。
    let catalog = Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", r(2))
                .insert("Outlines", r(2)),
        ),
    );
    let pages = Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", Value::Array(Array::new())),
        ),
    );
    let mut doc = Document::from_objects(
        vec![catalog, pages],
        Xref::new(),
        Trailer {
            dict: Dict::new().insert("Root", r(1)),
        },
        StartXref(0),
    );
    let mut out: Vec<u8> = Vec::new();
    let err = DocumentWriter::new()
        .build_hybrid(&mut doc, &mut out)
        .expect_err("nothing to hide must fail");
    assert!(matches!(err, WriteError::InvalidObject(_)), "{err}");
    assert!(out.is_empty(), "失败必须零字节");
}

#[test]
fn rejects_hybrid_target_version_below_pdf_1_5() {
    let mut doc = fixture_doc();
    let mut out: Vec<u8> = Vec::new();
    let err = DocumentWriter::new()
        .with_version(crate::core::types::PdfVersion::V1_4)
        .build_hybrid(&mut doc, &mut out)
        .expect_err("hybrid needs PDF 1.5");
    assert!(matches!(err, WriteError::UnsupportedVersion(_)), "{err}");
    assert!(out.is_empty(), "拒绝必须零字节");
}
