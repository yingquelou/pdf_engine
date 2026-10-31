//! 内容模型校验 → 写出 → 解析 → 诊断往返（ISO 32000-1 14.8.4）。
//!
//! 两次构建同一份两页文档：第一次给出符合 14.8.4 内容模型的结构树，解析后
//! `content_model_violations` 为空；第二次故意写出违规树（两个顶层元素、
//! 分组元素直接带内容项、`TOC` 下挂 `P`、`L` 下挂 `Lbl`、`Ruby` 只有 `RB`、
//! 无 `PrintField /Role` 的 `Form` 带标记内容），解析后逐条回读同一批发现，
//! 证明诊断读端与写端模型一致，且违规不阻断写出与解析。

use super::*;

#[test]
fn builder_content_model_violations_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        PageSize, PdfBuilder, PrintFieldAttributes, PrintFieldRole, StructureAttribute,
        StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::structure::{
        ChildModel, ContentModelRule, ContentModelViolation,
    };
    use pdf_engine::core::types::PdfVersion;

    // 符合内容模型的结构树：Document > (L > LI > (Lbl, LBody))、
    // Document > Table > TR > TH、Document > Form（带 PrintField /Role）。
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("L").with_element(
                        StructureElement::new("LI")
                            .with_element(StructureElement::new("Lbl").with_marked_content(0, 0))
                            .with_element(StructureElement::new("LBody").with_marked_content(0, 1)),
                    ),
                )
                .with_element(
                    StructureElement::new("Table").with_element(
                        StructureElement::new("TR")
                            .with_element(StructureElement::new("TH").with_marked_content(0, 2)),
                    ),
                )
                .with_element(
                    StructureElement::new("Form")
                        .with_attribute(StructureAttribute::print_field(PrintFieldAttributes::new(
                            PrintFieldRole::CheckBox,
                        )))
                        .with_marked_content(0, 3),
                ),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));
    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build compliant content model tree");
    let doc = parse_buf(&buf);
    let tree = doc
        .structure_tree()
        .expect("parse compliant tree")
        .expect("compliant tree present");
    assert!(tree.content_model_violations().is_empty());

    // 违规结构树：写端不校验 14.8.4，照常写出；读端逐条诊断。
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    let grouped = StructureElement::new("Document")
        .with_marked_content(0, 0)
        .with_element(StructureElement::new("TOC").with_element(StructureElement::new("P")))
        .with_element(
            StructureElement::new("L")
                .with_element(StructureElement::new("Lbl").with_marked_content(0, 1))
                .with_element(StructureElement::new("LI").with_marked_content(0, 2)),
        )
        .with_element(StructureElement::new("Ruby").with_element(StructureElement::new("RB")))
        .with_element(StructureElement::new("Form").with_marked_content(1, 0));
    builder.set_structure_tree(
        StructureTree::new()
            .with_element(grouped)
            .with_element(StructureElement::new("Sect")),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));
    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build violating content model tree");
    let doc = parse_buf(&buf);
    let tree = doc
        .structure_tree()
        .expect("parse violating tree")
        .expect("violating tree present");

    // arena 前序：0 Document、1 TOC、2 P、3 L、4 Lbl、5 LI、6 Ruby、7 RB、
    // 8 Form、9 Sect。
    let expected = vec![
        ContentModelViolation {
            element: None,
            kid: None,
            rule: ContentModelRule::TopLevelCount(2),
        },
        ContentModelViolation {
            element: Some(0),
            kid: Some(0),
            rule: ContentModelRule::ContentItemInGroupingElement,
        },
        ContentModelViolation {
            element: Some(1),
            kid: Some(0),
            rule: ContentModelRule::UnexpectedChild(ChildModel::TocEntry),
        },
        ContentModelViolation {
            element: Some(3),
            kid: Some(0),
            rule: ContentModelRule::UnexpectedChild(ChildModel::ListChild),
        },
        ContentModelViolation {
            element: Some(6),
            kid: None,
            rule: ContentModelRule::RubySequence,
        },
        ContentModelViolation {
            element: Some(8),
            kid: None,
            rule: ContentModelRule::FormObjectReference,
        },
    ];
    let violations = tree.content_model_violations();
    assert_eq!(violations, expected);
    let mandatory: Vec<ContentModelViolation> = violations
        .iter()
        .filter(|item| item.rule.is_mandatory())
        .cloned()
        .collect();
    assert_eq!(
        mandatory,
        vec![
            expected[0].clone(),
            expected[1].clone(),
            expected[4].clone(),
            expected[5].clone()
        ]
    );

    // 诊断不改变解析结果：树完整展开，标记信息照常回读。
    assert_eq!(tree.elements().len(), 10);
    assert!(
        doc.mark_info()
            .expect("parse mark info")
            .expect("mark info present")
            .marked
    );
}
