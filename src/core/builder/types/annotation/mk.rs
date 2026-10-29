//! 外观特性字典 `/MK`（12.5.2，Table 189）与图标适配字典 `/IF`（Table 247）
//! 的构建态与校验。
//!
//! 规范把 `/MK` 声明给 Widget（Table 188）与 Screen（Table 187）注解；本模块
//! 只承担注解级 `/MK`，即挂在写为 Screen 的注解上，Widget 端的 `/MK` 归
//! [`FormField`](super::super::FormField) 的字段写路径（E4）。
//!
//! `/MK` 自 PDF 1.2 引入、`/IF /FB` 自 PDF 1.5 引入，而 Screen 子类型本身
//! 自 PDF 1.5 引入（Table 169），因此这些条目的版本门禁随子类型一并生效，
//! 无需新增 `PdfFeature`。`/I` `/RI` `/IX` 三个图标按 Table 189 要求写成
//! Form XObject 的间接引用，句柄取自 [`PdfBuilder::add_form`]
//! （[`PdfBuilder`]: super::super::PdfBuilder）。

use super::{AnnotationKind, PdfAnnotation};

/// 图标缩放时机（`/IF /SW`，Table 247）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconScaleWhen {
    /// 始终缩放（`/A`，缺省）。
    Always,
    /// 仅当图标大于注解矩形时缩放（`/B`）。
    Bigger,
    /// 仅当图标小于注解矩形时缩放（`/S`）。
    Smaller,
    /// 从不缩放（`/N`）。
    Never,
}

impl IconScaleWhen {
    /// `/SW` 名字。
    pub fn name(self) -> &'static str {
        match self {
            Self::Always => "A",
            Self::Bigger => "B",
            Self::Smaller => "S",
            Self::Never => "N",
        }
    }
}

/// 图标缩放类型（`/IF /S`，Table 247）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconScaling {
    /// 变形缩放，填满注解矩形、不顾宽高比（`/A`）。
    Anamorphic,
    /// 等比缩放，保持宽高比（`/P`，缺省）。
    Proportional,
}

impl IconScaling {
    /// `/S` 名字。
    pub fn name(self) -> &'static str {
        match self {
            Self::Anamorphic => "A",
            Self::Proportional => "P",
        }
    }
}

/// 图标适配字典（`/MK /IF`，Table 247）的构建态。
#[derive(Debug, Clone, Default)]
pub struct IconFit {
    scale_when: Option<IconScaleWhen>,
    scaling: Option<IconScaling>,
    leftover: Option<[f64; 2]>,
    fit_border: Option<bool>,
}

impl IconFit {
    /// 空的图标适配字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// 缩放时机 `/SW`（缺省 `Always`）。
    pub fn with_scale_when(mut self, scale_when: IconScaleWhen) -> Self {
        self.scale_when = Some(scale_when);
        self
    }

    /// 缩放类型 `/S`（缺省 `Proportional`）。
    pub fn with_scaling(mut self, scaling: IconScaling) -> Self {
        self.scaling = Some(scaling);
        self
    }

    /// 剩余空间比例 `/A`：左右与上下各 0.0–1.0（缺省 `[0.5 0.5]`）。
    pub fn with_leftover(mut self, left: f64, bottom: f64) -> Self {
        self.leftover = Some([left, bottom]);
        self
    }

    /// 边框内缩放标志 `/FB`（PDF 1.5，缺省 false）。
    pub fn with_fit_border(mut self, fit_border: bool) -> Self {
        self.fit_border = Some(fit_border);
        self
    }

    /// 缩放时机 `/SW`。
    pub(crate) fn scale_when(&self) -> Option<IconScaleWhen> {
        self.scale_when
    }

    /// 缩放类型 `/S`。
    pub(crate) fn scaling(&self) -> Option<IconScaling> {
        self.scaling
    }

    /// 剩余空间比例 `/A`。
    pub(crate) fn leftover(&self) -> Option<[f64; 2]> {
        self.leftover
    }

    /// 边框内缩放标志 `/FB`。
    pub(crate) fn fit_border(&self) -> Option<bool> {
        self.fit_border
    }

    /// 校验取值：`/A` 两个分量须为有限的 0.0–1.0。
    fn validate(&self) -> Result<(), String> {
        if let Some([left, bottom]) = self.leftover {
            for component in [left, bottom] {
                if !component.is_finite() || !(0.0..=1.0).contains(&component) {
                    return Err(
                        "icon fit /A must hold two finite numbers within 0.0–1.0 (Table 247)"
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }
}

/// 外观特性字典（`/MK`，Table 189）的构建态。
#[derive(Debug, Clone, Default)]
pub struct AppearanceCharacteristics {
    rotation: Option<i64>,
    border_color: Option<Vec<f64>>,
    background_color: Option<Vec<f64>>,
    caption: Option<String>,
    rollover_caption: Option<String>,
    alternate_caption: Option<String>,
    icon: Option<usize>,
    rollover_icon: Option<usize>,
    alternate_icon: Option<usize>,
    icon_fit: Option<IconFit>,
    text_position: Option<u8>,
}

impl AppearanceCharacteristics {
    /// 空的外观特性字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// 旋转角 `/R`：须为 90 的倍数（缺省 0）。
    pub fn with_rotation(mut self, degrees: i64) -> Self {
        self.rotation = Some(degrees);
        self
    }

    /// 边框颜色 `/BC`：0、1、3 或 4 个 0.0–1.0 分量（0 个表示无色）。
    pub fn with_border_color(mut self, components: Vec<f64>) -> Self {
        self.border_color = Some(components);
        self
    }

    /// 背景颜色 `/BG`：形态同 `/BC`。
    pub fn with_background_color(mut self, components: Vec<f64>) -> Self {
        self.background_color = Some(components);
        self
    }

    /// 常态题注 `/CA`（按钮字段）。
    pub fn with_caption(mut self, caption: impl Into<String>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    /// 悬停题注 `/RC`（仅 pushbutton 字段）。
    pub fn with_rollover_caption(mut self, caption: impl Into<String>) -> Self {
        self.rollover_caption = Some(caption.into());
        self
    }

    /// 按下（交替）题注 `/AC`（仅 pushbutton 字段）。
    pub fn with_alternate_caption(mut self, caption: impl Into<String>) -> Self {
        self.alternate_caption = Some(caption.into());
        self
    }

    /// 常态图标 `/I`：Form XObject 句柄。
    pub fn with_icon(mut self, form: usize) -> Self {
        self.icon = Some(form);
        self
    }

    /// 悬停图标 `/RI`：Form XObject 句柄。
    pub fn with_rollover_icon(mut self, form: usize) -> Self {
        self.rollover_icon = Some(form);
        self
    }

    /// 按下（交替）图标 `/IX`：Form XObject 句柄。
    pub fn with_alternate_icon(mut self, form: usize) -> Self {
        self.alternate_icon = Some(form);
        self
    }

    /// 图标适配字典 `/IF`（Table 247）。
    pub fn with_icon_fit(mut self, icon_fit: IconFit) -> Self {
        self.icon_fit = Some(icon_fit);
        self
    }

    /// 题注位置 `/TP`：0–6（缺省 0）。
    pub fn with_text_position(mut self, position: u8) -> Self {
        self.text_position = Some(position);
        self
    }

    /// 旋转角 `/R`。
    pub(crate) fn rotation(&self) -> Option<i64> {
        self.rotation
    }

    /// 边框颜色 `/BC`。
    pub(crate) fn border_color(&self) -> Option<&[f64]> {
        self.border_color.as_deref()
    }

    /// 背景颜色 `/BG`。
    pub(crate) fn background_color(&self) -> Option<&[f64]> {
        self.background_color.as_deref()
    }

    /// 常态题注 `/CA`。
    pub(crate) fn caption(&self) -> Option<&str> {
        self.caption.as_deref()
    }

    /// 悬停题注 `/RC`。
    pub(crate) fn rollover_caption(&self) -> Option<&str> {
        self.rollover_caption.as_deref()
    }

    /// 按下题注 `/AC`。
    pub(crate) fn alternate_caption(&self) -> Option<&str> {
        self.alternate_caption.as_deref()
    }

    /// 图标适配字典 `/IF`。
    pub(crate) fn icon_fit(&self) -> Option<&IconFit> {
        self.icon_fit.as_ref()
    }

    /// 题注位置 `/TP`。
    pub(crate) fn text_position(&self) -> Option<u8> {
        self.text_position
    }

    /// 三个图标（常态、悬停、按下）的 Form XObject 句柄。
    pub(crate) fn icon_handles(&self) -> [Option<usize>; 3] {
        [self.icon, self.rollover_icon, self.alternate_icon]
    }

    /// 按 Table 189 校验取值。
    fn validate(&self) -> Result<(), String> {
        if let Some(rotation) = self.rotation
            && rotation.rem_euclid(90) != 0
        {
            return Err(format!(
                "appearance characteristics /R must be a multiple of 90, got {rotation} \
                 (Table 189)"
            ));
        }
        for (key, color) in [("/BC", &self.border_color), ("/BG", &self.background_color)] {
            let Some(components) = color else {
                continue;
            };
            if !matches!(components.len(), 0 | 1 | 3 | 4) {
                return Err(format!(
                    "appearance characteristics {key} must have 0, 1, 3 or 4 components, \
                     got {}",
                    components.len()
                ));
            }
            if components.iter().any(|component| !component.is_finite()) {
                return Err(format!(
                    "appearance characteristics {key} components must be finite"
                ));
            }
            if components
                .iter()
                .any(|component| !(0.0..=1.0).contains(component))
            {
                return Err(format!(
                    "appearance characteristics {key} components must be within 0.0–1.0"
                ));
            }
        }
        if let Some(position) = self.text_position
            && position > 6
        {
            return Err(format!(
                "appearance characteristics /TP must be 0–6, got {position} (Table 189)"
            ));
        }
        if let Some(icon_fit) = &self.icon_fit {
            icon_fit.validate()?;
        }
        Ok(())
    }
}

impl PdfAnnotation {
    /// 外观特性字典 `/MK`（Table 189；仅 Screen 注解，Widget 端归 E4）。
    pub fn with_appearance_characteristics(
        mut self,
        characteristics: AppearanceCharacteristics,
    ) -> Self {
        self.characteristics = Some(characteristics);
        self
    }
}

impl PdfAnnotation {
    /// 已声明的外观特性字典（Table 189）。
    pub(crate) fn appearance_characteristics(&self) -> Option<&AppearanceCharacteristics> {
        self.characteristics.as_ref()
    }

    /// 外观特性字典的子类型归属与取值校验；由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_appearance_characteristics(&self) -> Result<(), String> {
        let Some(characteristics) = &self.characteristics else {
            return Ok(());
        };
        // Table 188/187 把 `/MK` 声明给 Widget 与 Screen；写端的注解级 `/MK`
        // 只落在 Screen 上（Widget 由 E4 的字段写路径承担）。
        self.require_kind(AnnotationKind::Screen, "Screen annotation /MK")?;
        characteristics.validate()
    }
}
