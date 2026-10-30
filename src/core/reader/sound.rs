//! 声音对象读取（ISO 32000-1 13.3 Table 294）。
//!
//! 声音对象是包含采样数据的流：`/R` 采样率（必填）、`/C` 声道数
//! （缺省 1）、`/B` 每声道每样本位数（缺省 8）、`/E` 编码格式
//! （缺省 `Raw`）与 `/CO` 压缩格式。`/E` 的四个标准值在 [`SoundEncoding`]
//! 中类型化，非标准名保留原文；`/CO` 在 ISO 32000-1 中没有定义标准值，
//! 因此只校验其为名字并保留原文。
//!
//! `/F` 外部声音文件不在此模型内解析：流数据仍按 `/Filter` 解码后返回，
//! 外部文件存在时通常为空。
//!
//! [`Document::sound`] 接受间接引用或直接流，拒绝非流对象、缺少或非数值的
//! `/R`、非整数或小于 1 的 `/C` `/B`、非名字的 `/E` `/CO`，以及过滤器
//! 解码失败；解码后字节数超过 [`MAX_SOUND_STREAM_BYTES`] 时报
//! [`SoundError::LimitExceeded`]。

use std::fmt;

use crate::core::reader::ParseError;
use crate::core::reader::navigation::{NavigationError, resolve};
use crate::core::types::{Document, Value};

#[cfg(test)]
mod tests;

/// 解码后声音流字节数上限（64 MiB），防护不可信文件。
pub const MAX_SOUND_STREAM_BYTES: usize = 64 * 1024 * 1024;

/// 采样数据编码格式（`/E`，Table 294）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SoundEncoding {
    /// 未指定或无符号值 0..2^B-1（`/Raw`，缺省）。
    #[default]
    Raw,
    /// 二进制补码有符号值（`/Signed`）。
    Signed,
    /// mu-law 编码（`/muLaw`）。
    MuLaw,
    /// A-law 编码（`/ALaw`）。
    ALaw,
    /// 非标准编码名（原样保留）。
    Other(String),
}

impl SoundEncoding {
    /// 由 `/E` 名字解析；未知名返回 [`SoundEncoding::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Raw" => Self::Raw,
            "Signed" => Self::Signed,
            "muLaw" => Self::MuLaw,
            "ALaw" => Self::ALaw,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准编码名；[`SoundEncoding::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Raw => "Raw",
            Self::Signed => "Signed",
            Self::MuLaw => "muLaw",
            Self::ALaw => "ALaw",
            Self::Other(other) => other,
        }
    }
}

/// 解析后的声音对象。
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    /// `/R` 采样率（每秒样本数），必填且为正有限数。
    pub sample_rate: f64,
    /// `/C` 声道数，缺省 1。
    pub channels: u32,
    /// `/B` 每声道每样本位数，缺省 8。
    pub bits_per_sample: u32,
    /// `/E` 编码格式，缺省 [`SoundEncoding::Raw`]。
    pub encoding: SoundEncoding,
    /// `/CO` 压缩格式名；缺失表示未压缩。ISO 32000-1 未定义标准值。
    pub compression: Option<String>,
    /// 过滤器解码后的采样数据字节。
    pub data: Vec<u8>,
}

/// 声音对象解析失败原因。
#[derive(Debug)]
pub enum SoundError {
    /// 间接引用无法解析。
    Navigation(NavigationError),
    /// 非流对象或字典条目缺失、类型/取值非法。
    Malformed(String),
    /// 流过滤器解码失败。
    Decode(String),
    /// 解码后数据超过 [`MAX_SOUND_STREAM_BYTES`]。
    LimitExceeded(&'static str),
}

impl fmt::Display for SoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Navigation(error) => write!(f, "{error}"),
            Self::Malformed(message) => write!(f, "invalid sound object: {message}"),
            Self::Decode(message) => write!(f, "sound stream decode failed: {message}"),
            Self::LimitExceeded(what) => write!(f, "sound stream limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for SoundError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Navigation(error) => Some(error),
            Self::Malformed(_) | Self::Decode(_) | Self::LimitExceeded(_) => None,
        }
    }
}

impl From<NavigationError> for SoundError {
    fn from(error: NavigationError) -> Self {
        Self::Navigation(error)
    }
}

impl From<ParseError> for SoundError {
    fn from(error: ParseError) -> Self {
        Self::Malformed(error.to_string())
    }
}

fn malformed(message: impl Into<String>) -> SoundError {
    SoundError::Malformed(message.into())
}

impl Document {
    /// 把声音对象（间接引用或直接流）解析为 [`Sound`]。
    ///
    /// `/R` 缺失、非数值或非正，`/C` `/B` 非正整数，`/E` `/CO` 非名字，
    /// 或流过滤器解码失败时返回类型化错误；`/Type` 若存在必须为 `Sound`。
    /// 缺省条目按 Table 294 物化：`/C` 1、`/B` 8、`/E` `Raw`。
    pub fn sound(&self, value: &Value) -> Result<Sound, SoundError> {
        let value = resolve(self, value)?;
        let Value::Stream(dict, _) = &value else {
            return Err(malformed("value is not a stream"));
        };
        if let Some(kind) = dict.get("Type") {
            match kind.as_name() {
                Some("Sound") => {}
                Some(other) => {
                    return Err(malformed(format!("/Type must be Sound, found {other}")));
                }
                None => return Err(malformed("/Type must be a name")),
            }
        }
        let sample_rate = match dict.get("R") {
            None => return Err(malformed("required entry /R (sample rate) is missing")),
            Some(Value::Integer(rate)) => *rate as f64,
            Some(Value::Float(rate)) => *rate,
            Some(_) => return Err(malformed("/R (sample rate) must be a number")),
        };
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return Err(malformed(
                "/R (sample rate) must be a positive finite number",
            ));
        }
        let channels = parse_positive_integer(dict.get("C"), 1, "/C (channel count)")?;
        let bits_per_sample = parse_positive_integer(dict.get("B"), 8, "/B (bits per sample)")?;
        let encoding = match dict.get("E") {
            None => SoundEncoding::Raw,
            Some(value) => SoundEncoding::from_name(
                value
                    .as_name()
                    .ok_or_else(|| malformed("/E (encoding) must be a name"))?,
            ),
        };
        let compression = match dict.get("CO") {
            None => None,
            Some(value) => Some(
                value
                    .as_name()
                    .ok_or_else(|| malformed("/CO (compression) must be a name"))?
                    .to_string(),
            ),
        };
        let data = self
            .decode_stream_bytes(&value)
            .map_err(SoundError::Decode)?;
        if data.len() > MAX_SOUND_STREAM_BYTES {
            return Err(SoundError::LimitExceeded("decoded sound stream bytes"));
        }
        Ok(Sound {
            sample_rate,
            channels,
            bits_per_sample,
            encoding,
            compression,
            data,
        })
    }
}

/// 可选正整数条目；缺失时返回 `default`，非整数或小于 1 报错。
fn parse_positive_integer(
    value: Option<&Value>,
    default: u32,
    context: &str,
) -> Result<u32, SoundError> {
    let Some(value) = value else {
        return Ok(default);
    };
    match value {
        Value::Integer(number) if (1..=i64::from(u32::MAX)).contains(number) => Ok(*number as u32),
        _ => Err(malformed(format!("{context} must be a positive integer"))),
    }
}
