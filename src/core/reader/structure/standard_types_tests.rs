//! 标准结构类型目录读端测试（ISO 32000-1 14.8.4 Table 333–340）。

use super::standard_types::{StandardStructureType, StructureTypeCategory};
use super::tests::{arr, document_with_root, element, insert, name, struct_root};
use crate::core::types::{Dict, Value};

/// 结构树根对象号（与 `super::tests::document_with_root` 一致）。
const ROOT: i64 = 2;

/// 14.8.4 的 49 个标准名与分组（Table 333–340）。
const STANDARD_TYPES: &[(&str, StandardStructureType, StructureTypeCategory)] = &[
    // Table 333 Grouping：12 个。
    (
        "Document",
        StandardStructureType::Document,
        StructureTypeCategory::Grouping,
    ),
    (
        "Part",
        StandardStructureType::Part,
        StructureTypeCategory::Grouping,
    ),
    (
        "Art",
        StandardStructureType::Article,
        StructureTypeCategory::Grouping,
    ),
    (
        "Sect",
        StandardStructureType::Section,
        StructureTypeCategory::Grouping,
    ),
    (
        "Div",
        StandardStructureType::Division,
        StructureTypeCategory::Grouping,
    ),
    (
        "BlockQuote",
        StandardStructureType::BlockQuote,
        StructureTypeCategory::Grouping,
    ),
    (
        "Caption",
        StandardStructureType::Caption,
        StructureTypeCategory::Grouping,
    ),
    (
        "TOC",
        StandardStructureType::TableOfContents,
        StructureTypeCategory::Grouping,
    ),
    (
        "TOCI",
        StandardStructureType::TableOfContentsItem,
        StructureTypeCategory::Grouping,
    ),
    (
        "Index",
        StandardStructureType::Index,
        StructureTypeCategory::Grouping,
    ),
    (
        "NonStruct",
        StandardStructureType::NonStruct,
        StructureTypeCategory::Grouping,
    ),
    (
        "Private",
        StandardStructureType::Private,
        StructureTypeCategory::Grouping,
    ),
    // Table 334 BLSE：13 个（Table 是 BLSE）。
    (
        "P",
        StandardStructureType::Paragraph,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "H",
        StandardStructureType::Heading,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "H1",
        StandardStructureType::Heading1,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "H2",
        StandardStructureType::Heading2,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "H3",
        StandardStructureType::Heading3,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "H4",
        StandardStructureType::Heading4,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "H5",
        StandardStructureType::Heading5,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "H6",
        StandardStructureType::Heading6,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "L",
        StandardStructureType::List,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "LI",
        StandardStructureType::ListItem,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "Lbl",
        StandardStructureType::Label,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "LBody",
        StandardStructureType::ListBody,
        StructureTypeCategory::BlockLevel,
    ),
    (
        "Table",
        StandardStructureType::Table,
        StructureTypeCategory::BlockLevel,
    ),
    // Table 337 表格元素：6 个。
    (
        "TR",
        StandardStructureType::TableRow,
        StructureTypeCategory::TableElement,
    ),
    (
        "TH",
        StandardStructureType::TableHeader,
        StructureTypeCategory::TableElement,
    ),
    (
        "TD",
        StandardStructureType::TableData,
        StructureTypeCategory::TableElement,
    ),
    (
        "THead",
        StandardStructureType::TableHead,
        StructureTypeCategory::TableElement,
    ),
    (
        "TBody",
        StandardStructureType::TableBody,
        StructureTypeCategory::TableElement,
    ),
    (
        "TFoot",
        StandardStructureType::TableFooter,
        StructureTypeCategory::TableElement,
    ),
    // Table 338 行内元素：8 个。
    (
        "Span",
        StandardStructureType::Span,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "Quote",
        StandardStructureType::Quote,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "Note",
        StandardStructureType::Note,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "Reference",
        StandardStructureType::Reference,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "BibEntry",
        StandardStructureType::BibEntry,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "Code",
        StandardStructureType::Code,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "Link",
        StandardStructureType::Link,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "Annot",
        StandardStructureType::Annot,
        StructureTypeCategory::InlineLevel,
    ),
    // Table 339 Ruby/Warichu：7 个。
    (
        "Ruby",
        StandardStructureType::Ruby,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "RB",
        StandardStructureType::RubyBase,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "RT",
        StandardStructureType::RubyText,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "RP",
        StandardStructureType::RubyPunctuation,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "Warichu",
        StandardStructureType::Warichu,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "WT",
        StandardStructureType::WarichuText,
        StructureTypeCategory::InlineLevel,
    ),
    (
        "WP",
        StandardStructureType::WarichuPunctuation,
        StructureTypeCategory::InlineLevel,
    ),
    // Table 340 Illustration：3 个。
    (
        "Figure",
        StandardStructureType::Figure,
        StructureTypeCategory::Illustration,
    ),
    (
        "Formula",
        StandardStructureType::Formula,
        StructureTypeCategory::Illustration,
    ),
    (
        "Form",
        StandardStructureType::Form,
        StructureTypeCategory::Illustration,
    ),
];

#[test]
fn standard_structure_types_cover_tables_333_to_340() {
    assert_eq!(STANDARD_TYPES.len(), 49, "14.8.4 共 49 个标准名");
    for (name, variant, category) in STANDARD_TYPES {
        let parsed = StandardStructureType::from_name(name);
        assert_eq!(&parsed, variant, "{name} 的语义化变体");
        assert_eq!(parsed.as_str(), *name, "{name} 的规范名往返");
        assert!(parsed.is_standard(), "{name} 应属标准目录");
        assert_eq!(parsed.category(), Some(*category), "{name} 的分组");
    }

    // 分组计数与各表条目数一致：333=12、334=13、337=6、338=8、339=7、340=3。
    let count = |category: StructureTypeCategory| {
        STANDARD_TYPES
            .iter()
            .filter(|(_, _, entry)| *entry == category)
            .count()
    };
    assert_eq!(count(StructureTypeCategory::Grouping), 12);
    assert_eq!(count(StructureTypeCategory::BlockLevel), 13);
    assert_eq!(count(StructureTypeCategory::TableElement), 6);
    assert_eq!(count(StructureTypeCategory::InlineLevel), 15);
    assert_eq!(count(StructureTypeCategory::Illustration), 3);

    // 未知名保留原值，不属标准目录、无分组。
    let custom = StandardStructureType::from_name("Custom");
    assert_eq!(custom, StandardStructureType::Other("Custom".into()));
    assert!(!custom.is_standard());
    assert_eq!(custom.category(), None);
    assert_eq!(custom.as_str(), "Custom");
}

#[test]
fn standard_structure_type_categories_follow_tables() {
    assert_eq!(
        StandardStructureType::from_name("Table").category(),
        Some(StructureTypeCategory::BlockLevel),
        "Table 是 BLSE（Table 334）"
    );
    for name in ["TR", "TH", "TD", "THead", "TBody", "TFoot"] {
        assert_eq!(
            StandardStructureType::from_name(name).category(),
            Some(StructureTypeCategory::TableElement),
            "{name} 是表格元素（Table 337）"
        );
    }
    for name in ["Figure", "Formula", "Form"] {
        assert_eq!(
            StandardStructureType::from_name(name).category(),
            Some(StructureTypeCategory::Illustration),
            "{name} 是插图元素（Table 340）"
        );
    }
    for name in ["Ruby", "RB", "RT", "RP", "Warichu", "WT", "WP"] {
        assert_eq!(
            StandardStructureType::from_name(name).category(),
            Some(StructureTypeCategory::InlineLevel),
            "{name} 是行内元素（Table 339）"
        );
    }
    for name in ["Document", "Caption"] {
        assert_eq!(
            StandardStructureType::from_name(name).category(),
            Some(StructureTypeCategory::Grouping),
            "{name} 是分组元素（Table 333）"
        );
    }
    assert_eq!(
        StandardStructureType::from_name("P").category(),
        Some(StructureTypeCategory::BlockLevel)
    );
}

#[test]
fn standard_structure_type_resolves_direct_mapped_and_custom_names() {
    let role_map = Dict::new()
        .insert("MyPara", name("Body"))
        .insert("Body", name("P"));
    let doc = document_with_root(
        struct_root(Dict::new())
            .insert("RoleMap", Value::Dict(role_map))
            .insert(
                "K",
                arr(vec![Value::Ref(5, 0), Value::Ref(6, 0), Value::Ref(7, 0)]),
            ),
    );
    insert(&doc, 5, Value::Dict(element("Figure", ROOT)));
    insert(&doc, 6, Value::Dict(element("MyPara", ROOT)));
    insert(&doc, 7, Value::Dict(element("Custom", ROOT)));

    let tree = doc.structure_tree().expect("structure tree").expect("root");
    assert_eq!(
        tree.standard_type(0),
        Some(StandardStructureType::Figure),
        "/S 直接是标准名"
    );
    assert_eq!(tree.resolved_role("MyPara"), Some("P"), "多级映射链的终点");
    assert_eq!(
        tree.standard_type(1),
        Some(StandardStructureType::Paragraph),
        "MyPara → Body → P 映射后取标准类型"
    );
    assert_eq!(
        tree.standard_type(2),
        Some(StandardStructureType::Other("Custom".into())),
        "未映射的自定义名字保留原值"
    );
    assert_eq!(
        tree.standard_type(2).and_then(|parsed| parsed.category()),
        None,
        "自定义名字没有 14.8.4 分组"
    );
    assert_eq!(tree.standard_type(3), None, "越界下标");
}

#[test]
fn standard_structure_type_marks_pdf_1_5_extensions() {
    for name in [
        "THead", "TBody", "TFoot", "Annot", "Ruby", "RB", "RT", "RP", "Warichu", "WT", "WP",
    ] {
        assert!(
            StandardStructureType::from_name(name).requires_pdf_1_5(),
            "{name} 自 PDF 1.5 引入"
        );
    }
    for name in ["P", "Document", "Figure", "Link", "TH"] {
        assert!(
            !StandardStructureType::from_name(name).requires_pdf_1_5(),
            "{name} 属 Tagged PDF 基线（PDF 1.3）"
        );
    }
    assert_eq!(
        STANDARD_TYPES
            .iter()
            .filter(|(_, variant, _)| variant.requires_pdf_1_5())
            .count(),
        11,
        "扩展标准名恰为 11 个"
    );
}
