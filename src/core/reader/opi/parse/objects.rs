use crate::core::reader::action::{FileSpec, parse_file_spec};
use crate::core::reader::navigation::resolve;
use crate::core::reader::opi::{
    MAX_OPI_ARRAY, OpiColor, OpiError, OpiInks, OpiTagText, limit_exceeded, malformed,
};
use crate::core::types::{Dict, Document, Value};

use super::accessors::{
    required_bytes, required_integer_for, required_number_for, required_text, resolved_array,
};

pub(super) fn parse_color(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<OpiColor, OpiError> {
    let array = resolved_array(document, value, &format!("{context} /Color"))?;
    if array.0.len() != 5 {
        return Err(malformed(format!(
            "{context} /Color must contain four numbers and a colour name, got {} items",
            array.0.len()
        )));
    }
    let mut components = [0.0; 4];
    for (index, component) in components.iter_mut().enumerate() {
        *component = required_number_for(
            document,
            &array.0[index],
            &format!("{context} /Color component {index}"),
        )?;
        if !(0.0..=1.0).contains(component) {
            return Err(malformed(format!(
                "{context} /Color components must be in 0.0–1.0, got {component}"
            )));
        }
    }
    let color_name = required_bytes(
        document,
        &array.0[4],
        &format!("{context} /Color colour name"),
    )?;
    Ok(OpiColor {
        cmyk: components,
        color_name,
    })
}

pub(super) fn parse_inks(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<OpiInks, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context} /Inks: {error}")))?;
    match &resolved {
        Value::Name(name) => match name.as_str() {
            "full_color" => Ok(OpiInks::FullColor),
            "registration" => Ok(OpiInks::Registration),
            other => Err(malformed(format!(
                "{context} /Inks name must be full_color or registration, got /{other}"
            ))),
        },
        Value::Array(array) => {
            if array.0.is_empty() {
                return Err(malformed(format!("{context} /Inks array is empty")));
            }
            let first = resolve(document, &array.0[0])
                .map_err(|error| malformed(format!("{context} /Inks: {error}")))?;
            if first.as_name() != Some("monochrome") {
                return Err(malformed(format!(
                    "{context} /Inks array must start with /monochrome"
                )));
            }
            let rest = &array.0[1..];
            if rest.len() % 2 != 0 {
                return Err(malformed(format!(
                    "{context} /Inks array must contain name/tint pairs"
                )));
            }
            let count = rest.len() / 2;
            if count > MAX_OPI_ARRAY {
                return Err(limit_exceeded("OPI 2.0 /Inks"));
            }
            let mut inks = Vec::with_capacity(count);
            for pair in rest.as_chunks::<2>().0 {
                let name = required_bytes(document, &pair[0], &format!("{context} /Inks name"))?;
                let tint =
                    required_number_for(document, &pair[1], &format!("{context} /Inks tint"))?;
                if !(0.0..=1.0).contains(&tint) {
                    return Err(malformed(format!(
                        "{context} /Inks tint must be in 0.0–1.0, got {tint}"
                    )));
                }
                inks.push((name, tint));
            }
            Ok(OpiInks::Monochrome(inks))
        }
        other => Err(malformed(format!(
            "{context} /Inks must be a name or an array, got {}",
            other.type_name()
        ))),
    }
}

pub(super) fn parse_tags_v13(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<Vec<(i64, String)>, OpiError> {
    let Some(value) = dict.get("Tags") else {
        return Ok(Vec::new());
    };
    let array = resolved_array(document, value, &format!("{context} /Tags"))?;
    if array.0.len() > MAX_OPI_ARRAY {
        return Err(limit_exceeded("OPI 1.3 /Tags"));
    }
    if array.0.len() % 2 != 0 {
        return Err(malformed(format!(
            "{context} /Tags must contain tag number / text pairs"
        )));
    }
    let mut tags = Vec::with_capacity(array.0.len() / 2);
    for pair in array.0.as_chunks::<2>().0 {
        let tag = required_integer_for(document, &pair[0], &format!("{context} /Tags tag number"))?;
        let text = required_text(document, &pair[1], &format!("{context} /Tags text"))?;
        tags.push((tag, text));
    }
    Ok(tags)
}

pub(super) fn parse_tags_v20(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<Vec<(i64, OpiTagText)>, OpiError> {
    let Some(value) = dict.get("Tags") else {
        return Ok(Vec::new());
    };
    let array = resolved_array(document, value, &format!("{context} /Tags"))?;
    if array.0.len() > MAX_OPI_ARRAY {
        return Err(limit_exceeded("OPI 2.0 /Tags"));
    }
    if array.0.len() % 2 != 0 {
        return Err(malformed(format!(
            "{context} /Tags must contain tag number / text pairs"
        )));
    }
    let mut tags = Vec::with_capacity(array.0.len() / 2);
    for pair in array.0.as_chunks::<2>().0 {
        let tag = required_integer_for(document, &pair[0], &format!("{context} /Tags tag number"))?;
        let resolved = resolve(document, &pair[1])
            .map_err(|error| malformed(format!("{context} /Tags text: {error}")))?;
        let text = match &resolved {
            Value::Array(texts) => {
                if texts.0.len() > MAX_OPI_ARRAY {
                    return Err(limit_exceeded("OPI 2.0 /Tags text array"));
                }
                let mut values = Vec::with_capacity(texts.0.len());
                for item in &texts.0 {
                    values.push(required_text(
                        document,
                        item,
                        &format!("{context} /Tags text"),
                    )?);
                }
                OpiTagText::Texts(values)
            }
            _ => OpiTagText::Text(required_text(
                document,
                &resolved,
                &format!("{context} /Tags text"),
            )?),
        };
        tags.push((tag, text));
    }
    Ok(tags)
}

pub(super) fn required_file(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<FileSpec, OpiError> {
    let value = dict
        .get("F")
        .ok_or_else(|| malformed(format!("{context} /F is required")))?;
    parse_file_spec(document, value).map_err(|error| malformed(format!("{context} /F: {error}")))
}
