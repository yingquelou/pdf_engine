//! 命名模板页写端（ISO 32000-1 12.7.6、Table 31 `/Templates`）。
//!
//! 模板页是**不可见页面**：对象类型为 `/Type /Template` 而非 `/Page`，
//! 不进入页面树，也不写 `/Parent` 与 `/B`（12.7.6 明文要求）。名称字典
//! 的 `/Names /Templates` 名称树以 PDF 文本串为键指向这些模板页对象。
//!
//! 本切片只写出模板页字典本身（`/Type`、`/MediaBox` 与空的 `/Resources`），
//! **不生成内容流**，也不写 `/Contents`；模板页的实际内容需后续切片补齐。
//! 对象在 [`PdfBuilder::build`] 的导航后处理阶段从现有最大对象号之后分配，
//! 页面树 `/Kids` 与 `/Pages /Count` 不受影响。PDF 1.3 起可用。

use std::collections::BTreeSet;

use crate::core::builder::types::PageSize;
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, Value};
use crate::core::writer::WriteError;

use super::PdfBuilder;
use super::invalid;

/// 校验模板页名称与尺寸；在分配任何对象号之前调用，保持零字节输出契约。
///
/// 名称是名称树键（PDF 文本串）：须非空、不含 NUL 字符且互不重复。
/// 尺寸为 `/MediaBox` 的宽高，须为正的有限数（模板页不在页面树中，
/// 无法继承上级节点的 MediaBox，故必须显式写出）。
pub(super) fn validate(templates: &[(String, PageSize)]) -> Result<(), WriteError> {
    let mut names = BTreeSet::new();
    for (name, size) in templates {
        if name.is_empty() {
            return Err(invalid("template page name is empty"));
        }
        if name.contains('\0') {
            return Err(invalid(format!(
                "template page name {name:?} contains a NUL character"
            )));
        }
        if !names.insert(name.as_str()) {
            return Err(invalid(format!("duplicate template page name {name:?}")));
        }
        let (width, height) = size.dimensions();
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(invalid(format!(
                "template page {name:?} MediaBox must be positive and finite"
            )));
        }
    }
    Ok(())
}

/// 写出模板页对象，返回按注册顺序排列的间接引用。
///
/// 对象号从当前文档最大对象号之后分配；调用方随后用返回值构造
/// `/Names /Templates` 名称树。模板页字典不含 `/Parent` 与 `/B`。
pub(super) fn write(doc: &mut Document, templates: &[(String, PageSize)]) -> Vec<Value> {
    let first_num = doc
        .objects_for_write()
        .iter()
        .map(|object| object.num)
        .max()
        .unwrap_or(0)
        + 1;
    let mut refs = Vec::with_capacity(templates.len());
    for (offset, (_, size)) in templates.iter().enumerate() {
        let num = first_num + offset as i64;
        let (width, height) = size.dimensions();
        let media_box = Array::new()
            .push(Value::Float(0.0))
            .push(Value::Float(0.0))
            .push(Value::Float(width))
            .push(Value::Float(height));
        // 页面对象要求 /Resources（Table 30）；模板页无法从页面树继承，
        // 因此写空字典。本切片没有内容流，空资源即可。
        let dict = Dict::new()
            .insert("Type", Value::Name("Template".into()))
            .insert("MediaBox", Value::Array(media_box))
            .insert("Resources", Value::Dict(Dict::new()));
        doc.set_object(Object::new(num, 0, Value::Dict(dict)));
        refs.push(Value::Ref(num, 0));
    }
    doc.features.borrow_mut().insert(PdfFeature::PageTemplates);
    refs
}

impl PdfBuilder {
    /// 注册一个命名模板页（12.7.6），写入目录 `/Names /Templates` 名称树，
    /// 返回注册顺序下标。
    ///
    /// 模板页是不可见页面：以 `/Type /Template` 的间接对象写出，`/MediaBox`
    /// 取自 `size`，**不含** `/Parent` 与 `/B`，也不进入页面树（`/Pages`
    /// 的 `/Kids` 与 `/Count` 不变）。本切片不生成内容流，模板页没有
    /// `/Contents`。名称为名称树键（PDF 文本串），须非空、不含 NUL 且
    /// 互不重复，否则构建返回 [`WriteError::InvalidObject`]；目标版本低于
    /// 1.3 返回 [`WriteError::UnsupportedVersion`]，均零字节输出。
    pub fn add_template(&mut self, name: impl Into<String>, size: PageSize) -> usize {
        let handle = self.navigation.templates.len();
        self.navigation.templates.push((name.into(), size));
        handle
    }
}
