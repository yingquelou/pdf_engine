//! 便携集合写端（ISO 32000-1 12.3.5、Table 155–158）。
//!
//! 集合字典是目录 `/Collection` 的直接字典，随导航后处理与目录一起写回。
//! 字段模式（Table 156/157）、初始文档（`/D`）、初始视图（`/View`）与
//! 排序（Table 158）均在构建期校验，非法输入零字节失败。PDF 1.7 起可用。

use std::collections::BTreeSet;

use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

use super::super::types::text_string_value;
use super::invalid;

/// 集合字段的数据类型（Table 157 `/Subtype` 的 8 个标准值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionFieldType {
    /// `S`：PDF 文本串。
    Text,
    /// `D`：PDF 日期串。
    Date,
    /// `N`：PDF 数值。
    Number,
    /// `F`：嵌入文件流文件名。
    FileName,
    /// `Desc`：文件规格字典的描述。
    Description,
    /// `ModDate`：嵌入文件参数字典的修改日期。
    ModificationDate,
    /// `CreationDate`：嵌入文件参数字典的创建日期。
    CreationDate,
    /// `Size`：嵌入文件参数字典的字节数。
    Size,
}

impl CollectionFieldType {
    const fn name(self) -> &'static str {
        match self {
            Self::Text => "S",
            Self::Date => "D",
            Self::Number => "N",
            Self::FileName => "F",
            Self::Description => "Desc",
            Self::ModificationDate => "ModDate",
            Self::CreationDate => "CreationDate",
            Self::Size => "Size",
        }
    }
}

/// 集合字段字典（Table 157）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionField {
    /// 字段键，同时是 `/Schema` 字典中的键（合法 PDF 名字，唯一）。
    pub key: String,
    /// `/Subtype` 数据类型。
    pub field_type: CollectionFieldType,
    /// `/N` 呈现给用户的字段名（非空文本串）。
    pub name: String,
    /// `/O` 用户界面中的相对次序。
    pub order: Option<i64>,
    /// `/V` 字段是否初始可见；`None` 表示缺省 `true` 且不写出。
    pub visible: Option<bool>,
    /// `/E` 阅读器是否应提供编辑；`None` 表示缺省 `false` 且不写出。
    pub editable: Option<bool>,
}

impl CollectionField {
    /// 新建字段字典；`/O` `/V` `/E` 留空（取规范缺省）。
    pub fn new(
        key: impl Into<String>,
        field_type: CollectionFieldType,
        name: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            field_type,
            name: name.into(),
            order: None,
            visible: None,
            editable: None,
        }
    }

    /// 设置 `/O` 相对次序。
    pub fn with_order(mut self, order: i64) -> Self {
        self.order = Some(order);
        self
    }

    /// 设置 `/V` 初始可见性。
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    /// 设置 `/E` 可编辑标志。
    pub fn with_editable(mut self, editable: bool) -> Self {
        self.editable = Some(editable);
        self
    }
}

/// 集合初始视图（Table 155 `/View`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CollectionView {
    /// `D`：多列详情模式（缺省，不写出）。
    #[default]
    Details,
    /// `T`：图标平铺模式。
    Tiles,
    /// `H`：初始隐藏集合视图。
    Hidden,
}

impl CollectionView {
    const fn name(self) -> &'static str {
        match self {
            Self::Details => "D",
            Self::Tiles => "T",
            Self::Hidden => "H",
        }
    }
}

/// 集合排序字典（Table 158）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionSort {
    /// `/S` 排序字段名，至少一项，且必须已声明在集合模式中。
    pub fields: Vec<String>,
    /// `/A` 每个字段的升序标志；空表示全部升序（缺省，不写出）。
    pub ascending: Vec<bool>,
}

impl CollectionSort {
    /// 新建全升序排序。
    pub fn new(fields: Vec<String>) -> Self {
        Self {
            fields,
            ascending: Vec::new(),
        }
    }

    /// 逐字段设置升降序；空向量表示全部升序。
    pub fn with_ascending(mut self, ascending: Vec<bool>) -> Self {
        self.ascending = ascending;
        self
    }
}

/// 便携集合配置（12.3.5），写入目录 `/Collection`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Collection {
    /// `/Schema` 字段模式；`None` 时不写 `/Schema`。
    pub schema: Option<Vec<CollectionField>>,
    /// `/D` 初始展示文档在 EmbeddedFiles 名称树中的键；`None` 不写。
    pub initial_document: Option<String>,
    /// `/View` 初始视图；缺省 [`CollectionView::Details`] 不写出。
    pub view: CollectionView,
    /// `/Sort` 初始排序。
    pub sort: Option<CollectionSort>,
}

impl Collection {
    /// 空集合（只有 `/Type /Collection`）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置字段模式。
    pub fn with_schema(mut self, schema: Vec<CollectionField>) -> Self {
        self.schema = Some(schema);
        self
    }

    /// 设置初始展示文档的 EmbeddedFiles 名称树键。
    pub fn with_initial_document(mut self, name: impl Into<String>) -> Self {
        self.initial_document = Some(name.into());
        self
    }

    /// 设置初始视图。
    pub fn with_view(mut self, view: CollectionView) -> Self {
        self.view = view;
        self
    }

    /// 设置初始排序。
    pub fn with_sort(mut self, sort: CollectionSort) -> Self {
        self.sort = Some(sort);
        self
    }
}

/// 校验集合配置；错误串直接用于 [`WriteError`]。
pub(super) fn validate(collection: &Collection) -> Result<(), WriteError> {
    if let Some(schema) = &collection.schema {
        let mut keys = BTreeSet::new();
        for field in schema {
            if !is_valid_name(&field.key) {
                return Err(invalid(format!(
                    "collection field key {:?} is not a valid PDF name",
                    field.key
                )));
            }
            if !keys.insert(field.key.as_str()) {
                return Err(invalid(format!(
                    "collection schema declares field {:?} twice",
                    field.key
                )));
            }
            if field.name.is_empty() {
                return Err(invalid(format!(
                    "collection field {:?} has an empty display name",
                    field.key
                )));
            }
        }
    }
    if let Some(name) = &collection.initial_document
        && name.is_empty()
    {
        return Err(invalid("collection initial document name is empty"));
    }
    if let Some(sort) = &collection.sort {
        let Some(schema) = &collection.schema else {
            return Err(invalid(
                "collection sort requires a schema declaring the sort fields",
            ));
        };
        if sort.fields.is_empty() {
            return Err(invalid("collection sort must name at least one field"));
        }
        if !sort.ascending.is_empty() && sort.ascending.len() != sort.fields.len() {
            return Err(invalid(format!(
                "collection sort /A has {} entries for {} fields",
                sort.ascending.len(),
                sort.fields.len()
            )));
        }
        for field in &sort.fields {
            if !schema.iter().any(|declared| &declared.key == field) {
                return Err(invalid(format!(
                    "collection sort field {field:?} is not declared in the schema"
                )));
            }
        }
    }
    Ok(())
}

/// 写出 `/Collection` 字典（Table 155）；只发出已声明的条目。
pub(super) fn collection_dict(collection: &Collection) -> Dict {
    let mut dict = Dict::new().insert("Type", Value::Name("Collection".into()));
    if let Some(fields) = &collection.schema {
        let mut schema = Dict::new().insert("Type", Value::Name("CollectionSchema".into()));
        for field in fields {
            let mut entry = Dict::new()
                .insert("Type", Value::Name("CollectionField".into()))
                .insert("Subtype", Value::Name(field.field_type.name().into()))
                .insert("N", text_string_value(&field.name));
            if let Some(order) = field.order {
                entry = entry.insert("O", Value::Integer(order));
            }
            if let Some(visible) = field.visible {
                entry = entry.insert("V", Value::Bool(visible));
            }
            if let Some(editable) = field.editable {
                entry = entry.insert("E", Value::Bool(editable));
            }
            schema = schema.insert(field.key.as_str(), Value::Dict(entry));
        }
        dict = dict.insert("Schema", Value::Dict(schema));
    }
    if let Some(name) = &collection.initial_document {
        dict = dict.insert("D", text_string_value(name));
    }
    if collection.view != CollectionView::Details {
        dict = dict.insert("View", Value::Name(collection.view.name().into()));
    }
    if let Some(sort) = &collection.sort {
        let mut sort_dict = Dict::new().insert("Type", Value::Name("CollectionSort".into()));
        sort_dict = if let [field] = sort.fields.as_slice() {
            sort_dict.insert("S", Value::Name(field.clone()))
        } else {
            let names = sort
                .fields
                .iter()
                .map(|field| Value::Name(field.clone()))
                .collect();
            sort_dict.insert("S", Value::Array(Array(names)))
        };
        // `/A` 缺省全升序；显式全升序与缺省等价，不物化。
        if sort.ascending.iter().any(|ascending| !ascending) {
            sort_dict = if let [ascending] = sort.ascending.as_slice() {
                sort_dict.insert("A", Value::Bool(*ascending))
            } else {
                sort_dict.insert(
                    "A",
                    Value::Array(Array(
                        sort.ascending
                            .iter()
                            .map(|value| Value::Bool(*value))
                            .collect(),
                    )),
                )
            };
        }
        dict = dict.insert("Sort", Value::Dict(sort_dict));
    }
    dict
}

/// 集合字段键与排序字段名按 PDF 名字写出，必须无空白与定界符。
fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|ch| {
            let code = ch as u32;
            code > 0x20
                && code < 0x7f
                && !matches!(
                    ch,
                    '(' | ')' | '<' | '>' | '[' | ']' | '{' | '}' | '/' | '%' | '#'
                )
        })
}
