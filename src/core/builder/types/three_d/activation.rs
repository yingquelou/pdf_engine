//! 3D 激活字典（Table 299）。
//!
//! 六个条目全部可选；缺省 `XA` / `L` / `PI` / `U` / `true` / `false`
//! 由呈现端应用，写端不物化。未知名字复用读端 `Other` 变体原样写出，
//! 但须通过 PDF 名字语法校验。

use super::super::validate_name_value;
use crate::core::reader::three_d::{
    ThreeDActivationPolicy, ThreeDArtworkState, ThreeDDeactivationPolicy,
};
use crate::core::types::{Dict, Value};

/// 3D 激活字典（Table 299）的构建态。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreeDActivation {
    activation: Option<ThreeDActivationPolicy>,
    activation_state: Option<ThreeDArtworkState>,
    deactivation: Option<ThreeDDeactivationPolicy>,
    deactivation_state: Option<ThreeDArtworkState>,
    toolbar: Option<bool>,
    navigation_panel: Option<bool>,
}

impl ThreeDActivation {
    /// 新建全缺省激活字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// 激活时机 `/A`（缺省 `XA`）。
    pub fn with_activation(mut self, policy: ThreeDActivationPolicy) -> Self {
        self.activation = Some(policy);
        self
    }

    /// 激活时的 artwork 状态 `/AIS`（缺省 `L`）。
    pub fn with_activation_state(mut self, state: ThreeDArtworkState) -> Self {
        self.activation_state = Some(state);
        self
    }

    /// 去激活时机 `/D`（缺省 `PI`）。
    pub fn with_deactivation(mut self, policy: ThreeDDeactivationPolicy) -> Self {
        self.deactivation = Some(policy);
        self
    }

    /// 去激活时的 artwork 状态 `/DIS`（缺省 `U`）。
    pub fn with_deactivation_state(mut self, state: ThreeDArtworkState) -> Self {
        self.deactivation_state = Some(state);
        self
    }

    /// 激活后是否默认显示工具栏 `/TB`（PDF 1.7，缺省 true）。
    pub fn with_toolbar(mut self, toolbar: bool) -> Self {
        self.toolbar = Some(toolbar);
        self
    }

    /// 激活后是否默认显示导航面板 `/NP`（PDF 1.7，缺省 false）。
    pub fn with_navigation_panel(mut self, visible: bool) -> Self {
        self.navigation_panel = Some(visible);
        self
    }

    /// 校验名字条目语法。
    pub(crate) fn validate(&self) -> Result<(), String> {
        for (value, context) in [
            (
                self.activation.as_ref().map(ThreeDActivationPolicy::name),
                "3D activation /A",
            ),
            (
                self.activation_state.as_ref().map(ThreeDArtworkState::name),
                "3D activation /AIS",
            ),
            (
                self.deactivation
                    .as_ref()
                    .map(ThreeDDeactivationPolicy::name),
                "3D activation /D",
            ),
            (
                self.deactivation_state
                    .as_ref()
                    .map(ThreeDArtworkState::name),
                "3D activation /DIS",
            ),
        ] {
            if let Some(name) = value {
                validate_name_value(name, context)?;
            }
        }
        Ok(())
    }

    /// 写成激活字典；缺省条目不物化。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new();
        if let Some(policy) = &self.activation {
            dict = dict.insert("A", Value::Name(policy.name().into()));
        }
        if let Some(state) = &self.activation_state {
            dict = dict.insert("AIS", Value::Name(state.name().into()));
        }
        if let Some(policy) = &self.deactivation {
            dict = dict.insert("D", Value::Name(policy.name().into()));
        }
        if let Some(state) = &self.deactivation_state {
            dict = dict.insert("DIS", Value::Name(state.name().into()));
        }
        if let Some(toolbar) = self.toolbar {
            dict = dict.insert("TB", Value::Bool(toolbar));
        }
        if let Some(panel) = self.navigation_panel {
            dict = dict.insert("NP", Value::Bool(panel));
        }
        dict
    }
}
