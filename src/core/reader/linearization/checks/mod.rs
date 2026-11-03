//! 附录 F 的一致性诊断：把 F.2/F.3 的 shall 逐条落成可判定、可测试的最小实现。
//!
//! 每条规则只读已解析出的结构（`Document::xref`、trailer、目录、首页对象）与
//! 定长窗口，不做全文预载，也不解码 F.4 的 hint 表内容。读不到的部分一律**不**
//! 下诊断（避免把 I/O 截断误报成违规），因此诊断集合是「可证伪的违规」而非
//! 完整校验。所有诊断只进入 [`super::LinearizationInfo::issues`]，不写
//! `Document::warnings`。
//!
//! 文件分工：对象组与对象形态的规则在 [`objects`]，交叉引用表与 trailer 的规则
//! 在 [`tables`]；本文件保留识别入口 [`collect`]、引用扫描（F.3.3/F.3.6）、
//! 偏移与范围（Table F.1 `L`/`E`/`T`）、版本门禁（F.3.2）与共用读对象辅助。

use crate::core::reader::lexer::Lexer;
use crate::core::reader::navigation::catalog;
use crate::core::reader::object::parse_indirect_object;
use crate::core::types::{Dict, Document, PdfVersion, Value, XrefKind};

use super::{LinearizationInfo, LinearizationIssue, MAIN_TABLE_WINDOW};

mod objects;
mod tables;

/// 收集附录 F 的一致性诊断（返回顺序即规则顺序，稳定可测）。
pub(super) fn collect(document: &Document, info: &LinearizationInfo) -> Vec<LinearizationIssue> {
    let mut issues = Vec::new();
    objects::placement(info, &mut issues);
    let sections = document.xref.0.len();
    // F.3.1 把两段索引与号序写成一条 shall；单段（或异常多段）文件只报段数，
    // 号序/表序规则在此处不适用（无法可靠区分首页段与主段）。
    let two_sections = sections == 2;
    dictionary_indexing(document, info, &mut issues);
    objects::object_groups(document, info, sections, &mut issues);
    objects::object_stream_limits(document, info, &mut issues);
    objects::first_page_section(document, info, &mut issues);
    if two_sections {
        tables::table_entry_order(document, info, &mut issues);
        // F.3.4 的首页 trailer 以「存在独立的首页表」为前提：单段文件连
        // F.3.1 都不满足，其唯一 trailer 无 `Prev` 属正常形态，不下诊断。
        tables::first_page_trailer(document, info, &mut issues);
    }
    hint_streams(document, info, &mut issues);
    bounds(info, &mut issues);
    if two_sections {
        tables::main_cross_reference(document, info, &mut issues);
    }
    version_gate(document, &mut issues);
    issues
}

/// F.3.3：字典不得被引用，且首页 xref 表须含它的 normal 条目。
///
/// 「no references … anywhere in the document」在本库按**有界扫描**判定：只查
/// trailer 字典、目录字典与首页页对象字典三处（全档扫描需要预载所有对象，与
/// 「识别」这一目的的代价不成比例；这是本库读端策略而非规范豁免）。
fn dictionary_indexing(
    document: &Document,
    info: &LinearizationInfo,
    issues: &mut Vec<LinearizationIssue>,
) {
    let target = info.object_number;
    let mut locations: Vec<(&'static str, Option<Dict>)> = vec![
        ("trailer", Some(document.trailer.dict.clone())),
        ("document catalogue", catalog(document).ok()),
    ];
    if let Some(dict) = first_page_dict(document, info) {
        locations.push(("first page object", Some(dict)));
    }
    for (location, dict) in locations {
        let Some(dict) = dict else { continue };
        if let Some(key) = referencing_key(&dict, target) {
            issues.push(LinearizationIssue::ReferencedFrom { location, key });
        }
    }
    if !matches!(
        document.xref.get(target),
        Some(entry) if entry.kind == XrefKind::Normal
    ) {
        issues.push(LinearizationIssue::NotNormalEntry {
            role: "linearization parameter dictionary",
            object_number: target,
        });
    }
}

/// F.3.6：hint 表须封装在流对象里。只核对目标对象形态，**不**解码
/// hint 表内容（F.4 的二进制解码不在本切片范围）。
fn hint_streams(
    document: &Document,
    info: &LinearizationInfo,
    issues: &mut Vec<LinearizationIssue>,
) {
    let Some(streams) = info.hint_streams else {
        return;
    };
    let mut targets = vec![("primary", streams.primary)];
    if let Some(overflow) = streams.overflow {
        targets.push(("overflow", overflow));
    }
    for (which, range) in targets {
        if let Some(file_length) = info.file_size {
            let end = range.offset.saturating_add(range.length);
            if range.offset > file_length || end > file_length {
                issues.push(LinearizationIssue::HintRangeBeyondFile {
                    which,
                    end,
                    file_length,
                });
            }
        }
        // 读不到或解析失败时不下诊断：窗口截断不等于违规。
        if let Some(value) = read_object_at(document, range.offset)
            && !matches!(value, Value::Stream(..))
        {
            issues.push(LinearizationIssue::HintTargetNotStream {
                which,
                offset: range.offset,
            });
        }
    }
}

/// Table F.1：`L` 与实际长度须精确相等；`E`、`T` 不得越过文件末尾。
fn bounds(info: &LinearizationInfo, issues: &mut Vec<LinearizationIssue>) {
    let Some(file_length) = info.file_size else {
        return;
    };
    if let Some(declared) = info.declared_file_length
        && declared != file_length
    {
        issues.push(LinearizationIssue::FileLengthMismatch {
            declared,
            actual: file_length,
        });
    }
    for (key, offset) in [("E", info.first_page_end), ("T", info.main_xref_offset)] {
        if let Some(offset) = offset
            && offset > file_length
        {
            issues.push(LinearizationIssue::OffsetBeyondFile {
                key,
                offset,
                file_length,
            });
        }
    }
}

/// F.3.2：「Linearization … may be applied to any PDF file of version 1.1 or
/// greater」。F.1 另述线性化作为**可选特性**自 PDF 1.2 起可用；结构性门禁
/// 按 F.3.2 取 1.1（声明 1.0 的文件带线性化字典即违规）。
fn version_gate(document: &Document, issues: &mut Vec<LinearizationIssue>) {
    let version = document.source_version();
    if version < PdfVersion::V1_1 {
        issues.push(LinearizationIssue::VersionBelowGate {
            version: version.to_string(),
        });
    }
}

/// 首页页对象字典（能解析且确为字典时）。
fn first_page_dict(document: &Document, info: &LinearizationInfo) -> Option<Dict> {
    let object_number = info.first_page_object?;
    let object = document.get_object(object_number).ok()??;
    match object.value {
        Value::Dict(dict) => Some(dict),
        _ => None,
    }
}

/// 在字典（含一层数组展开）中查找指向 `target` 的间接引用，返回条目名。
fn referencing_key(dict: &Dict, target: i64) -> Option<String> {
    dict.0
        .iter()
        .find(|(_, value)| value_references(value, target))
        .map(|(key, _)| key.clone())
}

/// 值是否为（或内含）指向 `target` 的间接引用。
fn value_references(value: &Value, target: i64) -> bool {
    match value {
        Value::Ref(number, _) => *number == target,
        Value::Array(array) => array.0.iter().any(|item| value_references(item, target)),
        _ => false,
    }
}

/// 按绝对偏移解析一个间接对象；读不到或语法断裂返回 `None`。
pub(super) fn read_object_at(document: &Document, offset: u64) -> Option<Value> {
    let bytes = document.read_at(offset, MAIN_TABLE_WINDOW)?;
    let mut lex = Lexer::at(&bytes, offset);
    lex.skip_ws();
    if !matches!(lex.peek(), Some(byte) if byte.is_ascii_digit()) {
        return None;
    }
    parse_indirect_object(&mut lex)
        .ok()
        .map(|object| object.value)
}
