//! 电影对象与电影激活字典写端（ISO 32000-1 13.4 Table 295/296）。
//!
//! [`Movie`] 是电影字典的构建态：`/F` 自描述电影文件名（写成内联文件
//! 规格 `<< /Type /Filespec /F (name) >>`）、可选 `/Aspect` 像素尺寸、
//! `/Rotate` 旋转角与 `/Poster` 布尔海报。ISO 32000-1 的电影字典没有
//! `/Type` 条目，因此不写出 `/Type`。
//!
//! [`MovieActivation`] 供 Movie 注解的 `/A`（Table 186）与 Movie 动作
//! （Table 209）共用：只写出显式声明的条目，Table 296 的缺省值不物化
//! （`/Mode` 缺省 `Once` 同样省略）。`/Start` `/Duration` 的 8 字节形态
//! 以十六进制字节串写出，保证任意位模式往返无损；海报图像流本体不在
//! 写端建模（只支持布尔形态）。
//!
//! 校验：文件名须为非空可打印 ASCII；`/Rotate` 为 90 的倍数；`/Rate`
//! 有限；`/Volume` 在 -1.0..=1.0；`/FWScale` 两个正整数；`/FWPosition`
//! 两个 0.0..=1.0 的有限数；时间值与刻度非负/正整数。

use crate::core::reader::movie::{MovieMode, MovieTime, MovieTimeValue};
use crate::core::types::{Array, Dict, Value};

/// Movie 注解 `/A` 的构建态（Table 186）：布尔或激活字典。
#[derive(Debug, Clone, PartialEq)]
pub enum MoviePlayback {
    /// 布尔形态：`true` 按缺省参数播放，`false` 不播放。
    Flag(bool),
    /// 电影激活字典形态。
    Activation(MovieActivation),
}

impl MoviePlayback {
    /// 校验播放设置；错误串直接用于 `WriteError`。
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Flag(_) => Ok(()),
            Self::Activation(activation) => activation.validate(),
        }
    }

    /// 写出 `/A` 值：布尔或激活字典。
    pub(crate) fn to_value(&self) -> Value {
        match self {
            Self::Flag(flag) => Value::Bool(*flag),
            Self::Activation(activation) => Value::Dict(activation.to_dict()),
        }
    }
}

/// 电影字典的构建态（Table 295）。
#[derive(Debug, Clone, PartialEq)]
pub struct Movie {
    file_name: String,
    aspect: Option<(u32, u32)>,
    rotate: i32,
    poster: bool,
}

impl Movie {
    /// 新建电影字典；`file_name` 为自描述电影文件名（写成 `/F` 文件规格）。
    pub fn new(file_name: impl Into<String>) -> Self {
        Self {
            file_name: file_name.into(),
            aspect: None,
            rotate: 0,
            poster: false,
        }
    }

    /// `/Aspect` 像素尺寸 `[width height]`；纯声音电影应省略。
    pub fn with_aspect(mut self, width: u32, height: u32) -> Self {
        self.aspect = Some((width, height));
        self
    }

    /// `/Rotate` 相对页面顺时针旋转角（须为 90 的倍数，缺省 0 不写出）。
    pub fn with_rotate(mut self, rotate: i32) -> Self {
        self.rotate = rotate;
        self
    }

    /// `/Poster` 是否从电影文件取海报图像（`true`）；`false` 为缺省不写出。
    pub fn with_poster(mut self, poster: bool) -> Self {
        self.poster = poster;
        self
    }

    /// 校验文件名与几何条目；错误串直接用于 `WriteError`。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.file_name.is_empty() {
            return Err("movie /F file name must not be empty".into());
        }
        if !self
            .file_name
            .bytes()
            .all(|byte| (0x20..=0x7E).contains(&byte))
        {
            return Err("movie /F file name must contain printable ASCII characters only".into());
        }
        if self.rotate % 90 != 0 {
            return Err(format!(
                "movie /Rotate must be a multiple of 90, got {}",
                self.rotate
            ));
        }
        if let Some((width, height)) = self.aspect
            && (width == 0 || height == 0)
        {
            return Err("movie /Aspect width and height must be positive".into());
        }
        Ok(())
    }

    /// 写成电影字典（内联在 Movie 注解 `/Movie`）。
    pub(crate) fn to_dict(&self) -> Dict {
        let filespec = Dict::new()
            .insert("Type", Value::Name("Filespec".into()))
            .insert("F", Value::String(self.file_name.clone()));
        let mut dict = Dict::new().insert("F", Value::Dict(filespec));
        if let Some((width, height)) = self.aspect {
            dict = dict.insert(
                "Aspect",
                Value::Array(Array(vec![
                    Value::Integer(i64::from(width)),
                    Value::Integer(i64::from(height)),
                ])),
            );
        }
        if self.rotate != 0 {
            dict = dict.insert("Rotate", Value::Integer(i64::from(self.rotate)));
        }
        if self.poster {
            dict = dict.insert("Poster", Value::Bool(true));
        }
        dict
    }
}

/// 电影激活字典的构建态（Table 296）。
#[derive(Debug, Clone, PartialEq)]
pub struct MovieActivation {
    start: Option<MovieTime>,
    duration: Option<MovieTime>,
    rate: Option<f64>,
    volume: Option<f64>,
    show_controls: Option<bool>,
    mode: MovieMode,
    synchronous: Option<bool>,
    fw_scale: Option<(u32, u32)>,
    fw_position: Option<(f64, f64)>,
}

impl Default for MovieActivation {
    fn default() -> Self {
        Self::new()
    }
}

impl MovieActivation {
    /// 新建激活字典；所有条目使用 Table 296 的缺省语义。
    pub fn new() -> Self {
        Self {
            start: None,
            duration: None,
            rate: None,
            volume: None,
            show_controls: None,
            mode: MovieMode::Once,
            synchronous: None,
            fw_scale: None,
            fw_position: None,
        }
    }

    /// `/Start` 播放片段起点。
    pub fn with_start(mut self, start: MovieTime) -> Self {
        self.start = Some(start);
        self
    }

    /// `/Duration` 播放片段时长。
    pub fn with_duration(mut self, duration: MovieTime) -> Self {
        self.duration = Some(duration);
        self
    }

    /// `/Rate` 初始播放速率（缺省 1.0 不写出；负数倒放）。
    pub fn with_rate(mut self, rate: f64) -> Self {
        self.rate = Some(rate);
        self
    }

    /// `/Volume` 初始音量（-1.0..=1.0；缺省 1.0 不写出）。
    pub fn with_volume(mut self, volume: f64) -> Self {
        self.volume = Some(volume);
        self
    }

    /// `/ShowControls` 是否显示控制器栏（缺省 false 不写出）。
    pub fn with_show_controls(mut self, show_controls: bool) -> Self {
        self.show_controls = Some(show_controls);
        self
    }

    /// `/Mode` 播放模式（缺省 `Once` 不写出）。
    pub fn with_mode(mut self, mode: MovieMode) -> Self {
        self.mode = mode;
        self
    }

    /// `/Synchronous` 是否同步播放（缺省 false 不写出）。
    pub fn with_synchronous(mut self, synchronous: bool) -> Self {
        self.synchronous = Some(synchronous);
        self
    }

    /// `/FWScale` 浮动窗口放大倍率 `[分子 分母]`。
    pub fn with_fw_scale(mut self, numerator: u32, denominator: u32) -> Self {
        self.fw_scale = Some((numerator, denominator));
        self
    }

    /// `/FWPosition` 浮动窗口相对位置（0.0..=1.0；缺省 `[0.5 0.5]`）。
    pub fn with_fw_position(mut self, horizontal: f64, vertical: f64) -> Self {
        self.fw_position = Some((horizontal, vertical));
        self
    }

    /// 校验条目取值；错误串直接用于 `WriteError`。
    pub(crate) fn validate(&self) -> Result<(), String> {
        for (context, time) in [("/Start", &self.start), ("/Duration", &self.duration)] {
            if let Some(time) = time {
                validate_time(time, context)?;
            }
        }
        if let Some(rate) = self.rate
            && !rate.is_finite()
        {
            return Err("movie /Rate must be finite".into());
        }
        if let Some(volume) = self.volume
            && (!volume.is_finite() || !(-1.0..=1.0).contains(&volume))
        {
            return Err(format!(
                "movie /Volume must be within -1.0..=1.0, got {volume}"
            ));
        }
        if let MovieMode::Other(name) = &self.mode {
            return Err(format!(
                "movie /Mode {name:?} is not one of Once, Open, Repeat, Palindrome"
            ));
        }
        if let Some((numerator, denominator)) = self.fw_scale
            && (numerator == 0 || denominator == 0)
        {
            return Err("movie /FWScale numerator and denominator must be positive".into());
        }
        if let Some((horizontal, vertical)) = self.fw_position {
            for (context, value) in [("horizontal", horizontal), ("vertical", vertical)] {
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    return Err(format!(
                        "movie /FWPosition {context} component must be within 0.0..=1.0"
                    ));
                }
            }
        }
        Ok(())
    }

    /// 写出显式声明的激活条目（缺省值不物化）。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new();
        if let Some(start) = &self.start {
            dict = dict.insert("Start", time_value(start));
        }
        if let Some(duration) = &self.duration {
            dict = dict.insert("Duration", time_value(duration));
        }
        if let Some(rate) = self.rate {
            dict = dict.insert("Rate", Value::Float(rate));
        }
        if let Some(volume) = self.volume {
            dict = dict.insert("Volume", Value::Float(volume));
        }
        if let Some(show_controls) = self.show_controls {
            dict = dict.insert("ShowControls", Value::Bool(show_controls));
        }
        if self.mode != MovieMode::Once {
            dict = dict.insert("Mode", Value::Name(self.mode.name().into()));
        }
        if let Some(synchronous) = self.synchronous {
            dict = dict.insert("Synchronous", Value::Bool(synchronous));
        }
        if let Some((numerator, denominator)) = self.fw_scale {
            dict = dict.insert(
                "FWScale",
                Value::Array(Array(vec![
                    Value::Integer(i64::from(numerator)),
                    Value::Integer(i64::from(denominator)),
                ])),
            );
        }
        if let Some((horizontal, vertical)) = self.fw_position {
            dict = dict.insert(
                "FWPosition",
                Value::Array(Array(vec![
                    Value::Float(horizontal),
                    Value::Float(vertical),
                ])),
            );
        }
        dict
    }
}

fn validate_time(time: &MovieTime, context: &str) -> Result<(), String> {
    match time.value {
        MovieTimeValue::Integer(value) if value > i64::MAX as u64 => {
            return Err(format!("movie {context} integer time exceeds 64-bit range"));
        }
        MovieTimeValue::Bytes(bytes) if bytes[0] & 0x80 != 0 => {
            return Err(format!("movie {context} time value must be nonnegative"));
        }
        MovieTimeValue::Integer(_) | MovieTimeValue::Bytes(_) => {}
    }
    if let Some(scale) = time.time_scale
        && scale == 0
    {
        return Err(format!("movie {context} time scale must be positive"));
    }
    Ok(())
}

fn time_value(time: &MovieTime) -> Value {
    let base = match &time.value {
        MovieTimeValue::Integer(value) => Value::Integer(*value as i64),
        MovieTimeValue::Bytes(bytes) => Value::HexString(hex_upper(bytes)),
    };
    match time.time_scale {
        None => base,
        Some(scale) => Value::Array(Array(vec![base, Value::Integer(scale as i64)])),
    }
}

fn hex_upper(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit(u32::from(byte >> 4), 16).expect("nibble"));
        out.push(char::from_digit(u32::from(byte & 0x0F), 16).expect("nibble"));
    }
    out
}
