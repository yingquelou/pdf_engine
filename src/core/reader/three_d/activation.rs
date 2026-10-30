//! 3D 激活字典（Table 299）。
//!
//! `/A`、`/AIS`、`/D`、`/DIS` 四组名字条目与 `/TB` `/NP` 两个布尔条目。
//! 缺省值（`XA`/`L`/`PI`/`U`/`true`/`false`）由呈现端应用，读端不物化；
//! 未知名字落成 `Other` 保留原文，条目类型错误（非名字/非布尔）报错。

use crate::core::types::{Dict, Document};

use super::{ThreeDError, optional_bool, optional_name};

/// 激活时机（`/A`，缺省 `XA`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDActivationPolicy {
    /// `PO`：页面打开即激活。
    PageOpen,
    /// `PV`：页面任一部分可见即激活。
    PageVisible,
    /// `XA`：等待脚本或用户显式激活（缺省）。
    Explicit,
    /// 未识别的名字（原样保留）。
    Other(String),
}

impl ThreeDActivationPolicy {
    fn from_name(name: &str) -> Self {
        match name {
            "PO" => Self::PageOpen,
            "PV" => Self::PageVisible,
            "XA" => Self::Explicit,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 标准名字；[`Self::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::PageOpen => "PO",
            Self::PageVisible => "PV",
            Self::Explicit => "XA",
            Self::Other(other) => other,
        }
    }
}

/// 3D artwork 状态（`/AIS` `/DIS`）：未实例化、已实例化或实时。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDArtworkState {
    /// `U`：未实例化（仅 `/DIS` 缺省）。
    Uninstantiated,
    /// `I`：已实例化，禁用脚本驱动的实时动画。
    Instantiated,
    /// `L`：已实例化并允许实时动画（`/AIS` 缺省）。
    Live,
    /// 未识别的名字（原样保留）。
    Other(String),
}

impl ThreeDArtworkState {
    fn from_name(name: &str) -> Self {
        match name {
            "U" => Self::Uninstantiated,
            "I" => Self::Instantiated,
            "L" => Self::Live,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 标准名字；[`Self::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Uninstantiated => "U",
            Self::Instantiated => "I",
            Self::Live => "L",
            Self::Other(other) => other,
        }
    }
}

/// 去激活时机（`/D`，缺省 `PI`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDDeactivationPolicy {
    /// `PC`：页面关闭即去激活。
    PageClose,
    /// `PI`：页面不可见即去激活（缺省）。
    PageInvisible,
    /// `XD`：等待脚本或用户显式去激活。
    Explicit,
    /// 未识别的名字（原样保留）。
    Other(String),
}

impl ThreeDDeactivationPolicy {
    fn from_name(name: &str) -> Self {
        match name {
            "PC" => Self::PageClose,
            "PI" => Self::PageInvisible,
            "XD" => Self::Explicit,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 标准名字；[`Self::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::PageClose => "PC",
            Self::PageInvisible => "PI",
            Self::Explicit => "XD",
            Self::Other(other) => other,
        }
    }
}

/// 3D 激活字典（Table 299）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreeDActivation {
    /// `/A` 激活时机；`None` 表示缺省 `XA`。
    pub activation: Option<ThreeDActivationPolicy>,
    /// `/AIS` 激活时的 artwork 状态；`None` 表示缺省 `L`。
    pub activation_state: Option<ThreeDArtworkState>,
    /// `/D` 去激活时机；`None` 表示缺省 `PI`。
    pub deactivation: Option<ThreeDDeactivationPolicy>,
    /// `/DIS` 去激活时的 artwork 状态；`None` 表示缺省 `U`。
    pub deactivation_state: Option<ThreeDArtworkState>,
    /// `/TB` 激活并取得焦点时是否默认显示工具栏（PDF 1.7，缺省 true）。
    pub toolbar: Option<bool>,
    /// `/NP` 激活时是否默认显示视图/模型树界面（PDF 1.7，缺省 false）。
    pub navigation_panel: Option<bool>,
}

/// 解析已解引用的激活字典；各条目缺失时保持 `None`。
pub(super) fn parse_activation(
    document: &Document,
    dict: &Dict,
) -> Result<ThreeDActivation, ThreeDError> {
    let activation = optional_name(document, dict, "A", "3D activation")?
        .map(|name| ThreeDActivationPolicy::from_name(&name));
    let activation_state = optional_name(document, dict, "AIS", "3D activation")?
        .map(|name| ThreeDArtworkState::from_name(&name));
    let deactivation = optional_name(document, dict, "D", "3D activation")?
        .map(|name| ThreeDDeactivationPolicy::from_name(&name));
    let deactivation_state = optional_name(document, dict, "DIS", "3D activation")?
        .map(|name| ThreeDArtworkState::from_name(&name));
    let toolbar = optional_bool(document, dict, "TB", "3D activation")?;
    let navigation_panel = optional_bool(document, dict, "NP", "3D activation")?;
    Ok(ThreeDActivation {
        activation,
        activation_state,
        deactivation,
        deactivation_state,
        toolbar,
        navigation_panel,
    })
}
