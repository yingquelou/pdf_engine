//! Table F.1 条目的类型化（线性化参数字典的七个条目）。
//!
//! 原文（Table F.1，ISO 32000-1）关键约定：
//! - `Linearized`（number，Required）：「A version identification for the
//!   linearized format」；
//! - `L`（integer，Required）：「The length of the entire file in bytes. It
//!   shall be exactly equal to the actual length of the PDF file」；
//! - `H`（array，Required）：「An array of two or four integers,
//!   [offset1 length1] or [offset1 length1 offset2 length2]」，其中
//!   「offset1 shall be the offset of the primary hint stream from the beginning
//!   of the file. (This is the beginning of the stream object, not the beginning
//!   of the stream data.) length1 shall be the length of this stream, including
//!   stream object overhead」；若流字典 `/Length` 是间接引用，被指对象须紧随流
//!   对象，且 `length1` 也须含该长度对象的开销；
//! - `O`/`E`/`N`/`T`（integer，Required）；
//! - `P`（integer，Optional）：「The page number of the first page …
//!   Default value: 0」。
//!
//! 字典里既没有 `/V`，也没有 hint 流对象号条目（`V` 属 Table F.2 的 hint 表
//! 键），故本模块不提供这两个字段。

use crate::core::types::{Dict, Value};

use super::{LinearizationInfo, LinearizationIssue};

/// Table F.1 `H` 的一对 `(offset, length)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintRange {
    /// hint 流对象的起始偏移（Table F.1：流对象起始，而非流数据起始）。
    pub offset: u64,
    /// hint 流长度，含流对象开销（Table F.1：including stream object overhead）。
    pub length: u64,
}

/// Table F.1 `H`：主 hint 流与可选溢出 hint 流。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintStreams {
    /// 主 hint 流（F.3.6：part 5，必有）。
    pub primary: HintRange,
    /// 溢出 hint 流（F.3.6：part 10，可选，`H` 的第二个数对）。
    pub overflow: Option<HintRange>,
}

impl LinearizationInfo {
    /// 类型化 Table F.1 条目，并把缺失/类型/直接对象诊断追加到 `issues`。
    pub(super) fn type_entries(&mut self) {
        // F.3.3：「All values in this dictionary shall be direct objects」。
        // 字典是 BTreeMap，遍历顺序确定。
        for (key, value) in self.dict.0.iter() {
            if matches!(value, Value::Ref(..)) {
                self.issues
                    .push(LinearizationIssue::IndirectValue(key.clone()));
            }
        }

        self.version = version_entry(&self.dict, &mut self.issues);
        self.declared_file_length = non_negative(
            "L",
            integer_entry("L", &self.dict, true, &mut self.issues),
            &mut self.issues,
        );
        self.hint_streams = hint_streams_entry(&self.dict, &mut self.issues);
        self.first_page_object = integer_entry("O", &self.dict, true, &mut self.issues);
        self.first_page_end = non_negative(
            "E",
            integer_entry("E", &self.dict, true, &mut self.issues),
            &mut self.issues,
        );
        self.page_count = integer_entry("N", &self.dict, true, &mut self.issues);
        self.main_xref_offset = non_negative(
            "T",
            integer_entry("T", &self.dict, true, &mut self.issues),
            &mut self.issues,
        );
        // `/P` 可选：缺失时按 Default value: 0（present-but-broken 亦落默认，
        // 类型诊断已在 `integer_entry` 给出）。
        self.first_page_number =
            integer_entry("P", &self.dict, false, &mut self.issues).unwrap_or(0);
    }
}

/// `Linearized`：number（例中写作 `1.0`，也有生成器写 `1`）。
fn version_entry(dict: &Dict, issues: &mut Vec<LinearizationIssue>) -> Option<f64> {
    let value = dict.get("Linearized")?;
    match value {
        Value::Float(number) => Some(*number),
        Value::Integer(number) => Some(*number as f64),
        // 间接引用已由 `IndirectValue` 报告，这里不再叠加类型诊断。
        Value::Ref(..) => None,
        other => {
            issues.push(LinearizationIssue::WrongType {
                key: "Linearized",
                expected: "number",
                found: other.type_name().to_string(),
            });
            None
        }
    }
}

/// Table F.1 的 integer 条目：缺失（仅必填时报告）或类型不符各出一条诊断。
fn integer_entry(
    key: &'static str,
    dict: &Dict,
    required: bool,
    issues: &mut Vec<LinearizationIssue>,
) -> Option<i64> {
    let Some(value) = dict.get(key) else {
        if required {
            issues.push(LinearizationIssue::MissingRequired(key));
        }
        return None;
    };
    match value {
        Value::Integer(number) => Some(*number),
        // 间接引用已由 `IndirectValue` 报告。
        Value::Ref(..) => None,
        other => {
            issues.push(LinearizationIssue::WrongType {
                key,
                expected: "integer",
                found: other.type_name().to_string(),
            });
            None
        }
    }
}

/// 偏移/长度条目：非负才可用（负值另出一条类型诊断）。
fn non_negative(
    key: &'static str,
    value: Option<i64>,
    issues: &mut Vec<LinearizationIssue>,
) -> Option<u64> {
    let number = value?;
    match u64::try_from(number) {
        Ok(offset) => Some(offset),
        Err(_) => {
            issues.push(LinearizationIssue::WrongType {
                key,
                expected: "non-negative integer",
                found: number.to_string(),
            });
            None
        }
    }
}

/// `H`：两个或四个整数的数组。
fn hint_streams_entry(dict: &Dict, issues: &mut Vec<LinearizationIssue>) -> Option<HintStreams> {
    let Some(value) = dict.get("H") else {
        issues.push(LinearizationIssue::MissingRequired("H"));
        return None;
    };
    let Value::Array(array) = value else {
        // 间接引用已由 `IndirectValue` 报告。
        if !matches!(value, Value::Ref(..)) {
            issues.push(LinearizationIssue::WrongType {
                key: "H",
                expected: "array of two or four integers",
                found: value.type_name().to_string(),
            });
        }
        return None;
    };
    let numbers = &array.0;
    if numbers.len() != 2 && numbers.len() != 4 {
        issues.push(LinearizationIssue::HintArrayLength { len: numbers.len() });
        return None;
    }
    let mut values = Vec::with_capacity(numbers.len());
    for item in numbers {
        let Value::Integer(number) = item else {
            issues.push(LinearizationIssue::WrongType {
                key: "H",
                expected: "integer element",
                found: item.type_name().to_string(),
            });
            return None;
        };
        match u64::try_from(*number) {
            Ok(number) => values.push(number),
            Err(_) => {
                issues.push(LinearizationIssue::WrongType {
                    key: "H",
                    expected: "non-negative integer element",
                    found: number.to_string(),
                });
                return None;
            }
        }
    }
    let primary = HintRange {
        offset: values[0],
        length: values[1],
    };
    let overflow = if values.len() == 4 {
        Some(HintRange {
            offset: values[2],
            length: values[3],
        })
    } else {
        None
    };
    Some(HintStreams { primary, overflow })
}
