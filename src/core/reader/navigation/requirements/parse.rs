//! 12.10 需求字典与 requirement handler 的解析（Table 264/265；自
//! `requirements/mod.rs` 拆出以守 600 行/25600 字节上限）。
//!
//! 本文件只做「读 + 类型化 + 记违例」：数组内部**任何 shall 违例都不返回
//! `Err`**，以便调用方拿到降级但仍完整的视图（与 `extensions.rs` 把空字典
//! 按 7.12.1 shall 直接拒绝不同：12.10 的 `/Requirements` 是可选条目，损坏
//! 时规范后果只是「阅读器无法满足/校验该需求」，没有让整读失败的依据）。
//! 悬空的 `/Requirements` 引用仍按目录级路径返回 `NavigationError`。

use crate::core::types::{Array, Dict, Document, PdfTextString, Value};

use super::{
    DocumentRequirements, HandlerType, RequirementDictionary, RequirementHandler, RequirementType,
    RequirementsIssue, resolve,
};

/// Table 264：`/Requirements` 数组的逐元素建模。
pub(crate) fn read_requirements_array(document: &Document, array: &Array) -> DocumentRequirements {
    let mut requirements = Vec::new();
    let mut issues = Vec::new();
    for (index, entry) in array.0.iter().enumerate() {
        match resolve(document, entry) {
            Ok(Value::Dict(dict)) => requirements.push(read_requirement(document, index, &dict)),
            Ok(other) => issues.push(RequirementsIssue::EntryNotDictionary {
                index,
                found: other.type_name(),
            }),
            Err(error) => issues.push(RequirementsIssue::EntryUnresolvable {
                index,
                message: error.to_string(),
            }),
        }
    }
    DocumentRequirements {
        requirements,
        issues,
    }
}

/// Table 264：单个需求字典（含其 `RH` 数组）。
fn read_requirement(document: &Document, index: usize, dict: &Dict) -> RequirementDictionary {
    let mut issues = Vec::new();
    // Table 264：/Type 可选，存在时 shall 为 Requirement。
    let type_name = match dict.get("Type") {
        None => None,
        Some(value) => match value.as_name() {
            None => {
                issues.push(RequirementsIssue::RequirementTypeNotName {
                    index,
                    found: value.type_name(),
                });
                None
            }
            Some(name) => {
                if name != "Requirement" {
                    issues.push(RequirementsIssue::RequirementTypeNotRequirement {
                        index,
                        name: name.to_owned(),
                    });
                }
                Some(name.to_owned())
            }
        },
    };
    // Table 264：/S 必需；12.10.1 规定 PDF 1.7 唯一合法值是 EnableJavaScripts。
    let subtype = match dict.get("S") {
        None => {
            issues.push(RequirementsIssue::SubtypeMissing { index });
            RequirementType::Missing
        }
        Some(value) => match value.as_name() {
            None => {
                issues.push(RequirementsIssue::SubtypeNotName {
                    index,
                    found: value.type_name(),
                });
                RequirementType::NotName(value.type_name())
            }
            Some("EnableJavaScripts") => RequirementType::EnableJavaScripts,
            Some(name) => {
                issues.push(RequirementsIssue::SubtypeNotEnableJavaScripts {
                    index,
                    name: name.to_owned(),
                });
                RequirementType::Unknown(name.to_owned())
            }
        },
    };
    // Table 264：/RH 可选，requirement handler 字典数组。
    let mut handlers = Vec::new();
    let mut handler_count = 0usize;
    let mut has_handlers = false;
    if let Some(value) = dict.get("RH") {
        has_handlers = true;
        match resolve(document, value) {
            Ok(Value::Array(array)) => {
                handler_count = array.0.len();
                for (handler, entry) in array.0.iter().enumerate() {
                    match resolve(document, entry) {
                        Ok(Value::Dict(inner)) => {
                            handlers.push(read_handler(index, handler, &inner, &mut issues));
                        }
                        Ok(other) => issues.push(RequirementsIssue::HandlerEntryNotDictionary {
                            index,
                            handler,
                            found: other.type_name(),
                        }),
                        Err(error) => {
                            issues.push(RequirementsIssue::HandlerEntryUnresolvable {
                                index,
                                handler,
                                message: error.to_string(),
                            });
                        }
                    }
                }
            }
            Ok(other) => issues.push(RequirementsIssue::HandlersNotArray {
                index,
                found: other.type_name(),
            }),
            Err(error) => issues.push(RequirementsIssue::HandlersUnresolvable {
                index,
                message: error.to_string(),
            }),
        }
    }
    if has_handlers && subtype == RequirementType::EnableJavaScripts {
        // 12.10.1（L26123–L26125）：EnableJavaScripts 存在时
        // 「The RH entry shall not be used in PDF 1.7」。
        issues.push(RequirementsIssue::RhUsedWithEnableJavaScripts {
            index,
            handler_count,
        });
    }
    RequirementDictionary {
        type_name,
        subtype,
        handlers,
        issues,
        dict: dict.clone(),
    }
}

/// Table 265：单个 requirement handler 字典；违例记进所属需求的 `issues`。
fn read_handler(
    index: usize,
    handler: usize,
    dict: &Dict,
    issues: &mut Vec<RequirementsIssue>,
) -> RequirementHandler {
    let type_name = match dict.get("Type") {
        None => None,
        Some(value) => match value.as_name() {
            None => {
                issues.push(RequirementsIssue::HandlerTypeNotName {
                    index,
                    handler,
                    found: value.type_name(),
                });
                None
            }
            Some(name) => {
                if name != "ReqHandler" {
                    issues.push(RequirementsIssue::HandlerTypeNotReqHandler {
                        index,
                        handler,
                        name: name.to_owned(),
                    });
                }
                Some(name.to_owned())
            }
        },
    };
    let subtype = match dict.get("S") {
        None => {
            issues.push(RequirementsIssue::HandlerSubtypeMissing { index, handler });
            HandlerType::Missing
        }
        Some(value) => match value.as_name() {
            None => {
                issues.push(RequirementsIssue::HandlerSubtypeNotName {
                    index,
                    handler,
                    found: value.type_name(),
                });
                HandlerType::NotName(value.type_name())
            }
            Some("JS") => HandlerType::JavaScript,
            Some("NoOp") => HandlerType::NoOp,
            Some(name) => {
                issues.push(RequirementsIssue::HandlerSubtypeNotPermitted {
                    index,
                    handler,
                    name: name.to_owned(),
                });
                HandlerType::Unknown(name.to_owned())
            }
        },
    };
    let mut script = None;
    if let Some(value) = dict.get("Script") {
        match text_string(value) {
            Some(text) => {
                if subtype != HandlerType::JavaScript {
                    // Table 265：/Script「valid only if the S entry has a
                    // value of JS」；违例仍保留解码后的原值，便于诊断。
                    issues.push(RequirementsIssue::ScriptOnlyForJavaScript { index, handler });
                }
                script = Some(text);
            }
            None => issues.push(RequirementsIssue::ScriptNotTextString {
                index,
                handler,
                found: value.type_name(),
            }),
        }
    }
    RequirementHandler {
        type_name,
        subtype,
        script,
        dict: dict.clone(),
    }
}

/// PDF 文本串 → Unicode 字符串。
///
/// 只接受 literal string 与 hex string（Table 265 把 `/Script` 标为 text
/// string，name 对象不算，故 `Script` 写成 name 会记
/// [`RequirementsIssue::ScriptNotTextString`]）。解码失败（未定义的
/// PDFDocEncoding 字节、奇数位或含非法字符的十六进制串）时保留原始内容，
/// 与 `navigation/templates.rs` 的 `decode_name` 一致，属库策略。
fn text_string(value: &Value) -> Option<String> {
    let (bytes, raw) = match value {
        Value::String(text) => (
            text.chars().map(|ch| ch as u32 as u8).collect(),
            text.clone(),
        ),
        Value::HexString(hex) => (hex_decode(hex)?, hex.clone()),
        _ => return None,
    };
    Some(
        PdfTextString::decode(&bytes)
            .map(PdfTextString::into_string)
            .unwrap_or(raw),
    )
}

/// 十六进制串还原为字节；长度非偶或含非十六进制字符时返回 `None`。
fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = hex
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let (pairs, tail) = digits.as_chunks::<2>();
    if !tail.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        bytes.push(high * 16 + low);
    }
    Some(bytes)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
