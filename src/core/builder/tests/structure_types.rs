//! PDF 1.5 扩展标准结构类型的写端测试（ISO 32000-1 14.8.4）。
//!
//! 覆盖 THead/TBody/TFoot/Annot/Ruby/RB/RT/RP/Warichu/WT/WP 的直接
//! `/S` 与经 `/RoleMap` 映射两种到达方式、PDF 1.4 的零字节拒绝与 PDF 1.5
//! 的写出 → 解析回读，以及各 14.8.4 分组在往返后的 `standard_type`。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::reader::structure::{
    StandardStructureType as ReaderStandardType, StructureTypeCategory as ReaderCategory,
};
use crate::core::types::{Document, PdfFeature, PdfVersion};

/// PDF 1.5 引入的 11 个标准结构属性名（14.8.4 Table 337–339）。
const EXTENDED_NAMES: &[&str] = &[
    "THead", "TBody", "TFoot", "Annot", "Ruby", "RB", "RT", "RP", "Warichu", "WT", "WP",
];

/// 写出后重新解析：结构树只在 `build` 的导航后处理中物化。
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
        "pdf-engine-structure-types-{}-{}.pdf",
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

fn reader_features(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}

/// `Document` 下挂单个给定结构类型的元素（元素带 MCR）。
fn document_with_type(structure_type: &str) -> StructureTree {
    StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(StructureElement::new(structure_type).with_marked_content(0, 0)),
    )
}

#[test]
fn extended_structure_types_require_pdf_1_5() {
    for name in EXTENDED_NAMES {
        // PDF 1.4：门禁在任何对象号分配/字节写出之前拒绝。
        let mut rejected = one_page(PdfVersion::V1_4);
        rejected.set_structure_tree(document_with_type(name));
        assert_version_error(&mut rejected, "require PDF 1.5");

        // PDF 1.5：写出成功，解析后 standard_type 回到同一标准名。
        let mut accepted = one_page(PdfVersion::V1_5);
        accepted.set_structure_tree(document_with_type(name));
        let doc = build_and_parse(&mut accepted);
        let tree = doc
            .structure_tree()
            .expect("structure tree")
            .expect("tree present");
        assert_eq!(
            tree.standard_type(1),
            Some(ReaderStandardType::from_name(name)),
            "{name} 写端往返"
        );
        let features = reader_features(&doc);
        assert!(
            features.contains(&PdfFeature::StructureTypeExtensions),
            "{name}: {features:?}"
        );
    }
}

#[test]
fn role_mapped_extended_structure_types_require_pdf_1_5() {
    let tree = StructureTree::new()
        .with_role("CustomMark", "Annot")
        .with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("CustomMark").with_marked_content(0, 0)),
        );

    let mut rejected = one_page(PdfVersion::V1_4);
    rejected.set_structure_tree(tree.clone());
    assert_version_error(&mut rejected, "require PDF 1.5");

    let mut accepted = one_page(PdfVersion::V1_5);
    accepted.set_structure_tree(tree);
    let doc = build_and_parse(&mut accepted);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    assert_eq!(
        tree.resolved_role("CustomMark"),
        Some("Annot"),
        "/RoleMap 映射链终点"
    );
    assert_eq!(
        tree.standard_type(1),
        Some(ReaderStandardType::Annot),
        "自定义 /S 经 RoleMap 解析为 1.5 标准类型"
    );
}

#[test]
fn standard_structure_types_roundtrip_with_categories() {
    let tree = StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(
                StructureElement::new("P")
                    .with_element(StructureElement::new("Span").with_marked_content(0, 0))
                    .with_element(StructureElement::new("Figure").with_marked_content(0, 1)),
            )
            .with_element(
                StructureElement::new("L").with_element(
                    StructureElement::new("LI")
                        .with_element(StructureElement::new("Lbl").with_marked_content(0, 2))
                        .with_element(StructureElement::new("LBody").with_marked_content(0, 3)),
                ),
            )
            .with_element(
                StructureElement::new("Table").with_element(
                    StructureElement::new("TR")
                        .with_element(StructureElement::new("TH").with_marked_content(0, 4))
                        .with_element(StructureElement::new("TD").with_marked_content(0, 5)),
                ),
            )
            .with_element(
                StructureElement::new("Ruby")
                    .with_element(StructureElement::new("RB").with_marked_content(0, 6))
                    .with_element(StructureElement::new("RT").with_marked_content(0, 7))
                    .with_element(StructureElement::new("RP").with_marked_content(0, 8)),
            ),
    );
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let expected: &[(usize, ReaderStandardType, ReaderCategory)] = &[
        (0, ReaderStandardType::Document, ReaderCategory::Grouping),
        (1, ReaderStandardType::Paragraph, ReaderCategory::BlockLevel),
        (2, ReaderStandardType::Span, ReaderCategory::InlineLevel),
        (3, ReaderStandardType::Figure, ReaderCategory::Illustration),
        (4, ReaderStandardType::List, ReaderCategory::BlockLevel),
        (5, ReaderStandardType::ListItem, ReaderCategory::BlockLevel),
        (6, ReaderStandardType::Label, ReaderCategory::BlockLevel),
        (7, ReaderStandardType::ListBody, ReaderCategory::BlockLevel),
        (8, ReaderStandardType::Table, ReaderCategory::BlockLevel),
        (
            9,
            ReaderStandardType::TableRow,
            ReaderCategory::TableElement,
        ),
        (
            10,
            ReaderStandardType::TableHeader,
            ReaderCategory::TableElement,
        ),
        (
            11,
            ReaderStandardType::TableData,
            ReaderCategory::TableElement,
        ),
        (12, ReaderStandardType::Ruby, ReaderCategory::InlineLevel),
        (
            13,
            ReaderStandardType::RubyBase,
            ReaderCategory::InlineLevel,
        ),
        (
            14,
            ReaderStandardType::RubyText,
            ReaderCategory::InlineLevel,
        ),
        (
            15,
            ReaderStandardType::RubyPunctuation,
            ReaderCategory::InlineLevel,
        ),
    ];
    assert_eq!(tree.elements().len(), expected.len());
    for (index, standard, category) in expected {
        let parsed = tree.standard_type(*index).expect("standard type");
        assert_eq!(&parsed, standard, "元素 {index} 的标准类型");
        assert_eq!(parsed.category(), Some(*category), "元素 {index} 的分组");
        assert!(parsed.is_standard(), "元素 {index} 应为标准名");
    }
    let features = reader_features(&doc);
    assert!(
        features.contains(&PdfFeature::StructureTypeExtensions),
        "{features:?}"
    );
}
