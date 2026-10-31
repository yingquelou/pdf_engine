//! 逻辑结构树的根映射、属性/类与页面父信息解析。
//!
//! 这些条目不参与结构元素 arena 的递归展开：`/RoleMap`、`/ClassMap`
//! 在根层读取，`/A`、`/C` 属于单个结构元素，页面 `/StructParents`
//! 在结构树构建完成后按父树查找。属性对象按 14.7.5（Table 327–329）
//! 校验：`/O` 必填，用户属性属性对象另带 `/P` 数组。

use std::collections::BTreeMap;

use crate::core::reader::content::text_value;
use crate::core::types::{Dict, Document, Value};

use super::{
    MAX_STRUCTURE_CHILDREN, MAX_STRUCTURE_MAP_ENTRIES, PageStructureParents, ParentTreeEntry,
    StructureAttribute, StructureClass, StructureError, UserProperties, UserProperty, malformed,
    resolve_value,
};

/// `/ID`：字面串每个 `char` 作一字节；十六进制串奇数位末尾补 0。
pub(super) fn id_bytes(value: &Value) -> Result<Vec<u8>, StructureError> {
    match value {
        Value::String(text) => Ok(text
            .chars()
            .map(|character| character as u32 as u8)
            .collect()),
        Value::HexString(hex) => {
            let mut digits: Vec<u8> = hex
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect();
            if digits.len() % 2 == 1 {
                digits.push(b'0');
            }
            let mut bytes = Vec::with_capacity(digits.len() / 2);
            for pair in digits.as_chunks::<2>().0 {
                let high = hex_digit(pair[0])
                    .ok_or_else(|| malformed("/ID is not a valid hexadecimal string"))?;
                let low = hex_digit(pair[1])
                    .ok_or_else(|| malformed("/ID is not a valid hexadecimal string"))?;
                bytes.push(high * 16 + low);
            }
            Ok(bytes)
        }
        other => Err(malformed(format!(
            "/ID must be a string or hexadecimal string ({})",
            other.type_name()
        ))),
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// 文本条目：只接受字符串/十六进制串，经 `content::text_value` 解码。
pub(super) fn optional_text(dict: &Dict, key: &str) -> Result<Option<String>, StructureError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    if !matches!(value, Value::String(_) | Value::HexString(_)) {
        return Err(malformed(format!(
            "structure element /{key} must be a text string ({})",
            value.type_name()
        )));
    }
    text_value(value)
        .map(Some)
        .ok_or_else(|| malformed(format!("structure element /{key} cannot be decoded")))
}

/// `/R`：非负整数，缺省 0。
pub(super) fn parse_revision(dict: &Dict) -> Result<u32, StructureError> {
    match dict.get("R") {
        None => Ok(0),
        Some(Value::Integer(value)) if *value >= 0 && *value <= u32::MAX as i64 => {
            Ok(*value as u32)
        }
        Some(other) => Err(malformed(format!(
            "structure element /R must be a non-negative integer ({})",
            other.type_name()
        ))),
    }
}

/// `/A`：单个属性对象或数组；数组项后的整数为前一项修订号。
pub(super) fn parse_attributes(
    document: &Document,
    dict: &Dict,
) -> Result<Vec<StructureAttribute>, StructureError> {
    let Some(value) = dict.get("A") else {
        return Ok(Vec::new());
    };
    let value = resolve_value(document, value, "structure element /A")?;
    let items = match value {
        Value::Array(items) => {
            if items.0.len() > MAX_STRUCTURE_CHILDREN {
                return Err(StructureError::LimitExceeded("structure attributes"));
            }
            items
        }
        single => {
            return Ok(vec![attribute_object(
                document,
                &single,
                "structure attribute",
            )?]);
        }
    };
    let mut attributes = Vec::new();
    let mut index = 0;
    while index < items.0.len() {
        let item = &items.0[index];
        if matches!(item, Value::Integer(_)) {
            return Err(malformed(
                "structure attribute revision number has no preceding attribute",
            ));
        }
        let mut attribute = attribute_object(document, item, "structure attribute")?;
        if let Some(Value::Integer(revision)) = items.0.get(index + 1) {
            index += 1;
            attribute.revision = Some(non_negative_revision(*revision, "structure attribute")?);
        }
        attributes.push(attribute);
        index += 1;
    }
    Ok(attributes)
}

/// 属性对象（14.7.5.2 Table 327）：解引用后必须是字典或流，`/O` 必填。
///
/// `/O` 为 `/UserProperties` 时按 Table 328/329 解析 `/P` 用户属性数组。
fn attribute_object(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<StructureAttribute, StructureError> {
    let resolved = resolve_value(document, value, context)?;
    let Some(dict) = resolved.as_dict() else {
        return Err(malformed(format!(
            "structure attribute must be a dictionary or stream ({})",
            resolved.type_name()
        )));
    };
    let owner = match dict.get("O") {
        Some(Value::Name(name)) if !name.is_empty() => name.clone(),
        Some(Value::Name(_)) => {
            return Err(malformed("structure attribute /O must not be empty"));
        }
        Some(other) => {
            return Err(malformed(format!(
                "structure attribute /O must be a name ({})",
                other.type_name()
            )));
        }
        None => return Err(malformed("structure attribute /O is required (Table 327)")),
    };
    let user_properties = if owner == "UserProperties" {
        Some(parse_user_properties(document, dict)?)
    } else {
        None
    };
    Ok(StructureAttribute {
        owner,
        value: resolved,
        revision: None,
        user_properties,
    })
}

/// 用户属性属性对象（14.7.5.3 Table 328/329）。
fn parse_user_properties(
    document: &Document,
    dict: &Dict,
) -> Result<UserProperties, StructureError> {
    let properties = match dict.get("P") {
        Some(Value::Array(items)) => items,
        Some(other) => {
            return Err(malformed(format!(
                "user properties /P must be an array ({})",
                other.type_name()
            )));
        }
        None => return Err(malformed("user properties /P is required (Table 328)")),
    };
    if properties.0.len() > MAX_STRUCTURE_CHILDREN {
        return Err(StructureError::LimitExceeded("structure user properties"));
    }
    let mut parsed = Vec::with_capacity(properties.0.len());
    for (index, item) in properties.0.iter().enumerate() {
        let resolved = resolve_value(document, item, "user property")?;
        let Some(property) = resolved.as_dict() else {
            return Err(malformed(format!(
                "user property {index} must be a dictionary ({})",
                resolved.type_name()
            )));
        };
        let name = user_property_text(property, "N", index)?.ok_or_else(|| {
            malformed(format!("user property {index} /N is required (Table 329)"))
        })?;
        let Some(value) = property.get("V") else {
            return Err(malformed(format!(
                "user property {index} /V is required (Table 329)"
            )));
        };
        let formatted = user_property_text(property, "F", index)?;
        let hidden = match property.get("H") {
            None => false,
            Some(Value::Bool(hidden)) => *hidden,
            Some(other) => {
                return Err(malformed(format!(
                    "user property {index} /H must be a boolean ({})",
                    other.type_name()
                )));
            }
        };
        parsed.push(UserProperty {
            name,
            value: value.clone(),
            formatted,
            hidden,
            dict: property.clone(),
        });
    }
    Ok(UserProperties {
        properties: parsed,
        dict: dict.clone(),
    })
}

/// 用户属性文本条目：只接受字符串/十六进制串；缺失返回 `None`。
fn user_property_text(
    dict: &Dict,
    key: &str,
    index: usize,
) -> Result<Option<String>, StructureError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    if !matches!(value, Value::String(_) | Value::HexString(_)) {
        return Err(malformed(format!(
            "user property {index} /{key} must be a text string ({})",
            value.type_name()
        )));
    }
    text_value(value)
        .map(Some)
        .ok_or_else(|| malformed(format!("user property {index} /{key} cannot be decoded")))
}

/// `/C`：单个类名或数组；数组项后的整数为前一项修订号。
pub(super) fn parse_classes(
    document: &Document,
    dict: &Dict,
) -> Result<Vec<StructureClass>, StructureError> {
    let Some(value) = dict.get("C") else {
        return Ok(Vec::new());
    };
    let value = resolve_value(document, value, "structure element /C")?;
    let items = match value {
        Value::Array(items) => items,
        single => {
            let class = class_value(&single)?;
            return Ok(vec![StructureClass {
                name: class,
                revision: None,
                attributes: Vec::new(),
            }]);
        }
    };
    let mut classes = Vec::new();
    let mut index = 0;
    while index < items.0.len() {
        let item = &items.0[index];
        if matches!(item, Value::Integer(_)) {
            return Err(malformed(
                "structure class revision number has no preceding class",
            ));
        }
        let name = class_value(item)?;
        let revision = match items.0.get(index + 1) {
            Some(Value::Integer(revision)) => {
                index += 1;
                Some(non_negative_revision(*revision, "structure class")?)
            }
            _ => None,
        };
        classes.push(StructureClass {
            name,
            revision,
            attributes: Vec::new(),
        });
        index += 1;
    }
    Ok(classes)
}

/// 类名必须是 `Value::Name` 且非空。
fn class_value(value: &Value) -> Result<String, StructureError> {
    match value {
        Value::Name(name) if !name.is_empty() => Ok(name.clone()),
        Value::Name(_) => Err(malformed("structure class name must not be empty")),
        other => Err(malformed(format!(
            "structure class must be a name ({})",
            other.type_name()
        ))),
    }
}

fn non_negative_revision(value: i64, context: &str) -> Result<u32, StructureError> {
    if value < 0 || value > u32::MAX as i64 {
        return Err(malformed(format!(
            "{context} revision number must be a non-negative integer"
        )));
    }
    Ok(value as u32)
}

/// `/RoleMap`：名字 → 名字，键值均非空。
pub(super) fn parse_role_map(
    document: &Document,
    root: &Dict,
) -> Result<BTreeMap<String, String>, StructureError> {
    let Some(value) = root.get("RoleMap") else {
        return Ok(BTreeMap::new());
    };
    let value = resolve_value(document, value, "/RoleMap")?;
    let dict = value
        .as_dict()
        .ok_or_else(|| malformed("/RoleMap is not a dictionary"))?;
    check_map_limit(dict, "RoleMap entries")?;
    let mut map = BTreeMap::new();
    for (key, value) in &dict.0 {
        if key.is_empty() {
            return Err(malformed("/RoleMap key must not be empty"));
        }
        let target = match value {
            Value::Name(name) if !name.is_empty() => name.clone(),
            Value::Name(_) => {
                return Err(malformed(format!(
                    "/RoleMap /{key} target must not be empty"
                )));
            }
            other => {
                return Err(malformed(format!(
                    "/RoleMap /{key} must be a name ({})",
                    other.type_name()
                )));
            }
        };
        map.insert(key.clone(), target);
    }
    Ok(map)
}

/// `/ClassMap`：类名 → 属性对象列表（14.7.5.2）。
///
/// 每个值可以是单个属性对象或属性对象数组，逐项按 Table 327 校验；类
/// 属性不带修订号（类属性没有继承，修订号只出现在元素 `/C` 中）。
pub(super) fn parse_class_map(
    document: &Document,
    root: &Dict,
) -> Result<BTreeMap<String, Vec<StructureAttribute>>, StructureError> {
    let Some(value) = root.get("ClassMap") else {
        return Ok(BTreeMap::new());
    };
    let value = resolve_value(document, value, "/ClassMap")?;
    let dict = value
        .as_dict()
        .ok_or_else(|| malformed("/ClassMap is not a dictionary"))?;
    check_map_limit(dict, "ClassMap entries")?;
    let mut map = BTreeMap::new();
    for (key, value) in &dict.0 {
        let context = format!("/ClassMap /{key}");
        let attributes = match value {
            Value::Array(items) => {
                if items.0.len() > MAX_STRUCTURE_CHILDREN {
                    return Err(StructureError::LimitExceeded(
                        "structure class map attributes",
                    ));
                }
                let mut attributes = Vec::with_capacity(items.0.len());
                for item in &items.0 {
                    attributes.push(attribute_object(document, item, &context)?);
                }
                attributes
            }
            single => vec![attribute_object(document, single, &context)?],
        };
        map.insert(key.clone(), attributes);
    }
    Ok(map)
}

fn check_map_limit(dict: &Dict, what: &'static str) -> Result<(), StructureError> {
    if dict.0.len() > MAX_STRUCTURE_MAP_ENTRIES {
        return Err(StructureError::LimitExceeded(what));
    }
    Ok(())
}

/// 页面 `/StructParents` → 父树页面数组。
pub(super) fn page_struct_parents(
    document: &Document,
    page_index: usize,
) -> Result<Option<PageStructureParents>, StructureError> {
    let (page_value, _) = document
        .locate_page_node_with_ref(page_index)
        .map_err(|error| malformed(format!("page {page_index}: {error}")))?;
    let page = page_value
        .as_dict()
        .ok_or_else(|| malformed(format!("page {page_index} is not a dictionary")))?;
    let Some(value) = page.get("StructParents") else {
        return Ok(None);
    };
    if page.get("StructParent").is_some() {
        return Err(malformed(format!(
            "page {page_index} has both /StructParents and /StructParent"
        )));
    }
    let key = match value {
        Value::Integer(key) if *key >= 0 => *key,
        other => {
            return Err(malformed(format!(
                "page /StructParents must be a non-negative integer ({})",
                other.type_name()
            )));
        }
    };
    let Some(tree) = document.structure_tree()? else {
        return Err(malformed("document has no structure tree"));
    };
    let Some(entry) = tree.parent_tree_entry(key) else {
        return Err(malformed(format!(
            "parent tree has no entry for /StructParents key {key}"
        )));
    };
    match entry {
        ParentTreeEntry::Page(parents) => Ok(Some(PageStructureParents {
            key,
            parents: parents.clone(),
        })),
        ParentTreeEntry::Object(_) => Err(malformed(format!(
            "parent tree entry for /StructParents key {key} is not a page MCID array"
        ))),
    }
}
