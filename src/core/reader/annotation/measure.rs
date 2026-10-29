//! 注解 measure 字典解析（12.9，Table 261–263）。
//!
//! `/Measure` 自 PDF 1.7 起可用于 Line（Table 175）与 Polygon/PolyLine
//! （Table 178）。本模块类型化 measure 字典（Table 261）、直角坐标测量条目
//! （Table 262）与数字格式数组（Table 263）；读端保持宽容：必填条目缺失
//! 留存为 `None`/空数组，只对类型错误与越界取值报 `Malformed`。

use super::{AnnotationError, Result, entries::number, malformed};
use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, Value};

/// 单个数字格式数组的元素上限（防护不可信文件）。
const MAX_NUMBER_FORMATS: usize = 64;
/// `/R` 比例串长度上限（防护不可信文件）。
const MAX_RATIO_LEN: usize = 1024;

/// 数字格式字典的分数显示方式（`/F`，Table 263）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumberFraction {
    /// 按 `/D` 精度显示小数（`/D`，缺省）。
    Decimal,
    /// 按 `/D` 分母显示分数（`/F`）。
    Fraction,
    /// 无小数部分，四舍五入到整数（`/R`）。
    Round,
    /// 无小数部分，截断到整数（`/T`）。
    Truncate,
    /// 非标准名（原样保留）。
    Other(String),
}

impl NumberFraction {
    /// 由 `/F` 名字解析；未知名返回 [`NumberFraction::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "D" => Self::Decimal,
            "F" => Self::Fraction,
            "R" => Self::Round,
            "T" => Self::Truncate,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准名称；[`NumberFraction::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Decimal => "D",
            Self::Fraction => "F",
            Self::Round => "R",
            Self::Truncate => "T",
            Self::Other(other) => other,
        }
    }
}

/// 单位标签相对数值的位置（`/O`，Table 263，缺省 `S`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumberLabelOrder {
    /// 数值后置标签（`/S`）。
    Suffix,
    /// 数值前置标签（`/P`）。
    Prefix,
    /// 非标准名（原样保留）。
    Other(String),
}

impl NumberLabelOrder {
    /// 由 `/O` 名字解析；未知名返回 [`NumberLabelOrder::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "S" => Self::Suffix,
            "P" => Self::Prefix,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准名称；[`NumberLabelOrder::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Suffix => "S",
            Self::Prefix => "P",
            Self::Other(other) => other,
        }
    }
}

/// 数字格式字典（Table 263）：一个计量单位及其文本格式化信息。
#[derive(Debug, Clone, PartialEq)]
pub struct NumberFormat {
    /// `/U` 单位标签（必填文本串）。
    pub unit: Option<String>,
    /// `/C` 换算因子（必填数值）。
    pub conversion: Option<f64>,
    /// `/F` 分数显示方式（缺省 `D`）。
    pub fraction: Option<NumberFraction>,
    /// `/D` 精度或分母（正整数）。
    pub precision: Option<u32>,
    /// `/FD` 是否禁止约分/截去低位零（缺省 false）。
    pub force_denominator: Option<bool>,
    /// `/RT` 千位分隔文本。
    pub thousands_separator: Option<String>,
    /// `/RD` 小数点文本。
    pub decimal_separator: Option<String>,
    /// `/PS` 单位标签左侧拼接文本。
    pub prefix_spacing: Option<String>,
    /// `/SS` 单位标签右侧拼接文本。
    pub suffix_spacing: Option<String>,
    /// `/O` 标签位置（缺省 `S`）。
    pub label_order: Option<NumberLabelOrder>,
    /// 完整数字格式字典，未建模条目经此访问。
    pub dict: Dict,
}

/// 注解 measure 字典（Table 261/262）。
#[derive(Debug, Clone, PartialEq)]
pub struct Measure {
    /// `/Subtype` 坐标系类型名（缺省 `RL` 直角坐标）。
    pub subtype: Option<String>,
    /// `/R` 比例串（Table 262 必填文本串）。
    pub ratio: Option<String>,
    /// `/X` 沿 x 轴的换算数组（必填）。
    pub x: Option<Vec<NumberFormat>>,
    /// `/Y` 沿 y 轴的换算数组（x/y 单位不同时必填）。
    pub y: Option<Vec<NumberFormat>>,
    /// `/D` 距离换算数组（必填）。
    pub distance: Option<Vec<NumberFormat>>,
    /// `/A` 面积换算数组（必填）。
    pub area: Option<Vec<NumberFormat>>,
    /// `/T` 角度换算数组。
    pub angle: Option<Vec<NumberFormat>>,
    /// `/S` 斜率换算数组。
    pub slope: Option<Vec<NumberFormat>>,
    /// `/O` 测量坐标系原点（默认用户空间坐标）。
    pub origin: Option<[f64; 2]>,
    /// `/CYX` y 轴最大单位到 x 轴最大单位的换算因子。
    pub conversion_yx: Option<f64>,
    /// 完整 measure 字典，未建模条目经此访问。
    pub dict: Dict,
}

/// 解析注解 `/Measure`；条目缺失返回 `None`，非字典报 `Malformed`。
pub(super) fn parse_measure(document: &Document, dict: &Dict) -> Result<Option<Measure>> {
    let Some(value) = dict.get("Measure") else {
        return Ok(None);
    };
    parse_measure_value(document, value).map(Some)
}

/// 解析一个 measure 字典值（可为间接引用）。
///
/// 注解 `/Measure`（12.5.6）与页面视口字典的 `/Measure`（12.9.2 Table 260）
/// 共用同一解析器。
pub(crate) fn parse_measure_value(document: &Document, value: &Value) -> Result<Measure> {
    let resolved =
        resolve(document, value).map_err(|err| AnnotationError::Malformed(err.to_string()))?;
    let measure = resolved
        .as_dict()
        .ok_or_else(|| malformed("measure dictionary is not a dictionary"))?;
    Ok(Measure {
        subtype: optional_name(measure, "Subtype")?,
        ratio: optional_text(measure, "R")?,
        x: parse_number_format_array(document, measure, "X")?,
        y: parse_number_format_array(document, measure, "Y")?,
        distance: parse_number_format_array(document, measure, "D")?,
        area: parse_number_format_array(document, measure, "A")?,
        angle: parse_number_format_array(document, measure, "T")?,
        slope: parse_number_format_array(document, measure, "S")?,
        origin: parse_origin(document, measure)?,
        conversion_yx: match measure.get("CYX") {
            None => None,
            Some(value) => Some(number(value, "measure /CYX")?),
        },
        dict: measure.clone(),
    })
}

/// 解析数字格式数组：元素须为字典（可为间接引用）。
fn parse_number_format_array(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<Option<Vec<NumberFormat>>> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let resolved =
        resolve(document, value).map_err(|err| AnnotationError::Malformed(err.to_string()))?;
    let array = resolved
        .as_array()
        .ok_or_else(|| malformed(format!("measure /{key} is not an array")))?;
    if array.len() > MAX_NUMBER_FORMATS {
        return Err(malformed(format!(
            "measure /{key} has too many number formats"
        )));
    }
    let mut out = Vec::with_capacity(array.len());
    for item in array.iter() {
        out.push(parse_number_format(document, item, key)?);
    }
    Ok(Some(out))
}

/// 解析单个数字格式字典（Table 263）。
fn parse_number_format(document: &Document, value: &Value, key: &str) -> Result<NumberFormat> {
    let resolved =
        resolve(document, value).map_err(|err| AnnotationError::Malformed(err.to_string()))?;
    let format = resolved
        .as_dict()
        .ok_or_else(|| malformed(format!("measure /{key} element is not a dictionary")))?;
    let precision = match format.get("D") {
        None => None,
        Some(value) => {
            let number = number(value, "measure number format /D")?;
            if number.fract() != 0.0 || !(1.0..=u32::MAX as f64).contains(&number) {
                return Err(malformed(
                    "measure number format /D must be a positive integer",
                ));
            }
            Some(number as u32)
        }
    };
    Ok(NumberFormat {
        unit: optional_text(format, "U")?,
        conversion: match format.get("C") {
            None => None,
            Some(value) => Some(number(value, "measure number format /C")?),
        },
        fraction: optional_name(format, "F")?.map(|name| NumberFraction::from_name(&name)),
        precision,
        force_denominator: match format.get("FD") {
            None => None,
            Some(Value::Bool(flag)) => Some(*flag),
            Some(_) => return Err(malformed("measure number format /FD must be a boolean")),
        },
        thousands_separator: optional_text(format, "RT")?,
        decimal_separator: optional_text(format, "RD")?,
        prefix_spacing: optional_text(format, "PS")?,
        suffix_spacing: optional_text(format, "SS")?,
        label_order: optional_name(format, "O")?.map(|name| NumberLabelOrder::from_name(&name)),
        dict: format.clone(),
    })
}

/// `/O` 测量坐标系原点：恰两个有限数值。
fn parse_origin(document: &Document, dict: &Dict) -> Result<Option<[f64; 2]>> {
    let Some(value) = dict.get("O") else {
        return Ok(None);
    };
    let resolved =
        resolve(document, value).map_err(|err| AnnotationError::Malformed(err.to_string()))?;
    let array = resolved
        .as_array()
        .ok_or_else(|| malformed("measure /O is not an array"))?;
    if array.len() != 2 {
        return Err(malformed("measure /O must hold two numbers"));
    }
    Ok(Some([
        number(&array.0[0], "measure /O")?,
        number(&array.0[1], "measure /O")?,
    ]))
}

/// 名字条目；非名字报 `Malformed`。
fn optional_name(dict: &Dict, key: &str) -> Result<Option<String>> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => Ok(Some(
            value
                .as_name()
                .ok_or_else(|| malformed(format!("measure /{key} must be a name")))?
                .to_string(),
        )),
    }
}

/// 文本串条目；非文本串报 `Malformed`，`/R` 限制长度。
fn optional_text(dict: &Dict, key: &str) -> Result<Option<String>> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    if !matches!(value, Value::String(_) | Value::HexString(_)) {
        return Err(malformed(format!("measure /{key} must be a text string")));
    }
    let decoded =
        text_value(value).ok_or_else(|| malformed(format!("measure /{key} is not valid text")))?;
    if key == "R" && decoded.len() > MAX_RATIO_LEN {
        return Err(malformed("measure /R is too long"));
    }
    Ok(Some(decoded))
}
