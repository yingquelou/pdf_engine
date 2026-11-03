//! Table F.1 条目的类型化诊断：缺失、类型、直接对象约束、越界与默认值。

use super::*;
use crate::core::reader::linearization::LinearizationIssue as Issue;

/// F.3.3：字典须含 Table F.1 标注 Required 的全部条目，缺一报一条。
#[test]
fn required_entries_cannot_be_missing() {
    for key in ["L", "H", "O", "E", "N", "T"] {
        let knobs = Knobs {
            omit: &[key],
            ..Default::default()
        };
        let info = info_of(&knobs, "lin-missing");
        expect_issue(&info, &Issue::MissingRequired(key));
    }
}

/// `/L` 缺失时线性化信息不可信（Table F.1 的 mismatch 条款同样按
/// 「treated as ordinary」处理）。
#[test]
fn missing_length_is_not_trustworthy() {
    let knobs = Knobs {
        omit: &["L"],
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-missing-l");
    assert_eq!(info.declared_file_length, None);
    assert!(info.treated_as_ordinary());
}

/// Table F.1 的 integer / number / array 类型约束。
#[test]
fn entries_are_typed_per_table_f1() {
    let cases: &[(&str, &str, Issue)] = &[
        (
            "Linearized",
            "(1.0)",
            Issue::WrongType {
                key: "Linearized",
                expected: "number",
                found: "string".to_string(),
            },
        ),
        (
            "L",
            "(length)",
            Issue::WrongType {
                key: "L",
                expected: "integer",
                found: "string".to_string(),
            },
        ),
        (
            "O",
            "7.5",
            Issue::WrongType {
                key: "O",
                expected: "integer",
                found: "float".to_string(),
            },
        ),
        (
            "N",
            "[2]",
            Issue::WrongType {
                key: "N",
                expected: "integer",
                found: "array".to_string(),
            },
        ),
        (
            "H",
            "10",
            Issue::WrongType {
                key: "H",
                expected: "array of two or four integers",
                found: "integer".to_string(),
            },
        ),
    ];
    for &(key, raw, ref issue) in cases {
        let knobs = Knobs {
            replaced: &[(key, raw)],
            ..Default::default()
        };
        let info = info_of(&knobs, "lin-wrong-type");
        expect_issue(&info, issue);
    }
}

/// 偏移与长度条目须是非负整数（Table F.1 的 integer 语义）。
#[test]
fn negative_offsets_and_lengths_are_rejected() {
    for key in ["L", "E", "T"] {
        let knobs = Knobs {
            replaced: &[(key, "-1")],
            ..Default::default()
        };
        let info = info_of(&knobs, "lin-negative");
        expect_issue(
            &info,
            &Issue::WrongType {
                key,
                expected: "non-negative integer",
                found: "-1".to_string(),
            },
        );
    }
}

/// Table F.1：`H` 只接受两个或四个整数。
#[test]
fn hint_array_length_must_be_two_or_four() {
    for (raw, len) in [("[1 2 3]", 3usize), ("[1]", 1), ("[1 2 3 4 5]", 5)] {
        let knobs = Knobs {
            replaced: &[("H", raw)],
            ..Default::default()
        };
        let info = info_of(&knobs, "lin-hint-len");
        expect_issue(&info, &Issue::HintArrayLength { len });
        assert_eq!(info.hint_streams, None);
    }
}

/// Table F.1：`H` 的元素须是非负整数。
#[test]
fn hint_array_elements_must_be_non_negative_integers() {
    let knobs = Knobs {
        replaced: &[("H", "[1 2 3 /Four]")],
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-hint-element");
    expect_issue(
        &info,
        &Issue::WrongType {
            key: "H",
            expected: "integer element",
            found: "name".to_string(),
        },
    );

    let knobs = Knobs {
        replaced: &[("H", "[1 -2]")],
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-hint-negative");
    expect_issue(
        &info,
        &Issue::WrongType {
            key: "H",
            expected: "non-negative integer element",
            found: "-2".to_string(),
        },
    );
}

/// F.3.3：「All values in this dictionary shall be direct objects」。
#[test]
fn dictionary_values_must_be_direct_objects() {
    for key in ["Linearized", "L", "H", "O", "E", "N", "T"] {
        let knobs = Knobs {
            replaced: &[(key, "1 0 R")],
            ..Default::default()
        };
        let info = info_of(&knobs, "lin-indirect-value");
        expect_issue(&info, &Issue::IndirectValue(key.to_string()));
    }
}

/// Table F.1 `/L`：「shall be exactly equal to the actual length」，
/// 不符即「shall be treated as ordinary PDF, ignoring linearization
/// information」——识别结果仍返回模型，但标记为不可信。
#[test]
fn length_mismatch_marks_file_as_ordinary() {
    let knobs = Knobs {
        replaced: &[("L", "123456789")],
        ..Default::default()
    };
    let bytes = build(&knobs);
    let document = open(&bytes, "lin-length-mismatch");
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized");
    expect_issue(
        &info,
        &Issue::FileLengthMismatch {
            declared: 123456789,
            actual: bytes.len() as u64,
        },
    );
    assert!(info.treated_as_ordinary());
    // 诊断只进 issues，不占用 `Document::warnings` 告警通道。
    assert!(document.warnings().is_empty());
}

/// Table F.1 `/E`、`/T`：偏移不得越过文件末尾。
#[test]
fn offsets_must_not_exceed_file_length() {
    for key in ["E", "T"] {
        let knobs = Knobs {
            replaced: &[(key, "99999999")],
            ..Default::default()
        };
        let info = info_of(&knobs, "lin-offset-beyond");
        let file_length = info.file_size.expect("file size");
        expect_issue(
            &info,
            &Issue::OffsetBeyondFile {
                key,
                offset: 99999999,
                file_length,
            },
        );
    }
}

/// Table F.1 `/H`：hint 流范围不得越过文件末尾。
#[test]
fn hint_range_must_not_exceed_file_length() {
    let knobs = Knobs {
        replaced: &[("H", "[999999999 8]")],
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-hint-beyond");
    let file_length = info.file_size.expect("file size");
    expect_issue(
        &info,
        &Issue::HintRangeBeyondFile {
            which: "primary",
            end: 1000000007,
            file_length,
        },
    );
}

/// Table F.1 `/P`：显式取值直接类型化；越界（`0 <= P < N`）给诊断。
#[test]
fn first_page_number_is_typed_and_bounded() {
    let knobs = Knobs {
        first_page_number: Some(1),
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-p-valid");
    assert_eq!(info.first_page_number, 1);
    assert!(info.is_consistent(), "issues: {:?}", info.issues);

    for number in [PAGE_TOTAL, -1] {
        let knobs = Knobs {
            first_page_number: Some(number),
            ..Default::default()
        };
        let info = info_of(&knobs, "lin-p-invalid");
        expect_issue(
            &info,
            &Issue::FirstPageNumberOutOfRange {
                page_number: number,
                page_count: PAGE_TOTAL,
            },
        );
    }
}

/// 类型不符的 `/P` 只报类型诊断，值落回默认 0（Optional 条目）。
#[test]
fn broken_optional_page_number_falls_back_to_default() {
    let knobs = Knobs {
        replaced: &[("P", "(zero)")],
        ..Default::default()
    };
    let info = info_of(&knobs, "lin-p-broken");
    expect_issue(
        &info,
        &Issue::WrongType {
            key: "P",
            expected: "integer",
            found: "string".to_string(),
        },
    );
    assert_eq!(info.first_page_number, 0);
}

/// 原样保留的字典便于读取 Table F.1 之外的扩展条目。
#[test]
fn raw_dictionary_is_retained() {
    let bytes = build(&Knobs::default());
    let info = open(&bytes, "lin-raw-dict")
        .linearization()
        .expect("no I/O error")
        .expect("recognized");
    assert_eq!(info.dict.get("O").and_then(|v| v.as_integer()), Some(PAGE1));
    assert_eq!(
        info.dict.get("N").and_then(|v| v.as_integer()),
        Some(PAGE_TOTAL)
    );
    assert!(info.dict.get("Linearized").is_some());
}

/// 每条诊断都带可定位的原文条款文本（供调用方展示）。
#[test]
fn issues_carry_clause_text() {
    let issues = [
        Issue::MissingRequired("N"),
        Issue::WrongType {
            key: "L",
            expected: "integer",
            found: "string".to_string(),
        },
        Issue::IndirectValue("T".to_string()),
        Issue::HintArrayLength { len: 3 },
        Issue::FileLengthMismatch {
            declared: 1,
            actual: 2,
        },
        Issue::DictionaryBeyondFirst1024 {
            end_offset: 2048,
            limit: 1024,
        },
        Issue::NotFirstBodyObject { object_number: 1 },
        Issue::ReferencedFrom {
            location: "document catalogue",
            key: "Linearization".to_string(),
        },
        Issue::NotNormalEntry {
            role: "linearization parameter dictionary",
            object_number: 5,
        },
        Issue::FirstPageTableFreeEntries { count: 1 },
        Issue::FirstPageTrailerEntry {
            key: "Prev",
            problem: "it is missing, but the entry shall carry the offset of the main \
                      cross-reference table"
                .to_string(),
        },
        Issue::TableEntryOrder {
            role: "primary hint stream",
            object_number: 9,
        },
        Issue::NonZeroGeneration {
            role: "cross-reference table entry",
            generation: 1,
        },
        Issue::ForbiddenInObjectStream {
            role: "page object",
        },
        Issue::CrossReferenceSections { count: 1 },
        Issue::ObjectNumberGroups {
            first_group_min: 5,
            second_group_max: 5,
        },
        Issue::FirstPageObject {
            object_number: 4,
            problem: "its /Type is not /Page".to_string(),
        },
        Issue::FirstPageMissingAttribute {
            object_number: 7,
            key: "MediaBox",
        },
        Issue::PageCountMismatch {
            declared: 3,
            actual: 2,
        },
        Issue::FirstPageNumberOutOfRange {
            page_number: 2,
            page_count: 2,
        },
        Issue::OffsetBeyondFile {
            key: "E",
            offset: 9,
            file_length: 1,
        },
        Issue::HintRangeBeyondFile {
            which: "primary",
            end: 9,
            file_length: 1,
        },
        Issue::HintTargetNotStream {
            which: "primary",
            offset: 3,
        },
        Issue::MainCrossReferenceOffset {
            offset: 3,
            problem: "the object there is not a stream".to_string(),
        },
        Issue::MainCrossReferenceTable {
            problem: "its first entry (object number 0) shall be a free entry".to_string(),
        },
        Issue::MainTrailerEntries {
            keys: ["Root".to_string()].to_vec(),
        },
        Issue::VersionBelowGate {
            version: "1.0".to_string(),
        },
    ];
    for issue in &issues {
        let text = issue.to_string();
        assert!(!text.is_empty(), "{issue:?} 应给出可读说明");
        assert!(
            text.contains("F.3") || text.contains("Table F.1"),
            "诊断须引用附录 F 条款：{text}"
        );
    }
    // 诊断本身可作为错误抛出（实现 `std::error::Error`）。
    let boxed: Box<dyn std::error::Error> = Box::new(Issue::MissingRequired("N"));
    assert!(boxed.to_string().contains("Table F.1"));
}
