//! 混合引用文件（7.5.8.4）写出往返：builder 造带结构树与大纲的文档 →
//! 全量写出 → 解析 → `build_hybrid` 重写 → 再解析，验证隐藏进对象流的
//! 结构树/大纲闭包经 `/XRefStm` 补充流仍可回读，并核对字节形态
//! （主表无 `/XRefStm`、更新段 trailer 带 `/Prev` + `/XRefStm`）。

use pdf_engine::core::builder::{
    OutlineEntry, PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
};
use pdf_engine::core::reader::navigation::{Destination, OutlineDestination};
use pdf_engine::core::reader::structure::StructureKid;
use pdf_engine::core::types::{Document, PdfVersion};
use pdf_engine::core::writer::{DocumentWriter, HybridReferencePolicy};

use super::*;

/// builder 造两页文档：大纲两层 + 结构树 Document > P（带 MCR），
/// 全量写出后解析成读端 Document。
fn parsed_builder_doc() -> Document {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    builder.outline(&[OutlineEntry {
        title: "Chapter 1".into(),
        destination: Some(OutlineDestination::Explicit(Destination::Fit { page: 0 })),
        action: None,
        color: None,
        flags: 0,
        expanded: true,
        children: vec![OutlineEntry {
            title: "Section 1.1".into(),
            destination: Some(OutlineDestination::Explicit(Destination::FitH {
                page: 1,
                top: Some(700.0),
            })),
            action: None,
            color: None,
            flags: 0,
            expanded: false,
            children: Vec::new(),
        }],
    }]);
    let tree = StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_id("doc")
            .with_element(
                StructureElement::new("P")
                    .with_id("para")
                    .with_marked_content(0, 0),
            ),
    );
    builder.set_structure_tree(tree);
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build outline + structure doc");
    parse_buf(&buf)
}

#[test]
fn hybrid_roundtrip_keeps_structure_tree_and_outline_readable() {
    let mut doc = parsed_builder_doc();
    let mut out: Vec<u8> = Vec::new();
    DocumentWriter::new()
        .hybrid_reference(HybridReferencePolicy::default())
        .build_hybrid(&mut doc, &mut out)
        .expect("build hybrid-reference file");

    let reparsed = parse_buf(&out);
    // 大纲闭包与结构树闭包都被打包进对象流（主表登记 65535 自由条目），
    // 须经 trailer /XRefStm 指向的补充流才能定位——重解析后仍须完整可读。
    let outline = reparsed
        .outline()
        .expect("outline parse")
        .expect("outline present");
    assert_eq!(outline.items.len(), 1);
    assert_eq!(outline.items[0].title, "Chapter 1");
    assert_eq!(outline.items[0].children.len(), 1);
    assert_eq!(outline.items[0].children[0].title, "Section 1.1");

    let tree = reparsed
        .structure_tree()
        .expect("structure parse")
        .expect("structure present");
    let types: Vec<&str> = tree
        .elements()
        .iter()
        .map(|element| element.structure_type.as_str())
        .collect();
    assert_eq!(types, ["Document", "P"]);
    let document = &tree.elements()[0];
    assert!(matches!(document.kids[..], [StructureKid::Element(1)]));
    assert_eq!(tree.elements()[1].id.as_deref(), Some(b"para".as_slice()));

    // 可见部分（页树/目录）不受影响。
    assert_eq!(reparsed.page_count().expect("page count"), 2);
}

#[test]
fn hybrid_bytes_have_classic_main_table_and_xref_stm_update_trailer() {
    let mut doc = parsed_builder_doc();
    let mut out: Vec<u8> = Vec::new();
    DocumentWriter::new()
        .build_hybrid(&mut doc, &mut out)
        .expect("build hybrid-reference file");
    let text = String::from_utf8_lossy(&out);

    // 更新段是空经典表（`xref 0 0`），文件以 startxref + %%EOF 收尾。
    assert!(text.contains("xref\n0 0\n"), "应含空更新段 xref 0 0");
    assert!(out.ends_with(b"%%EOF\n"));
    // 恰有两个 trailer：主段 + 更新段。
    assert_eq!(
        text.matches("trailer").count(),
        2,
        "混合引用应恰好产出主/更新两个 trailer"
    );
    // 主 trailer 不含 /XRefStm（Table 19 shall）；更新段 trailer 两者皆含。
    let main = text
        .split("trailer")
        .nth(1)
        .expect("main trailer text")
        .split("startxref")
        .next()
        .expect("main trailer dict");
    assert!(
        !main.contains("/XRefStm"),
        "主 trailer 不应含 /XRefStm: {main}"
    );
    let update = text
        .split("trailer")
        .nth(2)
        .expect("update trailer text")
        .split("startxref")
        .next()
        .expect("update trailer dict");
    assert!(
        update.contains("/Prev "),
        "更新段 trailer 应含 /Prev: {update}"
    );
    assert!(
        update.contains("/XRefStm "),
        "更新段 trailer 应含 /XRefStm: {update}"
    );
    // 隐藏号在主流中登记为 65535 自由条目（对象流/交叉引用流自身也是）。
    assert!(
        text.matches(" 65535 f").count() >= 3,
        "隐藏号/对象流号/流自身都应是 65535 自由条目"
    );
    // 主表首子段从 0 开始并覆盖声明的 /Size。
    let table = text
        .split("xref\n")
        .nth(1)
        .expect("main xref section")
        .split("trailer")
        .next()
        .expect("main table body");
    let mut lines = table.split_whitespace();
    assert_eq!(lines.next(), Some("0"), "主表子段应从 0 开始");
    let declared: usize = lines.next().expect("count").parse().unwrap();
    assert_eq!(
        lines.count(),
        declared * 3,
        "主表条目 token 数应与 /Size 一致"
    );
}

#[test]
fn hybrid_rejects_explicit_policy_without_hideable_keys() {
    let mut doc = parsed_builder_doc();
    let mut out: Vec<u8> = Vec::new();
    let err = DocumentWriter::new()
        .hybrid_reference(HybridReferencePolicy::catalog_keys(["NotAKey"]))
        .build_hybrid(&mut doc, &mut out)
        .expect_err("unknown catalog key must reject");
    assert!(
        err.to_string().contains("nothing to hide"),
        "unexpected error: {err}"
    );
    assert!(out.is_empty(), "拒绝必须零字节");
}
