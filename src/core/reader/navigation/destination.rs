//! 显式与命名目的地解析（ISO 32000-1 12.3.2）。

use crate::core::types::{Array, Document, Value};

use super::{NavigationError, PageTreeIndex, malformed, name_tree::string_key, resolve};

/// 本地显式目的地（12.3.2.2）。
///
/// 页面以 0 基索引表示；`/XYZ` 的三个参数为 `null` 或缺省时为 `None`。
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    Xyz {
        page: usize,
        left: Option<f64>,
        top: Option<f64>,
        zoom: Option<f64>,
    },
    Fit {
        page: usize,
    },
    FitH {
        page: usize,
        top: Option<f64>,
    },
    FitV {
        page: usize,
        left: Option<f64>,
    },
    FitR {
        page: usize,
        rect: [f64; 4],
    },
    FitB {
        page: usize,
    },
    FitBH {
        page: usize,
        top: Option<f64>,
    },
    FitBV {
        page: usize,
        left: Option<f64>,
    },
}

impl Destination {
    /// 目标页面索引。
    pub fn page(&self) -> usize {
        match self {
            Self::Xyz { page, .. }
            | Self::Fit { page }
            | Self::FitH { page, .. }
            | Self::FitV { page, .. }
            | Self::FitR { page, .. }
            | Self::FitB { page }
            | Self::FitBH { page, .. }
            | Self::FitBV { page, .. } => *page,
        }
    }
}

/// 命名目标值 → 目的地；值可为目的地数组或含 `/D` 的字典。
pub(crate) fn parse_named_destination(
    document: &Document,
    value: &Value,
    pages: &PageTreeIndex,
) -> Result<Destination, NavigationError> {
    let value = resolve(document, value)?;
    if let Value::Dict(dict) = &value {
        let dest = dict
            .get("D")
            .ok_or_else(|| malformed("named destination dictionary is missing /D"))?;
        return parse_destination(document, dest, pages);
    }
    parse_destination(document, &value, pages)
}

/// 解析本地显式目的地的目的地数组。
pub(crate) fn parse_destination(
    document: &Document,
    value: &Value,
    pages: &PageTreeIndex,
) -> Result<Destination, NavigationError> {
    let value = resolve(document, value)?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("destination is not an array"))?;
    if array.len() < 2 {
        return Err(malformed("destination array is shorter than 2 elements"));
    }
    let Some((num, generation)) = array.0[0].as_ref() else {
        return Err(malformed(
            "destination page is not an indirect page reference",
        ));
    };
    let page = pages
        .index_of(num, generation)
        .ok_or(NavigationError::PageNotFound)?;
    let Some(kind) = array.0[1].as_name() else {
        return Err(malformed("destination type is not a name"));
    };
    match kind {
        "XYZ" => {
            ensure_no_extra(document, array, 5)?;
            Ok(Destination::Xyz {
                page,
                left: optional_number(document, array, 2)?,
                top: optional_number(document, array, 3)?,
                zoom: optional_number(document, array, 4)?,
            })
        }
        "Fit" => {
            ensure_no_extra(document, array, 2)?;
            Ok(Destination::Fit { page })
        }
        "FitH" => {
            ensure_no_extra(document, array, 3)?;
            Ok(Destination::FitH {
                page,
                top: optional_number(document, array, 2)?,
            })
        }
        "FitV" => {
            ensure_no_extra(document, array, 3)?;
            Ok(Destination::FitV {
                page,
                left: optional_number(document, array, 2)?,
            })
        }
        "FitR" => {
            ensure_no_extra(document, array, 6)?;
            let rect = [
                required_number(document, array, 2)?,
                required_number(document, array, 3)?,
                required_number(document, array, 4)?,
                required_number(document, array, 5)?,
            ];
            Ok(Destination::FitR { page, rect })
        }
        "FitB" => {
            ensure_no_extra(document, array, 2)?;
            Ok(Destination::FitB { page })
        }
        "FitBH" => {
            ensure_no_extra(document, array, 3)?;
            Ok(Destination::FitBH {
                page,
                top: optional_number(document, array, 2)?,
            })
        }
        "FitBV" => {
            ensure_no_extra(document, array, 3)?;
            Ok(Destination::FitBV {
                page,
                left: optional_number(document, array, 2)?,
            })
        }
        other => Err(malformed(format!("unknown destination type /{other}"))),
    }
}

/// 超出该类型参数个数的尾部操作数只允许为 `null`（宽容真实文件中的占位）。
fn ensure_no_extra(
    document: &Document,
    array: &Array,
    limit: usize,
) -> Result<(), NavigationError> {
    for index in limit..array.len() {
        let value = resolve(document, &array.0[index])?;
        if !matches!(value, Value::Null) {
            return Err(malformed(format!(
                "destination has unexpected operand at index {index}"
            )));
        }
    }
    Ok(())
}

fn optional_number(
    document: &Document,
    array: &Array,
    index: usize,
) -> Result<Option<f64>, NavigationError> {
    let Some(value) = array.0.get(index) else {
        return Ok(None);
    };
    let value = resolve(document, value)?;
    match value {
        Value::Null => Ok(None),
        Value::Integer(number) => Ok(Some(number as f64)),
        Value::Float(number) => Ok(Some(number)),
        other => Err(malformed(format!(
            "destination operand {index} is not a number: {other:?}"
        ))),
    }
}

fn required_number(
    document: &Document,
    array: &Array,
    index: usize,
) -> Result<f64, NavigationError> {
    optional_number(document, array, index)?
        .ok_or_else(|| malformed(format!("destination operand {index} is missing")))
}

/// 大纲 `/Dest`（数组/名称/字符串）→ 大纲目的地。
pub(crate) fn parse_outline_destination(
    document: &Document,
    value: &Value,
    pages: &PageTreeIndex,
) -> Result<super::OutlineDestination, NavigationError> {
    let value = resolve(document, value)?;
    match &value {
        Value::Array(_) => {
            parse_destination(document, &value, pages).map(super::OutlineDestination::Explicit)
        }
        Value::Name(name) => Ok(super::OutlineDestination::Named(name.clone())),
        Value::String(_) | Value::HexString(_) => string_key(&value)
            .map(super::OutlineDestination::Named)
            .ok_or_else(|| malformed("outline /Dest string is malformed")),
        _ => Err(malformed(
            "outline /Dest is neither a destination array nor a name",
        )),
    }
}
