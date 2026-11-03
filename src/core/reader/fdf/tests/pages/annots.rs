//! `/Annots`（Table 243 raw L24885–L24889 + Table 251 raw L25129–L25132）
//! 的逐项类型化测试：12.7.7.3.4 的必需 `/Page` 整型序数（0 起，负数/非
//! 整型/缺失都只记诊断）、Table 243 明文排除的六种注解类型黑名单
//! （Link、Movie、Widget、PrinterMark、Screen、TrapNet）逐一核对，以及
//! 12.5.2 公共条目原值不丢弃。装配辅助复用父模块。

use super::*;
use crate::core::reader::fdf::PROHIBITED_FDF_ANNOTATION_TYPES;
#[test]
fn fdf_annots_checks_required_page_ordinal_and_subtype() {
    let view = annots_view_of(
        array(vec![
            annotation(&[("Page", integer(0)), ("Contents", text("hi"))]),
            annotation(&[]), // 缺 /Page（Required）
            dict_value(&[("Subtype", name_entry("Link")), ("Page", integer(2))]), // 类型被排除
            annotation(&[("Page", text("one"))]), // /Page 非整型
            dict_value(&[("Page", integer(-1))]), // 缺 /Subtype + 负序数
            integer(9),      // 非字典
        ]),
        Vec::new(),
    );
    assert_eq!(view.len(), 6);
    let first = &view.entries[0];
    assert_eq!(first.page, Some(0));
    assert!(first.page_ordinal_valid());
    assert!(first.subtype_permitted());
    assert_eq!(first.entry("Contents"), Some(&text("hi")), "原值不丢");
    let issues = annots_issues(&view);
    for expected in [
        FdfPagesIssue::AnnotationPageMissing {
            index: 1,
            subtype: Some("Text".to_string()),
        },
        FdfPagesIssue::AnnotationSubtypeProhibited {
            index: 2,
            subtype: "Link".to_string(),
        },
        FdfPagesIssue::AnnotationPageNotInteger {
            index: 3,
            found: "string",
        },
        FdfPagesIssue::AnnotationSubtypeMissing { index: 4 },
        FdfPagesIssue::AnnotationPageNegative { index: 4, page: -1 },
        FdfPagesIssue::AnnotationNotDictionary {
            index: 5,
            found: "integer",
        },
    ] {
        assert!(
            issues.contains(&expected),
            "缺少诊断 {expected:?}；实际：{issues:?}"
        );
    }
    assert!(!view.entries[2].subtype_permitted());
    assert!(!view.entries[4].page_ordinal_valid());
    // `/Page` 序数按声明序去重（0、2、-1）；畸形项不进倒排。
    assert_eq!(view.pages(), vec![0, 2, -1]);
    // 注解项的间接引用照常解引用，数组项原值保留。
    let referenced = annots_view_of(
        array(vec![Value::Ref(3, 0)]),
        vec![Object::new(3, 0, annotation(&[("Page", integer(1))]))],
    );
    assert_eq!(referenced.entries[0].page, Some(1));
    assert_eq!(referenced.entries[0].raw, Value::Ref(3, 0));
    assert_eq!(referenced.reference, None);
}

#[test]
fn fdf_annots_blacklist_covers_every_type_excluded_by_table_243() {
    // raw L24887–L24889 逐个核对：六类被排除，其余标准类型可用。
    assert_eq!(
        PROHIBITED_FDF_ANNOTATION_TYPES,
        [
            "Link",
            "Movie",
            "Widget",
            "PrinterMark",
            "Screen",
            "TrapNet"
        ]
    );
    for banned in PROHIBITED_FDF_ANNOTATION_TYPES {
        let view = annots_view_of(
            array(vec![dict_value(&[
                ("Subtype", name_entry(banned)),
                ("Page", integer(0)),
            ])]),
            Vec::new(),
        );
        assert!(!view.entries[0].subtype_permitted(), "/{banned} 应被排除");
        assert!(
            view.entries[0]
                .issues
                .contains(&FdfPagesIssue::AnnotationSubtypeProhibited {
                    index: 0,
                    subtype: banned.to_string(),
                }),
            "/{banned} 应有类型化诊断"
        );
    }
    for allowed in ["Text", "FreeText", "Square", "Highlight", "Stamp", "Popup"] {
        let view = annots_view_of(
            array(vec![dict_value(&[
                ("Subtype", name_entry(allowed)),
                ("Page", integer(0)),
            ])]),
            Vec::new(),
        );
        assert!(view.entries[0].subtype_permitted(), "/{allowed} 应可用");
        assert!(view.entries[0].issues.is_empty());
    }
}

#[test]
fn fdf_annotation_page_zero_is_the_first_page() {
    // Table 251（raw L25131–L25132）「where page 0 is the first page」：
    // 0 是合法序数，不是「缺省」也不是违例。
    let view = annots_view_of(array(vec![annotation(&[("Page", integer(0))])]), Vec::new());
    assert_eq!(view.entries[0].page, Some(0));
    assert!(view.entries[0].issues.is_empty());
    assert!(view.pages().contains(&0));
    // 浮点 /Page 不猜值（与 parse::parse_integer 同口径）。
    let float = annots_view_of(
        array(vec![annotation(&[("Page", Value::Float(1.5))])]),
        Vec::new(),
    );
    assert_eq!(float.entries[0].page, None);
    assert!(
        float.entries[0]
            .issues
            .contains(&FdfPagesIssue::AnnotationPageNotInteger {
                index: 0,
                found: "float",
            })
    );
}
