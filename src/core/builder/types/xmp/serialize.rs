//! `XmpPacket` → RDF/XML 的确定性序列化（14.3.2）。
//!
//! 输出固定为 `<x:xmpmeta><rdf:RDF><rdf:Description rdf:about="">` 结构；
//! 属性命名空间在 `rdf:Description` 上统一声明，前缀按 URI 排序分配。

use std::collections::{BTreeMap, BTreeSet};

use super::{XmpListKind, XmpPacket, XmpPacketError, XmpValue};

const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XMP_NS: &str = "adobe:ns:meta/";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
/// 与读端一致的单包大小上限。
const MAX_PACKET_BYTES: usize = 4 * 1024 * 1024;

impl XmpPacket {
    /// 序列化为完整 RDF/XML 包。
    ///
    /// 输出与属性插入顺序无关：属性按 Clark 键排序，命名空间按 URI
    /// 排序后依次分配 `ns1`、`ns2`…。`rdf` 与 `x` 前缀保留，RDF
    /// 语法命名空间与 XML 命名空间不接受为属性键。
    ///
    /// # 错误
    ///
    /// 非法 Clark 键、非法 XML 字符、非法 `rdf:datatype`、读端无法
    /// 无损表达的形态或超过 4 MiB 的包分别返回
    /// [`XmpPacketError`] 对应变体。
    pub fn to_xml(&self) -> Result<String, XmpPacketError> {
        let mut namespaces = BTreeSet::new();
        for (key, value) in &self.properties {
            let (uri, _) = split_key(key)?;
            namespaces.insert(uri.to_string());
            collect_namespaces(value, &mut namespaces)?;
        }
        let prefixes = assign_prefixes(&namespaces);

        let mut out = String::new();
        out.push_str("<x:xmpmeta xmlns:x=\"");
        out.push_str(XMP_NS);
        out.push_str("\"><rdf:RDF xmlns:rdf=\"");
        out.push_str(RDF_NS);
        out.push_str("\"><rdf:Description rdf:about=\"\"");
        for uri in &namespaces {
            // 属性键不允许 RDF/XML 命名空间，因此这里只需跳过 xmpmeta
            // 已声明的 `x`。
            if uri == XMP_NS {
                continue;
            }
            let prefix = prefixes
                .get(uri)
                .expect("every collected namespace has a prefix");
            out.push_str(" xmlns:");
            out.push_str(prefix);
            out.push_str("=\"");
            escape_value(&mut out, uri, "namespace URI")?;
            out.push('"');
        }
        out.push('>');
        for (key, value) in &self.properties {
            write_property(&mut out, key, value, &prefixes)?;
        }
        out.push_str("</rdf:Description></rdf:RDF></x:xmpmeta>");

        if out.len() > MAX_PACKET_BYTES {
            return Err(XmpPacketError::PacketTooLarge(out.len()));
        }
        Ok(out)
    }
}

/// 验证 Clark 键并拆出 `(命名空间, 本地名)`。
fn split_key(key: &str) -> Result<(&str, &str), XmpPacketError> {
    let Some(rest) = key.strip_prefix('{') else {
        return Err(invalid_key(
            key,
            "expected Clark notation \"{namespace}local\"",
        ));
    };
    let Some((uri, local)) = rest.split_once('}') else {
        return Err(invalid_key(key, "missing closing '}'"));
    };
    if uri.is_empty() {
        return Err(invalid_key(key, "namespace must not be empty"));
    }
    if uri == RDF_NS {
        return Err(invalid_key(key, "RDF namespace is reserved for syntax"));
    }
    if uri == XML_NS || uri == XMLNS_NS {
        return Err(invalid_key(key, "XML namespace is reserved"));
    }
    if uri.chars().any(|c| c.is_whitespace() || !is_xml_char(c)) {
        return Err(invalid_key(
            key,
            "namespace must not contain whitespace or invalid XML characters",
        ));
    }
    if !valid_local_name(local) {
        return Err(invalid_key(
            key,
            "local name must start with a letter or '_' and contain only letters, digits, '.', '_' or '-'",
        ));
    }
    Ok((uri, local))
}

fn invalid_key(key: &str, reason: &str) -> XmpPacketError {
    XmpPacketError::InvalidKey(format!("{key:?}: {reason}"))
}

fn valid_local_name(local: &str) -> bool {
    let mut chars = local.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// 递归收集值中出现过的属性命名空间，同时校验嵌套键与类型化元数据。
fn collect_namespaces(value: &XmpValue, out: &mut BTreeSet<String>) -> Result<(), XmpPacketError> {
    match value {
        XmpValue::Text(_) => Ok(()),
        XmpValue::List { items, .. } => {
            for item in items {
                if let Some(lang) = &item.lang {
                    validate_language(lang)?;
                }
            }
            Ok(())
        }
        XmpValue::Collection(items) => {
            for item in items {
                collect_namespaces(item, out)?;
            }
            Ok(())
        }
        XmpValue::Resource(map) => {
            for (key, child) in map {
                let (uri, _) = split_key(key)?;
                out.insert(uri.to_string());
                collect_namespaces(child, out)?;
            }
            Ok(())
        }
        XmpValue::Typed {
            value,
            datatype,
            language,
        } => {
            check_typed_value(value, "typed value")?;
            if let Some(uri) = datatype {
                validate_datatype(uri)?;
            }
            if let Some(lang) = language {
                validate_language(lang)?;
            }
            Ok(())
        }
        XmpValue::Reference(uri) => validate_reference(uri),
        XmpValue::Literal(xml) => validate_literal(xml),
    }
}

fn assign_prefixes(namespaces: &BTreeSet<String>) -> BTreeMap<String, String> {
    let mut prefixes = BTreeMap::new();
    let mut index = 1usize;
    for uri in namespaces {
        let prefix = if uri == XMP_NS {
            "x".to_string()
        } else {
            let prefix = format!("ns{index}");
            index += 1;
            prefix
        };
        prefixes.insert(uri.clone(), prefix);
    }
    prefixes
}

/// 写出一个属性元素；`key` 已在校验阶段通过。
fn write_property(
    out: &mut String,
    key: &str,
    value: &XmpValue,
    prefixes: &BTreeMap<String, String>,
) -> Result<(), XmpPacketError> {
    let (uri, local) = split_key(key)?;
    let prefix = prefixes.get(uri).ok_or_else(|| {
        XmpPacketError::InvalidKey(format!("{key:?}: namespace has no assigned prefix"))
    })?;
    let name = format!("{prefix}:{local}");
    write_element(out, &name, value, prefixes)
}

/// 写出元素体；`name` 已含前缀（集合条目固定为 `rdf:li`）。
fn write_element(
    out: &mut String,
    name: &str,
    value: &XmpValue,
    prefixes: &BTreeMap<String, String>,
) -> Result<(), XmpPacketError> {
    out.push('<');
    out.push_str(name);

    match value {
        XmpValue::Text(text) => {
            out.push('>');
            escape_value(out, text, name)?;
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
        XmpValue::List { kind, items } => {
            out.push('>');
            out.push_str("<rdf:");
            let element = list_element(*kind);
            out.push_str(element);
            if items.is_empty() {
                out.push_str("/>");
            } else {
                out.push('>');
                for item in items {
                    out.push_str("<rdf:li");
                    if let Some(lang) = &item.lang {
                        write_language(out, lang)?;
                    }
                    out.push('>');
                    escape_value(out, &item.value, name)?;
                    out.push_str("</rdf:li>");
                }
                out.push_str("</rdf:");
                out.push_str(element);
                out.push('>');
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
        XmpValue::Collection(items) => {
            out.push_str(" rdf:parseType=\"Collection\"");
            if items.is_empty() {
                out.push_str("/>");
            } else {
                out.push('>');
                for item in items {
                    write_element(out, "rdf:li", item, prefixes)?;
                }
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
        }
        XmpValue::Resource(map) => {
            out.push_str(" rdf:parseType=\"Resource\"");
            if map.is_empty() {
                out.push_str("/>");
            } else {
                out.push('>');
                for (child_key, child) in map {
                    write_property(out, child_key, child, prefixes)?;
                }
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
        }
        XmpValue::Typed {
            value,
            datatype,
            language,
        } => {
            let text = check_typed_value(value, name)?;
            if let Some(uri) = datatype {
                validate_datatype(uri)?;
            }
            out.push_str(" rdf:parseType=\"Resource\"");
            if let Some(lang) = language {
                write_language(out, lang)?;
            }
            out.push('>');
            out.push_str("<rdf:value>");
            escape_value(out, text, name)?;
            out.push_str("</rdf:value>");
            if let Some(uri) = datatype {
                out.push_str("<rdf:datatype>");
                escape_value(out, uri, name)?;
                out.push_str("</rdf:datatype>");
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
        XmpValue::Reference(uri) => {
            validate_reference(uri)?;
            out.push_str(" rdf:resource=\"");
            escape_value(out, uri, name)?;
            out.push_str("\"/>");
        }
        XmpValue::Literal(xml) => {
            validate_literal(xml)?;
            out.push_str(" rdf:parseType=\"Literal\">");
            out.push_str(xml);
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
    }
    Ok(())
}

fn list_element(kind: XmpListKind) -> &'static str {
    match kind {
        XmpListKind::Alt => "Alt",
        XmpListKind::Seq => "Seq",
        XmpListKind::Bag => "Bag",
    }
}

/// 读端只能把 `rdf:value` 的文本折叠回 [`XmpValue::Typed`]；其他主值
/// 形态会退化为资源并丢失类型信息，因此在写端拒绝。
fn check_typed_value<'a>(value: &'a XmpValue, context: &str) -> Result<&'a str, XmpPacketError> {
    match value {
        XmpValue::Text(text) => Ok(text),
        other => Err(XmpPacketError::UnsupportedValue(format!(
            "{context}: typed value must be text, got {}",
            value_kind(other)
        ))),
    }
}

fn value_kind(value: &XmpValue) -> &'static str {
    match value {
        XmpValue::Text(_) => "text",
        XmpValue::List { .. } => "list",
        XmpValue::Collection(_) => "collection",
        XmpValue::Resource(_) => "resource",
        XmpValue::Typed { .. } => "typed value",
        XmpValue::Reference(_) => "reference",
        XmpValue::Literal(_) => "literal",
    }
}

fn write_language(out: &mut String, language: &str) -> Result<(), XmpPacketError> {
    validate_language(language)?;
    out.push_str(" xml:lang=\"");
    escape_value(out, language, "xml:lang")?;
    out.push('"');
    Ok(())
}

fn validate_language(language: &str) -> Result<(), XmpPacketError> {
    if language
        .chars()
        .any(|c| c.is_whitespace() || !is_xml_char(c))
    {
        return Err(XmpPacketError::InvalidCharacter(format!(
            "xml:lang {language:?} contains whitespace or invalid XML characters"
        )));
    }
    Ok(())
}

/// 与读端一致的 `rdf:datatype` 规则：`scheme ":"` 前缀且其余非空，
/// 不含空白/控制字符（本库不解释 URI）。
fn validate_datatype(uri: &str) -> Result<(), XmpPacketError> {
    let valid_scheme = match uri.split_once(':') {
        Some((scheme, rest)) => {
            !rest.is_empty()
                && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }
        None => false,
    };
    if !valid_scheme || uri.chars().any(|c| c.is_whitespace() || !is_xml_char(c)) {
        return Err(XmpPacketError::InvalidDatatype(format!(
            "{uri:?} is not an absolute URI without whitespace or control characters"
        )));
    }
    Ok(())
}

/// `rdf:resource` 引用：非空，且不含空白/控制字符/非法 XML 字符。
/// 与读端点一致，不解析 URI、不强制 scheme。
fn validate_reference(uri: &str) -> Result<(), XmpPacketError> {
    if uri.is_empty() || uri.chars().any(|c| c.is_whitespace() || !is_xml_char(c)) {
        return Err(XmpPacketError::InvalidReference(format!(
            "{uri:?} must be a non-empty URI without whitespace or invalid XML characters"
        )));
    }
    Ok(())
}

/// `rdf:parseType="Literal"` 的原始标记：调用方保证良构与命名空间自包含；
/// 写端只拒绝会破坏文档的结构（处理指令、`<!DOCTYPE`、裸 `]]>`）与非法
/// XML 字符。空串合法，标记本身不转义、不改写。
fn validate_literal(xml: &str) -> Result<(), XmpPacketError> {
    if let Some(c) = xml.chars().find(|c| !is_xml_char(*c)) {
        return Err(XmpPacketError::InvalidLiteral(format!(
            "U+{:04X} in literal markup",
            c as u32
        )));
    }
    // 不区分大小写；两个记号都只含 ASCII，按小写副本查找不影响 Unicode。
    let lower = xml.to_ascii_lowercase();
    if lower.contains("<?") {
        return Err(XmpPacketError::InvalidLiteral(
            "processing instructions are not allowed in literal markup".into(),
        ));
    }
    if lower.contains("<!doctype") {
        return Err(XmpPacketError::InvalidLiteral(
            "DOCTYPE declarations are not allowed in literal markup".into(),
        ));
    }
    if xml.contains("]]>") {
        return Err(XmpPacketError::InvalidLiteral(
            "']]>' is not allowed in literal markup".into(),
        ));
    }
    Ok(())
}

/// XML 1.0 `Char`：Tab/LF/CR、#x20–#xD7FF、#xE000–#xFFFD、
/// #x10000–#x10FFFF；Rust `char` 已排除代理项。
fn is_xml_char(c: char) -> bool {
    match c {
        '\t' | '\n' | '\r' => true,
        '\u{FFFE}' | '\u{FFFF}' => false,
        c => !c.is_control(),
    }
}

/// 转义文本与属性值并拒绝非法字符。`'`/`"` 在两种位置统一转义，
/// 输出因此只含五种预定义实体。
fn escape_value(out: &mut String, value: &str, context: &str) -> Result<(), XmpPacketError> {
    for c in value.chars() {
        if !is_xml_char(c) {
            return Err(XmpPacketError::InvalidCharacter(format!(
                "U+{:04X} in {context}",
                c as u32
            )));
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    Ok(())
}
