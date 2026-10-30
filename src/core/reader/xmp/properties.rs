//! XMP 属性视图（14.3.2）：`rdf:Description` 的条目、列表与结构化值。
//!
//! 把 XML 节点树折叠为 Clark 记法键的 [`XmpValue`] 映射；嵌套资源与
//! 类型化值在这里成形，属性数量预算在此统一执行。

use std::collections::BTreeMap;

use super::xml::{XmlAttr, XmlNode};
use super::{
    MAX_DEPTH, MAX_PROPERTIES, RDF_NS, XML_NS, XmpError, XmpListItem, XmpListKind, XmpValue,
    malformed,
};

/// 属性数量预算：顶层属性与嵌套资源共用 `MAX_PROPERTIES` 上限。
pub(super) struct PropertyBudget {
    count: usize,
}

impl PropertyBudget {
    pub(super) fn new() -> Self {
        Self { count: 0 }
    }

    fn insert(
        &mut self,
        out: &mut BTreeMap<String, XmpValue>,
        key: String,
        value: XmpValue,
    ) -> Result<(), XmpError> {
        if !out.contains_key(&key) {
            if self.count >= MAX_PROPERTIES {
                return Err(XmpError::LimitExceeded("XMP property count"));
            }
            self.count += 1;
        }
        out.insert(key, value);
        Ok(())
    }
}

fn is_description(node: &XmlNode) -> bool {
    node.ns == RDF_NS && node.local == "Description"
}

fn is_rdf_value(node: &XmlNode) -> bool {
    node.ns == RDF_NS && node.local == "value"
}

pub(super) fn collect_properties(
    node: &XmlNode,
    out: &mut BTreeMap<String, XmpValue>,
    budget: &mut PropertyBudget,
) -> Result<usize, XmpError> {
    if is_description(node) {
        // `rdf:Description` 是属性容器；其子元素是属性值，不再作为独立
        // 描述递归发现（否则嵌套资源的属性会泄漏到顶层）。
        collect_entries(node, out, budget, 0)?;
        return Ok(1);
    }
    let mut descriptions = 0;
    for child in &node.children {
        descriptions += collect_properties(child, out, budget)?;
    }
    Ok(descriptions)
}

/// 把一个资源节点的属性（XML 属性）与属性元素（子元素）写入 `out`。
fn collect_entries(
    node: &XmlNode,
    out: &mut BTreeMap<String, XmpValue>,
    budget: &mut PropertyBudget,
    depth: usize,
) -> Result<(), XmpError> {
    if depth > MAX_DEPTH {
        return Err(XmpError::LimitExceeded("XMP element depth"));
    }
    for attr in &node.attrs {
        if is_rdf_syntax_attr(attr) {
            continue;
        }
        budget.insert(
            out,
            clark(&attr.ns, &attr.local),
            XmpValue::Text(attr.value.clone()),
        )?;
    }
    for child in &node.children {
        if is_description(child) {
            // 节点元素形式的 `rdf:Description` 与当前资源是同一节点，属性并入。
            collect_entries(child, out, budget, depth + 1)?;
        } else {
            let value = property_value(child, budget, depth + 1)?;
            budget.insert(out, clark(&child.ns, &child.local), value)?;
        }
    }
    Ok(())
}

fn is_rdf_syntax_attr(attr: &XmlAttr) -> bool {
    attr.ns == XML_NS
        || (attr.ns == RDF_NS
            && matches!(
                attr.local.as_str(),
                "about" | "ID" | "nodeID" | "parseType" | "datatype" | "resource"
            ))
}

fn property_value(
    node: &XmlNode,
    budget: &mut PropertyBudget,
    depth: usize,
) -> Result<XmpValue, XmpError> {
    if depth > MAX_DEPTH {
        return Err(XmpError::LimitExceeded("XMP element depth"));
    }
    if is_description(node) {
        return description_value(node, budget, depth);
    }
    if let Some(reference) = find_attr(node, RDF_NS, "resource") {
        return reference_value(node, reference);
    }
    match (
        find_attr(node, RDF_NS, "parseType"),
        find_attr(node, RDF_NS, "datatype"),
    ) {
        (Some(_), Some(_)) => Err(malformed(
            "rdf:datatype cannot be combined with rdf:parseType",
        )),
        (Some(parse_type), None) => match parse_type {
            // 只有 Resource 与 Literal 有结构化表示；Collection 等其他
            // RDF/XML 取值没有对应的 XMP 值形态，按损坏处理。
            "Resource" => resource_value(node, budget, depth),
            "Literal" => Ok(XmpValue::Literal(node.inner.clone())),
            other => Err(malformed(format!(
                "unsupported rdf:parseType {other:?} (only Resource and Literal are supported)"
            ))),
        },
        (None, Some(datatype)) => {
            if !node.children.is_empty() {
                return Err(malformed(
                    "rdf:datatype requires a property element with literal content",
                ));
            }
            validate_datatype(datatype)?;
            Ok(XmpValue::Typed {
                value: Box::new(XmpValue::Text(node.text.trim().to_string())),
                datatype: Some(datatype.to_string()),
                language: find_attr(node, XML_NS, "lang").map(str::to_string),
            })
        }
        (None, None) => {
            if node.children.len() == 1 {
                let child = &node.children[0];
                if is_description(child) {
                    let value = description_value(child, budget, depth + 1)?;
                    return Ok(inherit_language(node, value));
                }
                if let Some(kind) = list_kind(child) {
                    let items = child
                        .children
                        .iter()
                        .filter(|item| item.ns == RDF_NS && item.local == "li")
                        .map(list_item)
                        .collect();
                    return Ok(XmpValue::List { kind, items });
                }
            }
            Ok(XmpValue::Text(node.text.trim().to_string()))
        }
    }
}

/// `xml:lang` 沿元素继承：属性元素上的语言标记补足类型化值缺失的语言。
fn inherit_language(node: &XmlNode, value: XmpValue) -> XmpValue {
    match value {
        XmpValue::Typed {
            value,
            datatype,
            language,
        } => XmpValue::Typed {
            value,
            datatype,
            language: language.or_else(|| find_attr(node, XML_NS, "lang").map(str::to_string)),
        },
        other => other,
    }
}

/// 属性元素上的 `rdf:resource`：只允许空元素、无 `rdf:parseType`/
/// `rdf:datatype` 且无实义文本；URI 已由属性解码完成实体还原。
fn reference_value(node: &XmlNode, reference: &str) -> Result<XmpValue, XmpError> {
    if reference.trim().is_empty() {
        return Err(malformed("rdf:resource must not be empty"));
    }
    if find_attr(node, RDF_NS, "parseType").is_some()
        || find_attr(node, RDF_NS, "datatype").is_some()
    {
        return Err(malformed(
            "rdf:resource cannot be combined with rdf:parseType or rdf:datatype",
        ));
    }
    if !node.children.is_empty() {
        return Err(malformed(
            "rdf:resource property element must not have child elements",
        ));
    }
    if !node.text.trim().is_empty() {
        return Err(malformed(
            "rdf:resource property element must not have text content",
        ));
    }
    Ok(XmpValue::Reference(reference.to_string()))
}

/// `rdf:Description` 作为属性值：含 `rdf:value` 时为类型化值，否则为嵌套资源。
fn description_value(
    node: &XmlNode,
    budget: &mut PropertyBudget,
    depth: usize,
) -> Result<XmpValue, XmpError> {
    if find_attr(node, RDF_NS, "resource").is_some() {
        // 引用只属于属性元素；静默忽略会把引用读成空资源。
        return Err(malformed("rdf:resource is not allowed on rdf:Description"));
    }
    let datatype_attr = find_attr(node, RDF_NS, "datatype");
    let Some(value_child) = node.children.iter().find(|child| is_rdf_value(child)) else {
        if datatype_attr.is_some() || datatype_child_text(node).is_some() {
            return Err(malformed(
                "rdf:Description carries rdf:datatype without rdf:value",
            ));
        }
        return resource_value(node, budget, depth);
    };
    // `rdf:value`/`rdf:datatype` 之外的子元素是限定属性；有 `rdf:datatype`
    // 时无法无损折叠进 Typed，按损坏处理；否则整体保留为资源。
    let has_qualifiers = node
        .children
        .iter()
        .any(|child| !(child.ns == RDF_NS && matches!(child.local.as_str(), "value" | "datatype")));
    if has_qualifiers {
        if datatype_attr.is_some() {
            return Err(malformed(
                "typed rdf:Description must not carry extra properties",
            ));
        }
        return resource_value(node, budget, depth);
    }
    let mut datatype = datatype_uri(node, value_child);
    let mut language = language_of(node, value_child);
    let mut value = property_value(value_child, budget, depth + 1)?;
    if let XmpValue::Typed {
        value: inner,
        datatype: inner_datatype,
        language: inner_language,
    } = value
    {
        datatype = datatype.or(inner_datatype);
        language = language.or(inner_language);
        value = *inner;
    }
    if let Some(uri) = &datatype {
        validate_datatype(uri)?;
    }
    Ok(XmpValue::Typed {
        value: Box::new(value),
        datatype,
        language,
    })
}

fn resource_value(
    node: &XmlNode,
    budget: &mut PropertyBudget,
    depth: usize,
) -> Result<XmpValue, XmpError> {
    let mut map = BTreeMap::new();
    collect_entries(node, &mut map, budget, depth)?;
    finish_resource(node, map)
}

/// 仅含 `rdf:value`（及可选 `rdf:datatype`）的资源按 XMP 语义折叠为
/// [`XmpValue::Typed`]；存在其他属性时保留为 [`XmpValue::Resource`] 以免丢数据。
fn finish_resource(node: &XmlNode, map: BTreeMap<String, XmpValue>) -> Result<XmpValue, XmpError> {
    let value_key = clark(RDF_NS, "value");
    let datatype_key = clark(RDF_NS, "datatype");
    let only_value_and_datatype = map
        .keys()
        .all(|key| key == &value_key || key == &datatype_key);
    if !map.contains_key(&value_key) || !only_value_and_datatype {
        return Ok(XmpValue::Resource(map));
    }
    let datatype = match map.get(&datatype_key) {
        None => None,
        Some(XmpValue::Text(uri)) => Some(uri.clone()),
        Some(_) => return Ok(XmpValue::Resource(map)),
    };
    let (value, inner_datatype, inner_language) = match map.get(&value_key) {
        Some(XmpValue::Text(text)) => (XmpValue::Text(text.clone()), None, None),
        Some(XmpValue::Typed {
            value,
            datatype,
            language,
        }) => ((**value).clone(), datatype.clone(), language.clone()),
        _ => return Ok(XmpValue::Resource(map)),
    };
    let datatype = datatype.or(inner_datatype);
    let value_lang = node
        .children
        .iter()
        .find(|child| is_rdf_value(child))
        .and_then(|child| find_attr(child, XML_NS, "lang"))
        .map(str::to_string);
    let language = find_attr(node, XML_NS, "lang")
        .map(str::to_string)
        .or(value_lang)
        .or(inner_language);
    if let Some(uri) = &datatype {
        validate_datatype(uri)?;
    }
    Ok(XmpValue::Typed {
        value: Box::new(value),
        datatype,
        language,
    })
}

fn list_kind(node: &XmlNode) -> Option<XmpListKind> {
    match (node.ns.as_str(), node.local.as_str()) {
        (RDF_NS, "Alt") => Some(XmpListKind::Alt),
        (RDF_NS, "Seq") => Some(XmpListKind::Seq),
        (RDF_NS, "Bag") => Some(XmpListKind::Bag),
        _ => None,
    }
}

fn list_item(node: &XmlNode) -> XmpListItem {
    XmpListItem {
        lang: find_attr(node, XML_NS, "lang").map(str::to_string),
        value: node.text.trim().to_string(),
    }
}

fn find_attr<'a>(node: &'a XmlNode, ns: &str, local: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|attr| attr.ns == ns && attr.local == local)
        .map(|attr| attr.value.as_str())
}

/// 类型化值的 `rdf:datatype`：优先 `rdf:Description` 上的属性，其次
/// `rdf:value` 元素上的属性，最后接受 `<rdf:datatype>` 子元素文本。
fn datatype_uri(node: &XmlNode, value: &XmlNode) -> Option<String> {
    find_attr(node, RDF_NS, "datatype")
        .or_else(|| find_attr(value, RDF_NS, "datatype"))
        .map(str::to_string)
        .or_else(|| datatype_child_text(node))
}

fn datatype_child_text(node: &XmlNode) -> Option<String> {
    node.children
        .iter()
        .find(|child| child.ns == RDF_NS && child.local == "datatype")
        .map(|child| child.text.trim().to_string())
}

fn language_of(node: &XmlNode, value: &XmlNode) -> Option<String> {
    find_attr(node, XML_NS, "lang")
        .or_else(|| find_attr(value, XML_NS, "lang"))
        .map(str::to_string)
}

/// `rdf:datatype` 必须是绝对 URI（`scheme ":"` 前缀，无空白/控制字符）。
/// 本库不解释 URI、不做类型转换，只保留原字符串。
fn validate_datatype(uri: &str) -> Result<(), XmpError> {
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
    if !valid_scheme || uri.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(malformed(format!("invalid rdf:datatype URI {uri:?}")));
    }
    Ok(())
}

fn clark(ns: &str, local: &str) -> String {
    if ns.is_empty() {
        local.to_string()
    } else {
        format!("{{{ns}}}{local}")
    }
}
