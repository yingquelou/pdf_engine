//! 类型化 XMP 包写端（14.3.2）：RDF/XML 包的构造与确定性序列化。
//!
//! 本模块只依赖写端自身模型，不引用读端类型；序列化产出读端
//! [`crate::core::reader::xmp::XmpMetadata::parse`] 可解析的形状。
//! 支持四种值：文本、`rdf:Alt`/`rdf:Seq`/`rdf:Bag` 列表、
//! `rdf:parseType="Resource"` 嵌套资源，以及 `rdf:value` +
//! `rdf:datatype` + `xml:lang` 类型化值。
//!
//! 序列化契约：
//! - 键使用 Clark 记法 `{命名空间}本地名`；命名空间非空，本地名须为
//!   合法 XML 名称（字母/下划线开头，其后字母、数字、`.` `_` `-`，不含
//!   冒号）；`rdf`/XML 命名空间保留给语法，不接受为属性键；
//! - 命名空间按 URI 字典序确定前缀 `ns1`、`ns2`…，`rdf` 与 `x` 保留，
//!   输出与插入顺序无关；
//! - 文本与属性值转义 `& < > " '`，拒绝 XML 1.0 非法字符（允许
//!   Tab/LF/CR）；
//! - `rdf:datatype` 须为绝对 URI（`scheme ":"` 前缀，无空白/控制字符）；
//! - 类型化值的主值只支持文本：读端只能无损还原该子集，其他形态在
//!   序列化时报错；
//! - 输出不超过 4 MiB，与读端包大小上限一致。
//!
//! 与 [`crate::core::builder::PdfBuilder::set_xmp_metadata`] 的原文透传
//! 互斥：同时设置两者时构建失败（见
//! [`crate::core::builder::PdfBuilder::set_xmp_packet`]）。

use std::collections::BTreeMap;
use std::fmt;

mod serialize;

#[cfg(test)]
mod tests;

/// XMP 列表容器（`rdf:Alt`/`rdf:Seq`/`rdf:Bag`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmpListKind {
    /// 备选语言列表。
    Alt,
    /// 有序列表。
    Seq,
    /// 无序集合。
    Bag,
}

/// XMP 列表项（`rdf:li`）；`lang` 写成 `xml:lang`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmpListItem {
    /// `xml:lang` 语言标记。
    pub lang: Option<String>,
    /// 条目文本。
    pub value: String,
}

/// 类型化 XMP 值（写端镜像读端解析结果）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmpValue {
    /// 简单文本：写成属性元素的内容。
    Text(String),
    /// `rdf:Alt`/`rdf:Seq`/`rdf:Bag` 列表。
    List {
        /// 列表容器种类。
        kind: XmpListKind,
        /// `rdf:li` 条目。
        items: Vec<XmpListItem>,
    },
    /// `rdf:parseType="Resource"` 嵌套资源；键为 Clark 记法。
    Resource(BTreeMap<String, XmpValue>),
    /// 类型化值：`rdf:value` 主值、可选 `rdf:datatype` URI 与
    /// `xml:lang` 语言标记。
    Typed {
        /// 主值；序列化仅支持 [`XmpValue::Text`]。
        value: Box<XmpValue>,
        /// `rdf:datatype` 绝对 URI。
        datatype: Option<String>,
        /// `xml:lang` 语言标记。
        language: Option<String>,
    },
}

/// XMP 包构造错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmpPacketError {
    /// Clark 键不合法（缺大括号、空命名空间/本地名、非法本地名或
    /// 保留命名空间）。
    InvalidKey(String),
    /// 文本或属性值含 XML 1.0 不允许的字符。
    InvalidCharacter(String),
    /// `rdf:datatype` 不是绝对 URI，或含空白/控制字符。
    InvalidDatatype(String),
    /// 当前读端模型无法无损表达的值（如非文本主值的类型化值）。
    UnsupportedValue(String),
    /// 序列化结果超过 4 MiB 包大小上限。
    PacketTooLarge(usize),
}

impl fmt::Display for XmpPacketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey(message) => write!(f, "invalid XMP property key: {message}"),
            Self::InvalidCharacter(message) => {
                write!(f, "invalid XML character in XMP packet: {message}")
            }
            Self::InvalidDatatype(message) => write!(f, "invalid XMP datatype: {message}"),
            Self::UnsupportedValue(message) => write!(f, "unsupported XMP value: {message}"),
            Self::PacketTooLarge(size) => {
                write!(f, "XMP packet is {size} bytes, exceeding the 4 MiB limit")
            }
        }
    }
}

impl std::error::Error for XmpPacketError {}

/// 待写出的 XMP 包：Clark 键 → 类型化值的确定性映射。
///
/// 用 [`XmpPacket::set`] 添加属性，[`XmpPacket::to_xml`] 生成完整
/// RDF/XML 包；[`crate::core::builder::PdfBuilder::set_xmp_packet`] 将其
/// 挂到目录 `/Metadata`。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XmpPacket {
    properties: BTreeMap<String, XmpValue>,
}

impl XmpPacket {
    /// 新建空包；没有属性时仍序列化为一个空的 `rdf:Description`。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置属性；同一 Clark 键后写覆盖先写。
    pub fn set(&mut self, key: impl Into<String>, value: XmpValue) -> &mut Self {
        self.properties.insert(key.into(), value);
        self
    }

    /// 包内属性（Clark 键 → 值），按 Clark 键排序。
    pub fn properties(&self) -> &BTreeMap<String, XmpValue> {
        &self.properties
    }
}
