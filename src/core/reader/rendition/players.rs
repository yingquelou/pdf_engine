//! 媒体播放器字典读取（13.2.7 Table 290–293，PDF 1.5）。
//!
//! `/PL` 同时被 media clip data（Table 274）与 media play parameters
//! （Table 279）引用：`/MU` 必须使用、`/A` 任意使用、`/NU` 不使用三个
//! 可选数组（Table 290）各含媒体播放器信息字典（Table 291），其 `/PID`
//! 软件标识字典（Table 292）给出软件 URI、版本区间与操作系统。空数组
//! 等价于条目缺失；`/L` `/LI` `/H` `/HI` `/OS` 的缺省值不物化，
//! `/MH` `/BE` 无定义条目、保留原值。

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Document, Value};

use super::{MAX_DEPTH, RenditionError, check_type, malformed};

/// 媒体播放器字典（Table 290）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlayers {
    /// `/MU` 必须使用其中之一的播放器集合。
    pub must_use: Vec<MediaPlayerInfo>,
    /// `/A` 任意可用的播放器集合。
    pub any: Vec<MediaPlayerInfo>,
    /// `/NU` 不得使用的播放器集合。
    pub not_used: Vec<MediaPlayerInfo>,
}

/// 媒体播放器信息字典（Table 291）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlayerInfo {
    /// `/PID` 软件标识字典（Table 292）。
    pub software: SoftwareIdentifier,
    /// `/MH` 必须满足的条目（无定义条目，保留原值）。
    pub mh: Option<Value>,
    /// `/BE` 尽力满足的条目（无定义条目，保留原值）。
    pub be: Option<Value>,
}

/// 软件标识字典（Table 292）。
#[derive(Debug, Clone, PartialEq)]
pub struct SoftwareIdentifier {
    /// `/U` 软件 URI（ASCII）。
    pub uri: String,
    /// `/L` 版本下界；缺省为 `[0]`，不物化。
    pub lower: Option<Vec<i64>>,
    /// `/LI` 下界是否含入；缺省 `true`。
    pub lower_inclusive: Option<bool>,
    /// `/H` 版本上界；缺省为空数组（无穷大）。
    pub upper: Option<Vec<i64>>,
    /// `/HI` 上界是否含入；缺省 `true`。
    pub upper_inclusive: Option<bool>,
    /// `/OS` 操作系统标识字节串；空数组表示全部系统。
    pub operating_systems: Option<Vec<Vec<u8>>>,
}

pub(super) fn parse_media_players(
    document: &Document,
    value: &Value,
    depth: usize,
) -> Result<MediaPlayers, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed("media players nesting exceeds the depth limit"));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("media players is not a dictionary"))?;
    check_type(dict, "MediaPlayers", "media players")?;
    Ok(MediaPlayers {
        must_use: parse_player_array(document, dict.get("MU"), "MU", depth + 1)?,
        any: parse_player_array(document, dict.get("A"), "A", depth + 1)?,
        not_used: parse_player_array(document, dict.get("NU"), "NU", depth + 1)?,
    })
}

fn parse_player_array(
    document: &Document,
    value: Option<&Value>,
    key: &str,
    depth: usize,
) -> Result<Vec<MediaPlayerInfo>, RenditionError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if depth >= MAX_DEPTH {
        return Err(malformed("media players nesting exceeds the depth limit"));
    }
    let resolved = resolve(document, value)?;
    let entries = resolved
        .as_array()
        .ok_or_else(|| malformed(format!("media players /{key} is not an array")))?;
    let mut players = Vec::with_capacity(entries.0.len());
    for entry in &entries.0 {
        players.push(parse_player_info(document, entry, depth + 1)?);
    }
    Ok(players)
}

fn parse_player_info(
    document: &Document,
    value: &Value,
    depth: usize,
) -> Result<MediaPlayerInfo, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed(
            "media player info nesting exceeds the depth limit",
        ));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("media player info is not a dictionary"))?;
    check_type(dict, "MediaPlayerInfo", "media player info")?;
    let pid = dict
        .get("PID")
        .ok_or_else(|| malformed("media player info is missing required /PID"))?;
    let software = parse_software_identifier(document, pid, depth + 1)?;
    Ok(MediaPlayerInfo {
        software,
        mh: dict.get("MH").cloned(),
        be: dict.get("BE").cloned(),
    })
}

fn parse_software_identifier(
    document: &Document,
    value: &Value,
    depth: usize,
) -> Result<SoftwareIdentifier, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed(
            "software identifier nesting exceeds the depth limit",
        ));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("software identifier is not a dictionary"))?;
    check_type(dict, "SoftwareIdentifier", "software identifier")?;
    let uri = dict
        .get("U")
        .ok_or_else(|| malformed("software identifier is missing required /U"))?;
    let resolved = resolve(document, uri)?;
    let uri = text_value(&resolved)
        .ok_or_else(|| malformed("software identifier /U is not a text string"))?;
    if !uri.is_ascii() {
        return Err(malformed("software identifier /U is not ASCII"));
    }
    Ok(SoftwareIdentifier {
        uri,
        lower: parse_version_array(document, dict.get("L"), "L")?,
        lower_inclusive: parse_boolean(document, dict.get("LI"), "LI")?,
        upper: parse_version_array(document, dict.get("H"), "H")?,
        upper_inclusive: parse_boolean(document, dict.get("HI"), "HI")?,
        operating_systems: parse_operating_systems(document, dict.get("OS"))?,
    })
}

fn parse_version_array(
    document: &Document,
    value: Option<&Value>,
    key: &str,
) -> Result<Option<Vec<i64>>, RenditionError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let resolved = resolve(document, value)?;
    let entries = resolved
        .as_array()
        .ok_or_else(|| malformed(format!("software identifier /{key} is not an array")))?;
    let mut versions = Vec::with_capacity(entries.0.len());
    for entry in &entries.0 {
        let resolved = resolve(document, entry)?;
        let Value::Integer(version) = resolved else {
            return Err(malformed(format!(
                "software identifier /{key} entries are not integers"
            )));
        };
        if version < 0 {
            return Err(malformed(format!(
                "software identifier /{key} entries must not be negative"
            )));
        }
        versions.push(version);
    }
    Ok(Some(versions))
}

fn parse_boolean(
    document: &Document,
    value: Option<&Value>,
    key: &str,
) -> Result<Option<bool>, RenditionError> {
    let Some(value) = value else {
        return Ok(None);
    };
    match resolve(document, value)? {
        Value::Bool(flag) => Ok(Some(flag)),
        _ => Err(malformed(format!(
            "software identifier /{key} is not a boolean"
        ))),
    }
}

fn parse_operating_systems(
    document: &Document,
    value: Option<&Value>,
) -> Result<Option<Vec<Vec<u8>>>, RenditionError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let resolved = resolve(document, value)?;
    let entries = resolved
        .as_array()
        .ok_or_else(|| malformed("software identifier /OS is not an array"))?;
    let mut systems = Vec::with_capacity(entries.0.len());
    for entry in &entries.0 {
        let resolved = resolve(document, entry)?;
        let bytes = byte_string(&resolved)
            .ok_or_else(|| malformed("software identifier /OS entries are not byte strings"))?;
        systems.push(bytes);
    }
    Ok(Some(systems))
}

fn byte_string(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::String(text) => Some(text.chars().map(|character| character as u8).collect()),
        Value::HexString(hex) => hex_bytes(hex),
        _ => None,
    }
}

fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    let mut digits: Vec<u8> = hex
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    // 7.3.4.3: 末位数为奇数时以 0 补足。
    if digits.len() % 2 == 1 {
        digits.push(b'0');
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        bytes.push((high * 16 + low) as u8);
    }
    Some(bytes)
}
