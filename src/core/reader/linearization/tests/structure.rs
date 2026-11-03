//! F.2/F.3 结构一致性诊断：对象序、交叉引用段序与首页对象规则。
//!
//! 对象流的禁用规则（F.3.1）按 xref 条目种类判定；夹具是语法层字节串，
//! 故该组用例在解析后改写 `Document` 的 pub 条目字段来驱动。

use super::super::FIRST_BYTES_LIMIT;
use super::*;
use crate::core::reader::linearization::LinearizationIssue as Issue;
use crate::core::types::{Value, XrefKind};

/// F.3.3：「The linearization parameter dictionary shall be entirely
/// contained within the first 1024 bytes」——超界仍识别（宽进），并给诊断。
#[test]
fn dictionary_must_fit_within_the_first_1024_bytes() {
    let knobs = Knobs {
        padding: 1200,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-beyond-1024");
    assert_eq!(info.object_number, DICT, "越界字典照样应被识别");
    assert!(
        info.issues.iter().any(|issue| matches!(
            issue,
            Issue::DictionaryBeyondFirst1024 {
                end_offset,
                limit
            } if *end_offset > FIRST_BYTES_LIMIT && *limit == FIRST_BYTES_LIMIT
        )),
        "issues: {:?}",
        info.issues
    );
}

/// F.3.3：「the first object in the body of the file (part 2) shall be an
/// indirect dictionary object, the linearization parameter dictionary」。
#[test]
fn dictionary_must_be_the_first_body_object() {
    let knobs = Knobs {
        decoy_first: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-not-first");
    assert_eq!(info.first_body_object, PAGES);
    expect_issue(
        &info,
        &Issue::NotFirstBodyObject {
            object_number: PAGES,
        },
    );
}

/// F.3.3：「There shall be no references to this dictionary anywhere in the
/// document」——本库按有界扫描判定 trailer、目录与首页页对象三处。
#[test]
fn dictionary_must_not_be_referenced() {
    let cases: &[(&str, Issue)] = &[
        (
            "trailer",
            Issue::ReferencedFrom {
                location: "trailer",
                key: "Linear".to_string(),
            },
        ),
        (
            "catalogue",
            Issue::ReferencedFrom {
                location: "document catalogue",
                key: "Linearization".to_string(),
            },
        ),
        (
            "first page",
            Issue::ReferencedFrom {
                location: "first page object",
                key: "Linear".to_string(),
            },
        ),
    ];
    for (where_, expected) in cases {
        let knobs = match *where_ {
            "trailer" => Knobs {
                first_page_trailer_refs_dict: true,
                ..Default::default()
            },
            "catalogue" => Knobs {
                catalogue_refs_dict: true,
                ..Default::default()
            },
            _ => Knobs {
                first_page_refs_dict: true,
                ..Default::default()
            },
        };
        let info = info_of(&knobs, "lin-referenced");
        expect_issue(&info, expected);
    }
}

/// F.3.3/F.3.4：首页 xref 表须含字典的 normal 条目，且整表无 free 条目。
#[test]
fn dictionary_needs_a_normal_entry_in_the_first_page_table() {
    let knobs = Knobs {
        dict_entry_free: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-dict-entry-free");
    expect_issue(
        &info,
        &Issue::NotNormalEntry {
            role: "linearization parameter dictionary",
            object_number: DICT,
        },
    );
    expect_issue(&info, &Issue::FirstPageTableFreeEntries { count: 1 });
}

/// F.3.4：首页表「shall have no free entries」（字典条目之外的空洞）。
#[test]
fn first_page_table_rejects_free_entries() {
    let knobs = Knobs {
        first_page_free_entry: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-free-entry");
    expect_issue(&info, &Issue::FirstPageTableFreeEntries { count: 1 });
}

/// F.3.1：「All objects shall have a generation number of 0」——
/// 对象本体代号与 xref 条目代号各判一条。
#[test]
fn generations_must_be_zero() {
    let knobs = Knobs {
        dict_generation: 1,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-dict-generation");
    expect_issue(
        &info,
        &Issue::NonZeroGeneration {
            role: "linearization parameter dictionary",
            generation: 1,
        },
    );

    let knobs = Knobs {
        dict_entry_generation: 1,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-entry-generation");
    expect_issue(
        &info,
        &Issue::NonZeroGeneration {
            role: "cross-reference table entry",
            generation: 1,
        },
    );
}

/// F.3.4：首页表须把字典列在表首、把主 hint 流列在表尾。
#[test]
fn first_page_table_entry_order() {
    let knobs = Knobs {
        dict_entry_not_first: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-table-start");
    expect_issue(
        &info,
        &Issue::TableEntryOrder {
            role: "linearization parameter dictionary",
            object_number: DICT,
        },
    );

    let knobs = Knobs {
        swap_hint_entry: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-table-end");
    expect_issue(
        &info,
        &Issue::TableEntryOrder {
            role: "primary hint stream",
            object_number: FONT,
        },
    );
}

/// F.3.1：「These groups of objects shall be indexed by exactly two
/// cross-reference table sections」。
#[test]
fn exactly_two_cross_reference_sections() {
    let knobs = Knobs {
        single_table: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-single-table");
    expect_issues(&info, &[Issue::CrossReferenceSections { count: 1 }]);
}

/// F.3.1：第一组对象号须紧接第二组末尾之后（号序交错即违规）。
#[test]
fn object_groups_must_not_interleave_numbering() {
    let knobs = Knobs {
        main_extra_entry: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-interleaved-groups");
    expect_issue(
        &info,
        &Issue::ObjectNumberGroups {
            first_group_min: DICT,
            second_group_max: DICT,
        },
    );
}

/// F.3.1（PDF 1.5 起）：线性化字典、目录与页对象不得打入对象流。
#[test]
fn core_objects_may_not_live_in_object_streams() {
    for (role, number) in [
        ("linearization parameter dictionary", DICT),
        ("document catalogue", CATALOGUE),
        ("page object", PAGE1),
    ] {
        let bytes = build(&Knobs::default());
        let mut document = open(&bytes, "lin-object-stream");
        for section in &mut document.xref.0 {
            for index in 0..section.entries.len() {
                if section.start + index as i64 == number {
                    section.entries[index].kind = XrefKind::InObjStream {
                        stream_num: HINT,
                        index: 0,
                    };
                }
            }
        }
        let info = document
            .linearization()
            .expect("no I/O error")
            .expect("recognized");
        expect_issue(&info, &Issue::ForbiddenInObjectStream { role });
    }
}

/// F.3.4：「The first-page trailer shall contain valid Size and Root
/// entries」，且其 `Prev`「shall give the offset of the main
/// cross-reference table」——缺失或形态不符各判一条。
#[test]
fn first_page_trailer_carries_prev_size_and_root() {
    let bytes = build(&Knobs::default());
    for key in ["Prev", "Size", "Root"] {
        let mut document = open(&bytes, "lin-first-page-trailer");
        document.trailer.dict.0.remove(key);
        let info = document
            .linearization()
            .expect("no I/O error")
            .expect("recognized");
        assert!(
            info.issues.iter().any(|issue| matches!(
                issue,
                Issue::FirstPageTrailerEntry {
                    key: found,
                    problem
                } if *found == key && problem.starts_with("it is missing")
            )),
            "issues: {:?}",
            info.issues
        );
    }

    // 形态不符：`Root` 写成整数而非间接引用。
    let mut document = open(&bytes, "lin-first-page-trailer-type");
    document
        .trailer
        .dict
        .0
        .insert("Root".to_string(), Value::Integer(CATALOGUE));
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized");
    assert!(
        info.issues.iter().any(|issue| matches!(
            issue,
            Issue::FirstPageTrailerEntry {
                key: "Root",
                problem
            } if problem.contains("rather than a reference to the document catalogue")
        )),
        "issues: {:?}",
        info.issues
    );
}

/// Table F.1 `/O` 与 F.3.7：首页对象须是能用的页对象。
#[test]
fn first_page_object_must_be_a_page() {
    let knobs = Knobs {
        replaced: &[("O", "4")],
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-first-page-info");
    expect_issue(
        &info,
        &Issue::FirstPageObject {
            object_number: INFO,
            problem: "its /Type is not /Page".to_string(),
        },
    );

    let knobs = Knobs {
        replaced: &[("O", "99")],
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-first-page-missing");
    expect_issue(
        &info,
        &Issue::FirstPageObject {
            object_number: 99,
            problem: "object not found in the file".to_string(),
        },
    );

    // 首页对象写成非字典（内容流）同样不可用。
    let knobs = Knobs {
        replaced: &[("O", "3")],
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-first-page-stream");
    assert!(
        info.issues.iter().any(|issue| matches!(
            issue,
            Issue::FirstPageObject {
                object_number: CONTENTS2,
                problem
            } if problem.contains("rather than a page dictionary")
        )),
        "issues: {:?}",
        info.issues
    );
}

/// F.3.7：「the attributes may not be inherited」——`/MediaBox` 与
/// `/Resources` 须由首页对象显式给出。
#[test]
fn first_page_attributes_must_not_be_inherited() {
    for key in ["MediaBox", "Resources"] {
        let knobs = Knobs {
            missing_page_attribute: key,
            ..Default::default()
        };
        let info = info_of(&knobs, "lin-inherited-attribute");
        expect_issue(
            &info,
            &Issue::FirstPageMissingAttribute {
                object_number: PAGE1,
                key,
            },
        );
    }
}

/// Table F.1 `/N`：与目录页面树的实际页数比对。
#[test]
fn page_count_must_match_the_catalogue() {
    let knobs = Knobs {
        wrong_page_count: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-page-count");
    expect_issue(
        &info,
        &Issue::PageCountMismatch {
            declared: PAGE_TOTAL + 1,
            actual: PAGE_TOTAL as usize,
        },
    );
}

/// F.3.11：主表首条目（对象 0）须是 free 条目。
#[test]
fn main_table_first_entry_must_be_free() {
    let knobs = Knobs {
        main_first_entry_used: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-main-first-entry");
    expect_issue(
        &info,
        &Issue::MainCrossReferenceTable {
            problem: "its first entry (object number 0) shall be a free entry".to_string(),
        },
    );
}

/// F.3.11：「The main trailer … shall not contain any entries other than
/// Size」——传统表与交叉引用流两种主表都判。
#[test]
fn main_trailer_is_size_only() {
    let knobs = Knobs {
        main_trailer_extras: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-main-trailer");
    expect_issue(
        &info,
        &Issue::MainTrailerEntries {
            keys: ["Root".to_string()].to_vec(),
        },
    );

    let knobs = Knobs {
        version: "1.5",
        main_is_stream: true,
        main_trailer_extras: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-main-stream-trailer");
    expect_issue(
        &info,
        &Issue::MainTrailerEntries {
            keys: ["Root".to_string()].to_vec(),
        },
    );
}

/// Table F.1 `/T`：传统表取「首条目之前的空白字符」，交叉引用流文档取
/// 主 XRef 流对象起始；两者都不成立即定位失败。
#[test]
fn main_cross_reference_target_must_be_locatable() {
    // `/T` 指到 `xref` 关键字本身：该字节既非空白也非数字。
    let knobs = Knobs {
        bad_main_target: true,
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-bad-main-target");
    assert!(
        info.issues.iter().any(|issue| matches!(
            issue,
            Issue::MainCrossReferenceOffset { problem, .. }
                if problem.contains("0x78")
        )),
        "issues: {:?}",
        info.issues
    );

    // `/T` 指到头部行的 `P`：同样不是合法定位。
    let knobs = Knobs {
        replaced: &[("T", "1")],
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-bad-main-byte");
    assert!(
        info.issues.iter().any(|issue| matches!(
            issue,
            Issue::MainCrossReferenceOffset { offset, .. } if *offset == 1
        )),
        "issues: {:?}",
        info.issues
    );
}

/// F.3.6：「the tables shall be enclosed within a stream object」——
/// 只判目标形态，不解码 hint 表内容（F.4 不在本切片范围）。
#[test]
fn hint_table_target_must_be_a_stream() {
    let knobs = Knobs {
        bad_hint_target: true,
        ..Default::default()
    };
    let bytes = build(&knobs);
    let info = open(&bytes, "lin-hint-not-stream")
        .linearization()
        .expect("no I/O error")
        .expect("recognized");
    let dict_offset = object_offset(&bytes, "5 0");
    assert!(
        info.issues.iter().any(|issue| matches!(
            issue,
            Issue::HintTargetNotStream { which, offset }
                if *which == "primary" && *offset == dict_offset
        )),
        "issues: {:?}",
        info.issues
    );
}

/// F.3.2：「Linearization … may be applied to any PDF file of version 1.1
/// or greater」——门禁取 1.1（F.1 另述该可选特性自 1.2 起可用，见模块文档）。
#[test]
fn version_gate_is_one_point_one() {
    let knobs = Knobs {
        version: "1.0",
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-version-1-0");
    expect_issue(
        &info,
        &Issue::VersionBelowGate {
            version: "1.0".to_string(),
        },
    );

    // 1.1 声明的线性化文件同样可读，且不因版本给诊断。
    let knobs = Knobs {
        version: "1.1",
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-version-1-1");
    assert!(info.is_consistent(), "issues: {:?}", info.issues);
}
