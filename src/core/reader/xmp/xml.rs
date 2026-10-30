//! XMP 包的最小 XML 解析器（14.3.2）：元素、属性、字符数据与命名空间。
//!
//! 只解析良构性下限：标签配对、属性加引号、实体引用与 CDATA；
//! 不处理 DTD 与外部实体，也不做校验。

use std::collections::BTreeMap;

use super::{MAX_DEPTH, MAX_ELEMENTS, XML_NS, XmpError, malformed};

pub(super) struct XmlAttr {
    pub(super) ns: String,
    pub(super) local: String,
    pub(super) value: String,
}

pub(super) struct XmlNode {
    pub(super) ns: String,
    pub(super) local: String,
    pub(super) attrs: Vec<XmlAttr>,
    pub(super) xmlns: Vec<(String, String)>,
    pub(super) children: Vec<XmlNode>,
    pub(super) text: String,
    /// 开始标签 `>` 之后到对应结束标签 `<` 之前的源串原文（自闭合元素为
    /// 空串）：保留前缀、属性、实体与注释/CDATA/处理指令的原始拼写，供
    /// `rdf:parseType="Literal"` 逐字返回。
    pub(super) inner: String,
}

pub(super) struct XmlParser<'a> {
    pub(super) src: &'a str,
    pub(super) pos: usize,
    pub(super) elements: usize,
}

impl<'a> XmlParser<'a> {
    pub(super) fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            elements: 0,
        }
    }

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

    pub(super) fn parse_document(&mut self) -> Result<XmlNode, XmpError> {
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
            inner: String::new(),
        };
        if self_closing {
            return Ok(node);
        }
        // 原始内部标记：开始标签消费后的位置到结束标签 `</` 之前。
        let content_start = self.pos;
        let content_end = loop {
            if self.rest().is_empty() {
                return Err(malformed(format!("unclosed element {name}")));
            }
            if self.starts_with("</") {
                let content_end = self.pos;
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
                break content_end;
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
        };
        node.inner = self.src[content_start..content_end].to_string();
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

pub(super) fn collect_namespaces(node: &XmlNode, out: &mut BTreeMap<String, String>) {
    for (prefix, uri) in &node.xmlns {
        out.insert(prefix.clone(), uri.clone());
    }
    for child in &node.children {
        collect_namespaces(child, out);
    }
}
