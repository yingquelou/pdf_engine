//! 输出意图（ISO 32000-1 14.11.5、Table 365）。
//!
//! 目录 `/OutputIntents`（PDF 1.4，可选）是输出意图字典数组，声明文档
//! 交付时的预期输出条件。本模块读取 `/Type` `/S` `/OutputCondition`
//! `/OutputConditionIdentifier` `/RegistryName` `/Info` 与
//! `/DestOutputProfile`（ICC 配置文件流，格式同 ICCBased 颜色空间），
//! 其余条目保留在原始字典中。标准生产条件之外是否必须带 `/Info` 与
//! `/DestOutputProfile` 依赖工作流知识，读端不强制；`Custom` 标记的
//! 条件性要求仅由写端校验。

use std::fmt;

use crate::core::types::{Dict, Document, PdfTextString, Value};

use super::navigation::{catalog, resolve};

/// `/OutputIntents` 数组条目上限。
const MAX_OUTPUT_INTENTS: usize = 64;

/// `/S` 输出意图子类型（Table 365）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputIntentSubtype {
    /// `GTS_PDFX`：PDF/X 系列。
    GtsPdfX,
    /// `GTS_PDFA1`：PDF/A-1。
    GtsPdfA1,
    /// `ISO_PDFE1`：PDF/E-1。
    IsoPdfE1,
    /// 扩展定义名称（Annex E 命名），原样保留。
    Other(String),
}

impl OutputIntentSubtype {
    /// 按 `/S` 名称解析；未知名称保留为 [`OutputIntentSubtype::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "GTS_PDFX" => Self::GtsPdfX,
            "GTS_PDFA1" => Self::GtsPdfA1,
            "ISO_PDFE1" => Self::IsoPdfE1,
            other => Self::Other(other.to_owned()),
        }
    }

    /// `/S` 名称（标准名称或扩展原文）。
    pub fn name(&self) -> &str {
        match self {
            Self::GtsPdfX => "GTS_PDFX",
            Self::GtsPdfA1 => "GTS_PDFA1",
            Self::IsoPdfE1 => "ISO_PDFE1",
            Self::Other(name) => name,
        }
    }
}

/// `/DestOutputProfile` 输出配置文件流。
///
/// 按 Table 365 与 ICCBased 颜色空间同格式：`/N` 为颜色分量数（1–4），
/// `data` 为解码后的流字节。
#[derive(Debug, Clone, PartialEq)]
pub struct OutputProfile {
    /// 流对象的间接引用（直接流为 `None`）。
    pub reference: Option<(i64, i64)>,
    /// `/N` 颜色分量数。
    pub components: u8,
    /// 解码后的 ICC 配置文件字节。
    pub data: Vec<u8>,
}

/// 一个输出意图字典（Table 365）。
#[derive(Debug, Clone, PartialEq)]
pub struct OutputIntent {
    /// `/S` 子类型。
    pub subtype: OutputIntentSubtype,
    /// `/OutputCondition` 人类可读输出条件。
    pub output_condition: Option<String>,
    /// `/OutputConditionIdentifier`（必填）。
    pub output_condition_identifier: String,
    /// `/RegistryName` 注册表 URI。
    pub registry_name: Option<String>,
    /// `/Info` 补充说明。
    pub info: Option<String>,
    /// `/DestOutputProfile` ICC 配置文件流。
    pub destination_profile: Option<OutputProfile>,
    /// 原始条目字典（保留未建模条目）。
    pub dict: Dict,
}

/// 输出意图读取的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum OutputIntentError {
    /// 目录或条目结构不满足 Table 365。
    Malformed(String),
    /// 条目数超过上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for OutputIntentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "invalid output intent: {message}"),
            Self::LimitExceeded(what) => write!(f, "output intent limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for OutputIntentError {}

fn malformed(message: impl Into<String>) -> OutputIntentError {
    OutputIntentError::Malformed(message.into())
}

/// 解码文本串：字面字符串按原始字节、十六进制字符串先解十六进制，
/// 再按 PDFDocEncoding/UTF-16BE 解释。
fn text_value(value: &Value) -> Result<String, String> {
    let bytes = match value {
        Value::String(text) => text
            .chars()
            .map(|character| character as u32 as u8)
            .collect(),
        Value::HexString(hex) => {
            hex_bytes(hex).ok_or_else(|| "hex string has invalid digits".to_owned())?
        }
        other => return Err(format!("expected a text string, got {}", other.type_name())),
    };
    PdfTextString::decode(&bytes)
        .map(PdfTextString::into_string)
        .map_err(|error| format!("invalid text string encoding: {error}"))
}

fn required_text(dict: &Dict, key: &str, index: usize) -> Result<String, OutputIntentError> {
    let Some(value) = dict.get(key) else {
        return Err(malformed(format!("output intent {index} has no /{key}")));
    };
    text_value(value).map_err(|error| malformed(format!("output intent {index} /{key}: {error}")))
}

fn optional_text(
    dict: &Dict,
    key: &str,
    index: usize,
) -> Result<Option<String>, OutputIntentError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    text_value(value)
        .map(Some)
        .map_err(|error| malformed(format!("output intent {index} /{key}: {error}")))
}

impl Document {
    /// 读取目录 `/OutputIntents`（14.11.5 Table 365）。
    ///
    /// 目录缺失或不是字典、`/OutputIntents` 不是数组、条目结构违反
    /// Table 365 时返回 [`OutputIntentError::Malformed`]；条目数超过 64
    /// 返回 [`OutputIntentError::LimitExceeded`]。条目缺失时返回空列表。
    pub fn output_intents(&self) -> Result<Vec<OutputIntent>, OutputIntentError> {
        let catalog = catalog(self).map_err(|error| malformed(error.to_string()))?;
        let Some(value) = catalog.get("OutputIntents") else {
            return Ok(Vec::new());
        };
        let value =
            resolve(self, value).map_err(|error| malformed(format!("/OutputIntents: {error}")))?;
        let array = value
            .as_array()
            .ok_or_else(|| malformed("/OutputIntents is not an array"))?;
        if array.len() > MAX_OUTPUT_INTENTS {
            return Err(OutputIntentError::LimitExceeded("output intents"));
        }

        let mut intents = Vec::with_capacity(array.len());
        for (index, entry) in array.iter().enumerate() {
            let entry = resolve(self, entry)
                .map_err(|error| malformed(format!("output intent {index}: {error}")))?;
            let dict = entry
                .as_dict()
                .ok_or_else(|| malformed(format!("output intent {index} is not a dictionary")))?
                .clone();
            if let Some(value) = dict.get("Type")
                && value.as_name() != Some("OutputIntent")
            {
                return Err(malformed(format!(
                    "output intent {index} /Type is not OutputIntent"
                )));
            }
            let subtype = dict
                .get("S")
                .and_then(Value::as_name)
                .map(OutputIntentSubtype::from_name)
                .ok_or_else(|| malformed(format!("output intent {index} has no /S name")))?;
            let output_condition = optional_text(&dict, "OutputCondition", index)?;
            let output_condition_identifier =
                required_text(&dict, "OutputConditionIdentifier", index)?;
            let registry_name = optional_text(&dict, "RegistryName", index)?;
            let info = optional_text(&dict, "Info", index)?;
            let destination_profile = match dict.get("DestOutputProfile") {
                Some(value) => Some(self.output_profile(value, index)?),
                None => None,
            };
            intents.push(OutputIntent {
                subtype,
                output_condition,
                output_condition_identifier,
                registry_name,
                info,
                destination_profile,
                dict,
            });
        }
        Ok(intents)
    }

    /// 解析 `/DestOutputProfile`：须为流，`/N` 为 1–4 的整数。
    fn output_profile(
        &self,
        value: &Value,
        index: usize,
    ) -> Result<OutputProfile, OutputIntentError> {
        let reference = value.as_ref();
        let value = resolve(self, value).map_err(|error| {
            malformed(format!("output intent {index} /DestOutputProfile: {error}"))
        })?;
        let Value::Stream(dict, _) = &value else {
            return Err(malformed(format!(
                "output intent {index} /DestOutputProfile is not a stream"
            )));
        };
        let components = dict.get("N").and_then(Value::as_integer).ok_or_else(|| {
            malformed(format!(
                "output intent {index} /DestOutputProfile has no integer /N"
            ))
        })?;
        if !(1..=4).contains(&components) {
            return Err(malformed(format!(
                "output intent {index} /DestOutputProfile /N must be 1 to 4 components, got {components}"
            )));
        }
        let data = self.decode_stream_bytes(&value).map_err(|error| {
            malformed(format!("output intent {index} /DestOutputProfile: {error}"))
        })?;
        Ok(OutputProfile {
            reference,
            components: components as u8,
            data,
        })
    }
}

/// 十六进制字符串 → 字节；奇数位补 `0`，非法数字返回 `None`。
fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = hex
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let padded;
    let pairs = if digits.len() % 2 == 1 {
        padded = {
            let mut value = digits.clone();
            value.push(b'0');
            value
        };
        &padded[..]
    } else {
        &digits[..]
    };
    let mut bytes = Vec::with_capacity(pairs.len() / 2);
    for pair in pairs.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        bytes.push((high * 16 + low) as u8);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests;
