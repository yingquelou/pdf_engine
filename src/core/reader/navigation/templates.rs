//! 命名模板页读取（ISO 32000-1 12.7.6、Table 31 `/Templates`）。
//!
//! 目录名称字典的 `/Templates` 名称树把名称字符串映射到**不可见**
//! （模板）页面对象：`/Type` 为 `Template` 而非 `Page`，且没有 `/Parent`
//! 或 `/B` 条目。名称树遍历复用既有加固遍历（深度、节点数与环限制），
//! 键按 PDF 文本串（PDFDocEncoding 或 UTF-16BE）解码。

use std::collections::BTreeMap;

use crate::core::types::{Dict, Document, PdfRect, PdfTextString, Value};

use super::{NavigationError, malformed, resolve};

/// 名称树中的一个不可见模板页（12.7.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct TemplatePage {
    /// 模板页字典（间接引用已解引用；`/Type` 必为 `Template`）。
    pub dict: Dict,
    /// `/MediaBox` 矩形；缺失或不是合法矩形时为 `None`。
    pub media_box: Option<PdfRect>,
}

impl Document {
    /// 读取目录 `/Names /Templates` 名称树（12.7.6），按键排序返回。
    ///
    /// 名称树缺省返回空映射。每个条目须解析为字典且 `/Type` 为
    /// `Template`（缺失或错误类型返回 [`NavigationError::Malformed`]）；
    /// 名称树节点损坏、成环或超出遍历上限由既有加固遍历报类型化错误。
    /// 键按 PDF 文本串解码为 Unicode 文本。
    pub fn page_templates(&self) -> Result<BTreeMap<String, TemplatePage>, NavigationError> {
        let entries = self.name_tree("Templates")?;
        let mut templates = BTreeMap::new();
        for (raw_name, value) in entries {
            let name = decode_name(&raw_name);
            let value = resolve(self, &value)?;
            let dict = value
                .as_dict()
                .ok_or_else(|| malformed(format!("template page /{name} is not a dictionary")))?;
            if dict.get("Type").and_then(Value::as_name) != Some("Template") {
                return Err(malformed(format!(
                    "template page /{name} /Type is not Template"
                )));
            }
            let media_box = match dict.get("MediaBox") {
                Some(value) => {
                    let value = resolve(self, value)?;
                    PdfRect::from_value(&value).ok()
                }
                None => None,
            };
            if templates
                .insert(
                    name.clone(),
                    TemplatePage {
                        dict: dict.clone(),
                        media_box,
                    },
                )
                .is_some()
            {
                return Err(malformed(format!(
                    "name tree /Templates has duplicate name {name:?}"
                )));
            }
        }
        Ok(templates)
    }
}

/// 名称树键从「原始字节串」（每字节一个 `char`，见 `name_tree`）还原为
/// PDF 文本串；解码失败（未定义的 PDFDocEncoding 字节等）时保留原文。
fn decode_name(raw: &str) -> String {
    let bytes: Vec<u8> = raw.chars().map(|ch| ch as u32 as u8).collect();
    PdfTextString::decode(&bytes)
        .map(PdfTextString::into_string)
        .unwrap_or_else(|_| raw.to_owned())
}

#[cfg(test)]
mod tests;
