//! XMP 元数据包读取（14.3.2）：目录 `/Metadata` 流的 RDF/XML 属性视图。
//!
//! 支持 `xpacket` 包装、`rdf:Description` 的属性与子元素、
//! `rdf:Alt`/`rdf:Seq`/`rdf:Bag` 列表、预定义/数字字符引用与命名空间
//! 前缀解析；属性键使用 Clark 记法（`{命名空间}本地名`）。不支持
//! `rdf:parseType` 资源嵌套与 XMP 类型化值，嵌套结构退化为文本。
//! 解析不处理 DTD 与外部实体，仅做良构性下限检查。

use std::collections::BTreeMap;
use std::fmt;

use crate::core::types::Document;

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

/// XMP 属性值：简单文本或列表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmpValue {
    Text(String),
    List {
        kind: XmpListKind,
        items: Vec<XmpListItem>,
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
        let mut parser = XmlParser {
            src: text,
            pos: 0,
            elements: 0,
        };
        let root = parser.parse_document()?;
        let mut namespaces = BTreeMap::new();
        collect_namespaces(&root, &mut namespaces);
        let mut properties = BTreeMap::new();
        let descriptions = collect_properties(&root, &mut properties)?;
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
    pub fn text(&self, key: &str) -> Option<&str> {
        match self.properties.get(key)? {
            XmpValue::Text(value) => Some(value),
            XmpValue::List { items, .. } => items.first().map(|item| item.value.as_str()),
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

struct XmlAttr {
    ns: String,
    local: String,
    value: String,
}

struct XmlNode {
    ns: String,
    local: String,
    attrs: Vec<XmlAttr>,
    xmlns: Vec<(String, String)>,
    children: Vec<XmlNode>,
    text: String,
}

struct XmlParser<'a> {
    src: &'a str,
    pos: usize,
    elements: usize,
}

impl<'a> XmlParser<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    fn bump(&mut self, bytes: usize) {
        self.pos += bytes;
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn skip_misc(&mut self) -> Result<(), XmpError> {
        loop {
            self.skip_ws();
            if self.starts_with("<!--") {
                let end = self
                    .rest()
                    .find("-->")
                    .ok_or_else(|| malformed("unterminated comment"))?;
                self.bump(end + 3);
            } else if self.starts_with("<?") {
                let end = self
                    .rest()
                    .find("?>")
                    .ok_or_else(|| malformed("unterminated processing instruction"))?;
                self.bump(end + 2);
            } else {
                return Ok(());
            }
        }
    }

    fn parse_document(&mut self) -> Result<XmlNode, XmpError> {
        self.skip_misc()?;
        if self.rest().is_empty() {
            return Err(malformed("empty packet"));
        }
        let mut base_scope = BTreeMap::new();
        base_scope.insert("xml".to_string(), XML_NS.to_string());
        let root = self.parse_element(&base_scope, 0)?;
        self.skip_misc()?;
        if !self.rest().is_empty() {
            return Err(malformed("trailing content after root element"));
        }
        Ok(root)
    }

    fn parse_element(
        &mut self,
        scope: &BTreeMap<String, String>,
        depth: usize,
    ) -> Result<XmlNode, XmpError> {
        if depth > MAX_DEPTH {
            return Err(XmpError::LimitExceeded("XMP element depth"));
        }
        self.elements += 1;
        if self.elements > MAX_ELEMENTS {
            return Err(XmpError::LimitExceeded("XMP element count"));
        }
        if !self.starts_with("<") || self.starts_with("</") {
            return Err(malformed("expected start tag"));
        }
        self.bump(1);
        let name = self.parse_name()?;

        let mut raw_attrs = Vec::new();
        let self_closing = loop {
            let before_ws = self.pos;
            self.skip_ws();
            if self.starts_with("/>") {
                self.bump(2);
                break true;
            }
            if self.starts_with(">") {
                self.bump(1);
                break false;
            }
            if self.pos == before_ws {
                return Err(malformed("missing whitespace before attribute"));
            }
            let attr_name = self.parse_name()?;
            self.skip_ws();
            if !self.starts_with("=") {
                return Err(malformed("attribute without value"));
            }
            self.bump(1);
            self.skip_ws();
            let quote = self
                .rest()
                .chars()
                .next()
                .filter(|c| *c == '"' || *c == '\'')
                .ok_or_else(|| malformed("attribute value must be quoted"))?;
            self.bump(1);
            let end = self
                .rest()
                .find(quote)
                .ok_or_else(|| malformed("unterminated attribute value"))?;
            let raw = &self.rest()[..end];
            self.bump(end + 1);
            raw_attrs.push((attr_name, decode_entities(raw)?));
        };

        let mut local_scope = scope.clone();
        let mut xmlns = Vec::new();
        for (name, value) in &raw_attrs {
            if name == "xmlns" {
                local_scope.insert(String::new(), value.clone());
                xmlns.push((String::new(), value.clone()));
            } else if let Some(prefix) = name.strip_prefix("xmlns:") {
                local_scope.insert(prefix.to_string(), value.clone());
                xmlns.push((prefix.to_string(), value.clone()));
            }
        }
        let (ns, local) = resolve_element_name(&name, &local_scope)?;

        let mut attrs = Vec::new();
        for (name, value) in raw_attrs {
            if name == "xmlns" || name.starts_with("xmlns:") {
                continue;
            }
            let (attr_ns, attr_local) = resolve_attr_name(&name, &local_scope)?;
            attrs.push(XmlAttr {
                ns: attr_ns,
                local: attr_local,
                value,
            });
        }

        let mut node = XmlNode {
            ns,
            local,
            attrs,
            xmlns,
            children: Vec::new(),
            text: String::new(),
        };
        if self_closing {
            return Ok(node);
        }
        loop {
            if self.rest().is_empty() {
                return Err(malformed(format!("unclosed element {name}")));
            }
            if self.starts_with("</") {
                self.bump(2);
                let close = self.parse_name()?;
                self.skip_ws();
                if !self.starts_with(">") {
                    return Err(malformed("malformed closing tag"));
                }
                self.bump(1);
                if close != name {
                    return Err(malformed(format!(
                        "mismatched closing tag: expected {name}, got {close}"
                    )));
                }
                break;
            }
            if self.starts_with("<!--") {
                let end = self
                    .rest()
                    .find("-->")
                    .ok_or_else(|| malformed("unterminated comment"))?;
                self.bump(end + 3);
            } else if self.starts_with("<![CDATA[") {
                let end = self
                    .rest()
                    .find("]]>")
                    .ok_or_else(|| malformed("unterminated CDATA section"))?;
                node.text.push_str(&self.rest()[9..end]);
                self.bump(end + 3);
            } else if self.starts_with("<?") {
                let end = self
                    .rest()
                    .find("?>")
                    .ok_or_else(|| malformed("unterminated processing instruction"))?;
                self.bump(end + 2);
            } else if self.starts_with("<!") {
                return Err(malformed("unsupported markup declaration"));
            } else if self.starts_with("<") {
                node.children
                    .push(self.parse_element(&local_scope, depth + 1)?);
            } else {
                let end = self
                    .rest()
                    .find('<')
                    .ok_or_else(|| malformed(format!("unclosed element {name}")))?;
                node.text.push_str(&decode_entities(&self.rest()[..end])?);
                self.bump(end);
            }
        }
        Ok(node)
    }

    fn parse_name(&mut self) -> Result<String, XmpError> {
        let start = self.pos;
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() || matches!(c, '/' | '>' | '=' | '"' | '\'' | '<' | '&') {
                break;
            }
            self.pos += c.len_utf8();
        }
        if self.pos == start {
            return Err(malformed("empty name"));
        }
        Ok(self.src[start..self.pos].to_string())
    }
}

fn resolve_element_name(
    name: &str,
    scope: &BTreeMap<String, String>,
) -> Result<(String, String), XmpError> {
    match name.split_once(':') {
        Some((prefix, local)) => {
            if local.is_empty() {
                return Err(malformed(format!("malformed qualified name {name}")));
            }
            let ns = scope
                .get(prefix)
                .cloned()
                .ok_or_else(|| malformed(format!("unbound namespace prefix {prefix}")))?;
            Ok((ns, local.to_string()))
        }
        None => Ok((scope.get("").cloned().unwrap_or_default(), name.to_string())),
    }
}

fn resolve_attr_name(
    name: &str,
    scope: &BTreeMap<String, String>,
) -> Result<(String, String), XmpError> {
    match name.split_once(':') {
        Some((prefix, local)) => {
            if local.is_empty() {
                return Err(malformed(format!("malformed qualified name {name}")));
            }
            let ns = scope
                .get(prefix)
                .cloned()
                .ok_or_else(|| malformed(format!("unbound namespace prefix {prefix}")))?;
            Ok((ns, local.to_string()))
        }
        None => Ok((String::new(), name.to_string())),
    }
}

fn decode_entities(raw: &str) -> Result<String, XmpError> {
    if !raw.contains('&') {
        return Ok(raw.to_string());
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let end = after
            .find(';')
            .ok_or_else(|| malformed("unterminated entity reference"))?;
        let entity = &after[..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                let code = u32::from_str_radix(&entity[2..], 16)
                    .map_err(|_| malformed(format!("invalid character reference &{entity};")))?;
                out.push(
                    char::from_u32(code)
                        .ok_or_else(|| malformed(format!("invalid code point in &{entity};")))?,
                );
            }
            _ if entity.starts_with('#') => {
                let code = entity[1..]
                    .parse::<u32>()
                    .map_err(|_| malformed(format!("invalid character reference &{entity};")))?;
                out.push(
                    char::from_u32(code)
                        .ok_or_else(|| malformed(format!("invalid code point in &{entity};")))?,
                );
            }
            _ => return Err(malformed(format!("unknown entity &{entity};"))),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn collect_namespaces(node: &XmlNode, out: &mut BTreeMap<String, String>) {
    for (prefix, uri) in &node.xmlns {
        out.insert(prefix.clone(), uri.clone());
    }
    for child in &node.children {
        collect_namespaces(child, out);
    }
}

fn collect_properties(
    node: &XmlNode,
    out: &mut BTreeMap<String, XmpValue>,
) -> Result<usize, XmpError> {
    let mut descriptions = 0;
    if node.ns == RDF_NS && node.local == "Description" {
        descriptions += 1;
        for attr in &node.attrs {
            if is_rdf_syntax_attr(attr) {
                continue;
            }
            insert_property(
                out,
                clark(&attr.ns, &attr.local),
                XmpValue::Text(attr.value.clone()),
            )?;
        }
        for child in &node.children {
            insert_property(out, clark(&child.ns, &child.local), property_value(child))?;
        }
    }
    for child in &node.children {
        descriptions += collect_properties(child, out)?;
    }
    Ok(descriptions)
}

fn is_rdf_syntax_attr(attr: &XmlAttr) -> bool {
    attr.ns == XML_NS
        || (attr.ns == RDF_NS
            && matches!(
                attr.local.as_str(),
                "about" | "ID" | "nodeID" | "parseType" | "datatype" | "resource"
            ))
}

fn insert_property(
    out: &mut BTreeMap<String, XmpValue>,
    key: String,
    value: XmpValue,
) -> Result<(), XmpError> {
    if !out.contains_key(&key) && out.len() >= MAX_PROPERTIES {
        return Err(XmpError::LimitExceeded("XMP property count"));
    }
    out.insert(key, value);
    Ok(())
}

fn property_value(node: &XmlNode) -> XmpValue {
    if node.children.len() == 1 {
        let list = &node.children[0];
        let kind = match (list.ns.as_str(), list.local.as_str()) {
            (RDF_NS, "Alt") => Some(XmpListKind::Alt),
            (RDF_NS, "Seq") => Some(XmpListKind::Seq),
            (RDF_NS, "Bag") => Some(XmpListKind::Bag),
            _ => None,
        };
        if let Some(kind) = kind {
            let items = list
                .children
                .iter()
                .filter(|item| item.ns == RDF_NS && item.local == "li")
                .map(|item| XmpListItem {
                    lang: item
                        .attrs
                        .iter()
                        .find(|attr| attr.ns == XML_NS && attr.local == "lang")
                        .map(|attr| attr.value.clone()),
                    value: item.text.trim().to_string(),
                })
                .collect();
            return XmpValue::List { kind, items };
        }
    }
    XmpValue::Text(node.text.trim().to_string())
}

fn clark(ns: &str, local: &str) -> String {
    if ns.is_empty() {
        local.to_string()
    } else {
        format!("{{{ns}}}{local}")
    }
}

#[cfg(test)]
mod tests;
