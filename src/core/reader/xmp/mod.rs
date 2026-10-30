//! XMP 元数据包读取（14.3.2）：目录 `/Metadata` 流的 RDF/XML 属性视图。
//!
//! 支持 `xpacket` 包装、`rdf:Description` 的属性与子元素、
//! `rdf:Alt`/`rdf:Seq`/`rdf:Bag` 列表、`rdf:parseType="Resource"` 嵌套资源、
//! 类型化值（`rdf:value` + `rdf:datatype` + `xml:lang`）、预定义/数字字符
//! 引用与命名空间前缀解析；属性键使用 Clark 记法（`{命名空间}本地名`）。
//! 解析不处理 DTD 与外部实体，仅做良构性下限检查；写端不在本模块。
//!
//! 结构化值语义（读端子集，不是完整的 XMP 类型系统）：
//! - `rdf:parseType="Resource"`，或作为属性值的 `rdf:Description` 节点元素
//!   → [`XmpValue::Resource`]：属性与子元素递归为 Clark 键映射；
//! - `rdf:datatype` 字面量、`rdf:Description` 内的 `rdf:value`、
//!   仅含 `rdf:value`（及可选 `rdf:datatype`）的 `parseType="Resource"`
//!   → [`XmpValue::Typed`]：`datatype` 原样保留 URI 字符串，不做类型转换；
//! - 其他 `rdf:parseType` 取值（`Literal`/`Collection` 等）以及缺
//!   `rdf:value` 的类型化结构视为 [`XmpError::Malformed`]。

use std::collections::BTreeMap;
use std::fmt;

use crate::core::types::Document;

mod properties;
mod xml;

use properties::{PropertyBudget, collect_properties};
use xml::{XmlParser, collect_namespaces};

/// XMP 包大小上限（4 MiB）。
const MAX_PACKET_BYTES: usize = 4 * 1024 * 1024;
/// 元素嵌套深度上限。
const MAX_DEPTH: usize = 32;
/// 元素总数上限。
const MAX_ELEMENTS: usize = 4096;
/// 属性（键值对）总数上限。
const MAX_PROPERTIES: usize = 1024;

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// XMP 包解析错误。
#[derive(Debug, Clone, PartialEq)]
pub enum XmpError {
    /// XML 结构不合法（标签未闭合、属性缺引号、未知实体等）。
    Malformed(String),
    /// 超出包大小、嵌套深度、元素或属性数量上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for XmpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "malformed XMP packet: {message}"),
            Self::LimitExceeded(what) => write!(f, "XMP limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for XmpError {}

/// XMP 列表容器（`rdf:Alt`/`rdf:Seq`/`rdf:Bag`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmpListKind {
    Alt,
    Seq,
    Bag,
}

/// XMP 列表项（`rdf:li`）；`lang` 取自 `xml:lang`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmpListItem {
    pub lang: Option<String>,
    pub value: String,
}

/// XMP 属性值：简单文本、列表、嵌套资源或类型化值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmpValue {
    /// 简单文本；未知的嵌套结构仍退化为该节点的文本。
    Text(String),
    /// `rdf:Alt`/`rdf:Seq`/`rdf:Bag` 列表。
    List {
        kind: XmpListKind,
        items: Vec<XmpListItem>,
    },
    /// 嵌套资源：来自 `rdf:parseType="Resource"` 或作为属性值的
    /// `rdf:Description` 节点元素；键为 Clark 记法。带额外限定属性
    /// （`rdf:value` 之外）的类型化结构也保留为资源，以免丢数据。
    Resource(BTreeMap<String, XmpValue>),
    /// 类型化值：主值来自 `rdf:value`（或带 `rdf:datatype` 的字面量），
    /// `datatype` 原样保留 URI 字符串，`language` 来自 `xml:lang`。
    Typed {
        value: Box<XmpValue>,
        datatype: Option<String>,
        language: Option<String>,
    },
}

/// XMP 包（14.3.2）的基本属性视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmpMetadata {
    /// 原始 XML 文本（含 `xpacket` 包装）。
    pub packet: String,
    /// 前缀 → 命名空间 URI；默认命名空间以空串为键。
    pub namespaces: BTreeMap<String, String>,
    /// Clark 记法键（`{namespace}local`）→ 属性值。
    pub properties: BTreeMap<String, XmpValue>,
}

impl XmpMetadata {
    /// 解析 XMP 包。
    ///
    /// 要求至少一个 `rdf:Description`；无 RDF 描述的包视为损坏。
    pub fn parse(packet: &str) -> Result<Self, XmpError> {
        if packet.len() > MAX_PACKET_BYTES {
            return Err(XmpError::LimitExceeded("XMP packet size"));
        }
        let text = packet.strip_prefix('\u{feff}').unwrap_or(packet);
        let root = XmlParser::new(text).parse_document()?;
        let mut namespaces = BTreeMap::new();
        collect_namespaces(&root, &mut namespaces);
        let mut properties = BTreeMap::new();
        let mut budget = PropertyBudget::new();
        let descriptions = collect_properties(&root, &mut properties, &mut budget)?;
        if descriptions == 0 {
            return Err(malformed("packet contains no rdf:Description"));
        }
        Ok(Self {
            packet: packet.to_string(),
            namespaces,
            properties,
        })
    }

    /// 取文本属性；列表属性返回首项，缺失返回 `None`。
    ///
    /// 结构化值（[`XmpValue::Resource`]/[`XmpValue::Typed`]）没有单一文本
    /// 表示，返回 `None`；调用方需直接检查 [`Self::properties`]。
    pub fn text(&self, key: &str) -> Option<&str> {
        match self.properties.get(key)? {
            XmpValue::Text(value) => Some(value),
            XmpValue::List { items, .. } => items.first().map(|item| item.value.as_str()),
            XmpValue::Resource(_) | XmpValue::Typed { .. } => None,
        }
    }
}

impl Document {
    /// 读取并解析 XMP 元数据（14.3.2）。
    ///
    /// 缺失 `/Metadata`、非流或流解码失败返回 `Ok(None)`（与
    /// [`Document::xmp_metadata`] 一致）；非 UTF-8 或 XML 损坏返回
    /// [`XmpError`]。
    pub fn xmp(&self) -> Result<Option<XmpMetadata>, XmpError> {
        let Some(bytes) = self.xmp_metadata() else {
            return Ok(None);
        };
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| malformed(format!("packet is not UTF-8: {error}")))?;
        XmpMetadata::parse(text).map(Some)
    }
}

fn malformed(message: impl Into<String>) -> XmpError {
    XmpError::Malformed(message.into())
}

#[cfg(test)]
mod tests;
