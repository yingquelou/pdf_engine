use super::helpers::{entry, integer, invalid, number};
use super::model::{SoftMask, SoftMaskSubtype};
use super::{BTreeMap, Dict, Document, Result, Value};

/// Parse a soft-mask dictionary (`Table 144`).
pub(super) fn parse_soft_mask(document: &Document, dict: &Dict) -> Result<SoftMask> {
    let subtype = match entry(document, dict, "S")? {
        None => return Err(invalid("soft mask /S is required")),
        Some(Value::Name(name)) if name == "Alpha" => SoftMaskSubtype::Alpha,
        Some(Value::Name(name)) if name == "Luminosity" => SoftMaskSubtype::Luminosity,
        Some(_) => return Err(invalid("soft mask /S must be /Alpha or /Luminosity")),
    };

    let group = dict
        .get("G")
        .ok_or_else(|| invalid("soft mask /G is required"))?
        .clone();
    let resolved_group = document.resolve_graphics_state_value(&group)?;
    let Value::Stream(stream_dict, _) = &resolved_group else {
        return Err(invalid(
            "soft mask /G must be a Form or Image XObject stream",
        ));
    };
    let subtype_value = stream_dict
        .get("Subtype")
        .map(|value| document.resolve_graphics_state_value(value))
        .transpose()?;
    match subtype_value.as_ref().and_then(Value::as_name) {
        Some("Form" | "Image") => {}
        _ => {
            return Err(invalid(
                "soft mask /G must be a Form or Image XObject stream",
            ));
        }
    }

    let backdrop = match entry(document, dict, "BC")? {
        None => None,
        Some(value) => {
            let array = value
                .as_array()
                .ok_or_else(|| invalid("soft mask /BC must be an array"))?;
            if array.is_empty() {
                return Err(invalid("soft mask /BC must not be empty"));
            }
            let mut components = Vec::with_capacity(array.len());
            for item in array.iter() {
                let item = document.resolve_graphics_state_value(item)?;
                let Some(component) = number(&item) else {
                    return Err(invalid("soft mask /BC entries must be numbers"));
                };
                if !(0.0..=1.0).contains(&component) {
                    return Err(invalid("soft mask /BC entries must be within 0..=1"));
                }
                components.push(component);
            }
            Some(components)
        }
    };

    let transfer = entry(document, dict, "TR")?;
    if let Some(Value::Name(name)) = &transfer
        && name.as_str() != "Identity"
    {
        return Err(invalid("soft mask /TR name must be /Identity"));
    }

    let smask_in_data = match entry(document, dict, "SMaskInData")? {
        None => None,
        Some(value) => {
            let Some(code) = integer(&value) else {
                return Err(invalid("soft mask /SMaskInData must be an integer"));
            };
            if !(0..=2).contains(&code) {
                return Err(invalid("soft mask /SMaskInData must be 0, 1 or 2"));
            }
            Some(code as u8)
        }
    };

    let matte = match entry(document, dict, "Matte")? {
        None => None,
        Some(value) => {
            let array = value
                .as_array()
                .ok_or_else(|| invalid("soft mask /Matte must be an array"))?;
            if array.is_empty() {
                return Err(invalid("soft mask /Matte must not be empty"));
            }
            let mut components = Vec::with_capacity(array.len());
            for item in array.iter() {
                let item = document.resolve_graphics_state_value(item)?;
                let Some(component) = number(&item) else {
                    return Err(invalid("soft mask /Matte entries must be numbers"));
                };
                components.push(component);
            }
            Some(components)
        }
    };

    const SOFT_MASK_ENTRIES: [&str; 7] = ["Type", "S", "G", "BC", "TR", "SMaskInData", "Matte"];
    let mut other = BTreeMap::new();
    for (key, value) in &dict.0 {
        if !SOFT_MASK_ENTRIES.contains(&key.as_str()) {
            other.insert(key.clone(), value.clone());
        }
    }

    Ok(SoftMask {
        subtype,
        group,
        backdrop,
        transfer,
        smask_in_data,
        matte,
        other,
    })
}
