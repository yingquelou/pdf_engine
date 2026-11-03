//! CMap 流字典（9.7.5.2，Table 120）**类型化读端**的单元测试；一致性诊断的
//! 用例在同级 `consistency` 模块，夹具函数由那边共享。
//!
//! 这里固定的不变量只有两条（都是「只读、不改行为」）：
//!
//! - 缺省条目不物化：未写的键读成 [`CMapEntry::Missing`]，`/WMode` 的默认值 0
//!   留给调用方按 Table 120 处理，本模块不做猜测；
//! - 未知/非法形态降级：类型不符、引用悬空、子字典不可解析一律进
//!   [`CMapEntry::Malformed`] 并保留实际看到的形态，绝不把原本能解析的 CMap
//!   变成错误（`type0_cmap_dictionaries`／`cmap_dictionary` 永不返回 `Err`）。

use super::consistency::{
    cid_font, cmap_stream, collection, encoding_rules, full_dict, has_rule, type0_font,
};
use crate::core::reader::font::{
    CMapBaseReference, CMapConsistencyRule, CMapDictionary, CMapEntry, CMapWritingMode,
};
use crate::core::types::{Array, Dict, Document, Value};

#[test]
fn reads_all_table_120_entries_into_the_typed_model() {
    let document = Document::new();
    let mut dict = full_dict("Test-CMap", collection("Adobe", "GB1", 3), 1);
    dict = dict
        .insert("UseCMap", Value::Name("Identity-H".into()))
        .insert("CMapType", Value::Integer(2))
        .insert("UID", Value::Integer(42));
    let font = type0_font(
        cmap_stream(dict, "Test-CMap", 1, 2),
        None,
        cid_font("CIDFontType2", collection("Adobe", "GB1", 3), true),
    );
    let cmaps = document.type0_cmap_dictionaries(&font);
    let cmap = cmaps.encoding.expect("/Encoding 流应被读取");
    assert_eq!(cmap.object_type, CMapEntry::Present("CMap".into()));
    assert_eq!(cmap.cmap_name, CMapEntry::Present("Test-CMap".into()));
    let info = cmap.system_info.value().expect("CIDSystemInfo 可读");
    assert_eq!(
        (
            info.registry.as_str(),
            info.ordering.as_str(),
            info.supplement
        ),
        ("Adobe", "GB1", 3)
    );
    assert_eq!(
        cmap.writing_mode,
        CMapEntry::Present(CMapWritingMode::Mode(1))
    );
    assert_eq!(
        cmap.use_cmap,
        Some(CMapBaseReference::Name("Identity-H".into()))
    );
    assert_eq!(cmap.cmap_type, CMapEntry::Present(2));
    assert_eq!(cmap.uid, CMapEntry::Present(42));
    assert!(cmap.declarations.parsed);
    assert_eq!(cmap.declarations.cmap_name.as_deref(), Some("Test-CMap"));
    assert_eq!(cmap.declarations.writing_mode, Some(1));
    assert_eq!(cmap.declarations.cmap_type, Some(2));
    assert_eq!(cmap.declarations.nonzero_use_font_number, None);
    assert!(!cmap.is_identity_encoding());
    assert!(document.cmap_consistency_issues(&font).is_empty());
    assert!(cmaps.to_unicode.is_none());
}

#[test]
fn absent_entries_are_not_materialized() {
    let document = Document::new();
    let font = type0_font(
        cmap_stream(Dict::new(), "Bare-CMap", 0, 2),
        None,
        cid_font("CIDFontType2", collection("Adobe", "Identity", 0), true),
    );
    let cmap = document
        .type0_cmap_dictionaries(&font)
        .encoding
        .expect("流字典可读");
    assert!(cmap.object_type.is_missing(), "/Type 缺省不物化");
    assert!(cmap.cmap_name.is_missing());
    assert!(cmap.system_info.is_missing());
    assert!(cmap.writing_mode.is_missing());
    assert!(cmap.cmap_type.is_missing());
    assert!(cmap.uid.is_missing());
    assert_eq!(cmap.use_cmap, None);
    // 缺省条目只对 /Encoding 侧按 Table 120 的 Required 报缺失，不报错。
    let issues = document.cmap_consistency_issues(&font);
    assert_eq!(
        encoding_rules(&issues),
        vec![
            CMapConsistencyRule::RequiredEntryMissing { key: "Type".into() },
            CMapConsistencyRule::RequiredEntryMissing {
                key: "CMapName".into()
            },
            CMapConsistencyRule::RequiredEntryMissing {
                key: "CIDSystemInfo".into()
            },
        ]
    );
    assert!(issues.iter().all(|issue| issue.rule.is_mandatory()));
}

#[test]
fn malformed_entries_degrade_instead_of_failing() {
    let document = Document::new();
    let dict = Dict::new()
        .insert("Type", Value::Integer(1))
        .insert("CMapName", Value::Array(Array::new()))
        .insert("CIDSystemInfo", Value::Ref(99, 0))
        .insert("WMode", Value::Name("vertical".into()))
        .insert("CMapType", Value::Float(2.5));
    let font = type0_font(
        cmap_stream(dict, "Bad-CMap", 0, 2),
        None,
        cid_font("CIDFontType2", collection("Adobe", "Identity", 0), true),
    );
    let cmap = document
        .type0_cmap_dictionaries(&font)
        .encoding
        .expect("流字典可读");
    assert_eq!(
        cmap.object_type,
        CMapEntry::Malformed {
            found: "integer".into()
        }
    );
    assert_eq!(
        cmap.cmap_name,
        CMapEntry::Malformed {
            found: "array".into()
        }
    );
    assert_eq!(
        cmap.system_info,
        CMapEntry::Malformed {
            found: "unresolved reference".into()
        }
    );
    assert_eq!(
        cmap.writing_mode,
        CMapEntry::Malformed {
            found: "name".into()
        }
    );
    assert_eq!(
        cmap.cmap_type,
        CMapEntry::Malformed {
            found: "real number".into()
        }
    );
    let issues = document.cmap_consistency_issues(&font);
    for key in ["Type", "CMapName", "CIDSystemInfo", "WMode"] {
        assert!(
            has_rule(
                &issues,
                &CMapConsistencyRule::EntryMalformed {
                    key: key.into(),
                    found: String::new(),
                }
            ) || issues.iter().any(|issue| matches!(
                &issue.rule,
                CMapConsistencyRule::EntryMalformed { key: found, .. } if found == key
            )),
            "{key} 的非法形态应被报出：{issues:?}"
        );
    }
    // /CMapType 非整数只降级，不产任何发现（非 Table 120 条目）。
    assert!(!issues.iter().any(|issue| matches!(
        &issue.rule,
        CMapConsistencyRule::EntryMalformed { key, .. } if key == "CMapType"
    )));
}

#[test]
fn non_type0_and_unreadable_values_yield_no_issues() {
    let document = Document::new();
    assert!(
        document
            .cmap_consistency_issues(&cid_font(
                "CIDFontType2",
                collection("Adobe", "GB1", 3),
                true
            ))
            .is_empty()
    );
    assert!(
        document
            .cmap_consistency_issues(&Value::Integer(1))
            .is_empty()
    );
    assert!(
        document
            .cmap_consistency_issues(&Value::Name("Type0".into()))
            .is_empty()
    );
    let cmaps = document.type0_cmap_dictionaries(&Value::Integer(7));
    assert_eq!(cmaps, Default::default());
    // 不是流时 `cmap_dictionary` 返回 None（不报错）。
    assert!(document.cmap_dictionary(&Value::Integer(7)).is_none());
    let dictionary: CMapDictionary = document
        .cmap_dictionary(&cmap_stream(Dict::new(), "X", 0, 2))
        .expect("流可读");
    assert_eq!(dictionary.declarations.cmap_name.as_deref(), Some("X"));
}
