//! 对象组与对象形态的一致性诊断：F.3.1/F.3.2/F.3.3/F.3.7 里按**对象**判定的规则。
//!
//! 交叉引用表与 trailer 的规则在 [`super::tables`]；识别、引用扫描与偏移/范围类
//! 规则留在 [`super`]。

use crate::core::types::{Document, SubXref, Value, XrefKind};

use super::super::{FIRST_BYTES_LIMIT, LinearizationInfo, LinearizationIssue};

/// F.3.2/F.3.3：字典须是头部之后正文的第一个对象，且整个落在文件前 1024 字节内。
pub(super) fn placement(info: &LinearizationInfo, issues: &mut Vec<LinearizationIssue>) {
    if info.end_offset > FIRST_BYTES_LIMIT {
        issues.push(LinearizationIssue::DictionaryBeyondFirst1024 {
            end_offset: info.end_offset,
            limit: FIRST_BYTES_LIMIT,
        });
    }
    if info.first_body_object != info.object_number {
        issues.push(LinearizationIssue::NotFirstBodyObject {
            object_number: info.first_body_object,
        });
    }
}

/// F.3.1：两对象组须由恰好两个交叉引用段索引；所有对象代号为 0；
/// 第一组的对象号须紧接第二组末尾。
pub(super) fn object_groups(
    document: &Document,
    info: &LinearizationInfo,
    sections: usize,
    issues: &mut Vec<LinearizationIssue>,
) {
    if sections != 2 {
        issues.push(LinearizationIssue::CrossReferenceSections { count: sections });
    }
    if info.generation != 0 {
        issues.push(LinearizationIssue::NonZeroGeneration {
            role: "linearization parameter dictionary",
            generation: info.generation,
        });
    }
    // 表项代号：每段最多报一次，避免大表刷屏。
    for section in &document.xref.0 {
        let Some(entry) = section
            .entries
            .iter()
            .find(|entry| entry.in_use() && entry.generation != 0)
        else {
            continue;
        };
        issues.push(LinearizationIssue::NonZeroGeneration {
            role: "cross-reference table entry",
            generation: entry.generation,
        });
    }
    if sections != 2 {
        return;
    }
    let Some(dict_index) = document
        .xref
        .0
        .iter()
        .position(|section| (section.start..section.end()).contains(&info.object_number))
    else {
        return;
    };
    // 恰好两段：字典所在段是首页段，另一段是主段。
    let first_section = &document.xref.0[dict_index];
    let main_section = &document.xref.0[1 - dict_index];
    let first_group_min = in_use_numbers(first_section).into_iter().min();
    let second_group_max = in_use_numbers(main_section).into_iter().max();
    if let (Some(min_first), Some(max_second)) = (first_group_min, second_group_max)
        && max_second >= min_first
    {
        issues.push(LinearizationIssue::ObjectNumberGroups {
            first_group_min: min_first,
            second_group_max: max_second,
        });
    }
}

/// F.3.1（PDF 1.5 起）：线性化字典、目录与页对象不得打入对象流。
pub(super) fn object_stream_limits(
    document: &Document,
    info: &LinearizationInfo,
    issues: &mut Vec<LinearizationIssue>,
) {
    let roles: [(&'static str, Option<i64>); 3] = [
        (
            "linearization parameter dictionary",
            Some(info.object_number),
        ),
        (
            "document catalogue",
            document.trailer.root().map(|(number, _)| number),
        ),
        ("page object", info.first_page_object),
    ];
    for (role, object_number) in roles {
        let Some(object_number) = object_number else {
            continue;
        };
        if matches!(
            document.xref.get(object_number),
            Some(entry) if matches!(entry.kind, XrefKind::InObjStream { .. })
        ) {
            issues.push(LinearizationIssue::ForbiddenInObjectStream { role });
        }
    }
}

/// F.3.7 与 Table F.1 `/O` `/N` `/P`：首页对象、页数与首页页码。
pub(super) fn first_page_section(
    document: &Document,
    info: &LinearizationInfo,
    issues: &mut Vec<LinearizationIssue>,
) {
    if let Some(object_number) = info.first_page_object {
        match document.get_object(object_number) {
            Ok(Some(object)) => {
                if object.generation != 0 {
                    issues.push(LinearizationIssue::NonZeroGeneration {
                        role: "first page object",
                        generation: object.generation,
                    });
                }
                match &object.value {
                    Value::Dict(dict) => {
                        if dict.get("Type").and_then(|value| value.as_name()) != Some("Page") {
                            issues.push(LinearizationIssue::FirstPageObject {
                                object_number,
                                problem: "its /Type is not /Page".to_string(),
                            });
                        }
                        // F.3.7：首页对象「shall explicitly specify all
                        // required attributes, such as Resources and MediaBox;
                        // the attributes may not be inherited」。
                        for key in ["MediaBox", "Resources"] {
                            if dict.get(key).is_none() {
                                issues.push(LinearizationIssue::FirstPageMissingAttribute {
                                    object_number,
                                    key,
                                });
                            }
                        }
                    }
                    other => issues.push(LinearizationIssue::FirstPageObject {
                        object_number,
                        problem: format!(
                            "its value is {} rather than a page dictionary",
                            other.type_name()
                        ),
                    }),
                }
            }
            Ok(None) => issues.push(LinearizationIssue::FirstPageObject {
                object_number,
                problem: "object not found in the file".to_string(),
            }),
            Err(error) => issues.push(LinearizationIssue::FirstPageObject {
                object_number,
                problem: error.message.clone(),
            }),
        }
    }

    // Table F.1 `/N`：与目录页面树的实际页数比对（页面树损坏时不下诊断）。
    if let Some(declared) = info.page_count
        && let Ok(actual) = document.page_count()
    {
        let agrees = usize::try_from(declared).is_ok_and(|declared| declared == actual);
        if !agrees {
            issues.push(LinearizationIssue::PageCountMismatch { declared, actual });
        }
    }

    // Table F.1 `/P`：须落在 `0 <= P < N`。
    if let Some(page_count) = info.page_count
        && page_count > 0
        && !(0..page_count).contains(&info.first_page_number)
    {
        issues.push(LinearizationIssue::FirstPageNumberOutOfRange {
            page_number: info.first_page_number,
            page_count,
        });
    }
}

/// 段内在用的对象号（跳过 free 条目）。
fn in_use_numbers(section: &SubXref) -> Vec<i64> {
    section
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.in_use())
        .map(|(index, _)| section.start + index as i64)
        .collect()
}
