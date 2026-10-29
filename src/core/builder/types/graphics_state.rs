//! 图形状态参数字典（`/ExtGState`）的逻辑配置。
//!
//! 对应 PDF 32000-1 §8.4.5 Table 58。写出端在页面资源字典中登记
//! `/ExtGState` 子字典，内容流用 `/GS<n> gs` 应用；[`GraphicsState`]
//! 覆盖不依赖外部对象的标量参数、`/D`、`/BM`、`/Font`、`/SMask`
//! 软蒙版字典与 `/TR`、`/BG`、`/BG2`、`/UCR`、`/UCR2` 函数条目、
//! `/TR2` 原始字典与 `/HT` 半色调包装字典（见 `types::halftone`）。
//!
//! `/SMask` 的 `/G` 引用由构建器按文档级 Form XObject 句柄解析为对象号，
//! 被引用的表单须是透明组（带 `/Group`），并在构建阶段连同表单对象写出。

use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

use super::Font;
use super::halftone::HalftoneDictionary;

/// 虚线样式（`/D`）：交替的虚线段与间隔长度、相位。
#[derive(Debug, Clone, PartialEq)]
pub struct DashPattern {
    /// 虚线段与间隔长度（非负；空数组表示实线）。
    pub array: Vec<f64>,
    /// 虚线相位（非负）。
    pub phase: f64,
}

impl DashPattern {
    /// 构造虚线样式。
    pub fn new(array: impl Into<Vec<f64>>, phase: f64) -> Self {
        Self {
            array: array.into(),
            phase,
        }
    }
}

/// 软蒙版子类型（`/S`，ISO 32000-1 §11.6.5.2 Table 144）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoftMaskSubtype {
    /// `/Alpha`：以蒙版组的透明度作为软蒙版值。
    Alpha,
    /// `/Luminosity`：以蒙版组的亮度作为软蒙版值。
    Luminosity,
}

impl SoftMaskSubtype {
    fn name(self) -> &'static str {
        match self {
            Self::Alpha => "Alpha",
            Self::Luminosity => "Luminosity",
        }
    }
}

/// `/SMask` 软蒙版字典（Table 144）。
///
/// `/G` 是透明组 Form XObject 的构建器句柄（[`PdfBuilder::form_xobject`]
/// 的返回值），构建阶段解析为对象引用；引用的表单必须带
/// [`GroupXObject`](super::GroupXObject)。`/TR` 传递函数以
/// [`PdfBuilder::function`] 句柄引用（组装阶段解析，须单输出）。
///
/// [`PdfBuilder::form_xobject`]: super::super::PdfBuilder::form_xobject
#[derive(Debug, Clone, PartialEq)]
pub struct SoftMask {
    /// `/S` 子类型。
    pub subtype: SoftMaskSubtype,
    /// `/G` 透明组 Form XObject 句柄。
    pub group: usize,
    /// `/BC` 背景色分量（每项 0–1）。
    pub backdrop: Option<Vec<f64>>,
    /// `/TR` 传递函数句柄（PDF 1.4 起弃用，省略即恒等）。
    pub transfer_function: Option<usize>,
    /// `/SMaskInData` 蒙版与图像数据的关系（0–2，PDF 1.4）。
    pub smask_in_data: Option<u8>,
    /// `/Matte` 衬底颜色分量（PDF 1.6，按组颜色空间原生值）。
    pub matte: Option<Vec<f64>>,
}

impl SoftMask {
    /// Alpha 软蒙版，引用指定透明组表单。
    pub fn alpha(group: usize) -> Self {
        Self {
            subtype: SoftMaskSubtype::Alpha,
            group,
            backdrop: None,
            transfer_function: None,
            smask_in_data: None,
            matte: None,
        }
    }

    /// Luminosity 软蒙版，引用指定透明组表单。
    pub fn luminosity(group: usize) -> Self {
        Self {
            subtype: SoftMaskSubtype::Luminosity,
            group,
            backdrop: None,
            transfer_function: None,
            smask_in_data: None,
            matte: None,
        }
    }

    /// 设置 `/BC` 背景色分量。
    pub fn with_backdrop(mut self, components: impl Into<Vec<f64>>) -> Self {
        self.backdrop = Some(components.into());
        self
    }

    /// 设置 `/TR` 传递函数（需先通过 [`PdfBuilder::function`] 注册）。
    ///
    /// [`PdfBuilder::function`]: super::super::PdfBuilder::function
    pub fn with_transfer_function(mut self, function: usize) -> Self {
        self.transfer_function = Some(function);
        self
    }

    /// 设置 `/SMaskInData`（0–2）。
    pub fn with_smask_in_data(mut self, value: u8) -> Self {
        self.smask_in_data = Some(value);
        self
    }

    /// 设置 `/Matte` 衬底颜色分量（PDF 1.6）。
    pub fn with_matte(mut self, components: impl Into<Vec<f64>>) -> Self {
        self.matte = Some(components.into());
        self
    }
}

/// `/Font` 条目：字体字典与文本状态字号。
///
/// PDF 要求字体写成**字体字典的间接引用**；构建器在
/// [`PdfBuilder::set_graphics_state`](super::super::PdfBuilder::set_graphics_state)
/// 时把字体登记到该页，构建阶段解析为对象号。
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsStateFont {
    /// 要引用的字体。
    pub font: Font,
    /// 文本状态字号（> 0）。
    pub size: f64,
}

impl GraphicsStateFont {
    /// 构造 `/Font` 条目。
    pub fn new(font: Font, size: f64) -> Self {
        Self { font, size }
    }
}

/// 图形状态参数（Table 58）。
///
/// 所有字段可选，仅写出的字段进入参数字典；可通过链式 `with_*`
/// 方法构造。取值在构建阶段校验，非法值返回
/// [`WriteError::InvalidObject`]。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraphicsState {
    /// `/LW` 线宽（≥ 0）。
    pub line_width: Option<f64>,
    /// `/LC` 线帽样式（0–2）。
    pub line_cap: Option<u8>,
    /// `/LJ` 线连接样式（0–2）。
    pub line_join: Option<u8>,
    /// `/ML` 斜接限制（≥ 1）。
    pub miter_limit: Option<f64>,
    /// `/D` 虚线样式。
    pub dash_pattern: Option<DashPattern>,
    /// `/RI` 渲染意图名。
    pub rendering_intent: Option<String>,
    /// `/OP` 描边是否叠印。
    pub overprint: Option<bool>,
    /// `/op` 非描边是否叠印。
    pub nonstroking_overprint: Option<bool>,
    /// `/OPM` 叠印模式（0 或 1）。
    pub overprint_mode: Option<u8>,
    /// `/Font` 字体与字号。
    pub font: Option<GraphicsStateFont>,
    /// `/FL` 平坦度容差（0–100）。
    pub flatness: Option<f64>,
    /// `/SM` 平滑度容差（0–1）。
    pub smoothness: Option<f64>,
    /// `/SA` 自动描边调整。
    pub stroke_adjustment: Option<bool>,
    /// `/BM` 混合模式；仅一个元素时写成名称，多个写成回退数组。
    pub blend_mode: Option<Vec<String>>,
    /// `/CA` 描边不透明度（0–1）。
    pub stroke_alpha: Option<f64>,
    /// `/ca` 填充不透明度（0–1）。
    pub fill_alpha: Option<f64>,
    /// `/AIS` 透明度是否按形状计算。
    pub alpha_is_shape: Option<bool>,
    /// `/TK` 文本挖空。
    pub text_knockout: Option<bool>,
    /// `/SMask` 软蒙版字典。
    pub soft_mask: Option<SoftMask>,
    /// `/TR` 传递函数句柄（[`PdfBuilder::function`] 返回值）。
    pub transfer_function: Option<usize>,
    /// `/BG` 黑版生成函数句柄（[`PdfBuilder::function`] 返回值）。
    pub black_generation: Option<usize>,
    /// `/BG2` 黑版生成函数句柄（PDF 1.3；优先于 `/BG`）。
    pub black_generation_2: Option<usize>,
    /// `/UCR` 底色去除函数句柄（[`PdfBuilder::function`] 返回值）。
    pub undercolor_removal: Option<usize>,
    /// `/UCR2` 底色去除函数句柄（PDF 1.3；优先于 `/UCR`）。
    pub undercolor_removal_2: Option<usize>,
    /// `/TR2` PostScript 传递函数 2 字典（PDF 1.3，弃用）：原样写出，
    /// 仅校验为字典，不解释其内部语义。
    pub transfer_function_2: Option<Value>,
    /// `/HT` 半色调包装字典（PDF 1.3）。
    pub halftone: Option<HalftoneDictionary>,
}

/// ExtGState 中函数类条目的已解析值（由组装阶段按句柄生成）。
#[derive(Debug, Default)]
pub(crate) struct GraphicsStateFunctions {
    pub transfer: Option<Value>,
    pub black_generation: Option<Value>,
    pub black_generation_2: Option<Value>,
    pub undercolor_removal: Option<Value>,
    pub undercolor_removal_2: Option<Value>,
    pub smask_transfer: Option<Value>,
    /// `/HT` 包装字典已解析出的值（函数句柄已替换为对象值）。
    pub halftone: Option<Value>,
}

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

    /// 函数类条目（名称与函数句柄），供组装阶段解析为字典值。
    pub(crate) fn function_handles(&self) -> impl Iterator<Item = (&'static str, usize)> + '_ {
        [
            ("TR", self.transfer_function),
            ("BG", self.black_generation),
            ("BG2", self.black_generation_2),
            ("UCR", self.undercolor_removal),
            ("UCR2", self.undercolor_removal_2),
        ]
        .into_iter()
        .filter_map(|(name, handle)| handle.map(|handle| (name, handle)))
    }

    /// 是否不依赖外部对象（`/Font`、`/SMask`、函数句柄或 `/HT`）。
    /// `/DR` 默认 ExtGState 只接受自包含状态。
    pub(crate) fn is_self_contained(&self) -> bool {
        self.font.is_none()
            && self.soft_mask.is_none()
            && self.halftone.is_none()
            && self.auxiliary_function_handles().is_empty()
    }

    /// 须为单输出的附加函数句柄：`/SMask /TR` 与 `/HT` 内函数。
    pub(crate) fn auxiliary_function_handles(&self) -> Vec<(&'static str, usize)> {
        let mut handles = Vec::new();
        if let Some(soft_mask) = &self.soft_mask
            && let Some(handle) = soft_mask.transfer_function
        {
            handles.push(("SMask /TR", handle));
        }
        if let Some(halftone) = &self.halftone {
            handles.extend(halftone.function_handles().map(|handle| ("HT", handle)));
        }
        handles
    }

    /// 校验并转换为 `/ExtGState` 参数字典。
    ///
    /// `font_num` 是 `/Font` 条目引用的字体字典对象号；有 `/Font`
    /// 时必须给出，无 `/Font` 时必须为 `None`。`soft_mask_group_num`
    /// 同理对应 `/SMask` 的 `/G` 表单对象号。`functions` 是各函数类
    /// 条目已解析的值（直接字典或流对象引用）。
    pub(crate) fn to_value(
        &self,
        font_num: Option<i64>,
        soft_mask_group_num: Option<i64>,
        functions: GraphicsStateFunctions,
    ) -> Result<Dict, WriteError> {
        if self.font.is_some() != font_num.is_some() {
            return Err(invalid("/Font entry and its object reference disagree"));
        }
        if self.soft_mask.is_some() != soft_mask_group_num.is_some() {
            return Err(invalid(
                "/SMask entry and its group object reference disagree",
            ));
        }
        if self.transfer_function.is_some() != functions.transfer.is_some() {
            return Err(invalid("/TR entry and its function value disagree"));
        }
        if self.black_generation.is_some() != functions.black_generation.is_some() {
            return Err(invalid("/BG entry and its function value disagree"));
        }
        if self.black_generation_2.is_some() != functions.black_generation_2.is_some() {
            return Err(invalid("/BG2 entry and its function value disagree"));
        }
        if self.undercolor_removal.is_some() != functions.undercolor_removal.is_some() {
            return Err(invalid("/UCR entry and its function value disagree"));
        }
        if self.undercolor_removal_2.is_some() != functions.undercolor_removal_2.is_some() {
            return Err(invalid("/UCR2 entry and its function value disagree"));
        }
        if self.soft_mask.is_none() && functions.smask_transfer.is_some() {
            return Err(invalid(
                "/SMask transfer function value without a soft mask",
            ));
        }
        if self
            .soft_mask
            .as_ref()
            .is_some_and(|soft_mask| soft_mask.transfer_function.is_some())
            != functions.smask_transfer.is_some()
        {
            return Err(invalid("/SMask /TR entry and its function value disagree"));
        }
        if self.halftone.is_some() != functions.halftone.is_some() {
            return Err(invalid("/HT entry and its resolved value disagree"));
        }
        let mut dict = Dict::new().insert("Type", Value::Name("ExtGState".into()));
        if let Some(width) = self.line_width {
            at_least("LW", width, 0.0)?;
            dict = dict.insert("LW", Value::Float(width));
        }
        if let Some(cap) = self.line_cap {
            code("LC", cap, 2)?;
            dict = dict.insert("LC", Value::Integer(i64::from(cap)));
        }
        if let Some(join) = self.line_join {
            code("LJ", join, 2)?;
            dict = dict.insert("LJ", Value::Integer(i64::from(join)));
        }
        if let Some(limit) = self.miter_limit {
            at_least("ML", limit, 1.0)?;
            dict = dict.insert("ML", Value::Float(limit));
        }
        if let Some(pattern) = &self.dash_pattern {
            for length in &pattern.array {
                at_least("D", *length, 0.0)?;
            }
            at_least("D", pattern.phase, 0.0)?;
            let dashes = pattern.array.iter().fold(Array::new(), |array, length| {
                array.push(Value::Float(*length))
            });
            dict = dict.insert(
                "D",
                Value::Array(
                    Array::new()
                        .push(Value::Array(dashes))
                        .push(Value::Float(pattern.phase)),
                ),
            );
        }
        if let Some(intent) = &self.rendering_intent {
            if !is_valid_pdf_name(intent) {
                return Err(invalid("/RI must be a valid PDF name"));
            }
            dict = dict.insert("RI", Value::Name(intent.clone()));
        }
        if let Some(overprint) = self.overprint {
            dict = dict.insert("OP", Value::Bool(overprint));
        }
        if let Some(overprint) = self.nonstroking_overprint {
            dict = dict.insert("op", Value::Bool(overprint));
        }
        if let Some(mode) = self.overprint_mode {
            code("OPM", mode, 1)?;
            dict = dict.insert("OPM", Value::Integer(i64::from(mode)));
        }
        if let Some(font) = &self.font {
            let num = font_num.ok_or_else(|| invalid("/Font reference is missing"))?;
            if !(font.size.is_finite() && font.size > 0.0) {
                return Err(invalid("/Font size must be positive"));
            }
            dict = dict.insert(
                "Font",
                Value::Array(
                    Array::new()
                        .push(Value::Ref(num, 0))
                        .push(Value::Float(font.size)),
                ),
            );
        }
        if let Some(flatness) = self.flatness {
            within("FL", flatness, 0.0, 100.0)?;
            dict = dict.insert("FL", Value::Float(flatness));
        }
        if let Some(smoothness) = self.smoothness {
            within("SM", smoothness, 0.0, 1.0)?;
            dict = dict.insert("SM", Value::Float(smoothness));
        }
        if let Some(adjust) = self.stroke_adjustment {
            dict = dict.insert("SA", Value::Bool(adjust));
        }
        if let Some(modes) = &self.blend_mode {
            if modes.is_empty() {
                return Err(invalid("/BM must contain at least one blend mode"));
            }
            for mode in modes {
                if !is_valid_pdf_name(mode) {
                    return Err(invalid("/BM entries must be valid PDF names"));
                }
            }
            let value = if modes.len() == 1 {
                Value::Name(modes[0].clone())
            } else {
                Value::Array(modes.iter().fold(Array::new(), |array, mode| {
                    array.push(Value::Name(mode.clone()))
                }))
            };
            dict = dict.insert("BM", value);
        }
        if let Some(alpha) = self.stroke_alpha {
            within("CA", alpha, 0.0, 1.0)?;
            dict = dict.insert("CA", Value::Float(alpha));
        }
        if let Some(alpha) = self.fill_alpha {
            within("ca", alpha, 0.0, 1.0)?;
            dict = dict.insert("ca", Value::Float(alpha));
        }
        if let Some(alpha_is_shape) = self.alpha_is_shape {
            dict = dict.insert("AIS", Value::Bool(alpha_is_shape));
        }
        if let Some(knockout) = self.text_knockout {
            dict = dict.insert("TK", Value::Bool(knockout));
        }
        if let Some(transfer) = functions.transfer {
            dict = dict.insert("TR", transfer);
        }
        if let Some(value) = functions.black_generation {
            dict = dict.insert("BG", value);
        }
        if let Some(value) = functions.black_generation_2 {
            dict = dict.insert("BG2", value);
        }
        if let Some(value) = functions.undercolor_removal {
            dict = dict.insert("UCR", value);
        }
        if let Some(value) = functions.undercolor_removal_2 {
            dict = dict.insert("UCR2", value);
        }
        if let Some(value) = &self.transfer_function_2 {
            if value.as_dict().is_none() {
                return Err(invalid("/TR2 must be a dictionary"));
            }
            dict = dict.insert("TR2", value.clone());
        }
        if let Some(value) = functions.halftone {
            dict = dict.insert("HT", value);
        }
        if let Some(soft_mask) = &self.soft_mask {
            let group_num =
                soft_mask_group_num.ok_or_else(|| invalid("/SMask /G reference is missing"))?;
            let mut mask_dict = Dict::new()
                .insert("S", Value::Name(soft_mask.subtype.name().into()))
                .insert("G", Value::Ref(group_num, 0));
            if let Some(components) = &soft_mask.backdrop {
                if components.is_empty() {
                    return Err(invalid("/SMask /BC must not be empty"));
                }
                for component in components {
                    within("SMask /BC", *component, 0.0, 1.0)?;
                }
                let array = components.iter().fold(Array::new(), |array, component| {
                    array.push(Value::Float(*component))
                });
                mask_dict = mask_dict.insert("BC", Value::Array(array));
            }
            if let Some(value) = functions.smask_transfer {
                mask_dict = mask_dict.insert("TR", value);
            }
            if let Some(in_data) = soft_mask.smask_in_data {
                if in_data > 2 {
                    return Err(invalid("/SMask /SMaskInData must be 0, 1 or 2"));
                }
                mask_dict = mask_dict.insert("SMaskInData", Value::Integer(i64::from(in_data)));
            }
            if let Some(components) = &soft_mask.matte {
                if components.is_empty() {
                    return Err(invalid("/SMask /Matte must not be empty"));
                }
                for component in components {
                    if !component.is_finite() {
                        return Err(invalid("/SMask /Matte components must be finite"));
                    }
                }
                let array = components.iter().fold(Array::new(), |array, component| {
                    array.push(Value::Float(*component))
                });
                mask_dict = mask_dict.insert("Matte", Value::Array(array));
            }
            dict = dict.insert("SMask", Value::Dict(mask_dict));
        }
        Ok(dict)
    }
}

fn invalid(message: impl Into<String>) -> WriteError {
    WriteError::InvalidObject(format!("invalid graphics state: {}", message.into()))
}

fn at_least(key: &str, value: f64, min: f64) -> Result<(), WriteError> {
    if !value.is_finite() {
        return Err(invalid(format!("/{key} must be finite")));
    }
    if value < min {
        return Err(invalid(format!("/{key} must be >= {min}")));
    }
    Ok(())
}

fn within(key: &str, value: f64, min: f64, max: f64) -> Result<(), WriteError> {
    if !value.is_finite() {
        return Err(invalid(format!("/{key} must be finite")));
    }
    if !(min..=max).contains(&value) {
        return Err(invalid(format!("/{key} must be within {min}..={max}")));
    }
    Ok(())
}

fn code(key: &str, value: u8, max: u8) -> Result<(), WriteError> {
    if value > max {
        return Err(invalid(format!("/{key} must be within 0..={max}")));
    }
    Ok(())
}

fn is_valid_pdf_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            (0x21..=0x7E).contains(&byte)
                && !matches!(
                    byte,
                    b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' | b'#'
                )
        })
}
