use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::reader::opi::{MAX_OPI_ARRAY, OpiError, limit_exceeded, malformed};
use crate::core::types::{Array, Dict, Document, Value};

pub(super) fn required_number(
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

pub(super) fn required_number_for(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<f64, OpiError> {
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

pub(super) fn required_integer_for(
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

pub(super) fn required_name(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<String, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    resolved
        .as_name()
        .map(str::to_owned)
        .ok_or_else(|| malformed(format!("{context} must be a name")))
}

pub(super) fn required_text(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<String, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    text_value(&resolved).ok_or_else(|| malformed(format!("{context} must be a text string")))
}

pub(super) fn required_bytes(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Vec<u8>, OpiError> {
    let resolved =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    byte_string(&resolved).ok_or_else(|| malformed(format!("{context} must be a byte string")))
}

pub(super) fn optional_number(
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

pub(super) fn optional_bool(
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

pub(super) fn optional_text(
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

pub(super) fn optional_bytes(
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

pub(super) fn required_numbers<const N: usize>(
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

pub(super) fn optional_numbers<const N: usize>(
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

pub(super) fn required_integers<const N: usize>(
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

pub(super) fn optional_integers<const N: usize>(
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

pub(super) fn integer_list(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Vec<i64>, OpiError> {
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

pub(super) fn resolved_array(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Array, OpiError> {
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
