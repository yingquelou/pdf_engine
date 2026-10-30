//! 媒体屏幕参数写端（13.2.6 Table 282–284）。
//!
//! 只写出显式声明的条目，Table 283 的缺省值不物化；`/W` 为浮动窗口时
//! 必须提供 `/F` 浮动窗口参数。校验由 [`MediaRendition::validate`] 在
//! 对象号分配前触发，非法取值返回 `InvalidObject` 并保持零字节输出。
//!
//! [`MediaRendition::validate`]: super::MediaRendition::validate

use super::super::text_string_value;
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

/// `/W` 窗口类型（Table 283）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaWindow {
    /// `0`：浮动窗口。
    Floating,
    /// `1`：全屏。
    FullScreen,
    /// `2`：隐藏。
    Hidden,
    /// `3`：屏幕注解矩形（缺省）。
    AnnotationRectangle,
    /// 未识别的枚举值（原样保留）。
    Other(i64),
}

impl MediaWindow {
    fn code(self) -> i64 {
        match self {
            Self::Floating => 0,
            Self::FullScreen => 1,
            Self::Hidden => 2,
            Self::AnnotationRectangle => 3,
            Self::Other(code) => code,
        }
    }
}

/// media screen parameters 字典（Table 282）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaScreenParameters {
    must_honour: Option<MediaScreenSettings>,
    best_effort: Option<MediaScreenSettings>,
}

impl MediaScreenParameters {
    /// 空屏幕参数；`/MH` `/BE` 均可省略。
    pub fn new() -> Self {
        Self::default()
    }

    /// `/MH` 必须满足的屏幕设置（Table 283）。
    pub fn with_must_honour(mut self, settings: MediaScreenSettings) -> Self {
        self.must_honour = Some(settings);
        self
    }

    /// `/BE` 尽力满足的屏幕设置（Table 283）。
    pub fn with_best_effort(mut self, settings: MediaScreenSettings) -> Self {
        self.best_effort = Some(settings);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), WriteError> {
        if let Some(settings) = &self.must_honour {
            settings.validate("MH")?;
        }
        if let Some(settings) = &self.best_effort {
            settings.validate("BE")?;
        }
        Ok(())
    }

    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert("Type", Value::Name("MediaScreenParams".into()));
        if let Some(settings) = &self.must_honour {
            dict = dict.insert("MH", Value::Dict(settings.to_dict()));
        }
        if let Some(settings) = &self.best_effort {
            dict = dict.insert("BE", Value::Dict(settings.to_dict()));
        }
        dict
    }
}

/// 屏幕设置（Table 283）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaScreenSettings {
    window: Option<MediaWindow>,
    background: Option<[f64; 3]>,
    opacity: Option<f64>,
    monitor: Option<i64>,
    floating: Option<MediaFloatingWindow>,
}

impl MediaScreenSettings {
    /// 空设置；全部条目省略。
    pub fn new() -> Self {
        Self::default()
    }

    /// `/W` 窗口类型；浮动窗口还须 [`Self::with_floating`]。
    pub fn with_window(mut self, window: MediaWindow) -> Self {
        self.window = Some(window);
        self
    }

    /// `/B` 背景色 `[r g b]`，各分量 0.0..=1.0。
    pub fn with_background(mut self, background: [f64; 3]) -> Self {
        self.background = Some(background);
        self
    }

    /// `/O` 不透明度 0.0..=1.0。
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// `/M` 显示器编号（Table 293，0..=6）。
    pub fn with_monitor(mut self, monitor: i64) -> Self {
        self.monitor = Some(monitor);
        self
    }

    /// `/F` 浮动窗口参数（Table 284）；`/W` 须为 `Floating`。
    pub fn with_floating(mut self, floating: MediaFloatingWindow) -> Self {
        self.floating = Some(floating);
        self
    }

    fn validate(&self, key: &str) -> Result<(), WriteError> {
        if let Some(background) = self.background
            && background
                .iter()
                .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
        {
            return Err(WriteError::InvalidObject(format!(
                "media screen parameters /{key} /B components must be within 0.0..=1.0"
            )));
        }
        if let Some(opacity) = self.opacity
            && (!opacity.is_finite() || !(0.0..=1.0).contains(&opacity))
        {
            return Err(WriteError::InvalidObject(format!(
                "media screen parameters /{key} /O must be within 0.0..=1.0"
            )));
        }
        if let Some(monitor) = self.monitor
            && !(0..=6).contains(&monitor)
        {
            return Err(WriteError::InvalidObject(format!(
                "media screen parameters /{key} /M must be within 0..=6"
            )));
        }
        match (self.window, &self.floating) {
            (Some(MediaWindow::Floating), None) => {
                return Err(WriteError::InvalidObject(format!(
                    "media screen parameters /{key} /F is required when /W is 0"
                )));
            }
            (Some(window), Some(_)) if window != MediaWindow::Floating => {
                return Err(WriteError::InvalidObject(format!(
                    "media screen parameters /{key} /F is only valid when /W is 0"
                )));
            }
            (None, Some(_)) => {
                return Err(WriteError::InvalidObject(format!(
                    "media screen parameters /{key} /F is only valid when /W is 0"
                )));
            }
            _ => {}
        }
        if let Some(floating) = &self.floating {
            floating.validate(&format!("media screen parameters /{key} /F"))?;
        }
        Ok(())
    }

    fn to_dict(&self) -> Dict {
        let mut dict = Dict::new();
        if let Some(window) = self.window {
            dict = dict.insert("W", Value::Integer(window.code()));
        }
        if let Some(background) = self.background {
            dict = dict.insert(
                "B",
                Value::Array(Array(background.into_iter().map(Value::Float).collect())),
            );
        }
        if let Some(opacity) = self.opacity {
            dict = dict.insert("O", Value::Float(opacity));
        }
        if let Some(monitor) = self.monitor {
            dict = dict.insert("M", Value::Integer(monitor));
        }
        if let Some(floating) = &self.floating {
            dict = dict.insert("F", Value::Dict(floating.to_dict()));
        }
        dict
    }
}

/// 浮动窗口参数（Table 284）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaFloatingWindow {
    dimensions: [i64; 2],
    resize_target: Option<i64>,
    position: Option<i64>,
    offscreen: Option<i64>,
    title_bar: Option<bool>,
    closable: Option<bool>,
    resizable: Option<i64>,
    title: Option<Vec<String>>,
}

impl MediaFloatingWindow {
    /// `/D` 播放矩形像素尺寸；两个分量均须非负。
    pub fn new(width: i64, height: i64) -> Self {
        Self {
            dimensions: [width, height],
            resize_target: None,
            position: None,
            offscreen: None,
            title_bar: None,
            closable: None,
            resizable: None,
            title: None,
        }
    }

    /// `/RT` 调整窗口大小的参照目标（0..=3）。
    pub fn with_resize_target(mut self, resize_target: i64) -> Self {
        self.resize_target = Some(resize_target);
        self
    }

    /// `/P` 窗口位置（0..=8）。
    pub fn with_position(mut self, position: i64) -> Self {
        self.position = Some(position);
        self
    }

    /// `/O` 窗口移出屏幕的方式（0..=2）。
    pub fn with_offscreen(mut self, offscreen: i64) -> Self {
        self.offscreen = Some(offscreen);
        self
    }

    /// `/T` 是否显示标题栏。
    pub fn with_title_bar(mut self, title_bar: bool) -> Self {
        self.title_bar = Some(title_bar);
        self
    }

    /// `/UC` 是否允许关闭窗口。
    pub fn with_closable(mut self, closable: bool) -> Self {
        self.closable = Some(closable);
        self
    }

    /// `/R` 是否允许调整窗口大小（0..=2）。
    pub fn with_resizable(mut self, resizable: i64) -> Self {
        self.resizable = Some(resizable);
        self
    }

    /// `/TT` 标题栏文本行；允许空行与空数组。
    pub fn with_title(mut self, title: Vec<String>) -> Self {
        self.title = Some(title);
        self
    }

    fn validate(&self, context: &str) -> Result<(), WriteError> {
        if self.dimensions.iter().any(|dimension| *dimension < 0) {
            return Err(WriteError::InvalidObject(format!(
                "{context} /D entries must not be negative"
            )));
        }
        for (key, value, low, high) in [
            ("/RT", self.resize_target, 0, 3),
            ("/P", self.position, 0, 8),
            ("/O", self.offscreen, 0, 2),
            ("/R", self.resizable, 0, 2),
        ] {
            if value.is_some_and(|value| !(low..=high).contains(&value)) {
                return Err(WriteError::InvalidObject(format!(
                    "{context} {key} must be within {low}..={high}"
                )));
            }
        }
        Ok(())
    }

    fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("FWParams".into()))
            .insert(
                "D",
                Value::Array(Array(
                    self.dimensions.into_iter().map(Value::Integer).collect(),
                )),
            );
        if let Some(resize_target) = self.resize_target {
            dict = dict.insert("RT", Value::Integer(resize_target));
        }
        if let Some(position) = self.position {
            dict = dict.insert("P", Value::Integer(position));
        }
        if let Some(offscreen) = self.offscreen {
            dict = dict.insert("O", Value::Integer(offscreen));
        }
        if let Some(title_bar) = self.title_bar {
            dict = dict.insert("T", Value::Bool(title_bar));
        }
        if let Some(closable) = self.closable {
            dict = dict.insert("UC", Value::Bool(closable));
        }
        if let Some(resizable) = self.resizable {
            dict = dict.insert("R", Value::Integer(resizable));
        }
        if let Some(title) = &self.title {
            dict = dict.insert(
                "TT",
                Value::Array(Array(
                    title.iter().map(|line| text_string_value(line)).collect(),
                )),
            );
        }
        dict
    }
}
