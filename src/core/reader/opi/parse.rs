//! OPI 版本字典与各条目的解析（Table 368–370）。

use crate::core::reader::action::{FileSpec, parse_file_spec};
use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Array, Dict, Document, Value};

use super::{
    MAX_OPI_ARRAY, Opi13Dictionary, Opi20Dictionary, OpiColor, OpiColorType, OpiError, OpiInks,
    OpiProxy, OpiTagText, limit_exceeded, malformed,
};

/// 从流/字典中解析 `/OPI`；条目缺省返回 `Ok(None)`。
pub(crate) fn parse_opi_entry(
    document: &Document,
    dict: &Dict,
) -> Result<Option<OpiProxy>, OpiError> {
    let Some(value) = dict.get("OPI") else {
        return Ok(None);
    };
    parse_version_dictionary(document, value).map(Some)
}

/// Table 368：单条目版本字典，键为 `1.3` / `2.0`，值为 OPI 字典。
fn parse_version_dictionary(document: &Document, value: &Value) -> Result<OpiProxy, OpiError> {
    let resolved = resolve(document, value)
        .map_err(|error| malformed(format!("OPI version dictionary: {error}")))?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("OPI version dictionary must be a dictionary"))?;
    if dict.0.len() != 1 {
        return Err(malformed(format!(
            "OPI version dictionary must contain exactly one entry, got {}",
            dict.0.len()
        )));
    }
    let (key, value) = dict.0.iter().next().expect("one entry");
    let resolved = resolve(document, value)
        .map_err(|error| malformed(format!("OPI /{key} dictionary: {error}")))?;
    let inner = resolved
        .as_dict()
        .ok_or_else(|| malformed(format!("OPI /{key} value must be a dictionary")))?
        .clone();
    match key.as_str() {
        "1.3" => parse_v13(document, inner).map(OpiProxy::V1_3),
        "2.0" => parse_v20(document, inner).map(OpiProxy::V2_0),
        other => Err(malformed(format!(
            "OPI version dictionary key must be /1.3 or /2.0, got /{other}"
        ))),
    }
}

fn parse_v13(document: &Document, dict: Dict) -> Result<Opi13Dictionary, OpiError> {
    let context = "OPI 1.3 dictionary";
    let type_entry = parse_type_entry(document, &dict, context)?;
    let version = required_number(document, &dict, "Version", context)?;
    if version != 1.3 {
        return Err(malformed(format!(
            "{context} /Version must be the number 1.3, got {version}"
        )));
    }
    let file = required_file(document, &dict, context)?;
    let id = optional_bytes(document, &dict, "ID", context)?;
    let comments = optional_text(document, &dict, "Comments", context)?;
    let size = required_integers::<2>(document, &dict, "Size", context)?;
    let crop_rect = required_integers::<4>(document, &dict, "CropRect", context)?;
    let crop_fixed = optional_numbers::<4>(document, &dict, "CropFixed", context)?;
    let position = required_numbers::<8>(document, &dict, "Position", context)?;
    check_position(&position, context)?;
    let resolution = optional_numbers::<2>(document, &dict, "Resolution", context)?;
    let color_type = match dict.get("ColorType") {
        None => None,
        Some(value) => {
            let name = required_name(document, value, &format!("{context} /ColorType"))?;
            Some(OpiColorType::from_name(&name).ok_or_else(|| {
                malformed(format!(
                    "{context} /ColorType must be Process, Spot or Separation, got /{name}"
                ))
            })?)
        }
    };
    let color = match dict.get("Color") {
        None => None,
        Some(value) => Some(parse_color(document, value, context)?),
    };
    let tint = optional_number(document, &dict, "Tint", context)?;
    if let Some(tint) = tint
        && !(0.0..=1.0).contains(&tint)
    {
        return Err(malformed(format!(
            "{context} /Tint must be in 0.0–1.0, got {tint}"
        )));
    }
    let overprint = optional_bool(document, &dict, "Overprint", context)?;
    let image_type = optional_integers::<2>(document, &dict, "ImageType", context)?;
    let gray_map = match dict.get("GrayMap") {
        None => None,
        Some(value) => {
            let values = integer_list(document, value, &format!("{context} /GrayMap"))?;
            let [_samples, bits] = image_type
                .ok_or_else(|| malformed(format!("{context} /GrayMap requires /ImageType")))?;
            let expected = gray_map_len(bits)?;
            if values.len() != expected {
                return Err(malformed(format!(
                    "{context} /GrayMap must contain 2^{bits} = {expected} integers, got {}",
                    values.len()
                )));
            }
            if values.iter().any(|value| !(0..=65535).contains(value)) {
                return Err(malformed(format!(
                    "{context} /GrayMap values must be in 0–65535"
                )));
            }
            Some(values)
        }
    };
    let transparency = optional_bool(document, &dict, "Transparency", context)?;
    let tags = parse_tags_v13(document, &dict, context)?;

    Ok(Opi13Dictionary {
        type_entry,
        version,
        file,
        id,
        comments,
        size,
        crop_rect,
        crop_fixed,
        position,
        resolution,
        color_type,
        color,
        tint,
        overprint,
        image_type,
        gray_map,
        transparency,
        tags,
        dict,
    })
}

fn parse_v20(document: &Document, dict: Dict) -> Result<Opi20Dictionary, OpiError> {
    let context = "OPI 2.0 dictionary";
    let type_entry = parse_type_entry(document, &dict, context)?;
    let version = required_number(document, &dict, "Version", context)?;
    if version != 2.0 {
        return Err(malformed(format!(
            "{context} /Version must be the number 2 or 2.0, got {version}"
        )));
    }
    let file = required_file(document, &dict, context)?;
    let main_image = optional_bytes(document, &dict, "MainImage", context)?;
    let tags = parse_tags_v20(document, &dict, context)?;
    let size = optional_numbers::<2>(document, &dict, "Size", context)?;
    let crop_rect = optional_numbers::<4>(document, &dict, "CropRect", context)?;
    check_size_and_crop(size, crop_rect, context)?;
    let overprint = optional_bool(document, &dict, "Overprint", context)?;
    let inks = match dict.get("Inks") {
        None => None,
        Some(value) => Some(parse_inks(document, value, context)?),
    };
    let included_image_dimensions =
        optional_integers::<2>(document, &dict, "IncludedImageDimensions", context)?;
    let included_image_quality = match dict.get("IncludedImageQuality") {
        None => None,
        Some(value) => {
            let number =
                required_number_for(document, value, &format!("{context} /IncludedImageQuality"))?;
            if !matches!(number, 1.0 | 2.0 | 3.0) {
                return Err(malformed(format!(
                    "{context} /IncludedImageQuality must be 1, 2 or 3, got {number}"
                )));
            }
            Some(number as u8)
        }
    };

    Ok(Opi20Dictionary {
        type_entry,
        version,
        file,
        main_image,
        tags,
        size,
        crop_rect,
        overprint,
        inks,
        included_image_dimensions,
        included_image_quality,
        dict,
    })
}

fn parse_type_entry(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<Option<String>, OpiError> {
    match dict.get("Type") {
        None => Ok(None),
        Some(value) => {
            let name = required_name(document, value, &format!("{context} /Type"))?;
            if name != "OPI" {
                return Err(malformed(format!(
                    "{context} /Type must be /OPI when present, got /{name}"
                )));
            }
            Ok(Some(name))
        }
    }
}

/// Table 369 `/Position`：`ulx-llx = urx-lrx` 且 `uly-lly = ury-lry`。
fn check_position(position: &[f64; 8], context: &str) -> Result<(), OpiError> {
    const TOLERANCE: f64 = 1e-6;
    let top_width = position[2] - position[0];
    let bottom_width = position[4] - position[6];
    let left_height = position[3] - position[1];
    let right_height = position[5] - position[7];
    if (top_width - bottom_width).abs() > TOLERANCE
        || (left_height - right_height).abs() > TOLERANCE
    {
        return Err(malformed(format!(
            "{context} /Position corners must form a rectangle \
             (ulx-llx = urx-lrx and uly-lly = ury-lry)"
        )));
    }
    Ok(())
}

/// Table 370：`/Size` 与 `/CropRect` 成对出现，且裁切框落在代理尺寸内。
fn check_size_and_crop(
    size: Option<[f64; 2]>,
    crop: Option<[f64; 4]>,
    context: &str,
) -> Result<(), OpiError> {
    match (size, crop) {
        (None, None) => Ok(()),
        (Some(_), None) | (None, Some(_)) => Err(malformed(format!(
            "{context} /Size and /CropRect must both be present or both be absent"
        ))),
        (Some(size), Some(crop)) => {
            if size[0] <= 0.0 || size[1] <= 0.0 {
                return Err(malformed(format!(
                    "{context} /Size must be positive, got [{} {}]",
                    size[0], size[1]
                )));
            }
            if crop[0] < 0.0 || crop[1] < 0.0 || crop[0] >= crop[2] || crop[1] >= crop[3] {
                return Err(malformed(format!(
                    "{context} /CropRect must satisfy 0 <= left < right and 0 <= top < bottom"
                )));
            }
            if crop[2] > size[0] || crop[3] > size[1] {
                return Err(malformed(format!(
                    "{context} /CropRect must lie within /Size [{}, {}]",
                    size[0], size[1]
                )));
            }
            Ok(())
        }
    }
}

fn gray_map_len(bits: i64) -> Result<usize, OpiError> {
    if !(0..=16).contains(&bits) {
        return Err(malformed(format!(
            "OPI 1.3 /ImageType bits per sample must be 0–16, got {bits}"
        )));
    }
    let len = 1usize << bits;
    if len > MAX_OPI_ARRAY {
        return Err(limit_exceeded("OPI 1.3 /GrayMap"));
    }
    Ok(len)
}

fn parse_color(document: &Document, value: &Value, context: &str) -> Result<OpiColor, OpiError> {
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

fn parse_inks(document: &Document, value: &Value, context: &str) -> Result<OpiInks, OpiError> {
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

fn parse_tags_v13(
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

fn parse_tags_v20(
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

fn required_file(document: &Document, dict: &Dict, context: &str) -> Result<FileSpec, OpiError> {
    let value = dict
        .get("F")
        .ok_or_else(|| malformed(format!("{context} /F is required")))?;
    parse_file_spec(document, value).map_err(|error| malformed(format!("{context} /F: {error}")))
}

fn required_number(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<f64, OpiError> {
    let value = dict
        .get(key)
        .ok_or_else(|| malformed(format!("{context} /{key} is required")))?;
    required_number_for(document, value, &format!("{context} /{key}"))
}

fn required_number_for(document: &Document, value: &Value, context: &str) -> Result<f64, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    match resolved {
        Value::Integer(number) => Ok(number as f64),
        Value::Float(number) if number.is_finite() => Ok(number),
        Value::Float(_) => Err(malformed(format!("{context} must be finite"))),
        other => Err(malformed(format!(
            "{context} must be a number, got {}",
            other.type_name()
        ))),
    }
}

fn required_integer_for(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<i64, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    match resolved {
        Value::Integer(number) => Ok(number),
        Value::Float(number) if number.is_finite() && number.fract() == 0.0 => Ok(number as i64),
        other => Err(malformed(format!(
            "{context} must be an integer, got {}",
            other.type_name()
        ))),
    }
}

fn required_name(document: &Document, value: &Value, context: &str) -> Result<String, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    resolved
        .as_name()
        .map(str::to_owned)
        .ok_or_else(|| malformed(format!("{context} must be a name")))
}

fn required_text(document: &Document, value: &Value, context: &str) -> Result<String, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    text_value(&resolved).ok_or_else(|| malformed(format!("{context} must be a text string")))
}

fn required_bytes(document: &Document, value: &Value, context: &str) -> Result<Vec<u8>, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    byte_string(&resolved).ok_or_else(|| malformed(format!("{context} must be a byte string")))
}

fn optional_number(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<f64>, OpiError> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => required_number_for(document, value, &format!("{context} /{key}")).map(Some),
    }
}

fn optional_bool(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<bool>, OpiError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let resolved = resolve(document, value)
        .map_err(|error| malformed(format!("{context} /{key}: {error}")))?;
    match resolved {
        Value::Bool(flag) => Ok(Some(flag)),
        other => Err(malformed(format!(
            "{context} /{key} must be a boolean, got {}",
            other.type_name()
        ))),
    }
}

fn optional_text(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<String>, OpiError> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => required_text(document, value, &format!("{context} /{key}")).map(Some),
    }
}

fn optional_bytes(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<Vec<u8>>, OpiError> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => required_bytes(document, value, &format!("{context} /{key}")).map(Some),
    }
}

fn required_numbers<const N: usize>(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<[f64; N], OpiError> {
    let value = dict
        .get(key)
        .ok_or_else(|| malformed(format!("{context} /{key} is required")))?;
    fixed_numbers(document, value, &format!("{context} /{key}"))
}

fn optional_numbers<const N: usize>(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<[f64; N]>, OpiError> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => fixed_numbers(document, value, &format!("{context} /{key}")).map(Some),
    }
}

fn fixed_numbers<const N: usize>(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<[f64; N], OpiError> {
    let array = resolved_array(document, value, context)?;
    if array.0.len() != N {
        return Err(malformed(format!(
            "{context} must contain {N} numbers, got {}",
            array.0.len()
        )));
    }
    let values = number_items(document, &array, context)?;
    let mut out = [0.0; N];
    out.copy_from_slice(&values[..N]);
    Ok(out)
}

fn required_integers<const N: usize>(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<[i64; N], OpiError> {
    let value = dict
        .get(key)
        .ok_or_else(|| malformed(format!("{context} /{key} is required")))?;
    fixed_integers(document, value, &format!("{context} /{key}"))
}

fn optional_integers<const N: usize>(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<[i64; N]>, OpiError> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => fixed_integers(document, value, &format!("{context} /{key}")).map(Some),
    }
}

fn fixed_integers<const N: usize>(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<[i64; N], OpiError> {
    let array = resolved_array(document, value, context)?;
    if array.0.len() != N {
        return Err(malformed(format!(
            "{context} must contain {N} integers, got {}",
            array.0.len()
        )));
    }
    let mut out = [0i64; N];
    for (index, item) in array.0.iter().enumerate() {
        out[index] = required_integer_for(document, item, context)?;
    }
    Ok(out)
}

fn integer_list(document: &Document, value: &Value, context: &str) -> Result<Vec<i64>, OpiError> {
    let array = resolved_array(document, value, context)?;
    if array.0.len() > MAX_OPI_ARRAY {
        return Err(limit_exceeded(context));
    }
    array
        .0
        .iter()
        .map(|item| required_integer_for(document, item, context))
        .collect()
}

fn number_items(document: &Document, array: &Array, context: &str) -> Result<Vec<f64>, OpiError> {
    array
        .0
        .iter()
        .map(|item| required_number_for(document, item, context))
        .collect()
}

fn resolved_array(document: &Document, value: &Value, context: &str) -> Result<Array, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    resolved
        .as_array()
        .cloned()
        .ok_or_else(|| malformed(format!("{context} must be an array")))
}

/// 字面串/十六进制串解码为原始字节（十六进制忽略空白，奇数位补 `0`）。
fn byte_string(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::String(text) => Some(text.as_bytes().to_vec()),
        Value::HexString(hex) => {
            let digits: Vec<u8> = hex
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect();
            let mut padded = digits.clone();
            if padded.len() % 2 == 1 {
                padded.push(b'0');
            }
            let mut out = Vec::with_capacity(padded.len() / 2);
            for pair in padded.as_chunks::<2>().0 {
                let high = (pair[0] as char).to_digit(16)?;
                let low = (pair[1] as char).to_digit(16)?;
                out.push((high * 16 + low) as u8);
            }
            Some(out)
        }
        _ => None,
    }
}
