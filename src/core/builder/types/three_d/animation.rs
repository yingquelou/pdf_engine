//! 3D 动画样式字典（Table 301）的构建态。
//!
//! `/Type` 恒为 `3DAnimationStyle`；`/Subtype` 由
//! [`ThreeDAnimationStyle::new`] 显式声明，`/PC` `/TM` 仅在调用
//! `with_*` 后写出。缺省值（`None` / 0 / 1）不物化。`/TM` 必须为有限
//! 正数，`Other` 样式名须为合法 PDF 名字，否则写出前拒绝。

use super::super::validate_name_value;
pub use crate::core::reader::three_d::ThreeDAnimationStyleType;
use crate::core::types::{Dict, Value};

/// 3D 动画样式字典（Table 301）的构建态。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDAnimationStyle {
    subtype: Option<ThreeDAnimationStyleType>,
    play_count: Option<i64>,
    time_multiplier: Option<f64>,
}

impl ThreeDAnimationStyle {
    /// 新建动画样式字典；`style` 为 `/Subtype`（Table 302）。
    pub fn new(style: ThreeDAnimationStyleType) -> Self {
        Self {
            subtype: Some(style),
            play_count: None,
            time_multiplier: None,
        }
    }

    /// 播放次数 `/PC`：非负为播放次数，负数为无限重复；缺省 0 不写出。
    pub fn with_play_count(mut self, count: i64) -> Self {
        self.play_count = Some(count);
        self
    }

    /// 时间倍率 `/TM`（必须为正数，>1 加速）；缺省 1 不写出。
    pub fn with_time_multiplier(mut self, multiplier: f64) -> Self {
        self.time_multiplier = Some(multiplier);
        self
    }

    /// 校验 `/Subtype` 名字语法与 `/TM` 取值范围。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(ThreeDAnimationStyleType::Other(name)) = &self.subtype {
            validate_name_value(name, "3D animation /Subtype")?;
        }
        if let Some(multiplier) = self.time_multiplier
            && (!multiplier.is_finite() || multiplier <= 0.0)
        {
            return Err("3D animation /TM must be a positive finite number".into());
        }
        Ok(())
    }

    /// 写成动画样式字典；显式条目才写出。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert("Type", Value::Name("3DAnimationStyle".into()));
        if let Some(subtype) = &self.subtype {
            let name = match subtype {
                ThreeDAnimationStyleType::None => "None",
                ThreeDAnimationStyleType::Linear => "Linear",
                ThreeDAnimationStyleType::Oscillating => "Oscillating",
                ThreeDAnimationStyleType::Other(name) => name.as_str(),
            };
            dict = dict.insert("Subtype", Value::Name(name.to_string()));
        }
        if let Some(count) = self.play_count {
            dict = dict.insert("PC", Value::Integer(count));
        }
        if let Some(multiplier) = self.time_multiplier {
            dict = dict.insert("TM", Value::Float(multiplier));
        }
        dict
    }
}
