use crate::core::types::Value;

use super::super::Font;
use super::super::halftone::HalftoneDictionary;
use super::model::{DashPattern, GraphicsState, GraphicsStateFont, SoftMask};

impl GraphicsState {
    /// 创建空参数字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 `/LW` 线宽。
    pub fn with_line_width(mut self, width: f64) -> Self {
        self.line_width = Some(width);
        self
    }

    /// 设置 `/LC` 线帽样式。
    pub fn with_line_cap(mut self, cap: u8) -> Self {
        self.line_cap = Some(cap);
        self
    }

    /// 设置 `/LJ` 线连接样式。
    pub fn with_line_join(mut self, join: u8) -> Self {
        self.line_join = Some(join);
        self
    }

    /// 设置 `/ML` 斜接限制。
    pub fn with_miter_limit(mut self, limit: f64) -> Self {
        self.miter_limit = Some(limit);
        self
    }

    /// 设置 `/D` 虚线样式。
    pub fn with_dash_pattern(mut self, pattern: DashPattern) -> Self {
        self.dash_pattern = Some(pattern);
        self
    }

    /// 设置 `/RI` 渲染意图。
    pub fn with_rendering_intent(mut self, intent: impl Into<String>) -> Self {
        self.rendering_intent = Some(intent.into());
        self
    }

    /// 设置 `/OP` 描边叠印。
    pub fn with_overprint(mut self, overprint: bool) -> Self {
        self.overprint = Some(overprint);
        self
    }

    /// 设置 `/op` 非描边叠印。
    pub fn with_nonstroking_overprint(mut self, overprint: bool) -> Self {
        self.nonstroking_overprint = Some(overprint);
        self
    }

    /// 设置 `/OPM` 叠印模式。
    pub fn with_overprint_mode(mut self, mode: u8) -> Self {
        self.overprint_mode = Some(mode);
        self
    }

    /// 设置 `/Font` 字体与字号。
    pub fn with_font(mut self, font: Font, size: f64) -> Self {
        self.font = Some(GraphicsStateFont { font, size });
        self
    }

    /// 设置 `/FL` 平坦度容差。
    pub fn with_flatness(mut self, flatness: f64) -> Self {
        self.flatness = Some(flatness);
        self
    }

    /// 设置 `/SM` 平滑度容差。
    pub fn with_smoothness(mut self, smoothness: f64) -> Self {
        self.smoothness = Some(smoothness);
        self
    }

    /// 设置 `/SA` 自动描边调整。
    pub fn with_stroke_adjustment(mut self, adjust: bool) -> Self {
        self.stroke_adjustment = Some(adjust);
        self
    }

    /// 设置单个 `/BM` 混合模式。
    pub fn with_blend_mode(mut self, mode: impl Into<String>) -> Self {
        self.blend_mode = Some(vec![mode.into()]);
        self
    }

    /// 设置 `/BM` 混合模式回退列表。
    pub fn with_blend_modes(mut self, modes: &[&str]) -> Self {
        self.blend_mode = Some(modes.iter().map(|mode| (*mode).to_string()).collect());
        self
    }

    /// 设置 `/CA` 描边不透明度。
    pub fn with_stroke_alpha(mut self, alpha: f64) -> Self {
        self.stroke_alpha = Some(alpha);
        self
    }

    /// 设置 `/ca` 填充不透明度。
    pub fn with_fill_alpha(mut self, alpha: f64) -> Self {
        self.fill_alpha = Some(alpha);
        self
    }

    /// 设置 `/AIS` 透明度按形状计算。
    pub fn with_alpha_is_shape(mut self, alpha_is_shape: bool) -> Self {
        self.alpha_is_shape = Some(alpha_is_shape);
        self
    }

    /// 设置 `/TK` 文本挖空。
    pub fn with_text_knockout(mut self, knockout: bool) -> Self {
        self.text_knockout = Some(knockout);
        self
    }

    /// 设置 `/SMask` 软蒙版字典。
    pub fn with_soft_mask(mut self, soft_mask: SoftMask) -> Self {
        self.soft_mask = Some(soft_mask);
        self
    }

    /// 设置 `/TR` 传递函数（需先通过 [`PdfBuilder::function`] 注册）。
    pub fn with_transfer_function(mut self, function: usize) -> Self {
        self.transfer_function = Some(function);
        self
    }

    /// 设置 `/BG` 黑版生成函数（需先通过 [`PdfBuilder::function`] 注册）。
    pub fn with_black_generation(mut self, function: usize) -> Self {
        self.black_generation = Some(function);
        self
    }

    /// 设置 `/BG2` 黑版生成函数（PDF 1.3）。
    pub fn with_black_generation_2(mut self, function: usize) -> Self {
        self.black_generation_2 = Some(function);
        self
    }

    /// 设置 `/UCR` 底色去除函数。
    pub fn with_undercolor_removal(mut self, function: usize) -> Self {
        self.undercolor_removal = Some(function);
        self
    }

    /// 设置 `/UCR2` 底色去除函数（PDF 1.3）。
    pub fn with_undercolor_removal_2(mut self, function: usize) -> Self {
        self.undercolor_removal_2 = Some(function);
        self
    }

    /// 设置 `/TR2` PostScript 传递函数 2 字典（PDF 1.3，弃用）。
    ///
    /// 值原样写出；构建阶段仅校验其为字典，不解释内部条目。
    pub fn with_transfer_function_2(mut self, dict: Value) -> Self {
        self.transfer_function_2 = Some(dict);
        self
    }

    /// 设置 `/HT` 半色调包装字典（PDF 1.3）。
    pub fn with_halftone(mut self, halftone: HalftoneDictionary) -> Self {
        self.halftone = Some(halftone);
        self
    }
}
