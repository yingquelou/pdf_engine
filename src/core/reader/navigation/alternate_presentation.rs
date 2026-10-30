//! 备用演示读取（ISO 32000-1 13.5、Table 297）。
//!
//! 目录名称字典的 `/AlternatePresentations` 名称树（PDF 1.4）把名称字符串
//! 映射到幻灯片字典。每个幻灯片须为 `/Type /SlideShow`、`/Subtype
//! /Embedded`，带一个 `/Resources` 名称树与指向其中某个资源键的
//! `/StartResource`。名称树键与 `/StartResource` 按 PDF 文本串解码（13.5
//! 注明幻灯片把名称树字符串解释为 UTF-8；与本库其余文本串路径一致，采用
//! PDFDocEncoding/UTF-16BE 解码，ASCII 名称与 UTF-8 等价）。名称树遍历
//! 复用既有加固遍历（深度、节点数与环限制），资源值原样保留为 [`Value`]。

use std::collections::BTreeMap;

use crate::core::types::{Dict, Document, PdfTextString, Value};

use super::{NavigationError, malformed, resolve};

/// 一个备用演示幻灯片（13.5 Table 297）。
#[derive(Debug, Clone, PartialEq)]
pub struct Slideshow {
    /// `/Resources` 名称树：解码后的资源名 → 已解引用的原始值。
    pub resources: BTreeMap<String, Value>,
    /// `/StartResource` 解码后的资源名（必为 `resources` 的键之一）。
    pub start_resource: String,
    /// 幻灯片字典（间接引用已解引用）。
    pub dict: Dict,
}

impl Document {
    /// 读取目录 `/Names /AlternatePresentations` 名称树（13.5），按键排序返回。
    ///
    /// 名称树缺省返回空映射。每个条目须解析为字典且 `/Type` 为
    /// `SlideShow`、`/Subtype` 为 `Embedded`，并带字典型 `/Resources`
    /// 名称树与匹配其中某个资源键的 `/StartResource`；违反任一条返回
    /// [`NavigationError::Malformed`]。资源值解引用后原样保留（不建模
    /// 文件规格或图像）。名称树节点损坏、成环或超出遍历上限由既有加固
    /// 遍历报类型化错误。
    pub fn alternate_presentations(&self) -> Result<BTreeMap<String, Slideshow>, NavigationError> {
        let entries = self.name_tree("AlternatePresentations")?;
        let mut shows = BTreeMap::new();
        for (raw_name, value) in entries {
            let name = decode_name(&raw_name);
            if shows.contains_key(&name) {
                return Err(malformed(format!(
                    "name tree /AlternatePresentations has duplicate name {name:?}"
                )));
            }
            let value = resolve(self, &value)?;
            let dict = value.as_dict().ok_or_else(|| {
                malformed(format!(
                    "alternate presentation /{name} is not a dictionary"
                ))
            })?;
            if dict.get("Type").and_then(Value::as_name) != Some("SlideShow") {
                return Err(malformed(format!(
                    "alternate presentation /{name} /Type is not SlideShow"
                )));
            }
            if dict.get("Subtype").and_then(Value::as_name) != Some("Embedded") {
                return Err(malformed(format!(
                    "alternate presentation /{name} /Subtype is not Embedded"
                )));
            }
            let resources_value = dict.get("Resources").ok_or_else(|| {
                malformed(format!("alternate presentation /{name} has no /Resources"))
            })?;
            let resources_value = resolve(self, resources_value)?;
            let resources_dict = resources_value.as_dict().ok_or_else(|| {
                malformed(format!(
                    "alternate presentation /{name} /Resources is not a dictionary"
                ))
            })?;
            let mut resources = BTreeMap::new();
            for (raw_key, resource) in super::name_tree::name_tree_entries(self, resources_dict)? {
                let key = decode_name(&raw_key);
                let resource = resolve(self, &resource)?;
                if resources.insert(key.clone(), resource).is_some() {
                    return Err(malformed(format!(
                        "alternate presentation /{name} /Resources has duplicate name {key:?}"
                    )));
                }
            }
            let start = dict.get("StartResource").ok_or_else(|| {
                malformed(format!(
                    "alternate presentation /{name} has no /StartResource"
                ))
            })?;
            let raw_start = super::name_tree::string_key(start).ok_or_else(|| {
                malformed(format!(
                    "alternate presentation /{name} /StartResource is not a string"
                ))
            })?;
            let start_resource = decode_name(&raw_start);
            if !resources.contains_key(&start_resource) {
                return Err(malformed(format!(
                    "alternate presentation /{name} /StartResource {start_resource:?} is not a /Resources key"
                )));
            }
            shows.insert(
                name,
                Slideshow {
                    resources,
                    start_resource,
                    dict: dict.clone(),
                },
            );
        }
        Ok(shows)
    }
}

/// 名称树键或 `/StartResource` 从「原始字节串」（每字节一个 `char`，见
/// `name_tree`）还原为 PDF 文本串；解码失败（未定义的 PDFDocEncoding
/// 字节等）时保留原文。与 `templates.rs` 的键解码一致。
fn decode_name(raw: &str) -> String {
    let bytes: Vec<u8> = raw.chars().map(|ch| ch as u32 as u8).collect();
    PdfTextString::decode(&bytes)
        .map(PdfTextString::into_string)
        .unwrap_or_else(|_| raw.to_owned())
}

#[cfg(test)]
mod tests;
