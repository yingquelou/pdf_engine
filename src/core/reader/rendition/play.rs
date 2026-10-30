//! 媒体播放参数读取（13.2.5 Table 279–281）。
//!
//! `/P` 字典分 `/MH` 必须满足与 `/BE` 尽力满足两组播放设置（Table 280），
//! `/D` 媒体时长再嵌 `/T` 时间跨度（Table 281 与 13.2.6.3 Table 289）。
//! 缺省值不物化：条目缺失保持 `None`，空字典解析为全 `None` 的设置。
//! `/PL` media players（13.2.7）仍保留原始值，留待后续切片。

use crate::core::reader::navigation::resolve;
use crate::core::types::{Document, Value};

use super::{MAX_DEPTH, RenditionError, check_type, malformed};

/// media play parameters 字典（Table 279）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlayParameters {
    /// `/PL` media players 字典（13.2.7，保留原值）。
    pub players: Option<Value>,
    /// `/MH` 必须满足的播放设置（Table 280）。
    pub must_honour: Option<MediaPlaySettings>,
    /// `/BE` 尽力满足的播放设置（Table 280）。
    pub best_effort: Option<MediaPlaySettings>,
}

/// 播放设置（Table 280）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlaySettings {
    /// `/V` 音量百分比；0 静音，负数非法。
    pub volume: Option<i64>,
    /// `/C` 是否显示播放器控制器。
    pub controller: Option<bool>,
    /// `/F` 缩放适配方式。
    pub fit: Option<MediaFit>,
    /// `/D` 媒体时长（Table 281）。
    pub duration: Option<MediaDuration>,
    /// `/A` 是否自动播放。
    pub auto_play: Option<bool>,
    /// `/RC` 重复次数；0 表示无限循环。
    pub repeat_count: Option<f64>,
}

/// `/F` 缩放适配方式（Table 280）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFit {
    /// `0`：缩放以完整适应播放区域。
    Meet,
    /// `1`：裁剪以填满播放区域。
    Slice,
    /// `2`：拉伸填满播放区域。
    Fill,
    /// `3`：不缩放，允许滚动。
    Scroll,
    /// `4`：不缩放，裁剪超出部分。
    Clip,
    /// `5`：播放器缺省行为。
    PlayerDefault,
    /// 未识别的枚举值（必须满足时播放器不可行）。
    Other(i64),
}

impl MediaFit {
    fn from_code(code: i64) -> Self {
        match code {
            0 => Self::Meet,
            1 => Self::Slice,
            2 => Self::Fill,
            3 => Self::Scroll,
            4 => Self::Clip,
            5 => Self::PlayerDefault,
            other => Self::Other(other),
        }
    }
}

/// 媒体时长（Table 281）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaDuration {
    /// `/S` 时长来源。
    pub kind: MediaDurationKind,
    /// `/T` 显式时间跨度；`/S` 为 `T` 时必需。
    pub timespan: Option<MediaTimespan>,
}

/// `/S` 媒体时长来源（Table 281）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaDurationKind {
    /// `I`：媒体内在时长。
    Intrinsic,
    /// `F`：无限时长。
    Infinity,
    /// `T`：由 `/T` 显式指定。
    Explicit,
    /// 未识别的名称（保留原值）。
    Other(String),
}

/// 时间跨度（Table 289）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaTimespan {
    /// `/V` 秒数；负数非法。
    pub seconds: f64,
}

pub(super) fn parse_play_parameters(
    document: &Document,
    value: &Value,
    depth: usize,
) -> Result<MediaPlayParameters, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed(
            "media play parameters nesting exceeds the depth limit",
        ));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("media play parameters is not a dictionary"))?;
    check_type(dict, "MediaPlayParams", "media play parameters")?;
    let must_honour = parse_play_settings(document, dict.get("MH"), "MH", depth + 1)?;
    let best_effort = parse_play_settings(document, dict.get("BE"), "BE", depth + 1)?;
    Ok(MediaPlayParameters {
        players: dict.get("PL").cloned(),
        must_honour,
        best_effort,
    })
}

fn parse_play_settings(
    document: &Document,
    value: Option<&Value>,
    key: &str,
    depth: usize,
) -> Result<Option<MediaPlaySettings>, RenditionError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if depth >= MAX_DEPTH {
        return Err(malformed(
            "media play parameters nesting exceeds the depth limit",
        ));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed(format!("media play parameters /{key} is not a dictionary")))?;
    let volume = optional_integer(
        document,
        dict.get("V"),
        &format!("media play parameters /{key} /V"),
    )?;
    if volume.is_some_and(|volume| volume < 0) {
        return Err(malformed(format!(
            "media play parameters /{key} /V must not be negative"
        )));
    }
    let controller = optional_boolean(
        document,
        dict.get("C"),
        &format!("media play parameters /{key} /C"),
    )?;
    let fit = optional_integer(
        document,
        dict.get("F"),
        &format!("media play parameters /{key} /F"),
    )?
    .map(MediaFit::from_code);
    let duration = match dict.get("D") {
        None => None,
        Some(value) => Some(parse_duration(document, value, depth + 1)?),
    };
    let auto_play = optional_boolean(
        document,
        dict.get("A"),
        &format!("media play parameters /{key} /A"),
    )?;
    let repeat_count = optional_number(
        document,
        dict.get("RC"),
        &format!("media play parameters /{key} /RC"),
    )?;
    if repeat_count.is_some_and(|count| count < 0.0) {
        return Err(malformed(format!(
            "media play parameters /{key} /RC must not be negative"
        )));
    }
    Ok(Some(MediaPlaySettings {
        volume,
        controller,
        fit,
        duration,
        auto_play,
        repeat_count,
    }))
}

fn parse_duration(
    document: &Document,
    value: &Value,
    depth: usize,
) -> Result<MediaDuration, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed("media duration nesting exceeds the depth limit"));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("media duration is not a dictionary"))?;
    check_type(dict, "MediaDuration", "media duration")?;
    let subtype = dict
        .get("S")
        .and_then(Value::as_name)
        .ok_or_else(|| malformed("media duration is missing a name /S"))?;
    let kind = match subtype {
        "I" => MediaDurationKind::Intrinsic,
        "F" => MediaDurationKind::Infinity,
        "T" => MediaDurationKind::Explicit,
        other => MediaDurationKind::Other(other.to_owned()),
    };
    let timespan = match dict.get("T") {
        None => None,
        Some(value) => Some(parse_timespan(document, value, depth + 1)?),
    };
    if matches!(kind, MediaDurationKind::Explicit) && timespan.is_none() {
        return Err(malformed("explicit media duration is missing /T"));
    }
    Ok(MediaDuration { kind, timespan })
}

fn parse_timespan(
    document: &Document,
    value: &Value,
    depth: usize,
) -> Result<MediaTimespan, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed("timespan nesting exceeds the depth limit"));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("timespan is not a dictionary"))?;
    check_type(dict, "Timespan", "timespan")?;
    match dict.get("S").and_then(Value::as_name) {
        Some("S") => {}
        Some(other) => {
            return Err(malformed(format!("timespan /S is {other}, expected S")));
        }
        None => return Err(malformed("timespan is missing a name /S")),
    }
    let value = dict
        .get("V")
        .ok_or_else(|| malformed("timespan is missing /V"))?;
    let seconds = number(document, value, "timespan /V")?;
    if seconds < 0.0 {
        return Err(malformed("timespan /V must not be negative"));
    }
    Ok(MediaTimespan { seconds })
}

fn optional_integer(
    document: &Document,
    value: Option<&Value>,
    context: &str,
) -> Result<Option<i64>, RenditionError> {
    match value {
        None => Ok(None),
        Some(value) => match resolve(document, value)? {
            Value::Integer(number) => Ok(Some(number)),
            _ => Err(malformed(format!("{context} is not an integer"))),
        },
    }
}

fn optional_boolean(
    document: &Document,
    value: Option<&Value>,
    context: &str,
) -> Result<Option<bool>, RenditionError> {
    match value {
        None => Ok(None),
        Some(value) => match resolve(document, value)? {
            Value::Bool(flag) => Ok(Some(flag)),
            _ => Err(malformed(format!("{context} is not a boolean"))),
        },
    }
}

fn optional_number(
    document: &Document,
    value: Option<&Value>,
    context: &str,
) -> Result<Option<f64>, RenditionError> {
    match value {
        None => Ok(None),
        Some(value) => number(document, value, context).map(Some),
    }
}

fn number(document: &Document, value: &Value, context: &str) -> Result<f64, RenditionError> {
    match resolve(document, value)? {
        Value::Integer(number) => Ok(number as f64),
        Value::Float(number) if number.is_finite() => Ok(number),
        Value::Float(_) => Err(malformed(format!("{context} must be finite"))),
        _ => Err(malformed(format!("{context} is not a number"))),
    }
}
