//! 文档需求（ISO 32000-1 12.10）的写端类型：requirement 字典
//! （12.10.1 Table 264）与 requirement handler 字典（12.10.2 Table 265）。
//!
//! 规范依据（`docs/PDF32000_2008.pdf`，`pdftotext -raw` 行号）：
//!
//! - 12.10.1（raw L26093–26125）：「Beginning with PDF 1.7, a document may
//!   specify requirements...The Requirements entry in the document catalogue
//!   ... shall specify an array of requirement dictionaries」（Table 264）
//!   `Type` 可选、存在时 shall 为 `Requirement`；`S` 必需（Required）、名字，
//!   「In PDF 1.7, the only defined requirement type shall be
//!   EnableJavaScripts」；`RH` 可选 = requirement handler 字典数组。
//! - Table 265（raw L26126–26146）：handler 字典的 `Type` 存在时 shall 为
//!   `ReqHandler`；`S` 必需，合法值 shall be `JS`（JavaScript handler）
//!   与 `NoOp`（「This value does not add any specific entry to the
//!   requirement handler dictionary」）；`Script` 可选、**文本串**、
//!   「valid only if the S entry has a value of JS」，其值是存放在文档名称
//!   字典（7.7.4）里的**文档级 JavaScript 动作的名字**，不是动作本体。
//! - 目录 Table 28（raw L3692）：`/Requirements` 条目 = Optional; PDF 1.7。
//!
//! # 由规范推出的写端口径（不是规范明文，而是 shall 的组合后果）
//!
//! raw L26125 原文：「The RH entry shall not be used in PDF 1.7」。
//! 而 `/Requirements` 目录条目只在 PDF 1.7 及以后才被允许存在，于是
//! **能携带 `/Requirements` 的版本上 `RH` 都不得写**：两者互斥。
//! [`DocumentRequirement`] 因此仍可承载 handler（模型保真，供原值回写与
//! 低版本实验），但默认写出路径由
//! [`crate::core::builder::navigation::requirements`] 的构建期校验一律
//! 拒绝非空 `RH`，零字节报错，不为「支持 RH」编造低版本合法性。
//! 规范给出的理由同样支持这一处置：`EnableJavaScripts` 是唯一已定义
//! requirement 类型，而 raw L26121–26124 说「the inclusion of the RH entry
//! that specifies a JavaScript segment would be pointless. Writing a
//! JavaScript segment to verify that JavaScript is enabled would not
//! achieve the desired goal」。
//!
//! # 超出规范的本库政策
//!
//! - `Type` 与 `S` 由类型化模型单点产出（本模块是唯一序列化路径），调用方
//!   拼不出非法值；写出前再对**已序列化**的数组做一次形态自检
//!   （`navigation::requirements::validate_written`），把 shall 级
//!   约束显式落实。
//! - `Script` 名字只要求非空：规范只说它是名称树里的键，未给字符集限制。

use crate::core::types::{Array, Dict, Value};

use super::text_string_value;

/// Table 264 `Type`：存在时 shall 为 `Requirement`。
pub(crate) const REQUIREMENT_TYPE: &str = "Requirement";

/// Table 265 `Type`：存在时 shall 为 `ReqHandler`。
pub(crate) const HANDLER_TYPE: &str = "ReqHandler";

/// requirement 字典的 `S`（Table 264）。ISO 32000-1 只定义一个值
/// （12.10.1：「the only defined requirement type shall be
/// EnableJavaScripts」），用枚举把闭集写进类型而不是自由字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentRequirementType {
    /// `EnableJavaScripts`：文档要求阅读器启用 JavaScript 执行。
    EnableJavaScripts,
}

impl DocumentRequirementType {
    /// 写出用的名字。
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::EnableJavaScripts => "EnableJavaScripts",
        }
    }
}

/// requirement handler 字典的 `S`（Table 265）：闭集 `JS` / `NoOp`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequirementHandlerType {
    /// `JS`：JavaScript handler，必须配 `/Script` 指向文档级 JavaScript
    /// 动作的名字。
    JavaScript,
    /// `NoOp`：让旧阅读器忽略未识别需求；不附加任何专有条目。
    NoOp,
}

impl RequirementHandlerType {
    /// 写出用的名字。
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::JavaScript => "JS",
            Self::NoOp => "NoOp",
        }
    }
}

/// requirement handler 字典（12.10.2 Table 265）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementHandler {
    /// `/S` handler 类型。
    pub handler_type: RequirementHandlerType,
    /// `/Script` 文档级 JavaScript 动作在名称树里的名字；仅 `JS` 合法。
    pub script: Option<String>,
}

impl RequirementHandler {
    /// JavaScript handler：`/S /JS` + `/Script (script)`。
    pub fn javascript(script: impl Into<String>) -> Self {
        Self {
            handler_type: RequirementHandlerType::JavaScript,
            script: Some(script.into()),
        }
    }

    /// `NoOp` handler：`/S /NoOp`，不写 `/Script`。
    pub fn no_op() -> Self {
        Self {
            handler_type: RequirementHandlerType::NoOp,
            script: None,
        }
    }

    /// 替换 `/Script` 名字；传 `None` 即省略该条目。
    pub fn with_script(mut self, script: Option<impl Into<String>>) -> Self {
        self.script = script.map(Into::into);
        self
    }
}

/// requirement 字典（12.10.1 Table 264）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRequirement {
    /// `/S` requirement 类型。
    pub requirement_type: DocumentRequirementType,
    /// `/RH` handler 数组；默认写出路径拒绝非空（见模块文档）。
    pub handlers: Vec<RequirementHandler>,
}

impl DocumentRequirement {
    /// `EnableJavaScripts` requirement（12.10.1 唯一已定义类型），不带
    /// handler。
    pub fn enable_javascripts() -> Self {
        Self {
            requirement_type: DocumentRequirementType::EnableJavaScripts,
            handlers: Vec::new(),
        }
    }

    /// 新建 requirement 并挂上 handler 数组——仅为模型保真保留的入口；
    /// raw L26125「The RH entry shall not be used in PDF 1.7」使默认写出
    /// 路径在任何可携带 `/Requirements` 的版本上都拒绝它。
    pub fn with_handlers(mut self, handlers: Vec<RequirementHandler>) -> Self {
        self.handlers = handlers;
        self
    }
}

/// 单个 requirement 字典 → 直接对象值（Table 264）。
pub(crate) fn requirement_value(requirement: &DocumentRequirement) -> Value {
    let mut dict = Dict::new()
        .insert("Type", Value::Name(REQUIREMENT_TYPE.into()))
        .insert("S", Value::Name(requirement.requirement_type.name().into()));
    if !requirement.handlers.is_empty() {
        let handlers = requirement.handlers.iter().map(handler_value).collect();
        dict = dict.insert("RH", Value::Array(Array(handlers)));
    }
    Value::Dict(dict)
}

/// 单个 handler 字典 → 直接对象值（Table 265）。
pub(crate) fn handler_value(handler: &RequirementHandler) -> Value {
    let mut dict = Dict::new()
        .insert("Type", Value::Name(HANDLER_TYPE.into()))
        .insert("S", Value::Name(handler.handler_type.name().into()));
    if let Some(script) = &handler.script {
        // Table 265：`Script` 类型是 text string（不是 name），值是名称树
        // 里文档级 JavaScript 动作的键。
        dict = dict.insert("Script", text_string_value(script));
    }
    Value::Dict(dict)
}
