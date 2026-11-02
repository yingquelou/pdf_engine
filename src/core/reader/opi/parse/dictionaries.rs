use crate::core::reader::navigation::resolve;
use crate::core::reader::opi::{
    Opi13Dictionary, Opi20Dictionary, OpiColorType, OpiError, OpiProxy, malformed,
};
use crate::core::types::{Dict, Document, Value};

use super::accessors::{
    integer_list, optional_bool, optional_bytes, optional_integers, optional_number,
    optional_numbers, optional_text, required_integers, required_name, required_number,
    required_number_for, required_numbers,
};
use super::checks::{check_position, check_size_and_crop, gray_map_len};
use super::objects::{parse_color, parse_inks, parse_tags_v13, parse_tags_v20, required_file};

/// Table 368：单条目版本字典，键为 `1.3` / `2.0`，值为 OPI 字典。
pub(super) fn parse_version_dictionary(
    document: &Document,
    value: &Value,
) -> Result<OpiProxy, OpiError> {
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
