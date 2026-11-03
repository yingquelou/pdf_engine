//! 文档需求（ISO 32000-1 12.10）写端入口：把 requirement 数组写进目录
//! `/Requirements`。
//!
//! 目录条目本身来自 Table 28（raw L3692）：`/Requirements` = **Optional;
//! PDF 1.7**；12.10.1（raw L26093）另以 shall 句式规定「The Requirements
//! entry in the document catalogue ... shall specify an array of requirement
//! dictionaries」。因此本模块：
//!
//! - 全部 requirement / handler 字典按**直接对象**嵌套写出（规范未要求间接
//!   引用，Table 264/265 只给字典条目），不分配任何对象号；
//! - 目标版本低于 PDF 1.7 携带 `/Requirements` →
//!   [`WriteError::UnsupportedVersion`]：门槛由 `mod.rs` 用
//!   `version.supports(PdfFeature::DocumentRequirements)` 判定（该变体的
//!   `min_version()` 即 Table 28 raw L3692 的 1.7），写出时同步把该特征
//!   登记进文档画像，读写共用同一口径；
//! - 默认写出路径**一律拒绝 `/RH`** → [`WriteError::InvalidObject`]：
//!   raw L26125「The RH entry shall not be used in PDF 1.7」，而低于 1.7
//!   又携带不了 `/Requirements`，两者互斥 ⇒ 没有任何合法版本能写出 `/RH`。
//!   [`DocumentRequirement::with_handlers`] 只是模型保真的入口（原值回写、
//!   低版本实验），不是可写出的通道。
//!
//! 构建期校验全部落在 [`apply`] 改写目录、写出任何字节之前，失败即零字节
//! 输出，与既有车道口径一致。校验分两层：
//!
//! 1. [`validate`]：类型化模型层面——handler 的 `S`↔`Script` 联动（Table
//!    265「valid only if the S entry has a value of JS」，故 `JS` 必带、
//!    `NoOp` 禁带）、`Script` 名字非空、非空 `RH` 拒绝。
//! 2. [`validate_written`]：对**已序列化**的数组再自检一遍 shall 级形态——
//!    数组非空（**本库政策**：规范只说它是 requirement 字典数组，未明写
//!    非空）、元素必须是字典、`Type` 存在时 shall 为 `Requirement` /
//!    `ReqHandler`、`S` 必需且取值受限（`EnableJavaScripts` / `JS` /
//!    `NoOp`）、`Script` 只允许是非空文本串且只在 `S` 为 `JS` 时出现。写端
//!    类型（`types::requirements`）是唯一序列化路径，这层自检保证模型日后
//!    扩展也不会发出非名字典。

use crate::core::builder::types::requirements::{
    DocumentRequirement, DocumentRequirementType, HANDLER_TYPE, REQUIREMENT_TYPE,
    RequirementHandler, RequirementHandlerType, requirement_value,
};
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

use super::PdfBuilder;

/// [`WriteError::InvalidObject`] 加统一前缀，便于区分车道来源。
fn invalid(message: impl Into<String>) -> WriteError {
    WriteError::InvalidObject(format!("document requirements: {}", message.into()))
}

/// 校验待写出的 requirement 数组（模型层）；在任何目录改写之前调用。
///
/// `/Script`↔`S` 联动不成立（Table 265）、`/Script` 为空串、以及任何非空
/// `RH` 均返回 [`WriteError::InvalidObject`]。handler 形态先于 `RH` 整体
/// 拒绝报错：前者给出可操作的细粒度原因，后者是无解的版本约束。
pub(in crate::core::builder) fn validate(
    requirements: &[DocumentRequirement],
) -> Result<(), WriteError> {
    for (index, requirement) in requirements.iter().enumerate() {
        for (handler_index, handler) in requirement.handlers.iter().enumerate() {
            validate_handler_model(index, handler_index, handler)?;
        }
        if !requirement.handlers.is_empty() {
            return Err(invalid(format!(
                "requirement {index} carries an /RH array; the RH entry shall not be used in PDF 1.7 (12.10.1, raw L26125) while PDF 1.7 is the first version that may hold the catalog /Requirements entry (Table 28, raw L3692), so no conforming writer may emit /RH"
            )));
        }
    }
    Ok(())
}

/// 单个 handler 的模型校验（Table 265 的 `S`↔`Script` 联动）。
fn validate_handler_model(
    requirement: usize,
    handler: usize,
    dict: &RequirementHandler,
) -> Result<(), WriteError> {
    match (dict.handler_type, dict.script.as_deref()) {
        (RequirementHandlerType::JavaScript, None) => Err(invalid(format!(
            "requirement {requirement} handler {handler} has /S /JS but no /Script; Table 265 makes /Script the name of the document-level JavaScript action to run"
        ))),
        (RequirementHandlerType::JavaScript, Some("")) => Err(invalid(format!(
            "requirement {requirement} handler {handler} has an empty /Script name; it must key a document-level JavaScript action in the document name dictionary (7.7.4)"
        ))),
        (RequirementHandlerType::NoOp, Some(_)) => Err(invalid(format!(
            "requirement {requirement} handler {handler} has /S /NoOp but also a /Script; Table 265: /NoOp does not add any specific entry and /Script is valid only if /S is JS"
        ))),
        _ => Ok(()),
    }
}

/// 目录 `/Requirements` 的值：直接对象数组 + 写出前自检。
///
/// 调用方保证 `requirements` 非空且已通过 [`validate`]。
pub(super) fn catalog_value(requirements: &[DocumentRequirement]) -> Result<Value, WriteError> {
    let items = requirements.iter().map(requirement_value).collect();
    let array = Value::Array(Array(items));
    validate_written(&array)?;
    Ok(array)
}

/// 对已序列化数组的 shall 级形态自检（Table 264/265）。
pub(in crate::core::builder) fn validate_written(value: &Value) -> Result<(), WriteError> {
    let Some(array) = value.as_array() else {
        return Err(invalid(format!(
            "the assembled /Requirements value must be an array (12.10.1), got {}",
            value.type_name()
        )));
    };
    if array.0.is_empty() {
        return Err(invalid(
            "the /Requirements array holds no requirement dictionary; an empty array carries no requirement at all, so this writer rejects it (12.10.1 describes the entry as an array of requirement dictionaries; the non-empty bound is this library's policy, not a shall)",
        ));
    }
    for (index, element) in array.0.iter().enumerate() {
        let Some(dict) = element.as_dict() else {
            return Err(invalid(format!(
                "requirement {index} must be a dictionary, got {}",
                element.type_name()
            )));
        };
        check_type(dict, REQUIREMENT_TYPE, "requirement", index)?;
        match dict.get("S").and_then(Value::as_name) {
            Some(name) if name == DocumentRequirementType::EnableJavaScripts.name() => {}
            Some(name) => {
                return Err(invalid(format!(
                    "requirement {index} has /S /{name}; the only defined requirement type shall be EnableJavaScripts (12.10.1)"
                )));
            }
            None => {
                return Err(invalid(format!(
                    "requirement {index} has no /S entry; Table 264 marks it Required"
                )));
            }
        }
        if let Some(rh) = dict.get("RH") {
            let Some(handlers) = rh.as_array() else {
                return Err(invalid(format!(
                    "requirement {index} /RH must be an array of requirement handler dictionaries (Table 264), got {}",
                    rh.type_name()
                )));
            };
            for (handler_index, handler) in handlers.0.iter().enumerate() {
                validate_written_handler(index, handler_index, handler)?;
            }
        }
    }
    Ok(())
}

/// 单个已序列化 handler 字典的形态自检（Table 265）。
fn validate_written_handler(
    requirement: usize,
    handler: usize,
    value: &Value,
) -> Result<(), WriteError> {
    let label = format!("requirement {requirement} handler {handler}");
    let Some(dict) = value.as_dict() else {
        return Err(invalid(format!(
            "{label} must be a dictionary, got {}",
            value.type_name()
        )));
    };
    check_type(dict, HANDLER_TYPE, "handler", handler)?;
    let name = match dict.get("S").and_then(Value::as_name) {
        Some(name) => name,
        None => {
            return Err(invalid(format!(
                "{label} has no /S entry; Table 265 marks it Required"
            )));
        }
    };
    let is_js = match name {
        "JS" => true,
        "NoOp" => false,
        other => {
            return Err(invalid(format!(
                "{label} has /S /{other}; valid requirement handler types shall be JS and NoOp (Table 265)"
            )));
        }
    };
    match dict.get("Script") {
        None if is_js => Err(invalid(format!(
            "{label} has /S /JS but no /Script (Table 265)"
        ))),
        None => Ok(()),
        Some(_) if !is_js => Err(invalid(format!(
            "{label} has a /Script entry while /S is /NoOp; Table 265: /Script is valid only if the S entry has a value of JS"
        ))),
        Some(script) if is_text_string(script) && !text_is_empty(script) => Ok(()),
        Some(script) => Err(invalid(format!(
            "{label} /Script must be a non-empty text string naming a document-level JavaScript action (Table 265, 7.7.4), got a {}",
            script.type_name()
        ))),
    }
}

/// `Type` 若存在须精确等于规范值（shall 级拼写）。
fn check_type(dict: &Dict, expected: &str, kind: &str, index: usize) -> Result<(), WriteError> {
    match dict.get("Type") {
        None => Ok(()),
        Some(value) if value.as_name() == Some(expected) => Ok(()),
        Some(value) => Err(invalid(format!(
            "{kind} {index} has /Type {}; when present it shall be {expected} (Table 264/265)",
            match value.as_name() {
                Some(name) => format!("/{name}"),
                None => format!("a {} value", value.type_name()),
            }
        ))),
    }
}

/// Table 265 的文本串类型（字面串、十六进制串、ASCII85 串）。
fn is_text_string(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_) | Value::HexString(_) | Value::Ascii85String(_)
    )
}

/// 文本串内容是否为空。
fn text_is_empty(value: &Value) -> bool {
    match value {
        Value::String(text) | Value::HexString(text) | Value::Ascii85String(text) => {
            text.is_empty()
        }
        _ => true,
    }
}

impl PdfBuilder {
    /// 追加一个文档需求（12.10），写入目录 `/Requirements` 数组。
    ///
    /// 每个 [`DocumentRequirement`] 写成一个直接对象 requirement 字典
    /// （Table 264）：`/Type /Requirement` 与 `/S`。12.10.1 里 PDF 1.7
    /// 唯一已定义的 requirement 类型是 `EnableJavaScripts`，由
    /// [`DocumentRequirement::enable_javascripts`] 给出；handler 字典
    /// （Table 265）由 [`RequirementHandler`] 描述。
    ///
    /// 构建期失败全部零字节输出：目标版本低于 1.7 返回
    /// [`WriteError::UnsupportedVersion`]（Table 28 raw L3692 把
    /// `/Requirements` 标为 PDF 1.7）；数组为空、handler 的 `S`↔`Script`
    /// 联动不成立（Table 265）、`/Script` 名字为空、以及**任何非空 `/RH`**
    /// 返回 [`WriteError::InvalidObject`]——后者由 raw L26125「The RH entry
    /// shall not be used in PDF 1.7」与 `/Requirements` 的 1.7 门槛互斥推出，
    /// 故 [`DocumentRequirement::with_handlers`] 在本库没有可写出的版本。
    /// 全部内容嵌套在目录内，不分配间接对象号。
    pub fn add_document_requirement(&mut self, requirement: DocumentRequirement) -> &mut Self {
        self.navigation.document_requirements.push(requirement);
        self
    }
}
