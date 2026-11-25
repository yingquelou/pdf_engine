//! 结构树与元素的写出：解析器往返（角色映射/类映射/父树/`/IDTree`/特性登记）、
//! 原始字节与 Table 322–324 条目映射、空树合法形态。

use super::super::*;
use super::support::{build_and_parse, one_page};
use crate::core::reader::structure::StructureKid as ReaderKid;
use crate::core::types::{PdfFeature, PdfVersion};

fn object_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("object lookup")
        .expect("object exists")
        .value
        .as_dict()
        .expect("object dictionary")
        .clone()
}

#[test]
fn structure_tree_roundtrips_through_parser() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.page(PageSize::A4);

    let attributes = Dict::new()
        .insert("O", Value::Name("Layout".into()))
        .insert("SpaceBefore", Value::Integer(12));
    let tree = StructureTree::new()
        .with_role("MyPara", "P")
        .with_class("Body", Value::Dict(attributes))
        .with_element(
            StructureElement::new("Document")
                .with_id("doc")
                .with_title("Doc")
                .with_lang("en-US")
                .with_element(
                    StructureElement::new("MyPara")
                        .with_id("para")
                        .with_title("Intro")
                        .with_alt("Introduction")
                        .with_expanded("Introduction")
                        .with_actual_text("Intro text")
                        .with_class_revision("Body", 1)
                        .with_revision(3)
                        .with_element(
                            StructureElement::new("Span")
                                .with_id("span")
                                .with_marked_content(0, 0),
                        ),
                )
                .with_element(
                    StructureElement::new("P")
                        .with_id("second")
                        .with_marked_content(1, 0),
                ),
        );
    b.set_structure_tree(tree);
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_user_properties(true),
    );

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let root = tree.root();
    assert_eq!(root.role_map.get("MyPara").map(String::as_str), Some("P"));
    let class = root
        .class_map
        .get("Body")
        .and_then(|attributes| attributes.first())
        .and_then(|attribute| attribute.value.as_dict())
        .expect("class map /Body");
    assert_eq!(class.get("O").and_then(Value::as_name), Some("Layout"));
    assert_eq!(
        class.get("SpaceBefore").and_then(Value::as_integer),
        Some(12)
    );
    assert_eq!(root.parent_tree_next_key, Some(2));

    let types: Vec<&str> = tree
        .elements()
        .iter()
        .map(|element| element.structure_type.as_str())
        .collect();
    assert_eq!(types, ["Document", "MyPara", "Span", "P"], "前序展开");

    let document = &tree.elements()[0];
    assert_eq!(document.parent, None);
    assert_eq!(document.id.as_deref(), Some(b"doc".as_slice()));
    assert_eq!(document.title.as_deref(), Some("Doc"));
    assert_eq!(document.lang.as_deref(), Some("en-US"));
    assert!(matches!(
        document.kids[..],
        [ReaderKid::Element(1), ReaderKid::Element(3)]
    ));

    let paragraph = &tree.elements()[1];
    assert_eq!(paragraph.parent, Some(0));
    assert_eq!(paragraph.id.as_deref(), Some(b"para".as_slice()));
    assert_eq!(paragraph.title.as_deref(), Some("Intro"));
    assert_eq!(paragraph.alt.as_deref(), Some("Introduction"));
    assert_eq!(paragraph.expanded.as_deref(), Some("Introduction"));
    assert_eq!(paragraph.actual_text.as_deref(), Some("Intro text"));
    assert_eq!(paragraph.revision, 3);
    assert_eq!(paragraph.classes.len(), 1);
    assert_eq!(paragraph.classes[0].name, "Body");
    assert_eq!(paragraph.classes[0].revision, Some(1));
    assert!(matches!(paragraph.kids[..], [ReaderKid::Element(2)]));

    let span = &tree.elements()[2];
    assert_eq!(span.parent, Some(1));
    match &span.kids[..] {
        [ReaderKid::MarkedContent(mcr)] => {
            assert_eq!(mcr.mcid, 0);
            let (page_num, _) = mcr.page.expect("MCR /Pg");
            let page = object_dict(&doc, page_num);
            assert_eq!(page.get("Type").and_then(Value::as_name), Some("Page"));
        }
        other => panic!("expected one MCR, got {other:?}"),
    }

    let second = &tree.elements()[3];
    assert_eq!(second.parent, Some(0));
    assert_eq!(second.id.as_deref(), Some(b"second".as_slice()));

    assert!(tree.element_by_id(b"doc").is_some());
    assert!(tree.element_by_id(b"para").is_some());
    assert!(tree.element_by_id(b"span").is_some());
    assert!(tree.element_by_id(b"second").is_some());
    assert!(tree.element_by_id(b"missing").is_none());

    assert_eq!(tree.mcid_parent(0, 0), Some(2));
    assert_eq!(tree.mcid_parent(0, 1), None);
    assert_eq!(tree.mcid_parent(1, 0), Some(3));
    assert_eq!(tree.resolved_role("MyPara"), Some("P"));

    let first_page = doc
        .page_struct_parents(0)
        .expect("page 0")
        .expect("/StructParents");
    assert_eq!(first_page.key, 0);
    assert_eq!(first_page.parents, vec![Some(2)]);
    let second_page = doc
        .page_struct_parents(1)
        .expect("page 1")
        .expect("/StructParents");
    assert_eq!(second_page.key, 1);
    assert_eq!(second_page.parents, vec![Some(3)]);

    let mark_info = doc.mark_info().expect("mark info").expect("present");
    assert!(mark_info.marked);
    assert!(mark_info.user_properties);
    assert!(!mark_info.suspects);

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    for feature in [
        PdfFeature::LogicalStructure,
        PdfFeature::StructureLanguageText,
        PdfFeature::StructureExpandedText,
        PdfFeature::MarkInfo,
        PdfFeature::MarkInfoUserProperties,
    ] {
        assert!(features.contains(&feature), "缺少特性 {feature}");
    }
    assert!(!features.contains(&PdfFeature::MarkInfoSuspects));
}

#[test]
fn structure_raw_bytes_match_table_entries() {
    let attributes = Dict::new().insert("O", Value::Name("Layout".into()));
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_structure_tree(
        StructureTree::new()
            .with_role("MyPara", "P")
            .with_class("Body", Value::Dict(attributes))
            .with_element(
                StructureElement::new("Document")
                    .with_id("doc")
                    .with_class_revision("Body", 2)
                    .with_revision(3)
                    .with_element(StructureElement::new("P").with_title("Intro"))
                    .with_marked_content(0, 0),
            ),
    );
    b.set_mark_info(StructureMarkInfo::new().with_marked(true));
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let text = String::from_utf8_lossy(&out);

    for entry in [
        "/Type /StructTreeRoot",
        "/Type /StructElem",
        "/Type /MCR",
        "/RoleMap << /MyPara /P >>",
        "/ClassMap <<",
        "/IDTree <<",
        "/ParentTree <<",
        "/ParentTreeNextKey 1",
        "/StructParents 0",
        "/MarkInfo <<",
        "/Marked true",
        "/S /Document",
        "/S /P",
        "/MCID 0",
        "/ID <646F63>",
        "/C [/Body 2]",
        "/R 3",
    ] {
        assert!(text.contains(entry), "缺少 {entry}: {text}");
    }
    // `/Pg` 显式写在 MCR 字典中（不依赖元素 /Pg 继承）。
    assert!(text.contains("/Pg 3 0 R"), "{text}");
    assert!(!text.contains("/Type /OBJR"), "本切片不写 OBJR: {text}");
}

#[test]
fn empty_structure_tree_is_allowed() {
    let mut b = one_page(PdfVersion::V1_3);
    b.set_structure_tree(StructureTree::new());
    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    assert!(tree.elements().is_empty());
    assert!(tree.root().kids.is_empty());
    assert!(tree.root().parent_tree.is_empty());
    assert_eq!(tree.root().parent_tree_next_key, Some(0));
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::LogicalStructure));
    assert!(!features.contains(&PdfFeature::StructureLanguageText));
}

/// 取结构树根元素（目录 `/StructTreeRoot /K` 指向的 `StructElem`）的 `/K` 原值。
fn root_struct_element_k(doc: &Document) -> Value {
    let catalog_num = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .expect("catalog ref")
        .0;
    let catalog = object_dict(doc, catalog_num);
    let root_num = catalog
        .get("StructTreeRoot")
        .and_then(Value::as_ref)
        .expect("struct tree root ref")
        .0;
    let root = object_dict(doc, root_num);
    let element_num = match root.get("K").expect("struct root /K") {
        Value::Array(array) => array
            .0
            .first()
            .and_then(Value::as_ref)
            .expect("root element ref")
            .0,
        other => panic!("unexpected struct root /K: {other:?}"),
    };
    object_dict(doc, element_num)
        .get("K")
        .expect("element /K")
        .clone()
}

#[test]
fn single_kid_is_written_as_scalar_not_wrapped_array() {
    let mut b = one_page(PdfVersion::V1_7);
    // 根元素恰有一个子元素：其 `/K` 应是标量间接引用，而非单元素数组。
    b.set_structure_tree(StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(StructureElement::new("P").with_page(0).with_inline_marked_content(0, 0)),
    ));
    let doc = build_and_parse(&mut b);
    let k = root_struct_element_k(&doc);
    assert!(matches!(k, Value::Ref(_, _)), "单一子项应写标量 /K: {k:?}");
}

#[test]
fn multiple_kids_are_still_written_as_array() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(StructureElement::new("P").with_page(0).with_inline_marked_content(0, 0))
            .with_element(StructureElement::new("P").with_page(0).with_inline_marked_content(0, 1)),
    ));
    let doc = build_and_parse(&mut b);
    let k = root_struct_element_k(&doc);
    match k {
        Value::Array(array) => assert_eq!(array.len(), 2),
        other => panic!("多于一项应写 /K 数组: {other:?}"),
    }
}

#[test]
fn single_kid_scalar_roundtrips_through_parser() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(StructureElement::new("P").with_page(0).with_inline_marked_content(0, 0)),
    ));
    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    assert!(matches!(
        tree.root().kids[..],
        [ReaderKid::Element(0)]
    ), "{:?}", tree.root().kids);
    let root_element = &tree.elements()[0];
    assert!(matches!(
        root_element.kids[..],
        [ReaderKid::Element(1)]
    ), "{:?}", root_element.kids);
    let child = &tree.elements()[1];
    assert!(matches!(child.kids[..], [ReaderKid::Mcid(0)]), "{:?}", child.kids);
}
