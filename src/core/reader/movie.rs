//! 电影对象与电影激活字典读取（ISO 32000-1 13.4 Table 295/296）。
//!
//! 电影字典描述电影的静态特征：`/F` 自描述电影文件（必填文件规格）、
//! `/Aspect` 像素尺寸、`/Rotate` 相对页面顺时针旋转角（90 的倍数，缺省 0）
//! 与 `/Poster` 海报（布尔或图像 XObject 流，缺省 `false`）。电影字典在
//! ISO 32000-1 中没有 `/Type` 条目，因此本模型不要求也不写出 `/Type`。
//!
//! 电影激活字典描述播放方式，同时用于 Movie 注解的 `/A`（Table 186）与
//! Movie 动作的动作字典（Table 209）。缺省值按 Table 296 不物化，唯播放
//! 模式 `/Mode` 缺省 `Once` 直接落到 [`MovieActivation::mode`]。
//! `/Start` `/Duration` 的时间值支持整数、8 字节二进制补码字符串与
//! `[时间 时间刻度]` 数组三种形态。
//!
//! [`Document::movie`] 接受间接引用或直接字典；13.4 注明这些特性已被
//! 13.2 的多媒体框架取代（obsolescent），本库只做语法层建模。

use std::fmt;

use crate::core::reader::embedded::Filespec;
use crate::core::reader::navigation::{NavigationError, resolve};
use crate::core::types::{Dict, Document, Value};

#[cfg(test)]
mod tests;

/// 电影字典（Table 295）。
#[derive(Debug, Clone, PartialEq)]
pub struct Movie {
    /// `/F` 自描述电影文件的文件规格（必填）。
    pub file: Filespec,
    /// `/Aspect` 像素尺寸 `[width height]`；纯声音电影应省略。
    pub aspect: Option<(u32, u32)>,
    /// `/Rotate` 相对页面顺时针旋转角（90 的倍数，缺省 0）。
    pub rotate: i32,
    /// `/Poster` 海报图像的呈现方式（缺省 `false`）。
    pub poster: Poster,
}

/// 电影海报 `/Poster`（Table 295）：布尔或图像 XObject 流。
#[derive(Debug, Clone, PartialEq)]
pub enum Poster {
    /// `false`：不显示海报（缺省）。
    Disabled,
    /// `true`：海报取自电影文件。
    FromMovieFile,
    /// 图像 XObject 流（直接流或间接引用原值）。
    Stream(Value),
}

/// 电影播放模式（`/Mode`，Table 296）；缺省 `Once`，未知名字保留。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MovieMode {
    /// 播放一次后停止（缺省）。
    #[default]
    Once,
    /// 播放并保持电影控制器打开。
    Open,
    /// 循环播放直到停止。
    Repeat,
    /// 正反往复播放直到停止。
    Palindrome,
    /// 非标准模式名。
    Other(String),
}

impl MovieMode {
    /// 由 `/Mode` 名字解析；未知名返回 [`MovieMode::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Once" => Self::Once,
            "Open" => Self::Open,
            "Repeat" => Self::Repeat,
            "Palindrome" => Self::Palindrome,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 标准模式名；[`MovieMode::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Once => "Once",
            Self::Open => "Open",
            Self::Repeat => "Repeat",
            Self::Palindrome => "Palindrome",
            Self::Other(other) => other,
        }
    }
}

/// 电影时间值（Table 296 `/Start` `/Duration` 的两种基本形态）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MovieTimeValue {
    /// 非负整数。
    Integer(u64),
    /// 8 字节二进制补码整数（大端、非负）。
    Bytes([u8; 8]),
}

/// 电影时间（Table 296）：基本值 + 可选时间刻度（每秒单位数）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovieTime {
    /// 时间值本身。
    pub value: MovieTimeValue,
    /// 数组形态 `[时间 时间刻度]` 的第二项；`None` 表示沿用电影自身刻度。
    pub time_scale: Option<u64>,
}

/// 电影激活字典（Table 296）。
#[derive(Debug, Clone, PartialEq)]
pub struct MovieActivation {
    /// `/Start` 播放片段起点；`None` 表示从头播放。
    pub start: Option<MovieTime>,
    /// `/Duration` 播放片段时长；`None` 表示播放到结尾。
    pub duration: Option<MovieTime>,
    /// `/Rate` 初始播放速率，缺省 1.0（负数倒放）。
    pub rate: Option<f64>,
    /// `/Volume` 初始音量（-1.0..=1.0），缺省 1.0。
    pub volume: Option<f64>,
    /// `/ShowControls` 是否显示控制器栏，缺省 `false`。
    pub show_controls: Option<bool>,
    /// `/Mode` 播放模式，缺省 `Once`。
    pub mode: MovieMode,
    /// `/Synchronous` 是否同步播放，缺省 `false`。
    pub synchronous: Option<bool>,
    /// `/FWScale` 浮动窗口放大倍率 `[分子 分母]`（正整数）。
    pub fw_scale: Option<(u32, u32)>,
    /// `/FWPosition` 浮动窗口相对位置 `[水平 垂直]`（0.0..=1.0），
    /// 缺省 `[0.5 0.5]`。
    pub fw_position: Option<(f64, f64)>,
}

/// Movie 注解的 `/A` 播放设置（Table 186）：布尔或电影激活字典。
#[derive(Debug, Clone, PartialEq)]
pub enum MoviePlayback {
    /// 布尔形态：`true` 按缺省参数播放，`false` 不播放（缺省 `true`）。
    Flag(bool),
    /// 电影激活字典形态。
    Activation(MovieActivation),
}

/// 电影对象/激活字典解析错误。
#[derive(Debug)]
pub enum MovieError {
    /// 间接引用无法解析。
    Navigation(NavigationError),
    /// 字典条目缺失、类型或取值非法，或文件规格无法解析。
    Malformed(String),
}

impl fmt::Display for MovieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Navigation(error) => write!(f, "{error}"),
            Self::Malformed(message) => write!(f, "invalid movie object: {message}"),
        }
    }
}

impl std::error::Error for MovieError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Navigation(error) => Some(error),
            Self::Malformed(_) => None,
        }
    }
}

impl From<NavigationError> for MovieError {
    fn from(error: NavigationError) -> Self {
        Self::Navigation(error)
    }
}

fn malformed(message: impl Into<String>) -> MovieError {
    MovieError::Malformed(message.into())
}

impl Document {
    /// 把电影字典（间接引用或直接字典）解析为 [`Movie`]。
    ///
    /// `/F` 缺失或文件规格损坏、`/Aspect` 非两个正整数、`/Rotate` 非
    /// 90 的倍数、`/Poster` 既非布尔也非流时返回类型化错误。
    pub fn movie(&self, value: &Value) -> Result<Movie, MovieError> {
        let resolved = resolve(self, value)?;
        let dict = match &resolved {
            Value::Dict(dict) => dict,
            _ => return Err(malformed("movie dictionary is not a dictionary")),
        };
        let file_value = dict
            .get("F")
            .ok_or_else(|| malformed("movie dictionary is missing required /F"))?;
        let file = self
            .filespec(file_value)
            .map_err(|error| malformed(format!("movie /F: {error}")))?;
        let aspect = match dict.get("Aspect") {
            None => None,
            Some(value) => Some(parse_positive_pair(self, value, "/Aspect")?),
        };
        let rotate = match dict.get("Rotate") {
            None => 0,
            Some(value) => {
                let number = match resolve(self, value)? {
                    Value::Integer(number) => number,
                    _ => return Err(malformed("/Rotate must be an integer")),
                };
                if number % 90 != 0 || number < i64::from(i32::MIN) || number > i64::from(i32::MAX)
                {
                    return Err(malformed("/Rotate must be a multiple of 90"));
                }
                number as i32
            }
        };
        let poster = match dict.get("Poster") {
            None => Poster::Disabled,
            Some(value) => match resolve(self, value)? {
                Value::Bool(true) => Poster::FromMovieFile,
                Value::Bool(false) => Poster::Disabled,
                Value::Stream(..) => Poster::Stream(value.clone()),
                _ => return Err(malformed("/Poster must be a boolean or an image stream")),
            },
        };
        Ok(Movie {
            file,
            aspect,
            rotate,
            poster,
        })
    }
}

/// 解析电影激活字典；Movie 注解 `/A` 与 Movie 动作共用。
pub(crate) fn parse_movie_activation(
    document: &Document,
    dict: &Dict,
) -> Result<MovieActivation, MovieError> {
    let start = match dict.get("Start") {
        None => None,
        Some(value) => Some(parse_movie_time(document, value, "/Start")?),
    };
    let duration = match dict.get("Duration") {
        None => None,
        Some(value) => Some(parse_movie_time(document, value, "/Duration")?),
    };
    let rate = match dict.get("Rate") {
        None => None,
        Some(value) => {
            let number = match resolve(document, value)? {
                Value::Integer(number) => number as f64,
                Value::Float(number) => number,
                _ => return Err(malformed("/Rate must be a number")),
            };
            if !number.is_finite() {
                return Err(malformed("/Rate must be finite"));
            }
            Some(number)
        }
    };
    let volume = match dict.get("Volume") {
        None => None,
        Some(value) => {
            let number = match resolve(document, value)? {
                Value::Integer(number) => number as f64,
                Value::Float(number) => number,
                _ => return Err(malformed("/Volume must be a number")),
            };
            if !number.is_finite() || !(-1.0..=1.0).contains(&number) {
                return Err(malformed("/Volume must be within -1.0..=1.0"));
            }
            Some(number)
        }
    };
    let show_controls = match dict.get("ShowControls") {
        None => None,
        Some(value) => Some(parse_bool(document, value, "/ShowControls")?),
    };
    let synchronous = match dict.get("Synchronous") {
        None => None,
        Some(value) => Some(parse_bool(document, value, "/Synchronous")?),
    };
    let mode = match dict.get("Mode") {
        None => MovieMode::Once,
        Some(value) => {
            let name = match resolve(document, value)? {
                Value::Name(name) => name,
                _ => return Err(malformed("/Mode must be a name")),
            };
            MovieMode::from_name(&name)
        }
    };
    let fw_scale = match dict.get("FWScale") {
        None => None,
        Some(value) => Some(parse_positive_pair(document, value, "/FWScale")?),
    };
    let fw_position = match dict.get("FWPosition") {
        None => None,
        Some(value) => {
            let values = match resolve(document, value)? {
                Value::Array(array) if array.len() == 2 => {
                    let mut out = [0.0; 2];
                    for (index, entry) in array.iter().enumerate() {
                        let number = match resolve(document, entry)? {
                            Value::Integer(number) => number as f64,
                            Value::Float(number) => number,
                            _ => return Err(malformed("/FWPosition entries must be numbers")),
                        };
                        if !number.is_finite() || !(0.0..=1.0).contains(&number) {
                            return Err(malformed("/FWPosition entries must be within 0.0..=1.0"));
                        }
                        out[index] = number;
                    }
                    (out[0], out[1])
                }
                _ => {
                    return Err(malformed("/FWPosition must be an array of two numbers"));
                }
            };
            Some(values)
        }
    };
    Ok(MovieActivation {
        start,
        duration,
        rate,
        volume,
        show_controls,
        mode,
        synchronous,
        fw_scale,
        fw_position,
    })
}

/// 解析 `[时间 时间刻度]` 或单一时间值（Table 296 `/Start` `/Duration`）。
fn parse_movie_time(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<MovieTime, MovieError> {
    let resolved = resolve(document, value)?;
    if let Value::Array(array) = &resolved {
        if array.len() != 2 {
            return Err(malformed(format!(
                "{context} array must hold [time time_scale]"
            )));
        }
        let base = parse_time_value(document, &array.0[0], context)?;
        let scale = match resolve(document, &array.0[1])? {
            Value::Integer(scale) if scale >= 1 => scale as u64,
            _ => {
                return Err(malformed(format!(
                    "{context} time scale must be a positive integer"
                )));
            }
        };
        return Ok(MovieTime {
            value: base,
            time_scale: Some(scale),
        });
    }
    Ok(MovieTime {
        value: parse_time_value(document, &resolved, context)?,
        time_scale: None,
    })
}

fn parse_time_value(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<MovieTimeValue, MovieError> {
    match resolve(document, value)? {
        Value::Integer(number) if number >= 0 => Ok(MovieTimeValue::Integer(number as u64)),
        Value::String(text) => time_bytes(text.as_bytes(), context),
        Value::HexString(hex) => {
            let bytes = decode_hex(&hex)
                .ok_or_else(|| malformed(format!("{context} has malformed hex string")))?;
            time_bytes(&bytes, context)
        }
        _ => Err(malformed(format!(
            "{context} must be a nonnegative integer or an 8-byte string"
        ))),
    }
}

fn time_bytes(bytes: &[u8], context: &str) -> Result<MovieTimeValue, MovieError> {
    let array: [u8; 8] = bytes.try_into().map_err(|_| {
        malformed(format!(
            "{context} 8-byte time string must be exactly 8 bytes"
        ))
    })?;
    if array[0] & 0x80 != 0 {
        return Err(malformed(format!(
            "{context} time value must be nonnegative"
        )));
    }
    Ok(MovieTimeValue::Bytes(array))
}

fn parse_positive_pair(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<(u32, u32), MovieError> {
    let array = match resolve(document, value)? {
        Value::Array(array) => array,
        _ => {
            return Err(malformed(format!(
                "{context} must be an array of two integers"
            )));
        }
    };
    if array.len() != 2 {
        return Err(malformed(format!(
            "{context} must be an array of two integers"
        )));
    }
    let mut out = [0u32; 2];
    for (index, entry) in array.iter().enumerate() {
        match resolve(document, entry)? {
            Value::Integer(number) if (1..=i64::from(u32::MAX)).contains(&number) => {
                out[index] = number as u32;
            }
            _ => {
                return Err(malformed(format!(
                    "{context} entries must be positive integers"
                )));
            }
        }
    }
    Ok((out[0], out[1]))
}

fn parse_bool(document: &Document, value: &Value, context: &str) -> Result<bool, MovieError> {
    match resolve(document, value)? {
        Value::Bool(flag) => Ok(flag),
        _ => Err(malformed(format!("{context} must be a boolean"))),
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = hex
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let (pairs, remainder) = digits.as_chunks::<2>();
    if !remainder.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let hi = hex_digit(pair[0])?;
        let lo = hex_digit(pair[1])?;
        bytes.push(hi * 16 + lo);
    }
    Some(bytes)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
