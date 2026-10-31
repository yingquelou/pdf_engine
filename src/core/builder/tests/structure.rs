//! 逻辑结构写端测试（ISO 32000-1 14.7）：目录 `/MarkInfo`（Table 321）、
//! 结构树根（Table 322）、结构元素（Table 323）、MCR（Table 324）、父树
//! 与 `/IDTree`（14.7.4.4）的写出与读端往返，以及版本门禁与输入校验的
//! 零字节输出契约。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
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

/// 写出后重新解析：结构树与父树只在 `build` 的导航后处理中物化。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-structure-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn assert_invalid(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

fn assert_version_error(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

fn one_page(version: PdfVersion) -> PdfBuilder {
    let mut b = PdfBuilder::with_version(version);
    b.page(PageSize::A4);
    b
}

fn simple_tree() -> StructureTree {
    StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(StructureElement::new("P").with_marked_content(0, 0)),
    )
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
        .and_then(Value::as_dict)
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
fn structure_tree_requires_pdf_1_3() {
    let mut b = one_page(PdfVersion::V1_2);
    b.set_structure_tree(simple_tree());
    assert_version_error(&mut b, "structure trees require PDF 1.3");

    let mut b = one_page(PdfVersion::V1_3);
    b.set_structure_tree(simple_tree());
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.3 supports structure trees");
    assert!(String::from_utf8_lossy(&out).contains("/Type /StructTreeRoot"));
}

#[test]
fn structure_language_and_actual_text_require_pdf_1_4() {
    let lang =
        || StructureTree::new().with_element(StructureElement::new("Document").with_lang("en-US"));
    let actual = || {
        StructureTree::new()
            .with_element(StructureElement::new("Document").with_actual_text("Hello"))
    };

    let mut b = one_page(PdfVersion::V1_3);
    b.set_structure_tree(lang());
    assert_version_error(&mut b, "/Lang and /ActualText require PDF 1.4");

    let mut b = one_page(PdfVersion::V1_3);
    b.set_structure_tree(actual());
    assert_version_error(&mut b, "/Lang and /ActualText require PDF 1.4");

    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(lang());
    b.build(&mut Vec::new()).expect("PDF 1.4 supports /Lang");
    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(actual());
    b.build(&mut Vec::new())
        .expect("PDF 1.4 supports /ActualText");
}

#[test]
fn structure_expanded_text_requires_pdf_1_5() {
    let tree = || {
        StructureTree::new()
            .with_element(StructureElement::new("Document").with_expanded("Internationalization"))
    };
    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(tree());
    assert_version_error(&mut b, "/E requires PDF 1.5");

    let mut b = one_page(PdfVersion::V1_5);
    b.set_structure_tree(tree());
    b.build(&mut Vec::new()).expect("PDF 1.5 supports /E");
}

#[test]
fn mark_info_requires_pdf_1_4() {
    let mut b = one_page(PdfVersion::V1_3);
    b.set_mark_info(StructureMarkInfo::new().with_marked(true));
    assert_version_error(&mut b, "mark info require PDF 1.4");

    let mut b = one_page(PdfVersion::V1_4);
    b.set_mark_info(StructureMarkInfo::new().with_marked(true));
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports /MarkInfo");
    assert!(String::from_utf8_lossy(&out).contains("/MarkInfo <<"));
}

#[test]
fn mark_info_user_properties_and_suspects_require_pdf_1_6() {
    let mut b = one_page(PdfVersion::V1_5);
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_user_properties(true),
    );
    assert_version_error(
        &mut b,
        "/MarkInfo /UserProperties and /Suspects require PDF 1.6",
    );

    let mut b = one_page(PdfVersion::V1_5);
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_suspects(true),
    );
    assert_version_error(
        &mut b,
        "/MarkInfo /UserProperties and /Suspects require PDF 1.6",
    );

    let mut b = one_page(PdfVersion::V1_6);
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_user_properties(true)
            .with_suspects(true),
    );
    let doc = build_and_parse(&mut b);
    let info = doc.mark_info().expect("mark info").expect("present");
    assert!(info.marked);
    assert!(info.user_properties);
    assert!(info.suspects);
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::MarkInfo));
    assert!(features.contains(&PdfFeature::MarkInfoUserProperties));
    assert!(features.contains(&PdfFeature::MarkInfoSuspects));
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

#[test]
fn structure_element_type_must_be_a_name() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(StructureElement::new("")));
    assert_invalid(&mut b, "structure element /S must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(StructureElement::new("My Para")));
    assert_invalid(&mut b, "is not a valid PDF name");
}

#[test]
fn structure_ids_are_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_id(Vec::new())),
    );
    assert_invalid(&mut b, "structure element /ID must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_id(vec![0x80u8])),
    );
    assert_invalid(&mut b, "must contain ASCII bytes only");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new()
            .with_element(StructureElement::new("P").with_id("dup"))
            .with_element(StructureElement::new("Span").with_id("dup")),
    );
    assert_invalid(&mut b, "is declared more than once");
}

#[test]
fn structure_class_names_are_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_class("")),
    );
    assert_invalid(&mut b, "structure class name must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_class("Bad Name")),
    );
    assert_invalid(&mut b, "is not a valid PDF name");
}

#[test]
fn structure_role_map_is_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_role("", "P"));
    assert_invalid(&mut b, "structure role map key must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_role("MyPara", ""));
    assert_invalid(&mut b, "structure role map target for \"MyPara\"");

    let mut b = one_page(PdfVersion::V1_7);
    let mut tree = StructureTree::new();
    for index in 0..4097 {
        tree.role_map.insert(format!("R{index}"), "P".into());
    }
    b.set_structure_tree(tree);
    assert_invalid(&mut b, "structure /RoleMap has 4097 entries (limit 4096)");
}

#[test]
fn structure_class_map_is_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class("", Value::Dict(Dict::new())));
    assert_invalid(&mut b, "structure class map key must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class("Body", Value::Integer(1)));
    assert_invalid(
        &mut b,
        "structure class map /Body must be an attribute dictionary or stream",
    );

    let mut b = one_page(PdfVersion::V1_7);
    let mut tree = StructureTree::new();
    for index in 0..4097 {
        tree.class_map
            .insert(format!("C{index}"), Value::Dict(Dict::new()));
    }
    b.set_structure_tree(tree);
    assert_invalid(&mut b, "structure /ClassMap has 4097 entries (limit 4096)");
}

#[test]
fn structure_mcr_targets_are_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_marked_content(1, 0)),
    );
    assert_invalid(
        &mut b,
        "structure MCR page 1 is out of range (page count 1)",
    );

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_marked_content(0, -1)),
    );
    assert_invalid(&mut b, "structure MCR MCID must be non-negative, got -1");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_marked_content(0, 65536)),
    );
    assert_invalid(&mut b, "structure MCR MCID 65536 exceeds the limit 65535");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new()
            .with_element(StructureElement::new("P").with_marked_content(0, 0))
            .with_element(StructureElement::new("Span").with_marked_content(0, 0)),
    );
    assert_invalid(
        &mut b,
        "page 0 MCID 0 is claimed by more than one structure element",
    );
}

#[test]
fn structure_limits_mirror_reader() {
    // 深度：顶层 + 64 层子元素，最深一层 depth = 64。
    let mut nested = StructureElement::new("P");
    for _ in 0..64 {
        nested = StructureElement::new("P").with_element(nested);
    }
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(nested));
    assert_invalid(&mut b, "structure tree depth exceeds 64");

    // 元素数：4097 个顶层元素。
    let mut b = one_page(PdfVersion::V1_7);
    let mut tree = StructureTree::new();
    for _ in 0..4097 {
        tree = tree.with_element(StructureElement::new("P"));
    }
    b.set_structure_tree(tree);
    assert_invalid(&mut b, "structure tree has more than 4096 elements");

    // 单元素子项数：65537 个 MCR（在页范围校验之前先触发上限）。
    let mut element = StructureElement::new("P");
    element.kids = (0..65537)
        .map(|index| StructureKid::marked_content(0, index))
        .collect();
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(element));
    assert_invalid(&mut b, "structure element has 65537 children (limit 65536)");
}

#[test]
fn structure_revision_and_class_revisions_roundtrip() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_class_revision("Body", u32::MAX)
                .with_class("Inline")
                .with_revision(u32::MAX),
        ),
    );
    let doc = build_and_parse(&mut b);
    let tree = doc.structure_tree().expect("tree").expect("present");
    let element = &tree.elements()[0];
    assert_eq!(element.revision, u32::MAX);
    assert_eq!(element.classes.len(), 2);
    assert_eq!(element.classes[0].name, "Body");
    assert_eq!(element.classes[0].revision, Some(u32::MAX));
    assert_eq!(element.classes[1].name, "Inline");
    assert_eq!(element.classes[1].revision, None);

    // 修订号 0 为缺省，不写出 `/R`。
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(StructureElement::new("Document")));
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    assert!(!String::from_utf8_lossy(&out).contains("/R "));
}

#[test]
fn structure_class_map_stream_is_written_indirectly() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class(
        "Big",
        Value::Stream(
            Dict::new(),
            StreamData::Owned(b"layout attributes".to_vec()),
        ),
    ));
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("/ClassMap << /Big "), "{text}");
    assert!(
        text.contains("endstream"),
        "类映射流应作为间接对象写出: {text}"
    );

    let doc = build_and_parse(&mut b);
    let tree = doc.structure_tree().expect("tree").expect("present");
    let (stream_num, _) = tree
        .root()
        .class_map
        .get("Big")
        .and_then(Value::as_ref)
        .expect("类映射流应为间接引用");
    let stream = doc
        .get_object(stream_num)
        .expect("stream lookup")
        .expect("stream exists")
        .value;
    assert!(matches!(stream, Value::Stream(..)));
    let bytes = doc.stream_bytes(&stream).expect("stream bytes");
    assert_eq!(bytes, b"layout attributes");
}
