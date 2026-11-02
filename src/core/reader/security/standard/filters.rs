//! 7.6.5 crypt filter：把加密字典 `/CF` `/StrF` `/StmF` `/EFF` 与流上
//! 显式 `/Crypt` 过滤器选择为具体的 [`CryptMethod`]。

use crate::core::types::{Dict, Value};

use super::SecurityOptions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CryptMethod {
    Identity,
    Rc4 { key_len: usize },
    Aes128,
}

pub(super) fn explicit_crypt_filter(
    dict: &Dict,
    options: &SecurityOptions,
) -> Result<Option<CryptMethod>, String> {
    let filters: Vec<&str> = match dict.get("Filter") {
        Some(Value::Name(name)) => vec![name],
        Some(Value::Array(array)) => array
            .iter()
            .map(|filter| {
                filter
                    .as_name()
                    .ok_or_else(|| "invalid PDF stream /Filter array".to_string())
            })
            .collect::<Result<_, _>>()?,
        None => return Ok(None),
        Some(_) => return Err("invalid PDF stream /Filter".to_string()),
    };
    let Some(position) = filters.iter().position(|filter| *filter == "Crypt") else {
        return Ok(None);
    };
    if position != 0 {
        return Err("PDF /Crypt filter must be the first stream filter".to_string());
    }
    let params = match dict.get("DecodeParms").or_else(|| dict.get("DP")) {
        Some(Value::Dict(dict)) if filters.len() == 1 => Some(dict),
        Some(Value::Dict(_)) => {
            return Err("PDF /DecodeParms must be an array for multiple filters".to_string());
        }
        Some(Value::Array(array)) => match array.0.get(position) {
            Some(Value::Dict(dict)) => Some(dict),
            Some(Value::Null) | None => None,
            Some(_) => return Err("invalid /Crypt DecodeParms entry".to_string()),
        },
        None | Some(Value::Null) => None,
        Some(_) => return Err("invalid PDF stream /DecodeParms".to_string()),
    };
    if options.version < 4 {
        return Err("/Crypt filter requires Standard Security Handler version 4".to_string());
    }
    let name = match params.and_then(|params| params.get("Name")) {
        Some(Value::Name(name)) => name.as_str(),
        None => "Identity",
        Some(_) => return Err("invalid /Crypt DecodeParms /Name".to_string()),
    };
    named_crypt_filter(options, name).map(Some)
}

fn named_crypt_filter(options: &SecurityOptions, name: &str) -> Result<CryptMethod, String> {
    if name == "Identity" {
        return Ok(CryptMethod::Identity);
    }
    let filter = options
        .crypt_filters
        .as_ref()
        .and_then(|filters| filters.get(name))
        .and_then(Value::as_dict)
        .ok_or_else(|| format!("missing encryption crypt filter /{name}"))?;
    crypt_method(filter, name, options.file_key.len())
}

fn crypt_method(filter: &Dict, name: &str, file_key_len: usize) -> Result<CryptMethod, String> {
    let method = match filter.get("CFM") {
        Some(Value::Name(method)) => method.as_str(),
        None => "None",
        Some(_) => return Err(format!("invalid encryption crypt filter /{name} /CFM")),
    };
    let key_len = match filter.get("Length") {
        Some(Value::Integer(length)) if *length > 0 => usize::try_from(*length)
            .map_err(|_| format!("invalid encryption crypt filter /{name} /Length"))?,
        Some(_) => return Err(format!("invalid encryption crypt filter /{name} /Length")),
        None => file_key_len,
    };
    match method {
        "None" => {
            validate_auth_event(filter, name)?;
            Ok(CryptMethod::Identity)
        }
        "V2" if (5..=file_key_len).contains(&key_len) => {
            validate_auth_event(filter, name)?;
            Ok(CryptMethod::Rc4 { key_len })
        }
        "V2" => Err(format!(
            "invalid encryption crypt filter /{name} key length"
        )),
        "AESV2" if key_len == 16 && file_key_len == 16 => {
            validate_auth_event(filter, name)?;
            Ok(CryptMethod::Aes128)
        }
        "AESV2" => Err(format!("AESV2 crypt filter /{name} requires a 16-byte key")),
        method => Err(format!("unsupported encryption crypt method /{method}")),
    }
}

fn validate_auth_event(filter: &Dict, name: &str) -> Result<(), String> {
    if let Some(event) = filter.get("AuthEvent")
        && !matches!(event.as_name(), Some("DocOpen" | "EFOpen"))
    {
        return Err(format!(
            "invalid encryption crypt filter /{name} /AuthEvent"
        ));
    }
    Ok(())
}

pub(super) fn crypt_filter(
    dict: &Dict,
    selector: &str,
    default: CryptMethod,
    file_key_len: usize,
) -> Result<CryptMethod, String> {
    let name = match dict.get(selector) {
        Some(Value::Name(name)) => name.as_str(),
        None => return Ok(default),
        Some(_) => return Err(format!("invalid encryption /{selector}")),
    };
    if name == "Identity" {
        return Ok(CryptMethod::Identity);
    }
    let filters = dict
        .get("CF")
        .and_then(Value::as_dict)
        .ok_or("missing encryption /CF dictionary")?;
    let filter = filters
        .get(name)
        .and_then(Value::as_dict)
        .ok_or_else(|| format!("missing encryption crypt filter /{name}"))?;
    crypt_method(filter, name, file_key_len)
}
