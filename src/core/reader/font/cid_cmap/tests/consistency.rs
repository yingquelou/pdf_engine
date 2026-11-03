//! CMap ↔ CIDFont 一致性诊断出口的单元测试（9.7.3 / 9.7.5.2 / 9.7.5.3，
//! Table 116–120）。Table 120 **类型化读端**本身的用例在同级 `cmap_dict` 模块，
//! 那边的夹具函数由本模块共享。
//!
//! 固定的两条不变量：
//!
//! - 诊断只报告：字典条目写成非法形态（`/WMode 5`、`/Type` 非 name、
//!   `/CIDSystemInfo` 不可解析）时，既有 CMap 解析与 `decode_codes` 结果不变；
//! - 只按规范判：Required 缺失只对 `/Encoding` 侧提出，Supplement 差异与
//!   `/CMapType` 一律只作 should／策略线索（`is_mandatory()` 为 `false`）。

use crate::core::reader::font::{
    CMapConsistencyIssue, CMapConsistencyRule, CMapPosition, Type0Font,
};
use crate::core::types::{Array, Dict, Document, StreamData, Value};

/// Table 116 的 `/CIDSystemInfo` 三条目。
pub(super) fn collection(registry: &str, ordering: &str, supplement: i64) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Registry", Value::String(registry.into()))
            .insert("Ordering", Value::String(ordering.into()))
            .insert("Supplement", Value::Integer(supplement)),
    )
}

pub(super) fn cid_font(subtype: &str, info: Value, embedded: bool) -> Value {
    let mut descriptor = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
    if embedded {
        descriptor = descriptor.insert(
            "FontFile2",
            Value::Stream(Dict::new(), StreamData::Owned(Vec::new())),
        );
    }
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name(subtype.into()))
            .insert("BaseFont", Value::Name("TestCID".into()))
            .insert("CIDSystemInfo", info)
            .insert("FontDescriptor", Value::Dict(descriptor)),
    )
}

pub(super) fn type0_font(encoding: Value, to_unicode: Option<Value>, descendant: Value) -> Value {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name("Type0".into()))
        .insert("BaseFont", Value::Name("TestComposite".into()))
        .insert("Encoding", encoding)
        .insert(
            "DescendantFonts",
            Value::Array(Array::new().push(descendant)),
        );
    if let Some(to_unicode) = to_unicode {
        dict = dict.insert("ToUnicode", to_unicode);
    }
    Value::Dict(dict)
}

/// Table 120 五条目齐全的 CMap 流字典。
pub(super) fn full_dict(name: &str, info: Value, mode: i64) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("CMap".into()))
        .insert("CMapName", Value::Name(name.into()))
        .insert("CIDSystemInfo", info)
        .insert("WMode", Value::Integer(mode))
}

pub(super) fn cmap_stream(dict: Dict, name: &str, mode: i64, cmap_type: i64) -> Value {
    Value::Stream(
        dict,
        StreamData::Owned(
            format!(
                "/CMapName /{name} def\n/CMapType {cmap_type} def\n/WMode {mode} def\n\
                 1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                 1 begincidchar\n<0041> 1\nendcidchar"
            )
            .into_bytes(),
        ),
    )
}

/// 一个「字典与流内声明一致、字符集与字体一致」的干净 Type 0 字体。
fn clean_font(info: Value, font_info: Value) -> Value {
    let dict = full_dict("Test-CMap", info.clone(), 0);
    type0_font(
        cmap_stream(dict, "Test-CMap", 0, 2),
        None,
        cid_font("CIDFontType2", font_info, true),
    )
}

pub(super) fn encoding_rules(issues: &[CMapConsistencyIssue]) -> Vec<CMapConsistencyRule> {
    issues
        .iter()
        .filter(|issue| issue.position == CMapPosition::Encoding)
        .map(|issue| issue.rule.clone())
        .collect()
}

pub(super) fn has_rule(issues: &[CMapConsistencyIssue], rule: &CMapConsistencyRule) -> bool {
    issues.iter().any(|issue| &issue.rule == rule)
}

#[test]
fn compatible_registry_and_ordering_is_clean_and_supplement_is_should() {
    let document = Document::new();
    assert!(
        document
            .cmap_consistency_issues(&clean_font(
                collection("Adobe", "GB1", 3),
                collection("Adobe", "GB1", 3)
            ))
            .is_empty()
    );
    // Supplement 不等：只作 should（Table 116 L14560–14561 明确不参与兼容性）。
    let issues = document.cmap_consistency_issues(&clean_font(
        collection("Adobe", "GB1", 5),
        collection("Adobe", "GB1", 3),
    ));
    assert_eq!(
        encoding_rules(&issues),
        vec![CMapConsistencyRule::CidSystemInfoSupplementDiffers { cmap: 5, font: 3 }]
    );
    assert!(!issues[0].rule.is_mandatory());
    // Registry 或 Ordering 不同：shall（9.7.3 L14522–14523、L14529–14530）。
    let issues = document.cmap_consistency_issues(&clean_font(
        collection("Adobe", "CNS1", 3),
        collection("Adobe", "GB1", 3),
    ));
    assert_eq!(
        encoding_rules(&issues),
        vec![CMapConsistencyRule::CidSystemInfoIncompatible {
            cmap: "Adobe/CNS1".into(),
            font: "Adobe/GB1".into(),
        }]
    );
    assert!(issues[0].rule.is_mandatory());
    let issues = document.cmap_consistency_issues(&clean_font(
        collection("Microsoft", "GB1", 3),
        collection("Adobe", "GB1", 3),
    ));
    assert!(has_rule(
        &issues,
        &CMapConsistencyRule::CidSystemInfoIncompatible {
            cmap: "Microsoft/GB1".into(),
            font: "Adobe/GB1".into(),
        }
    ));
}

#[test]
fn identity_encoding_is_exempted_from_the_collection_pairing() {
    let document = Document::new();
    // Table 120 L14937–14938 ＋ Table 119 L14849–14851：Identity 不要求匹配字体集合。
    let font = clean_font(
        collection("Adobe", "Identity", 0),
        collection("Adobe", "GB1", 6),
    );
    assert!(
        document.cmap_consistency_issues(&font).is_empty(),
        "Identity CMap 与任意字符集的 CIDFont 搭配是规范允许的"
    );
    // 流内 /CMapName 声明 Identity-H 时同样豁免（字典名与流名不一致另有发现）。
    let dict = full_dict("Other-Name", collection("Adobe", "Identity", 0), 0);
    let font = type0_font(
        cmap_stream(dict, "Identity-H", 0, 2),
        None,
        cid_font("CIDFontType2", collection("Adobe", "GB1", 6), true),
    );
    let issues = document.cmap_consistency_issues(&font);
    assert_eq!(
        encoding_rules(&issues),
        vec![CMapConsistencyRule::CMapNameDiffersFromFile {
            dict: "Other-Name".into(),
            file: "Identity-H".into(),
        }]
    );
    assert!(issues[0].rule.is_mandatory());
}

#[test]
fn identity_name_encoding_requires_an_embedded_font_program() {
    let document = Document::new();
    let dangling = type0_font(
        Value::Name("Identity-H".into()),
        None,
        cid_font("CIDFontType2", collection("Adobe", "GB1", 3), false),
    );
    assert_eq!(
        document.cmap_consistency_issues(&dangling),
        vec![CMapConsistencyIssue {
            position: CMapPosition::Encoding,
            rule: CMapConsistencyRule::IdentityEncodingWithoutEmbeddedFont,
        }]
    );
    let embedded = type0_font(
        Value::Name("Identity-V".into()),
        None,
        cid_font("CIDFontType2", collection("Adobe", "GB1", 3), true),
    );
    assert!(document.cmap_consistency_issues(&embedded).is_empty());
    // 非 Identity 的预定义名不受该条约束。
    let other = type0_font(
        Value::Name("UniGB-UCS2-H".into()),
        None,
        cid_font("CIDFontType2", collection("Adobe", "GB1", 3), false),
    );
    assert!(document.cmap_consistency_issues(&other).is_empty());
    // 规范按**名字**约束（L14912「The Identity-H and Identity-V CMaps」）：
    // 自定义 CMap 名即使声明 Adobe-Identity 字符集，也不触发本条。
    let custom_identity = clean_font(
        collection("Adobe", "Identity", 0),
        collection("Adobe", "Identity", 0),
    );
    let non_embedded = type0_font(
        cmap_stream(
            full_dict("My-Identity", collection("Adobe", "Identity", 0), 0),
            "My-Identity",
            0,
            2,
        ),
        None,
        cid_font("CIDFontType2", collection("Adobe", "Identity", 0), false),
    );
    assert!(
        document.cmap_consistency_issues(&non_embedded).is_empty(),
        "自定义 Identity 口径 CMap 不套预定义名的 shall"
    );
    assert!(
        document
            .type0_cmap_dictionaries(&custom_identity)
            .encoding
            .is_some_and(|cmap| cmap.is_identity_encoding() && !cmap.is_identity_cmap()),
        "两种口径必须可区分"
    );
    // 流内声明 Identity-H 的内嵌 CMap 则按预定义名判定。
    let declared = type0_font(
        cmap_stream(
            full_dict("Identity-H", collection("Adobe", "Identity", 0), 0),
            "Identity-H",
            0,
            2,
        ),
        None,
        cid_font("CIDFontType2", collection("Adobe", "Identity", 0), false),
    );
    assert_eq!(
        document.cmap_consistency_issues(&declared),
        vec![CMapConsistencyIssue {
            position: CMapPosition::Encoding,
            rule: CMapConsistencyRule::IdentityEncodingWithoutEmbeddedFont,
        }]
    );
}

#[test]
fn writing_mode_and_cmap_type_forms_are_reported_as_declared() {
    let document = Document::new();
    // 字典 /WMode 5（非法整数）＋流内 /WMode 0。
    let dict = full_dict("Test-CMap", collection("Adobe", "Identity", 0), 5);
    let font = type0_font(
        cmap_stream(dict, "Test-CMap", 0, 2),
        None,
        cid_font("CIDFontType2", collection("Adobe", "Identity", 0), true),
    );
    assert_eq!(
        document.cmap_consistency_issues(&font),
        vec![CMapConsistencyIssue {
            position: CMapPosition::Encoding,
            rule: CMapConsistencyRule::WModeOutOfRange { value: 5 },
        }]
    );
    // 字典 /WMode 0 与流内 /WMode 1 不一致（Table 120 L14942–14943，shall）。
    let dict = full_dict("Test-CMap", collection("Adobe", "Identity", 0), 0);
    let font = type0_font(
        cmap_stream(dict, "Test-CMap", 1, 2),
        None,
        cid_font("CIDFontType2", collection("Adobe", "Identity", 0), true),
    );
    assert_eq!(
        document.cmap_consistency_issues(&font),
        vec![CMapConsistencyIssue {
            position: CMapPosition::Encoding,
            rule: CMapConsistencyRule::WModeDiffersFromFile { dict: 0, file: 1 },
        }]
    );
    // /CMapType 与 /UID 不是 Table 120 条目：只出 should／策略级线索。
    let mut dict = full_dict("Test-CMap", collection("Adobe", "Identity", 0), 0);
    dict = dict
        .insert("CMapType", Value::Integer(7))
        .insert("UID", Value::Integer(-3));
    let font = type0_font(
        cmap_stream(dict, "Test-CMap", 0, 1),
        None,
        cid_font("CIDFontType2", collection("Adobe", "Identity", 0), true),
    );
    let issues = document.cmap_consistency_issues(&font);
    assert_eq!(
        encoding_rules(&issues),
        vec![CMapConsistencyRule::CMapTypeUnsupported { value: 7 }]
    );
    assert!(!issues[0].rule.is_mandatory());
    let mut dict = full_dict("Test-CMap", collection("Adobe", "Identity", 0), 0);
    dict = dict.insert("CMapType", Value::Integer(2));
    let font = type0_font(
        cmap_stream(dict, "Test-CMap", 0, 1),
        None,
        cid_font("CIDFontType2", collection("Adobe", "Identity", 0), true),
    );
    let issues = document.cmap_consistency_issues(&font);
    assert_eq!(
        encoding_rules(&issues),
        vec![CMapConsistencyRule::CMapTypeDiffersFromFile { dict: 2, file: 1 }]
    );
    assert!(!issues[0].rule.is_mandatory());
}

#[test]
fn nonzero_use_font_number_violates_constraint_b() {
    let document = Document::new();
    let dict = full_dict("Test-CMap", collection("Adobe", "Identity", 0), 0);
    let body = "/CMapName /Test-CMap def\n/WMode 0 def\n\
                1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
                /F3 3 usefont\n1 begincidchar\n<0041> 1\nendcidchar";
    let font = type0_font(
        Value::Stream(dict, StreamData::Owned(body.as_bytes().to_vec())),
        None,
        cid_font("CIDFontType2", collection("Adobe", "Identity", 0), true),
    );
    assert_eq!(
        document.cmap_consistency_issues(&font),
        vec![CMapConsistencyIssue {
            position: CMapPosition::Encoding,
            rule: CMapConsistencyRule::NonZeroUseFontNumber { value: 3 },
        }]
    );
}

#[test]
fn to_unicode_side_reports_only_present_entries() {
    let document = Document::new();
    // ISO 自身的 /ToUnicode 例子（L15760–15772）只有 /Length：不判 Required。
    let to_unicode = Value::Stream(
        Dict::new(),
        StreamData::Owned(b"2 beginbfchar\n<0041> <004D>\nendbfchar".to_vec()),
    );
    let font = type0_font(
        Value::Name("Identity-H".into()),
        Some(to_unicode.clone()),
        cid_font("CIDFontType2", collection("Adobe", "GB1", 3), true),
    );
    let cmaps = document.type0_cmap_dictionaries(&font);
    assert!(cmaps.encoding.is_none());
    assert!(
        cmaps
            .to_unicode
            .as_ref()
            .is_some_and(|cmap| cmap.object_type.is_missing())
    );
    assert!(document.cmap_consistency_issues(&font).is_empty());
    // 但 /ToUnicode 里已经写出来的条目仍判形态与对照。
    let bad = cmap_stream(
        Dict::new()
            .insert("Type", Value::Name("Pages".into()))
            .insert("WMode", Value::Integer(9)),
        "TU-CMap",
        0,
        2,
    );
    let font = type0_font(
        Value::Name("Identity-H".into()),
        Some(bad),
        cid_font("CIDFontType2", collection("Adobe", "GB1", 3), true),
    );
    let issues = document.cmap_consistency_issues(&font);
    let unicode: Vec<_> = issues
        .iter()
        .filter(|issue| issue.position == CMapPosition::ToUnicode)
        .map(|issue| issue.rule.clone())
        .collect();
    assert_eq!(
        unicode,
        vec![
            CMapConsistencyRule::TypeNotCMap {
                found: "Pages".into()
            },
            CMapConsistencyRule::WModeOutOfRange { value: 9 },
        ]
    );
}

#[test]
fn diagnosis_leaves_existing_cmap_parsing_untouched() {
    let document = Document::new();
    // 字典侧全非法（`/WMode 5`、`/CIDSystemInfo` 缺失、`/Type` 非法），流内合法。
    let dict = Dict::new()
        .insert("Type", Value::Integer(3))
        .insert("WMode", Value::Integer(5))
        .insert("CMapName", Value::Name("Other-Name".into()));
    let font = type0_font(
        cmap_stream(dict, "Test-CMap", 1, 2),
        None,
        // 不挂 FontFile2：空的 TrueType 程序本身会被既有解析拒绝，本例只关心
        // 「诊断不改变解析结果」。
        cid_font("CIDFontType2", collection("Adobe", "GB1", 3), false),
    );
    let before = document.type0_font(&font).unwrap().unwrap();
    let codes_before = before.decode_codes(&[0x00, 0x41, 0x00, 0x42]).unwrap();
    let issues = document.cmap_consistency_issues(&font);
    let after = document.type0_font(&font).unwrap().unwrap();
    assert_eq!(
        codes_before,
        after.decode_codes(&[0x00, 0x41, 0x00, 0x42]).unwrap()
    );
    assert_eq!(before, after, "诊断调用不得改变 Type 0 字体的解析结果");
    assert!(matches!(before, Type0Font { .. }));
    assert!(before.is_vertical(), "书写模式仍由流内 /WMode 决定");
    assert_eq!(
        codes_before.iter().map(|code| code.cid).collect::<Vec<_>>(),
        vec![1, 0]
    );
    assert!(issues.len() >= 4, "非法字典条目必须被报出：{issues:?}");
    assert!(has_rule(
        &issues,
        &CMapConsistencyRule::WModeOutOfRange { value: 5 }
    ));
    assert!(issues.iter().any(|issue| matches!(
        &issue.rule,
        CMapConsistencyRule::EntryMalformed { key, .. } if key == "Type"
    )));
    assert!(has_rule(
        &issues,
        &CMapConsistencyRule::RequiredEntryMissing {
            key: "CIDSystemInfo".into()
        }
    ));
    assert!(has_rule(
        &issues,
        &CMapConsistencyRule::CMapNameDiffersFromFile {
            dict: "Other-Name".into(),
            file: "Test-CMap".into(),
        }
    ));
}
