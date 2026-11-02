//! 备用演示写端（ISO 32000-1 13.5、Table 297）。
//!
//! 每个幻灯片写成独立间接对象：`/Type /SlideShow`、`/Subtype /Embedded`、
//! 直接 `/Resources` 名称树（资源键 → 目录 `/Names /EmbeddedFiles` 中
//! 对应文件规格的引用）与 `/StartResource` 文本串。对象在
//! [`PdfBuilder::build`] 的导航后处理阶段从现有最大对象号之后分配；目录
//! `/Names /AlternatePresentations` 名称树与既有类别合并。资源名称树的
//! 键按 PDF 文本串编码（ASCII 与 13.5 的 UTF-8 解释一致）。PDF 1.4 起可用。
//!
//! 校验在分配任何对象号之前完成：幻灯片名称非空且唯一、`/StartResource`
//! 非空且已声明、资源键非空且唯一，且每个资源引用的嵌入文件名必须存在于
//! 已组装目录的 `/Names /EmbeddedFiles` 名称树。非法输入返回
//! [`WriteError::InvalidObject`]，零字节输出。

use std::collections::{BTreeMap, BTreeSet};

use crate::core::types::{Dict, Document, Object, PdfFeature, PdfTextString, Value};
use crate::core::writer::WriteError;

use super::super::types::text_string_value;
use super::PdfBuilder;
use super::{invalid, name_tree};

/// 写端幻灯片配置（13.5 Table 297）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slideshow {
    /// 名称树键（`/Names /AlternatePresentations` 中的显示名）。
    pub name: String,
    /// `/StartResource` 文本串，须为 `resources` 中的资源键。
    pub start_resource: String,
    /// 资源键 → 嵌入文件名（目录 `/Names /EmbeddedFiles` 名称树的键）。
    pub resources: Vec<(String, String)>,
}

impl Slideshow {
    /// 新建幻灯片配置；资源列表为空。
    pub fn new(name: impl Into<String>, start_resource: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start_resource: start_resource.into(),
            resources: Vec::new(),
        }
    }

    /// 追加一个资源：资源键与它引用的嵌入文件名。
    pub fn with_resource(
        mut self,
        name: impl Into<String>,
        embedded_file_name: impl Into<String>,
    ) -> Self {
        self.resources
            .push((name.into(), embedded_file_name.into()));
        self
    }
}

/// 校验幻灯片配置与嵌入文件引用；在分配任何对象号之前调用。
///
/// `embedded` 是已组装目录 `/Names /EmbeddedFiles` 名称树的解码结果
/// （嵌入文件名 → 文件规格值）。
pub(super) fn validate(
    shows: &[Slideshow],
    embedded: &BTreeMap<String, Value>,
) -> Result<(), WriteError> {
    let mut names = BTreeSet::new();
    for show in shows {
        if show.name.is_empty() {
            return Err(invalid("alternate presentation name is empty"));
        }
        if !names.insert(show.name.as_str()) {
            return Err(invalid(format!(
                "duplicate alternate presentation name {:?}",
                show.name
            )));
        }
        if show.start_resource.is_empty() {
            return Err(invalid(format!(
                "alternate presentation {:?} start resource is empty",
                show.name
            )));
        }
        let mut keys = BTreeSet::new();
        for (key, embedded_name) in &show.resources {
            if key.is_empty() {
                return Err(invalid(format!(
                    "alternate presentation {:?} resource name is empty",
                    show.name
                )));
            }
            if !keys.insert(key.as_str()) {
                return Err(invalid(format!(
                    "alternate presentation {:?} declares resource {key:?} twice",
                    show.name
                )));
            }
            if !embedded.contains_key(embedded_name) {
                return Err(invalid(format!(
                    "alternate presentation {:?} resource {key:?} references unknown embedded file {embedded_name:?}",
                    show.name
                )));
            }
        }
        if !keys.contains(show.start_resource.as_str()) {
            return Err(invalid(format!(
                "alternate presentation {:?} start resource {:?} is not a declared resource",
                show.name, show.start_resource
            )));
        }
    }
    Ok(())
}

/// 读取已组装目录 `/Names /EmbeddedFiles` 名称树：解码文件名 → 文件规格值。
///
/// 导航后处理在 assemble 写出嵌入文件之后运行，因此可直接从目录读取；
/// 名称树节点与条目在构建器产物中为直接对象，这里仍按引用解析以兼容
/// 手写文档。
pub(super) fn catalog_embedded_files(
    doc: &Document,
    catalog: &Dict,
) -> Result<BTreeMap<String, Value>, WriteError> {
    let mut files = BTreeMap::new();
    let Some(names) = catalog.get("Names") else {
        return Ok(files);
    };
    let names = resolve_object(doc, names)?;
    let names = names
        .as_dict()
        .ok_or_else(|| invalid("catalog /Names is not a dictionary"))?;
    let Some(node) = names.get("EmbeddedFiles") else {
        return Ok(files);
    };
    let node = resolve_object(doc, node)?;
    let node = node
        .as_dict()
        .ok_or_else(|| invalid("/Names /EmbeddedFiles is not a dictionary"))?;
    let Some(entries) = node.get("Names") else {
        return Ok(files);
    };
    let entries = resolve_object(doc, entries)?;
    let entries = entries
        .as_array()
        .ok_or_else(|| invalid("/Names /EmbeddedFiles /Names is not an array"))?;
    if !entries.len().is_multiple_of(2) {
        return Err(invalid(
            "/Names /EmbeddedFiles has an odd number of name/value entries",
        ));
    }
    for pair in entries.0.chunks(2) {
        let name = decode_text_key(&pair[0])
            .ok_or_else(|| invalid("/Names /EmbeddedFiles key is not a string"))?;
        if files.insert(name.clone(), pair[1].clone()).is_some() {
            return Err(invalid(format!(
                "/Names /EmbeddedFiles has duplicate name {name:?}"
            )));
        }
    }
    Ok(files)
}

/// 写出幻灯片对象，返回（幻灯片名称, 对象引用），供目录名称树合并。
///
/// 对象号从当前文档最大对象号之后按注册顺序分配；`/Resources` 为按键
/// 排序的直接名称树叶子节点。
pub(super) fn write(
    doc: &mut Document,
    shows: &[Slideshow],
    embedded: &BTreeMap<String, Value>,
) -> Vec<(String, Value)> {
    if shows.is_empty() {
        return Vec::new();
    }
    let first_num = doc
        .objects_for_write()
        .iter()
        .map(|object| object.num)
        .max()
        .unwrap_or(0)
        + 1;
    let mut refs = Vec::with_capacity(shows.len());
    for (index, show) in shows.iter().enumerate() {
        let num = first_num + index as i64;
        let resources = show
            .resources
            .iter()
            .map(|(key, embedded_name)| {
                let value = embedded
                    .get(embedded_name)
                    .cloned()
                    .expect("embedded file reference is validated");
                (key.clone(), value)
            })
            .collect();
        let dict = Dict::new()
            .insert("Type", Value::Name("SlideShow".into()))
            .insert("Subtype", Value::Name("Embedded".into()))
            .insert("Resources", name_tree(resources))
            .insert("StartResource", text_string_value(&show.start_resource));
        doc.set_object(Object::new(num, 0, Value::Dict(dict)));
        refs.push((show.name.clone(), Value::Ref(num, 0)));
    }
    doc.features
        .borrow_mut()
        .insert(PdfFeature::AlternatePresentations);
    refs
}

/// 解引用间接引用；直接对象原样克隆。
fn resolve_object(doc: &Document, value: &Value) -> Result<Value, WriteError> {
    match value {
        Value::Ref(num, generation) => doc
            .get_object(*num)
            .map_err(|error| invalid(format!("failed to read object {num} {generation}: {error}")))?
            .map(|object| object.value.clone())
            .ok_or_else(|| invalid(format!("object {num} {generation} not found"))),
        other => Ok(other.clone()),
    }
}

/// 名称树键（字面串或十六进制串）→ PDF 文本串解码结果。
fn decode_text_key(value: &Value) -> Option<String> {
    let bytes = match value {
        Value::String(text) => text.chars().map(|ch| ch as u32 as u8).collect(),
        Value::HexString(hex) => hex_bytes(hex)?,
        _ => return None,
    };
    Some(
        PdfTextString::decode(&bytes)
            .map(PdfTextString::into_string)
            .unwrap_or_else(|_| bytes.iter().map(|&byte| byte as char).collect()),
    )
}

fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = hex
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let padded;
    let digits = if digits.len() % 2 == 1 {
        padded = {
            let mut value = digits.clone();
            value.push(b'0');
            value
        };
        &padded[..]
    } else {
        &digits[..]
    };
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        bytes.push((high * 16 + low) as u8);
    }
    Some(bytes)
}

impl PdfBuilder {
    /// 注册一个备用演示幻灯片（13.5），写入目录
    /// `/Names /AlternatePresentations` 名称树。
    ///
    /// 每个资源的值为对应嵌入文件在目录 `/Names /EmbeddedFiles` 中的
    /// 文件规格引用，因此须先通过 [`PdfBuilder::embed_file`] 注册。幻灯片
    /// 名称须非空且唯一、`/StartResource` 须为已声明的资源键、资源键须非空
    /// 且唯一、引用的嵌入文件名须已注册，否则构建返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.4 返回
    /// [`WriteError::UnsupportedVersion`]，均零字节输出。
    pub fn add_alternate_presentation(&mut self, show: Slideshow) -> &mut Self {
        self.navigation.alternate_presentations.push(show);
        self
    }
}
