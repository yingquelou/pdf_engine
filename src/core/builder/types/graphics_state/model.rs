use crate::core::types::Value;

use super::super::Font;
use super::super::halftone::HalftoneDictionary;

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
    pub(super) fn name(self) -> &'static str {
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
    /// `/CS` 混合色彩空间参数（PDF 1.4；见 §11.6.3 与 PDF/VT-2 扩展）。
    ///
    /// 存储原样写出形态：一个 `Value::Name`（色彩空间名）或一个
    /// `Value::Array` 两元素数组（成员为 `Name` 或有限数）。setter 只
    /// 生成合法形态，`to_value` 再对形态与 PDF 名称合法性做二次校验。
    pub hybrid_color_space: Option<Value>,
    /// `/CSS` 混合色彩空间风格：只接受 `"Knockout"` 或 `"Native"`。
    ///
    /// 只在 [`Self::hybrid_color_space`] 出现时有效；非法组合与非法字符串
    /// 在装配阶段以 [`WriteError::InvalidObject`] 拒绝。
    ///
    /// [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject
    pub hybrid_color_space_style: Option<String>,
}
