//! 14.8.2.3.1 tag suspects 的写出 → 解析 → 回读往返。

use super::*;

#[test]
fn builder_tag_suspect_roundtrip_keeps_document_conforming() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::types::{PdfVersion, Value};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "First line");
    builder.end_structure_content(page);
    // 页序无法判定的一段内容：14.8.2.3.1 的 tag suspect。
    builder.begin_tag_suspect(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "Second line");
    builder.end_tag_suspect(page);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0)),
        ),
    );
    builder.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_suspects(true),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with structure content and tag suspect");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/P << /MCID 0 >> BDC",
        "/TagSuspect << /TagSuspect /Ordering >> BDC",
        "/StructParents 0",
        "/MarkInfo <<",
        "/Suspects true",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let info = doc
        .mark_info()
        .expect("mark info")
        .expect("/MarkInfo present");
    assert!(info.marked, "文档应声明符合标记内容约定");
    assert!(
        info.suspects,
        "含 tag suspects 的文档必须声明 /Suspects true"
    );

    let sections = doc.page_marked_content(page).expect("marked content");
    let suspects: Vec<Option<&str>> = sections
        .iter()
        .filter(|section| section.tag == "TagSuspect")
        .map(|section| {
            section
                .properties
                .as_ref()
                .and_then(|dict| dict.get("TagSuspect"))
                .and_then(Value::as_name)
        })
        .collect();
    assert_eq!(suspects, vec![Some("Ordering")], "唯一一段 tag suspect");

    assert_eq!(
        doc.artifact_attribution_violations(page)
            .expect("diagnose writer produced page"),
        vec![],
        "写端产出的 tag suspect 应满足 14.8.2.3.1 的两条 shall 与真实内容豁免"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("First line\nSecond line"),
        "tag suspect 段内的文本照常提取"
    );
}
