//! 过渡字典写端（12.4.4 Table 162）与 Transition 动作（12.6.4.14 Table 215）。
//!
//! 过渡字典同时服务于页面对象的 `/Trans` 与 Transition 动作的 `/Trans`；
//! 写端总是写出 `/Type /Trans` 与 `/S`，其余条目仅在显式声明时写出。

use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

/// 过渡样式（Table 162 `/S`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionStyle {
    /// 两线扫过屏幕。
    Split,
    /// 多条等距线同向扫过。
    Blinds,
    /// 矩形框由边缘扫入或由中心扫出。
    Box,
    /// 单线单向扫过。
    Wipe,
    /// 旧页渐隐。
    Dissolve,
    /// 宽带扫过的溶解效果。
    Glitter,
    /// 无特效，直接替换。
    R,
    /// 飞出或飞入（PDF 1.5）。
    Fly,
    /// 旧页被推出（PDF 1.5）。
    Push,
    /// 新页覆盖旧页（PDF 1.5）。
    Cover,
    /// 旧页移出（PDF 1.5）。
    Uncover,
    /// 新页渐显（PDF 1.5）。
    Fade,
}

impl TransitionStyle {
    fn name(self) -> &'static str {
        match self {
            Self::Split => "Split",
            Self::Blinds => "Blinds",
            Self::Box => "Box",
            Self::Wipe => "Wipe",
            Self::Dissolve => "Dissolve",
            Self::Glitter => "Glitter",
            Self::R => "R",
            Self::Fly => "Fly",
            Self::Push => "Push",
            Self::Cover => "Cover",
            Self::Uncover => "Uncover",
            Self::Fade => "Fade",
        }
    }

    fn supports_dimension(self) -> bool {
        matches!(self, Self::Split | Self::Blinds)
    }

    fn supports_motion(self) -> bool {
        matches!(self, Self::Split | Self::Box | Self::Fly)
    }

    fn supports_direction(self) -> bool {
        matches!(
            self,
            Self::Wipe | Self::Glitter | Self::Fly | Self::Cover | Self::Uncover | Self::Push
        )
    }
}

/// 过渡作用维度（`/Dm`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionDimension {
    /// 水平。
    Horizontal,
    /// 垂直。
    Vertical,
}

impl TransitionDimension {
    fn name(self) -> &'static str {
        match self {
            Self::Horizontal => "H",
            Self::Vertical => "V",
        }
    }
}

/// 过渡运动方向（`/M`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionMotion {
    /// 由页面边缘向内。
    Inward,
    /// 由中心向外。
    Outward,
}

impl TransitionMotion {
    fn name(self) -> &'static str {
        match self {
            Self::Inward => "I",
            Self::Outward => "O",
        }
    }
}

/// 过渡方向（`/Di`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionDirection {
    /// 逆时针角度：0、90、180、270 或 315（315 仅 Glitter）。
    Degrees(u16),
    /// 名字 `None`（仅 Fly）。
    None,
}

/// 过渡字典（Table 162）。
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    style: TransitionStyle,
    duration: Option<f64>,
    dimension: Option<TransitionDimension>,
    motion: Option<TransitionMotion>,
    direction: Option<TransitionDirection>,
    scale: Option<f64>,
    border: Option<bool>,
}

impl Transition {
    /// 以过渡样式构造；`R` 表示无特效。
    pub fn new(style: TransitionStyle) -> Self {
        Self {
            style,
            duration: None,
            dimension: None,
            motion: None,
            direction: None,
            scale: None,
            border: None,
        }
    }

    /// `/D`：过渡时长（秒），须为非负有限数。
    pub fn with_duration(mut self, seconds: f64) -> Self {
        self.duration = Some(seconds);
        self
    }

    /// `/Dm`：仅 Split 与 Blinds 允许。
    pub fn with_dimension(mut self, dimension: TransitionDimension) -> Self {
        self.dimension = Some(dimension);
        self
    }

    /// `/M`：仅 Split、Box 与 Fly 允许。
    pub fn with_motion(mut self, motion: TransitionMotion) -> Self {
        self.motion = Some(motion);
        self
    }

    /// `/Di`：仅 Wipe、Glitter、Fly、Cover、Uncover 与 Push 允许。
    pub fn with_direction(mut self, direction: TransitionDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    /// `/SS`（PDF 1.5）：仅 Fly 允许，须为正有限数。
    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale = Some(scale);
        self
    }

    /// `/B`（PDF 1.5）：仅 Fly 允许。
    pub fn with_border(mut self, border: bool) -> Self {
        self.border = Some(border);
        self
    }

    /// 校验条目取值与样式适用性（写出前的零字节契约）。
    pub(crate) fn validate(&self) -> Result<(), WriteError> {
        if let Some(duration) = self.duration
            && (!duration.is_finite() || duration < 0.0)
        {
            return Err(WriteError::InvalidObject(
                "transition /D must be a non-negative finite number".into(),
            ));
        }
        if let Some(scale) = self.scale
            && (!scale.is_finite() || scale <= 0.0)
        {
            return Err(WriteError::InvalidObject(
                "transition /SS must be a positive finite number".into(),
            ));
        }
        if self.dimension.is_some() && !self.style.supports_dimension() {
            return Err(WriteError::InvalidObject(format!(
                "transition /Dm is only defined for Split and Blinds, not {}",
                self.style.name()
            )));
        }
        if self.motion.is_some() && !self.style.supports_motion() {
            return Err(WriteError::InvalidObject(format!(
                "transition /M is only defined for Split, Box and Fly, not {}",
                self.style.name()
            )));
        }
        if self.direction.is_some() && !self.style.supports_direction() {
            return Err(WriteError::InvalidObject(format!(
                "transition /Di is only defined for Wipe, Glitter, Fly, Cover, Uncover and Push, not {}",
                self.style.name()
            )));
        }
        if let Some(direction) = self.direction {
            match direction {
                TransitionDirection::Degrees(90 | 180) if self.style != TransitionStyle::Wipe => {
                    return Err(WriteError::InvalidObject(
                        "transition /Di 90 and 180 are only defined for Wipe".into(),
                    ));
                }
                TransitionDirection::Degrees(315) if self.style != TransitionStyle::Glitter => {
                    return Err(WriteError::InvalidObject(
                        "transition /Di 315 is only defined for Glitter".into(),
                    ));
                }
                TransitionDirection::Degrees(0 | 90 | 180 | 270 | 315) => {}
                TransitionDirection::Degrees(other) => {
                    return Err(WriteError::InvalidObject(format!(
                        "transition /Di {other} is not one of 0, 90, 180, 270, 315"
                    )));
                }
                TransitionDirection::None if self.style != TransitionStyle::Fly => {
                    return Err(WriteError::InvalidObject(
                        "transition /Di None is only defined for Fly".into(),
                    ));
                }
                TransitionDirection::None => {}
            }
        }
        if self.scale.is_some() && self.style != TransitionStyle::Fly {
            return Err(WriteError::InvalidObject(
                "transition /SS is only defined for Fly".into(),
            ));
        }
        if self.border.is_some() && self.style != TransitionStyle::Fly {
            return Err(WriteError::InvalidObject(
                "transition /B is only defined for Fly".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn to_dict(&self) -> Result<Dict, WriteError> {
        self.validate()?;
        let mut dict = Dict::new()
            .insert("Type", Value::Name("Trans".into()))
            .insert("S", Value::Name(self.style.name().into()));
        if let Some(duration) = self.duration {
            dict = dict.insert("D", Value::Float(duration));
        }
        if let Some(dimension) = self.dimension {
            dict = dict.insert("Dm", Value::Name(dimension.name().into()));
        }
        if let Some(motion) = self.motion {
            dict = dict.insert("M", Value::Name(motion.name().into()));
        }
        if let Some(direction) = self.direction {
            let value = match direction {
                TransitionDirection::Degrees(degrees) => Value::Integer(i64::from(degrees)),
                TransitionDirection::None => Value::Name("None".into()),
            };
            dict = dict.insert("Di", value);
        }
        if let Some(scale) = self.scale {
            dict = dict.insert("SS", Value::Float(scale));
        }
        if let Some(border) = self.border {
            dict = dict.insert("B", Value::Bool(border));
        }
        Ok(dict)
    }
}
