//! 过渡字典读取模型（12.4.4 Table 162）。
//!
//! Transition 动作的 `/Trans`（12.6.4.14 Table 215）与页面对象的 `/Trans`
//! 条目共用本模型；缺省值按 Table 162 不物化为 `None`，唯一样式缺省
//! `R` 直接落到 [`Transition::style`]。

use std::fmt;

use crate::core::reader::navigation::resolve;
use crate::core::types::{Document, Value};

/// 过渡样式（Table 162 `/S`）；缺省 `R`，未知名字保留。
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// 无特效，直接替换（缺省）。
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
    /// 非标准样式名。
    Other(String),
}

impl TransitionStyle {
    pub(crate) fn from_name(name: &str) -> Self {
        match name {
            "Split" => Self::Split,
            "Blinds" => Self::Blinds,
            "Box" => Self::Box,
            "Wipe" => Self::Wipe,
            "Dissolve" => Self::Dissolve,
            "Glitter" => Self::Glitter,
            "R" => Self::R,
            "Fly" => Self::Fly,
            "Push" => Self::Push,
            "Cover" => Self::Cover,
            "Uncover" => Self::Uncover,
            "Fade" => Self::Fade,
            other => Self::Other(other.to_owned()),
        }
    }

    /// `/Dm` 仅 Split/Blinds 适用；未知样式不判不适用。
    fn supports_dimension(&self) -> bool {
        matches!(self, Self::Split | Self::Blinds | Self::Other(_))
    }

    /// `/M` 仅 Split/Box/Fly 适用；未知样式不判不适用。
    fn supports_motion(&self) -> bool {
        matches!(self, Self::Split | Self::Box | Self::Fly | Self::Other(_))
    }

    /// `/Di` 仅 Wipe/Glitter/Fly/Cover/Uncover/Push 适用；未知样式不判不适用。
    fn supports_direction(&self) -> bool {
        matches!(
            self,
            Self::Wipe
                | Self::Glitter
                | Self::Fly
                | Self::Cover
                | Self::Uncover
                | Self::Push
                | Self::Other(_)
        )
    }
}

/// 过渡作用维度（`/Dm`，仅 Split 与 Blinds）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionDimension {
    /// 水平。
    Horizontal,
    /// 垂直。
    Vertical,
}

/// 过渡运动方向（`/M`，仅 Split、Box 与 Fly）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionMotion {
    /// 由页面边缘向内。
    Inward,
    /// 由中心向外。
    Outward,
}

/// 过渡方向（`/Di`）：角度值或名字 `None`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionDirection {
    /// 逆时针角度：0、90、180、270 或 315（315 仅 Glitter）。
    Degrees(u16),
    /// 名字 `None`（仅 Fly 且起始缩放不为 1.0 时有意义）。
    None,
}

/// 过渡字典（Table 162）。
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// `/S`：过渡样式，缺省 `R`。
    pub style: TransitionStyle,
    /// `/D`：过渡时长（秒），缺省 1。
    pub duration: Option<f64>,
    /// `/Dm`：作用维度，缺省 `H`。
    pub dimension: Option<TransitionDimension>,
    /// `/M`：运动方向，缺省 `I`。
    pub motion: Option<TransitionMotion>,
    /// `/Di`：方向，缺省 0。
    pub direction: Option<TransitionDirection>,
    /// `/SS`（PDF 1.5）：Fly 的起止缩放，缺省 1.0。
    pub scale: Option<f64>,
    /// `/B`（PDF 1.5）：Fly 的飞入区域是否不透明，缺省 `false`。
    pub border: Option<bool>,
}

/// 过渡字典条目与样式的适用性诊断（Table 162）。
///
/// 读端按条目取值解析，不因适用性不符拒绝整份文档；这些诊断供调用方
/// 判断文件是否规范。规则与写端 `validate` 同源，未知样式（
/// [`TransitionStyle::Other`]）不判不适用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionApplicabilityIssue {
    /// `/Dm` 仅 Split/Blinds 适用。
    Dimension,
    /// `/M` 仅 Split/Box/Fly 适用。
    Motion,
    /// `/Di` 仅 Wipe/Glitter/Fly/Cover/Uncover/Push 适用。
    Direction,
    /// `/Di` 90/180 仅 Wipe 适用。
    Direction90Or180,
    /// `/Di` 315 仅 Glitter 适用。
    Direction315,
    /// `/Di None` 仅 Fly 适用。
    DirectionNone,
    /// `/SS` 仅 Fly 适用。
    Scale,
    /// `/B` 仅 Fly 适用。
    Border,
}

impl Transition {
    /// 按 Table 162 列出所有「条目存在但样式不适用」的诊断（顺序稳定）。
    pub fn applicability_issues(&self) -> Vec<TransitionApplicabilityIssue> {
        use TransitionApplicabilityIssue as Issue;
        let mut issues = Vec::new();
        if self.dimension.is_some() && !self.style.supports_dimension() {
            issues.push(Issue::Dimension);
        }
        if self.motion.is_some() && !self.style.supports_motion() {
            issues.push(Issue::Motion);
        }
        if self.direction.is_some() && !self.style.supports_direction() {
            issues.push(Issue::Direction);
        }
        if let Some(direction) = self.direction {
            match direction {
                TransitionDirection::Degrees(90 | 180) if self.style != TransitionStyle::Wipe => {
                    issues.push(Issue::Direction90Or180);
                }
                TransitionDirection::Degrees(315) if self.style != TransitionStyle::Glitter => {
                    issues.push(Issue::Direction315);
                }
                TransitionDirection::None if self.style != TransitionStyle::Fly => {
                    issues.push(Issue::DirectionNone);
                }
                _ => {}
            }
        }
        if self.scale.is_some() && self.style != TransitionStyle::Fly {
            issues.push(Issue::Scale);
        }
        if self.border.is_some() && self.style != TransitionStyle::Fly {
            issues.push(Issue::Border);
        }
        issues
    }
}

/// 过渡字典解析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// 字典结构或条目取值非法。
    Malformed(String),
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for TransitionError {}

/// 解析过渡字典（Table 162）；间接引用先解引用。
pub fn parse_transition(document: &Document, value: &Value) -> Result<Transition, TransitionError> {
    let value = resolve_value(document, value)?;
    let dict = value
        .as_dict()
        .ok_or_else(|| malformed("transition is not a dictionary"))?;
    if let Some(kind) = dict.get("Type") {
        let kind = resolve_value(document, kind)?;
        if kind.as_name() != Some("Trans") {
            return Err(malformed("transition /Type is not Trans"));
        }
    }
    let style = match dict.get("S") {
        None => TransitionStyle::R,
        Some(value) => match resolve_value(document, value)?.as_name() {
            Some(name) => TransitionStyle::from_name(name),
            None => return Err(malformed("transition /S is not a name")),
        },
    };
    let duration = optional_number(document, dict.get("D"), "D")?;
    if duration.is_some_and(|seconds| seconds < 0.0) {
        return Err(malformed("transition /D is negative"));
    }
    let dimension = match dict.get("Dm") {
        None => None,
        Some(value) => match resolve_value(document, value)?.as_name() {
            Some("H") => Some(TransitionDimension::Horizontal),
            Some("V") => Some(TransitionDimension::Vertical),
            _ => return Err(malformed("transition /Dm is not H or V")),
        },
    };
    let motion = match dict.get("M") {
        None => None,
        Some(value) => match resolve_value(document, value)?.as_name() {
            Some("I") => Some(TransitionMotion::Inward),
            Some("O") => Some(TransitionMotion::Outward),
            _ => return Err(malformed("transition /M is not I or O")),
        },
    };
    let direction = match dict.get("Di") {
        None => None,
        Some(value) => Some(parse_direction(document, &resolve_value(document, value)?)?),
    };
    let scale = optional_number(document, dict.get("SS"), "SS")?;
    if scale.is_some_and(|scale| scale <= 0.0) {
        return Err(malformed("transition /SS is not positive"));
    }
    let border = match dict.get("B") {
        None => None,
        Some(value) => match resolve_value(document, value)? {
            Value::Bool(flag) => Some(flag),
            _ => return Err(malformed("transition /B is not a boolean")),
        },
    };
    Ok(Transition {
        style,
        duration,
        dimension,
        motion,
        direction,
        scale,
        border,
    })
}

fn resolve_value(document: &Document, value: &Value) -> Result<Value, TransitionError> {
    resolve(document, value).map_err(|error| TransitionError::Malformed(error.to_string()))
}

fn optional_number(
    document: &Document,
    value: Option<&Value>,
    key: &str,
) -> Result<Option<f64>, TransitionError> {
    let Some(value) = value else {
        return Ok(None);
    };
    match resolve_value(document, value)? {
        Value::Integer(number) => Ok(Some(number as f64)),
        Value::Float(number) if number.is_finite() => Ok(Some(number)),
        _ => Err(malformed(format!(
            "transition /{key} is not a finite number"
        ))),
    }
}

fn parse_direction(
    document: &Document,
    value: &Value,
) -> Result<TransitionDirection, TransitionError> {
    match resolve_value(document, value)? {
        Value::Name(name) if name == "None" => Ok(TransitionDirection::None),
        Value::Name(_) => Err(malformed("transition /Di name is not None")),
        other => {
            let degrees = match other {
                Value::Integer(number) => number as f64,
                Value::Float(number) if number.is_finite() => number,
                _ => return Err(malformed("transition /Di is not a number or name")),
            };
            if degrees.fract() != 0.0 {
                return Err(malformed(
                    "transition /Di is not one of 0, 90, 180, 270, 315 or None",
                ));
            }
            match degrees as i64 {
                0 => Ok(TransitionDirection::Degrees(0)),
                90 => Ok(TransitionDirection::Degrees(90)),
                180 => Ok(TransitionDirection::Degrees(180)),
                270 => Ok(TransitionDirection::Degrees(270)),
                315 => Ok(TransitionDirection::Degrees(315)),
                _ => Err(malformed(
                    "transition /Di is not one of 0, 90, 180, 270, 315 or None",
                )),
            }
        }
    }
}

fn malformed(message: impl Into<String>) -> TransitionError {
    TransitionError::Malformed(message.into())
}
