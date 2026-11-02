//! Catalog `/OCProperties` entry point and the parsing of the `/OCGs` array,
//! OCG dictionaries and the `/D` default configuration, plus the PDF text and
//! hex string decoding helpers they rely on.

use crate::core::types::{Dict, Document, PdfTextString, Value};

use super::error::{Result, invalid};
use super::model::{
    BaseState, OptionalContentConfiguration, OptionalContentGroup, OptionalContentProperties,
};

impl Document {
    fn resolve_optional_content_value(&self, value: &Value) -> Result<Value> {
        match value {
            // 按对象号取最新修订，忽略引用中的代号（增量更新后引用可能写旧代）。
            Value::Ref(num, generation) => {
                let object = self.get_object(*num)?.ok_or_else(|| {
                    invalid(format!("unresolved object reference {num} {generation} R"))
                })?;
                Ok(object.value)
            }
            _ => Ok(value.clone()),
        }
    }

    /// Parse the catalog's `/OCProperties` entry (Table 100).
    ///
    /// Returns `Ok(None)` when the catalog has no `/OCProperties`.
    pub fn optional_content_properties(&self) -> Result<Option<OptionalContentProperties>> {
        let Some((root, generation)) = self.trailer.root() else {
            return Err(invalid("trailer is missing /Root"));
        };
        let catalog = self.get_object(root)?.ok_or_else(|| {
            invalid(format!(
                "unresolved catalog object reference {root} {generation} R"
            ))
        })?;
        let catalog = catalog
            .value
            .as_dict()
            .ok_or_else(|| invalid("catalog is not a dictionary"))?;
        let Some(properties) = catalog.get("OCProperties") else {
            return Ok(None);
        };
        let properties = self.resolve_optional_content_value(properties)?;
        let properties = properties
            .as_dict()
            .ok_or_else(|| invalid("/OCProperties must be a dictionary"))?;
        parse_properties(self, properties).map(Some)
    }
}

fn parse_properties(document: &Document, dict: &Dict) -> Result<OptionalContentProperties> {
    let groups_value = dict
        .get("OCGs")
        .ok_or_else(|| invalid("/OCProperties /OCGs is required"))?;
    let groups_value = document.resolve_optional_content_value(groups_value)?;
    let groups_array = groups_value
        .as_array()
        .ok_or_else(|| invalid("/OCProperties /OCGs must be an array"))?;
    let mut groups = Vec::with_capacity(groups_array.len());
    for item in groups_array.iter() {
        let Value::Ref(object, generation) = item else {
            return Err(invalid(
                "/OCProperties /OCGs entries must be indirect references",
            ));
        };
        // 按对象号取最新修订，忽略引用中的代号。
        let ocg = document.get_object(*object)?.ok_or_else(|| {
            invalid(format!(
                "unresolved OCG object reference {object} {generation} R"
            ))
        })?;
        groups.push(parse_group(document, *object, &ocg.value)?);
    }

    let default = match dict.get("D") {
        None => None,
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            let value = value
                .as_dict()
                .ok_or_else(|| invalid("/OCProperties /D must be a dictionary"))?;
            Some(parse_configuration(document, value)?)
        }
    };

    Ok(OptionalContentProperties { groups, default })
}

fn parse_group(document: &Document, object: i64, value: &Value) -> Result<OptionalContentGroup> {
    let dict = value
        .as_dict()
        .ok_or_else(|| invalid(format!("OCG {object} is not a dictionary")))?;
    if let Some(type_value) = dict.get("Type") {
        let type_value = document.resolve_optional_content_value(type_value)?;
        if type_value.as_name() != Some("OCG") {
            return Err(invalid(format!("OCG {object} /Type must be /OCG")));
        }
    }
    let name = match dict.get("Name") {
        None => return Err(invalid(format!("OCG {object} is missing /Name"))),
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            text_string(&value)
                .ok_or_else(|| invalid(format!("OCG {object} /Name must be a text string")))?
        }
    };
    let intent = match dict.get("Intent") {
        None => Vec::new(),
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            match value {
                Value::Name(name) => vec![name],
                Value::Array(items) => {
                    let mut names = Vec::with_capacity(items.len());
                    for item in items.iter() {
                        let item = document.resolve_optional_content_value(item)?;
                        let Some(name) = item.as_name() else {
                            return Err(invalid(format!(
                                "OCG {object} /Intent entries must be names"
                            )));
                        };
                        names.push(name.to_string());
                    }
                    names
                }
                _ => {
                    return Err(invalid(format!(
                        "OCG {object} /Intent must be a name or an array of names"
                    )));
                }
            }
        }
    };
    Ok(OptionalContentGroup {
        object,
        name,
        intent,
    })
}

fn parse_configuration(document: &Document, dict: &Dict) -> Result<OptionalContentConfiguration> {
    let base_state = match dict.get("BaseState") {
        None => BaseState::On,
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            match value.as_name() {
                Some("ON") => BaseState::On,
                Some("OFF") => BaseState::Off,
                Some("Unchanged") => BaseState::Unchanged,
                _ => {
                    return Err(invalid(
                        "/OCProperties /D /BaseState must be /ON, /OFF or /Unchanged",
                    ));
                }
            }
        }
    };
    let on = object_references(document, dict, "ON")?;
    let off = object_references(document, dict, "OFF")?;
    let order = match dict.get("Order") {
        None => Vec::new(),
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            let mut order = Vec::new();
            collect_order(&value, &mut order)?;
            order
        }
    };
    Ok(OptionalContentConfiguration {
        base_state,
        on,
        off,
        order,
    })
}

fn object_references(document: &Document, dict: &Dict, key: &str) -> Result<Vec<i64>> {
    let Some(value) = dict.get(key) else {
        return Ok(Vec::new());
    };
    let value = document.resolve_optional_content_value(value)?;
    let array = value
        .as_array()
        .ok_or_else(|| invalid(format!("/OCProperties /D /{key} must be an array")))?;
    let mut objects = Vec::with_capacity(array.len());
    for item in array.iter() {
        let Some((object, _)) = item.as_ref() else {
            return Err(invalid(format!(
                "/OCProperties /D /{key} entries must be indirect references"
            )));
        };
        objects.push(object);
    }
    Ok(objects)
}

fn collect_order(value: &Value, order: &mut Vec<i64>) -> Result<()> {
    match value {
        Value::Ref(object, _) => order.push(*object),
        Value::Array(items) => {
            for item in items.iter() {
                collect_order(item, order)?;
            }
        }
        _ => {
            return Err(invalid(
                "/OCProperties /D /Order entries must be OCG references or arrays",
            ));
        }
    }
    Ok(())
}

fn text_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::HexString(hex) => {
            let bytes = hex_bytes(hex)?;
            PdfTextString::decode(&bytes)
                .ok()
                .map(PdfTextString::into_string)
        }
        _ => None,
    }
}

fn hex_bytes(value: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for index in (0..digits.len()).step_by(2) {
        let high = (digits[index] as char).to_digit(16)?;
        let low = (digits[index + 1] as char).to_digit(16)?;
        bytes.push((high * 16 + low) as u8);
    }
    Some(bytes)
}
