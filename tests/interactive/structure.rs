//! 逻辑结构树写出 → 解析 → 回读往返（ISO 32000-1 14.7，Table 321–326）。
//!
//! 两页文档：根 `/RoleMap`（`MyPara` → `P`）、`/ClassMap` 属性类、嵌套
//! `Document > MyPara > Span` 与顶层 `P`；两页各有一个 MCR 子项。元素带
//! `/ID` `/T` `/Lang` `/Alt` `/E` `/ActualText` `/C`（含修订号）`/R`，
//! 目录带 `/MarkInfo`；解析后逐项校验前序、父链、父树、ID 树与特性画像。

use super::*;

#[test]
fn builder_structure_tree_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::structure::StructureKid;
    use pdf_engine::core::types::{PdfFeature, PdfVersion, Value};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);

    let attributes = Dict::new()
        .insert("O", Value::Name("Layout".into()))
        .insert("SpaceBefore", Value::Integer(18));
    let tree = StructureTree::new()
        .with_role("MyPara", "P")
        .with_class("Body", Value::Dict(attributes))
        .with_element(
            StructureElement::new("Document")
                .with_id("doc")
                .with_title("Report")
                .with_lang("en-US")
                .with_element(
                    StructureElement::new("MyPara")
                        .with_id("intro")
                        .with_title("Introduction")
                        .with_alt("Intro paragraph")
                        .with_expanded("Introduction")
                        .with_actual_text("Hello world")
                        .with_class_revision("Body", 2)
                        .with_revision(1)
                        .with_element(
                            StructureElement::new("Span")
                                .with_id("span")
                                .with_actual_text("Hello")
                                .with_marked_content(0, 0),
                        ),
                )
                .with_element(
                    StructureElement::new("P")
                        .with_id("second")
                        .with_title("Second page")
                        .with_marked_content(1, 0),
                ),
        );
    builder.set_structure_tree(tree);
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build structure tree");
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    let root = tree.root();
    assert_eq!(root.role_map.get("MyPara").map(String::as_str), Some("P"));
    let class = root
        .class_map
        .get("Body")
        .and_then(Value::as_dict)
        .expect("class map /Body");
    assert_eq!(class.get("O").and_then(Value::as_name), Some("Layout"));
    assert_eq!(
        class.get("SpaceBefore").and_then(Value::as_integer),
        Some(18)
    );
    assert_eq!(root.parent_tree_next_key, Some(2));

    let types: Vec<&str> = tree
        .elements()
        .iter()
        .map(|element| element.structure_type.as_str())
        .collect();
    assert_eq!(types, ["Document", "MyPara", "Span", "P"]);
    assert_eq!(tree.elements()[0].parent, None);
    assert_eq!(tree.elements()[1].parent, Some(0));
    assert_eq!(tree.elements()[2].parent, Some(1));
    assert_eq!(tree.elements()[3].parent, Some(0));

    let document = &tree.elements()[0];
    assert_eq!(document.id.as_deref(), Some(b"doc".as_slice()));
    assert_eq!(document.title.as_deref(), Some("Report"));
    assert_eq!(document.lang.as_deref(), Some("en-US"));
    assert!(matches!(
        document.kids[..],
        [StructureKid::Element(1), StructureKid::Element(3)]
    ));

    let paragraph = &tree.elements()[1];
    assert_eq!(paragraph.id.as_deref(), Some(b"intro".as_slice()));
    assert_eq!(paragraph.title.as_deref(), Some("Introduction"));
    assert_eq!(paragraph.alt.as_deref(), Some("Intro paragraph"));
    assert_eq!(paragraph.expanded.as_deref(), Some("Introduction"));
    assert_eq!(paragraph.actual_text.as_deref(), Some("Hello world"));
    assert_eq!(paragraph.revision, 1);
    assert_eq!(paragraph.classes.len(), 1);
    assert_eq!(paragraph.classes[0].name, "Body");
    assert_eq!(paragraph.classes[0].revision, Some(2));

    let span = &tree.elements()[2];
    assert_eq!(span.actual_text.as_deref(), Some("Hello"));
    match &span.kids[..] {
        [StructureKid::MarkedContent(mcr)] => {
            assert_eq!(mcr.mcid, 0);
            let (page_num, _) = mcr.page.expect("MCR /Pg");
            let page = doc
                .get_object(page_num)
                .expect("page lookup")
                .expect("page exists")
                .value;
            assert_eq!(
                page.as_dict()
                    .and_then(|d| d.get("Type"))
                    .and_then(Value::as_name),
                Some("Page")
            );
        }
        other => panic!("expected one MCR, got {other:?}"),
    }

    let second = &tree.elements()[3];
    assert_eq!(second.id.as_deref(), Some(b"second".as_slice()));
    match &second.kids[..] {
        [StructureKid::MarkedContent(mcr)] => assert_eq!(mcr.mcid, 0),
        other => panic!("expected one MCR, got {other:?}"),
    }

    for (id, structure_type) in [
        (b"doc".as_slice(), "Document"),
        (b"intro".as_slice(), "MyPara"),
        (b"span".as_slice(), "Span"),
        (b"second".as_slice(), "P"),
    ] {
        let element = tree.element_by_id(id).expect("element by ID");
        assert_eq!(element.structure_type, structure_type);
    }
    assert!(tree.element_by_id(b"missing").is_none());

    assert_eq!(tree.resolved_role("MyPara"), Some("P"));
    assert_eq!(tree.resolved_role("Span"), None);

    let first = tree.parent_tree_entry(0).expect("parent tree key 0");
    assert_eq!(
        first,
        &pdf_engine::core::reader::structure::ParentTreeEntry::Page(vec![Some(2)])
    );
    let second = tree.parent_tree_entry(1).expect("parent tree key 1");
    assert_eq!(
        second,
        &pdf_engine::core::reader::structure::ParentTreeEntry::Page(vec![Some(3)])
    );
    assert_eq!(tree.mcid_parent(0, 0), Some(2));
    assert_eq!(tree.mcid_parent(1, 0), Some(3));
    assert_eq!(tree.mcid_parent(0, 1), None);

    let first_page = doc
        .page_struct_parents(0)
        .expect("page 0 parents")
        .expect("/StructParents");
    assert_eq!(first_page.key, 0);
    assert_eq!(first_page.parents, vec![Some(2)]);
    let second_page = doc
        .page_struct_parents(1)
        .expect("page 1 parents")
        .expect("/StructParents");
    assert_eq!(second_page.key, 1);
    assert_eq!(second_page.parents, vec![Some(3)]);

    let mark_info = doc.mark_info().expect("mark info").expect("present");
    assert!(mark_info.marked);
    assert!(!mark_info.user_properties);
    assert!(!mark_info.suspects);

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    for feature in [
        PdfFeature::LogicalStructure,
        PdfFeature::StructureLanguageText,
        PdfFeature::StructureExpandedText,
        PdfFeature::MarkInfo,
    ] {
        assert!(features.contains(&feature), "缺少特性 {feature}");
    }
    assert!(!features.contains(&PdfFeature::MarkInfoUserProperties));

    let catalog = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .and_then(|(num, _)| doc.get_object(num).ok().flatten())
        .and_then(|object| object.value.as_dict().cloned())
        .expect("catalog");
    assert!(
        catalog
            .get("StructTreeRoot")
            .and_then(Value::as_ref)
            .is_some()
    );
    assert!(catalog.get("MarkInfo").and_then(Value::as_dict).is_some());
}
