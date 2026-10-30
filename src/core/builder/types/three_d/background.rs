//! 3D 背景字典（Table 306）。
//!
//! 六个条目全部可选，缺省为 `SC` / `DeviceRGB` / `[1 1 1]` / `false`，
//! 均不物化。`/CS` 与 `/C` 按原值透传，以容忍规范预告的未来取值；
//! `/Subtype` 只接受 `SC` 或自定义名字。

use super::super::validate_name_value;
use crate::core::reader::three_d::ThreeDBackgroundType;
use crate::core::types::{Dict, Value};

/// 3D 背景字典（Table 306）的构建态。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreeDBackground {
    subtype: Option<ThreeDBackgroundType>,
    color_space: Option<Value>,
    color: Option<Value>,
    entire_annotation: Option<bool>,
}

impl ThreeDBackground {
    /// 新建全缺省背景字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// 背景类型 `/Subtype`；缺省 `SC` 不写出。
    pub fn with_subtype(mut self, subtype: ThreeDBackgroundType) -> Self {
        self.subtype = Some(subtype);
        self
    }

    /// 颜色空间 `/CS`（原值透传；标准值 `DeviceRGB`）。
    pub fn with_color_space(mut self, color_space: Value) -> Self {
        self.color_space = Some(color_space);
        self
    }

    /// 背景颜色 `/C`（原值透传；缺省 `[1 1 1]`）。
    pub fn with_color(mut self, color: Value) -> Self {
        self.color = Some(color);
        self
    }

    /// 是否覆盖整个注解 `/EA`（缺省 false）。
    pub fn with_entire_annotation(mut self, entire: bool) -> Self {
        self.entire_annotation = Some(entire);
        self
    }

    /// 校验 `/Subtype` 名字语法。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(ThreeDBackgroundType::Other(name)) = &self.subtype {
            validate_name_value(name, "3D background /Subtype")?;
        }
        Ok(())
    }

    /// 写成背景字典；显式条目才写出。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert("Type", Value::Name("3DBG".into()));
        if let Some(subtype) = &self.subtype {
            let name = match subtype {
                ThreeDBackgroundType::SolidColor => "SC",
                ThreeDBackgroundType::Other(name) => name.as_str(),
            };
            dict = dict.insert("Subtype", Value::Name(name.to_string()));
        }
        if let Some(color_space) = &self.color_space {
            dict = dict.insert("CS", color_space.clone());
        }
        if let Some(color) = &self.color {
            dict = dict.insert("C", color.clone());
        }
        if let Some(entire) = self.entire_annotation {
            dict = dict.insert("EA", Value::Bool(entire));
        }
        dict
    }
}
