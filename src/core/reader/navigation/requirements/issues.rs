//! 12.10 的类型化 shall 违例（自 `requirements/mod.rs` 拆出以守 600 行/
//! 25600 字节上限；每变体的 `Display` 携带条款依据，便于把「为什么违规」
//! 直接交给调用方，同 `reader/linearization/issues.rs` 的手法）。

use std::fmt;

/// 12.10 文档需求的 shall 违例。
///
/// 变体一律保留**原始值**（名字/类型名/错误信息）与位置（`index` 是
/// `/Requirements` 数组下标，`handler` 是该条 `RH` 数组下标），不静默丢弃。
#[derive(Debug, Clone, PartialEq)]
pub enum RequirementsIssue {
    /// Table 3 / 12.10.1：目录 `/Requirements` shall 是需求字典数组。
    CatalogEntryNotArray { found: &'static str },
    /// 12.10.1：数组元素不是字典。
    EntryNotDictionary { index: usize, found: &'static str },
    /// 12.10.1：数组元素的间接引用不可解析。
    EntryUnresolvable { index: usize, message: String },
    /// Table 264：`Type` 存在但不是 name 对象。
    RequirementTypeNotName { index: usize, found: &'static str },
    /// Table 264：`Type` 是 name 但值不是 `Requirement`（shall）。
    RequirementTypeNotRequirement { index: usize, name: String },
    /// Table 264：`S` 是 Required 条目，缺失即违例。
    SubtypeMissing { index: usize },
    /// Table 264：`S` 存在但不是 name 对象。
    SubtypeNotName { index: usize, found: &'static str },
    /// Table 264 / 12.10.1：`S` 的值 shall 为 `EnableJavaScripts`；
    /// 未定义的名字原样保留在 `name`。
    SubtypeNotEnableJavaScripts { index: usize, name: String },
    /// Table 264：`RH` 存在但不是数组。
    HandlersNotArray { index: usize, found: &'static str },
    /// Table 264：`RH` 的间接引用不可解析。
    HandlersUnresolvable { index: usize, message: String },
    /// Table 264：`RH` 数组元素不是 requirement handler 字典。
    HandlerEntryNotDictionary {
        index: usize,
        handler: usize,
        found: &'static str,
    },
    /// Table 264：`RH` 数组元素的间接引用不可解析。
    HandlerEntryUnresolvable {
        index: usize,
        handler: usize,
        message: String,
    },
    /// 12.10.1（L26123–L26125）：`EnableJavaScripts` 存在时「The RH entry
    /// shall not be used in PDF 1.7」。`handler_count` 是 `RH` 数组声明的
    /// 元素数（含不可建模的元素）。
    RhUsedWithEnableJavaScripts { index: usize, handler_count: usize },
    /// Table 265：处理器 `Type` 存在但不是 name 对象。
    HandlerTypeNotName {
        index: usize,
        handler: usize,
        found: &'static str,
    },
    /// Table 265：处理器 `Type` 是 name 但值不是 `ReqHandler`（shall）。
    HandlerTypeNotReqHandler {
        index: usize,
        handler: usize,
        name: String,
    },
    /// Table 265：处理器 `S` 是 Required 条目，缺失即违例。
    HandlerSubtypeMissing { index: usize, handler: usize },
    /// Table 265：处理器 `S` 存在但不是 name 对象。
    HandlerSubtypeNotName {
        index: usize,
        handler: usize,
        found: &'static str,
    },
    /// Table 265：处理器 `S` shall 为 `JS` 或 `NoOp`；未定义名字原样保留。
    HandlerSubtypeNotPermitted {
        index: usize,
        handler: usize,
        name: String,
    },
    /// Table 265：`Script` 仅在 `S == JS` 时合法（valid only if）；
    /// 违例里仍保留解码后的原值（见 `RequirementHandler::script`）。
    ScriptOnlyForJavaScript { index: usize, handler: usize },
    /// Table 265：`Script` shall 是文本串（text string）。
    ScriptNotTextString {
        index: usize,
        handler: usize,
        found: &'static str,
    },
}

impl fmt::Display for RequirementsIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogEntryNotArray { found } => write!(
                f,
                "12.10.1 Table 3: catalogue /Requirements shall be an array of requirement dictionaries, found {found}"
            ),
            Self::EntryNotDictionary { index, found } => write!(
                f,
                "12.10.1: /Requirements entry {index} shall be a requirement dictionary, found {found}"
            ),
            Self::EntryUnresolvable { index, message } => write!(
                f,
                "12.10.1: /Requirements entry {index} cannot be resolved: {message}"
            ),
            Self::RequirementTypeNotName { index, found } => write!(
                f,
                "Table 264: requirement dictionary {index} /Type shall be a name, found {found}"
            ),
            Self::RequirementTypeNotRequirement { index, name } => write!(
                f,
                "Table 264: requirement dictionary {index} /Type shall be Requirement, got /{name}"
            ),
            Self::SubtypeMissing { index } => write!(
                f,
                "Table 264: requirement dictionary {index} is missing the required entry /S"
            ),
            Self::SubtypeNotName { index, found } => write!(
                f,
                "Table 264: requirement dictionary {index} /S shall be a name, found {found}"
            ),
            Self::SubtypeNotEnableJavaScripts { index, name } => write!(
                f,
                "12.10.1 Table 264: requirement dictionary {index} /S shall be EnableJavaScripts in PDF 1.7, got /{name}"
            ),
            Self::HandlersNotArray { index, found } => write!(
                f,
                "Table 264: requirement dictionary {index} /RH shall be an array of requirement handler dictionaries, found {found}"
            ),
            Self::HandlersUnresolvable { index, message } => write!(
                f,
                "Table 264: requirement dictionary {index} /RH cannot be resolved: {message}"
            ),
            Self::HandlerEntryNotDictionary {
                index,
                handler,
                found,
            } => write!(
                f,
                "Table 265: requirement dictionary {index} /RH entry {handler} shall be a requirement handler dictionary, found {found}"
            ),
            Self::HandlerEntryUnresolvable {
                index,
                handler,
                message,
            } => write!(
                f,
                "Table 265: requirement dictionary {index} /RH entry {handler} cannot be resolved: {message}"
            ),
            Self::RhUsedWithEnableJavaScripts {
                index,
                handler_count,
            } => write!(
                f,
                "12.10.1: the RH entry shall not be used in PDF 1.7, but requirement dictionary {index} pairs EnableJavaScripts with {handler_count} handler(s)"
            ),
            Self::HandlerTypeNotName {
                index,
                handler,
                found,
            } => write!(
                f,
                "Table 265: requirement handler {index}/{handler} /Type shall be a name, found {found}"
            ),
            Self::HandlerTypeNotReqHandler {
                index,
                handler,
                name,
            } => write!(
                f,
                "Table 265: requirement handler {index}/{handler} /Type shall be ReqHandler, got /{name}"
            ),
            Self::HandlerSubtypeMissing { index, handler } => write!(
                f,
                "Table 265: requirement handler {index}/{handler} is missing the required entry /S"
            ),
            Self::HandlerSubtypeNotName {
                index,
                handler,
                found,
            } => write!(
                f,
                "Table 265: requirement handler {index}/{handler} /S shall be a name, found {found}"
            ),
            Self::HandlerSubtypeNotPermitted {
                index,
                handler,
                name,
            } => write!(
                f,
                "Table 265: requirement handler {index}/{handler} /S shall be JS or NoOp, got /{name}"
            ),
            Self::ScriptOnlyForJavaScript { index, handler } => write!(
                f,
                "Table 265: requirement handler {index}/{handler} carries /Script although its /S is not JS (valid only if /S is JS)"
            ),
            Self::ScriptNotTextString {
                index,
                handler,
                found,
            } => write!(
                f,
                "Table 265: requirement handler {index}/{handler} /Script shall be a text string, found {found}"
            ),
        }
    }
}

impl RequirementsIssue {
    /// 违例所属需求字典在 `/Requirements` 数组中的下标；数组级违例
    /// （[`Self::CatalogEntryNotArray`]）没有下标，返回 `None`。
    pub fn requirement_index(&self) -> Option<usize> {
        match self {
            Self::CatalogEntryNotArray { .. } => None,
            Self::EntryNotDictionary { index, .. }
            | Self::EntryUnresolvable { index, .. }
            | Self::RequirementTypeNotName { index, .. }
            | Self::RequirementTypeNotRequirement { index, .. }
            | Self::SubtypeMissing { index, .. }
            | Self::SubtypeNotName { index, .. }
            | Self::SubtypeNotEnableJavaScripts { index, .. }
            | Self::HandlersNotArray { index, .. }
            | Self::HandlersUnresolvable { index, .. }
            | Self::RhUsedWithEnableJavaScripts { index, .. }
            | Self::HandlerEntryNotDictionary { index, .. }
            | Self::HandlerEntryUnresolvable { index, .. }
            | Self::HandlerTypeNotName { index, .. }
            | Self::HandlerTypeNotReqHandler { index, .. }
            | Self::HandlerSubtypeMissing { index, .. }
            | Self::HandlerSubtypeNotName { index, .. }
            | Self::HandlerSubtypeNotPermitted { index, .. }
            | Self::ScriptOnlyForJavaScript { index, .. }
            | Self::ScriptNotTextString { index, .. } => Some(*index),
        }
    }
}
