//! 注解 measure 字典（12.9，Table 261–263）的构建态、构造方法与校验。
//!
//! `/Measure` 自 PDF 1.7 起可用于 Line（Table 175）与 Polygon/PolyLine
//! （Table 178）。[`Measure`] 只建模直角坐标测量（`/Subtype /RL`，Table 262）：
//! `/R` `/X` `/D` `/A` 必填，`/Y` `/T` `/S` `/O` `/CYX` 可选；
//! 各换算数组的元素是 [`NumberFormat`]（Table 263）。

use super::{AnnotationKind, PdfAnnotation};
use crate::core::builder::types::text_string_value;
use crate::core::types::{Array, Dict, Value};

/// 单个数字格式数组的元素上限，与读端一致。
const MAX_NUMBER_FORMATS: usize = 64;

/// 数字格式的分数显示方式（`/F`，Table 263）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberFraction {
    /// 按 `/D` 精度显示小数（`/D`，缺省）。
    Decimal,
    /// 按 `/D` 分母显示分数（`/F`）。
    Fraction,
    /// 无小数部分，四舍五入到整数（`/R`）。
    Round,
    /// 无小数部分，截断到整数（`/T`）。
    Truncate,
}

impl NumberFraction {
    /// 名字（`/F` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Decimal => "D",
            Self::Fraction => "F",
            Self::Round => "R",
            Self::Truncate => "T",
        }
    }
}

/// 单位标签相对数值的位置（`/O`，Table 263，缺省 `S`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberLabelOrder {
    /// 数值后置标签（`/S`，缺省）。
    Suffix,
    /// 数值前置标签（`/P`）。
    Prefix,
}

impl NumberLabelOrder {
    /// 名字（`/O` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Suffix => "S",
            Self::Prefix => "P",
        }
    }
}

/// 数字格式字典（Table 263）：一个计量单位及其文本格式化信息。
#[derive(Debug, Clone, PartialEq)]
pub struct NumberFormat {
    unit: String,
    conversion: f64,
    fraction: Option<NumberFraction>,
    precision: Option<u32>,
    force_denominator: Option<bool>,
    thousands_separator: Option<String>,
    decimal_separator: Option<String>,
    prefix_spacing: Option<String>,
    suffix_spacing: Option<String>,
    label_order: Option<NumberLabelOrder>,
}

impl NumberFormat {
    /// 新建数字格式；`unit` 为单位标签（`/U`），`conversion` 为换算因子（`/C`）。
    pub fn new(unit: impl Into<String>, conversion: f64) -> Self {
        Self {
            unit: unit.into(),
            conversion,
            fraction: None,
            precision: None,
            force_denominator: None,
            thousands_separator: None,
            decimal_separator: None,
            prefix_spacing: None,
            suffix_spacing: None,
            label_order: None,
        }
    }

    /// 分数显示方式 `/F`。
    pub fn with_fraction(mut self, fraction: NumberFraction) -> Self {
        self.fraction = Some(fraction);
        self
    }

    /// 精度或分母 `/D`（正整数；`/F` 为 `D` 时须为 10 的倍数）。
    pub fn with_precision(mut self, precision: u32) -> Self {
        self.precision = Some(precision);
        self
    }

    /// 是否禁止约分/截去低位零 `/FD`。
    pub fn with_force_denominator(mut self, force: bool) -> Self {
        self.force_denominator = Some(force);
        self
    }

    /// 千位分隔文本 `/RT`（空串表示不加分隔）。
    pub fn with_thousands_separator(mut self, text: impl Into<String>) -> Self {
        self.thousands_separator = Some(text.into());
        self
    }

    /// 小数点文本 `/RD`（空串表示用默认值）。
    pub fn with_decimal_separator(mut self, text: impl Into<String>) -> Self {
        self.decimal_separator = Some(text.into());
        self
    }

    /// 单位标签左侧拼接文本 `/PS`（空串表示不拼接）。
    pub fn with_prefix_spacing(mut self, text: impl Into<String>) -> Self {
        self.prefix_spacing = Some(text.into());
        self
    }

    /// 单位标签右侧拼接文本 `/SS`（空串表示不拼接）。
    pub fn with_suffix_spacing(mut self, text: impl Into<String>) -> Self {
        self.suffix_spacing = Some(text.into());
        self
    }

    /// 标签位置 `/O`（缺省 `Suffix`）。
    pub fn with_label_order(mut self, order: NumberLabelOrder) -> Self {
        self.label_order = Some(order);
        self
    }

    fn validate(&self) -> Result<(), String> {
        if self.unit.is_empty() {
            return Err("measure number format /U must not be empty".into());
        }
        if !self.conversion.is_finite() {
            return Err("measure number format /C must be finite".into());
        }
        if let Some(precision) = self.precision {
            if precision == 0 {
                return Err("measure number format /D must be a positive integer".into());
            }
            let fraction = self.fraction.unwrap_or(NumberFraction::Decimal);
            if fraction == NumberFraction::Decimal && !precision.is_multiple_of(10) {
                return Err(
                    "measure number format /D must be a multiple of 10 when /F is D".into(),
                );
            }
        }
        if self.force_denominator.is_some() && self.precision.is_none() {
            return Err("measure number format /FD requires /D".into());
        }
        Ok(())
    }

    fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("U", text_string_value(&self.unit))
            .insert("C", Value::Float(self.conversion));
        if let Some(fraction) = self.fraction {
            dict = dict.insert("F", Value::Name(fraction.name().into()));
        }
        if let Some(precision) = self.precision {
            dict = dict.insert("D", Value::Integer(i64::from(precision)));
        }
        if let Some(force) = self.force_denominator {
            dict = dict.insert("FD", Value::Bool(force));
        }
        for (key, text) in [
            ("RT", &self.thousands_separator),
            ("RD", &self.decimal_separator),
            ("PS", &self.prefix_spacing),
            ("SS", &self.suffix_spacing),
        ] {
            if let Some(text) = text {
                dict = dict.insert(key, text_string_value(text));
            }
        }
        if let Some(order) = self.label_order {
            dict = dict.insert("O", Value::Name(order.name().into()));
        }
        dict
    }
}

/// 注解 measure 字典（Table 261/262）；直角坐标测量 `Subtype` 固定为 `RL`。
#[derive(Debug, Clone, PartialEq)]
pub struct Measure {
    ratio: String,
    x: Vec<NumberFormat>,
    y: Option<Vec<NumberFormat>>,
    distance: Vec<NumberFormat>,
    area: Vec<NumberFormat>,
    angle: Option<Vec<NumberFormat>>,
    slope: Option<Vec<NumberFormat>>,
    origin: Option<[f64; 2]>,
    conversion_yx: Option<f64>,
}

impl Measure {
    /// 新建直角坐标 measure 字典；`ratio` 为 `/R` 比例串，
    /// `x`/`distance`/`area` 为必填换算数组（均不得为空）。
    pub fn new(
        ratio: impl Into<String>,
        x: Vec<NumberFormat>,
        distance: Vec<NumberFormat>,
        area: Vec<NumberFormat>,
    ) -> Self {
        Self {
            ratio: ratio.into(),
            x,
            y: None,
            distance,
            area,
            angle: None,
            slope: None,
            origin: None,
            conversion_yx: None,
        }
    }

    /// 沿 y 轴的换算数组 `/Y`（x/y 单位不同时提供）。
    pub fn with_y(mut self, y: Vec<NumberFormat>) -> Self {
        self.y = Some(y);
        self
    }

    /// 角度换算数组 `/T`。
    pub fn with_angle(mut self, angle: Vec<NumberFormat>) -> Self {
        self.angle = Some(angle);
        self
    }

    /// 斜率换算数组 `/S`。
    pub fn with_slope(mut self, slope: Vec<NumberFormat>) -> Self {
        self.slope = Some(slope);
        self
    }

    /// 测量坐标系原点 `/O`（默认用户空间坐标）。
    pub fn with_origin(mut self, origin: [f64; 2]) -> Self {
        self.origin = Some(origin);
        self
    }

    /// y 轴最大单位到 x 轴最大单位的换算因子 `/CYX`；须同时提供 `/Y`。
    pub fn with_conversion_yx(mut self, factor: f64) -> Self {
        self.conversion_yx = Some(factor);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.ratio.is_empty() {
            return Err("measure /R ratio must not be empty".into());
        }
        for (key, formats) in [
            ("X", Some(&self.x)),
            ("Y", self.y.as_ref()),
            ("D", Some(&self.distance)),
            ("A", Some(&self.area)),
            ("T", self.angle.as_ref()),
            ("S", self.slope.as_ref()),
        ] {
            let Some(formats) = formats else {
                continue;
            };
            if formats.is_empty() {
                return Err(format!(
                    "measure /{key} number format array must not be empty"
                ));
            }
            if formats.len() > MAX_NUMBER_FORMATS {
                return Err(format!("measure /{key} has too many number formats"));
            }
            for format in formats {
                format.validate()?;
            }
        }
        if let Some(origin) = self.origin
            && !origin.iter().all(|v| v.is_finite())
        {
            return Err("measure /O must hold two finite numbers".into());
        }
        if let Some(factor) = self.conversion_yx {
            if !factor.is_finite() {
                return Err("measure /CYX must be finite".into());
            }
            if self.y.is_none() {
                return Err("measure /CYX requires /Y".into());
            }
        }
        Ok(())
    }

    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("Measure".into()))
            .insert("Subtype", Value::Name("RL".into()))
            .insert("R", text_string_value(&self.ratio))
            .insert("X", number_format_array(&self.x))
            .insert("D", number_format_array(&self.distance))
            .insert("A", number_format_array(&self.area));
        for (key, formats) in [("Y", &self.y), ("T", &self.angle), ("S", &self.slope)] {
            if let Some(formats) = formats {
                dict = dict.insert(key, number_format_array(formats));
            }
        }
        if let Some(origin) = self.origin {
            dict = dict.insert(
                "O",
                Value::Array(Array(vec![
                    Value::Float(origin[0]),
                    Value::Float(origin[1]),
                ])),
            );
        }
        if let Some(factor) = self.conversion_yx {
            dict = dict.insert("CYX", Value::Float(factor));
        }
        dict
    }
}

fn number_format_array(formats: &[NumberFormat]) -> Value {
    Value::Array(Array(
        formats
            .iter()
            .map(|format| Value::Dict(format.to_dict()))
            .collect(),
    ))
}

impl PdfAnnotation {
    /// 测量字典 `/Measure`（Table 261；PDF 1.7，仅 Line 与 Polygon/PolyLine）。
    pub fn with_measure(mut self, measure: Measure) -> Self {
        self.measure = Some(measure);
        self
    }

    /// `/Measure` 测量字典。
    pub(crate) fn measure(&self) -> Option<&Measure> {
        self.measure.as_ref()
    }

    /// `/Measure` 的子类型归属与取值校验；由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_measure_entries(&self) -> Result<(), String> {
        if let Some(measure) = &self.measure {
            self.require_kind_in(
                &[
                    AnnotationKind::Line,
                    AnnotationKind::Polygon,
                    AnnotationKind::PolyLine,
                ],
                "annotation /Measure",
            )?;
            measure.validate()?;
        }
        Ok(())
    }
}
