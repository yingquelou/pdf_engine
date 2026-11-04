//! 7.6.5 crypt filter：把加密字典 `/CF` `/StrF` `/StmF` `/EFF` 与流上
//! 显式 `/Crypt` 过滤器选择为具体的 [`CryptMethod`]。

use crate::core::filters::{ObjectResolver, normalize_filter_entries};
use crate::core::types::{Dict, Value};

use super::SecurityOptions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CryptMethod {
    Identity,
    Rc4 { key_len: usize },
    Aes128,
}

/// 解出流字典上显式 `/Crypt` 选择的具体 crypt filter。
///
/// `/Filter` 本体、`/Filter` 数组元素与 `/DecodeParms`（含缩写 `/DP`）都允许
/// 写成间接引用（7.3.10），故先用 7.3.7 规范化器（[`normalize_filter_entries`]）
/// 解引用再判定有无 `/Crypt`：
/// - 规范化后没有 `/Crypt` → `Ok(None)`，普通加密流沿用文件级默认 filter，
///   规范化器的 Table 5/7 交叉核对诊断不参与，无关流不因筛选器写法违例变红；
/// - 选中了 `/Crypt` → 严格：与 `/Filter`／`/DecodeParms` 相关的诊断
///   （引用缺失、指向非 Name／非字典、成环、超跳数、个数不配对）升级为错误，
///   与「`/Crypt` 必须是第一个筛选器即报错」的既有立场一致——选错 crypt filter
///   会拿错密钥、产出静默乱码，故不静默退回 `/Identity` 也不退回未解密。
pub(super) fn explicit_crypt_filter(
    dict: &Dict,
    resolver: &ObjectResolver<'_>,
    options: &SecurityOptions,
) -> Result<Option<CryptMethod>, String> {
    let candidate = match dict.get("Filter") {
        Some(Value::Name(name)) => name == "Crypt",
        Some(Value::Ref(_, _) | Value::Array(_)) => true,
        _ => false,
    };
    if !candidate {
        return Ok(None);
    }
    let (dict, issues) = normalize_filter_entries(dict, resolver);
    match (
        select_crypt_method(&dict, resolver, options),
        blocking_filter_issues(&issues),
    ) {
        // 没有 /Crypt：按文件级默认 filter 走，规范化诊断不参与。
        (Ok(None), _) => Ok(None),
        (Ok(Some(method)), None) => Ok(Some(method)),
        (Ok(Some(_)), Some(problem)) => Err(problem),
        (Err(error), None) => Err(error),
        (Err(error), Some(problem)) => Err(format!("{error}; {problem}")),
    }
}

/// 规范化之后按 `/Crypt` 的位置与参数字典选 filter；形态与既有直写对象一致。
fn select_crypt_method(
    dict: &Dict,
    resolver: &ObjectResolver<'_>,
    options: &SecurityOptions,
) -> Result<Option<CryptMethod>, String> {
    // 宽松扫描「链里有没有 /Crypt」：解不开的数组元素按「不是 /Crypt」计，
    // 由筛选器调度那侧（`Document::decode_stream_bytes`）另行诊断，这样
    // 普通加密流不会仅仅因为链上某处写法违例就整体变红。
    let filters: Vec<Option<&str>> = match dict.get("Filter") {
        Some(Value::Name(name)) => vec![Some(name.as_str())],
        Some(Value::Array(array)) => array.iter().map(Value::as_name).collect(),
        None => return Ok(None),
        Some(_) => return Err("invalid PDF stream /Filter".to_string()),
    };
    let Some(position) = filters.iter().position(|filter| *filter == Some("Crypt")) else {
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
    named_crypt_filter(options, resolver, name).map(Some)
}

/// 升级为错误的规范化诊断：只针对本流 `/Filter`／`/DecodeParms`（含 `/DP`）
/// 自身的形态与个数违例；Table 7 的 `/FFilter`／`/FDecodeParms` 与 crypt filter
/// 选择无关，不作阻断（否则外部文件流的写法问题会误伤解密）。
fn blocking_filter_issues(issues: &[String]) -> Option<String> {
    let blocking = issues
        .iter()
        .filter(|issue| {
            issue.starts_with("/Filter")
                || issue.starts_with("/DecodeParms")
                || issue.starts_with("/DP")
        })
        .cloned()
        .collect::<Vec<_>>();
    if blocking.is_empty() {
        return None;
    }
    Some(format!(
        "invalid PDF stream /Crypt filter entries: {}",
        blocking.join("; ")
    ))
}

/// `/CF` 里命名的 crypt filter。
///
/// `/CF` 的条目值合法写法可以是间接引用（`<< /CF << /StdCF 9 0 R >> >>`），
/// 故按需解引用；解不到字典时明确报错，不退回 `/Identity`（选错 filter 会拿错密钥）。
fn named_crypt_filter(
    options: &SecurityOptions,
    resolver: &ObjectResolver<'_>,
    name: &str,
) -> Result<CryptMethod, String> {
    if name == "Identity" {
        return Ok(CryptMethod::Identity);
    }
    let entry = options
        .crypt_filters
        .as_ref()
        .and_then(|filters| filters.get(name));
    let filter = match entry {
        Some(Value::Dict(filter)) => filter.clone(),
        Some(Value::Ref(num, generation)) => {
            let resolved = resolver(&Value::Ref(*num, *generation))?;
            match resolved {
                Some(Value::Dict(filter)) => filter,
                Some(_) => {
                    return Err(format!(
                        "invalid encryption crypt filter /{name}: /CF entry reference did not resolve to a dictionary"
                    ));
                }
                None => {
                    return Err(format!(
                        "invalid encryption crypt filter /{name}: /CF entry references an absent object"
                    ));
                }
            }
        }
        Some(_) => {
            return Err(format!(
                "invalid encryption crypt filter /{name}: /CF entry must be a dictionary"
            ));
        }
        None => return Err(format!("missing encryption crypt filter /{name}")),
    };
    crypt_method(&filter, name, options.file_key.len())
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
