//! 媒体播放参数写端（13.2.5 Table 279–281）。
//!
//! 只写出显式声明的条目，Table 280 的缺省值不物化；`/PL` media players
//! （13.2.7）留待后续切片。校验由 [`MediaRendition::validate`] 在对象号
//! 分配前触发，非法取值返回 `InvalidObject` 并保持零字节输出。
//!
//! [`MediaRendition::validate`]: super::MediaRendition::validate

use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

/// 播放适配方式 `/F`（Table 280）。
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
    /// 未识别的枚举值（原样保留）。
    Other(i64),
}

impl MediaFit {
    fn code(self) -> i64 {
        match self {
            Self::Meet => 0,
            Self::Slice => 1,
            Self::Fill => 2,
            Self::Scroll => 3,
            Self::Clip => 4,
            Self::PlayerDefault => 5,
            Self::Other(code) => code,
        }
    }
}

/// media play parameters 字典（Table 279）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaPlayParameters {
    must_honour: Option<MediaPlaySettings>,
    best_effort: Option<MediaPlaySettings>,
}

impl MediaPlayParameters {
    /// 空播放参数；`/MH` `/BE` 均可省略。
    pub fn new() -> Self {
        Self::default()
    }

    /// `/MH` 必须满足的播放设置（Table 280）。
    pub fn with_must_honour(mut self, settings: MediaPlaySettings) -> Self {
        self.must_honour = Some(settings);
        self
    }

    /// `/BE` 尽力满足的播放设置（Table 280）。
    pub fn with_best_effort(mut self, settings: MediaPlaySettings) -> Self {
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
        let mut dict = Dict::new().insert("Type", Value::Name("MediaPlayParams".into()));
        if let Some(settings) = &self.must_honour {
            dict = dict.insert("MH", Value::Dict(settings.to_dict()));
        }
        if let Some(settings) = &self.best_effort {
            dict = dict.insert("BE", Value::Dict(settings.to_dict()));
        }
        dict
    }
}

/// 播放设置（Table 280）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaPlaySettings {
    volume: Option<i64>,
    controller: Option<bool>,
    fit: Option<MediaFit>,
    duration: Option<MediaDuration>,
    auto_play: Option<bool>,
    repeat_count: Option<f64>,
}

impl MediaPlaySettings {
    /// 空设置；全部条目省略。
    pub fn new() -> Self {
        Self::default()
    }

    /// `/V` 音量百分比；负数非法，0 静音。
    pub fn with_volume(mut self, volume: i64) -> Self {
        self.volume = Some(volume);
        self
    }

    /// `/C` 是否显示播放器控制器。
    pub fn with_controller(mut self, controller: bool) -> Self {
        self.controller = Some(controller);
        self
    }

    /// `/F` 缩放适配方式。
    pub fn with_fit(mut self, fit: MediaFit) -> Self {
        self.fit = Some(fit);
        self
    }

    /// `/D` 媒体时长（Table 281）。
    pub fn with_duration(mut self, duration: MediaDuration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// `/A` 是否自动播放。
    pub fn with_auto_play(mut self, auto_play: bool) -> Self {
        self.auto_play = Some(auto_play);
        self
    }

    /// `/RC` 重复次数；0 表示无限循环，负数非法。
    pub fn with_repeat_count(mut self, repeat_count: f64) -> Self {
        self.repeat_count = Some(repeat_count);
        self
    }

    fn validate(&self, key: &str) -> Result<(), WriteError> {
        if self.volume.is_some_and(|volume| volume < 0) {
            return Err(WriteError::InvalidObject(format!(
                "media play parameters /{key} /V must not be negative"
            )));
        }
        if let Some(count) = self.repeat_count
            && (!count.is_finite() || count < 0.0)
        {
            return Err(WriteError::InvalidObject(format!(
                "media play parameters /{key} /RC must be a finite non-negative number"
            )));
        }
        if let Some(duration) = &self.duration {
            duration.validate(&format!("media play parameters /{key} /D"))?;
        }
        Ok(())
    }

    fn to_dict(&self) -> Dict {
        let mut dict = Dict::new();
        if let Some(volume) = self.volume {
            dict = dict.insert("V", Value::Integer(volume));
        }
        if let Some(controller) = self.controller {
            dict = dict.insert("C", Value::Bool(controller));
        }
        if let Some(fit) = self.fit {
            dict = dict.insert("F", Value::Integer(fit.code()));
        }
        if let Some(duration) = &self.duration {
            dict = dict.insert("D", Value::Dict(duration.to_dict()));
        }
        if let Some(auto_play) = self.auto_play {
            dict = dict.insert("A", Value::Bool(auto_play));
        }
        if let Some(repeat_count) = self.repeat_count {
            dict = dict.insert("RC", Value::Float(repeat_count));
        }
        dict
    }
}

/// 媒体时长（Table 281）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaDuration {
    kind: MediaDurationKind,
    timespan: Option<MediaTimespan>,
}

/// `/S` 时长来源；写端只构造 Table 281 定义的三个名称。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaDurationKind {
    Intrinsic,
    Infinity,
    Explicit,
}

impl MediaDuration {
    /// `/S /I`：媒体内在时长。
    pub fn intrinsic() -> Self {
        Self {
            kind: MediaDurationKind::Intrinsic,
            timespan: None,
        }
    }

    /// `/S /F`：无限时长。
    pub fn infinity() -> Self {
        Self {
            kind: MediaDurationKind::Infinity,
            timespan: None,
        }
    }

    /// `/S /T`：由时间跨度显式指定。
    pub fn explicit(timespan: MediaTimespan) -> Self {
        Self {
            kind: MediaDurationKind::Explicit,
            timespan: Some(timespan),
        }
    }

    fn validate(&self, context: &str) -> Result<(), WriteError> {
        if matches!(self.kind, MediaDurationKind::Explicit) && self.timespan.is_none() {
            return Err(WriteError::InvalidObject(format!(
                "{context} explicit duration is missing /T"
            )));
        }
        if let Some(timespan) = &self.timespan {
            timespan.validate(&format!("{context} /T"))?;
        }
        Ok(())
    }

    fn to_dict(&self) -> Dict {
        let subtype = match self.kind {
            MediaDurationKind::Intrinsic => "I",
            MediaDurationKind::Infinity => "F",
            MediaDurationKind::Explicit => "T",
        };
        let mut dict = Dict::new()
            .insert("Type", Value::Name("MediaDuration".into()))
            .insert("S", Value::Name(subtype.into()));
        if let Some(timespan) = &self.timespan {
            dict = dict.insert("T", Value::Dict(timespan.to_dict()));
        }
        dict
    }
}

/// 时间跨度（Table 289）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaTimespan {
    seconds: f64,
}

impl MediaTimespan {
    /// `/V` 秒数；负数非法。
    pub fn new(seconds: f64) -> Self {
        Self { seconds }
    }

    fn validate(&self, context: &str) -> Result<(), WriteError> {
        if !self.seconds.is_finite() || self.seconds < 0.0 {
            return Err(WriteError::InvalidObject(format!(
                "{context} /V must be a finite non-negative number"
            )));
        }
        Ok(())
    }

    fn to_dict(&self) -> Dict {
        Dict::new()
            .insert("Type", Value::Name("Timespan".into()))
            .insert("S", Value::Name("S".into()))
            .insert("V", Value::Float(self.seconds))
    }
}
