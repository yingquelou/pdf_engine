//! 半色调字典的类型化读端模型（ISO 32000-1 §10.5.4 Table 168–170）。
//!
//! [`Document::halftone`] 把 `/HT` 值解析为 [`HalftoneModel`]：类型 1 与
//! 类型 2 半色调字典按字段校验并类型化；类型 3–5 字典、色料数组与
//! 命名半色调（名称引用）保留原始值（`Unmodeled`），不猜测其语义。
//! 半色调包装字典（`/DefaultHalftoneType` + 按色料条目）解析为
//! [`CompositeHalftone`]，其色料值递归解析。

use std::collections::BTreeMap;

use crate::core::types::{Dict, Document, Value};

use super::{GraphicsStateError, invalid};

#[cfg(test)]
mod tests;

/// 类型 1 半色调字典（Table 169）。
#[derive(Debug, Clone, PartialEq)]
pub struct Type1Halftone {
    /// `/XFrequency` 网点频率（cells/inch，> 0）。
    pub x_frequency: f64,
    /// `/Angle` 网点角度（度）。
    pub angle: f64,
    /// `/SpotFunction` 解析后的函数值（字典或流）。
    pub spot_function: Value,
    /// `/TransferFunction` 解析后的函数值（可选）。
    pub transfer_function: Option<Value>,
}

/// 类型 2 半色调字典（Table 170）。
#[derive(Debug, Clone, PartialEq)]
pub struct Type2Halftone {
    /// `/Frequency` 网点频率（cells/inch，> 0）。
    pub frequency: f64,
    /// `/Angle` 网点角度（度）。
    pub angle: f64,
    /// `/Width` 网点直径比例（≥ 0）。
    pub width: f64,
    /// `/Uncompensated`；省略即 false。
    pub uncompensated: bool,
    /// `/SpotFunction` 解析后的函数值（字典或流）。
    pub spot_function: Value,
    /// `/TransferFunction` 解析后的函数值（可选）。
    pub transfer_function: Option<Value>,
}

/// 半色调包装字典（Table 168）。
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeHalftone {
    /// `/DefaultHalftoneType`（1–5；省略为 `None`）。
    pub default_type: Option<u8>,
    /// 色料名 → 半色调值（类型 1/2 已类型化，其余保留原值）。
    pub colorants: BTreeMap<String, HalftoneModel>,
    /// 包装字典中不建模的其余条目（`/DefaultColorant`、
    /// `*HalftoneType`、`/DefaultDecode`、`/Duplex` 等），原值保留。
    pub other: BTreeMap<String, Value>,
}

/// `/HT` 值的类型化模型。
#[derive(Debug, Clone, PartialEq)]
pub enum HalftoneModel {
    /// 类型 1 半色调字典。
    Type1(Type1Halftone),
    /// 类型 2 半色调字典。
    Type2(Type2Halftone),
    /// 半色调包装字典。
    Composite(CompositeHalftone),
    /// 不建模的形态（类型 3–5 字典、色料数组、命名半色调引用），
    /// 保留解析后的原始值以便透传。
    Unmodeled(Value),
}

impl Document {
    /// 解析 `/HT` 半色调值。
    ///
    /// `value` 可为字典、流、间接引用或名称（命名半色调引用解析为
    /// [`HalftoneModel::Unmodeled`]）。类型 1/2 的必需条目缺失或数值
    /// 非法时返回类型化错误。
    pub fn halftone(&self, value: &Value) -> Result<HalftoneModel, GraphicsStateError> {
        let resolved = self.resolve_graphics_state_value(value)?;
        match &resolved {
            Value::Name(_) => return Ok(HalftoneModel::Unmodeled(resolved)),
            Value::Dict(_) => {}
            _ => {
                return Err(invalid(format!(
                    "halftone must be a dictionary or name, got {}",
                    resolved.type_name()
                )));
            }
        }
        self.parse_halftone_dict(&resolved, &resolved)
    }

    /// 解析单个半色调字典；`written` 是未解析的原始值（供
    /// `Unmodeled` 保留写出形态，例如间接引用）。
    fn parse_halftone_dict(
        &self,
        value: &Value,
        written: &Value,
    ) -> Result<HalftoneModel, GraphicsStateError> {
        let resolved = self.resolve_graphics_state_value(value)?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| invalid("halftone must be a dictionary"))?;
        match dict.get("HalftoneType") {
            Some(type_value) => {
                let kind = integer(self, type_value)?
                    .and_then(|value| u8::try_from(value).ok())
                    .ok_or_else(|| invalid("halftone /HalftoneType must be an integer"))?;
                match kind {
                    1 => self.parse_type1(dict).map(HalftoneModel::Type1),
                    2 => self.parse_type2(dict).map(HalftoneModel::Type2),
                    3..=5 => Ok(HalftoneModel::Unmodeled(written.clone())),
                    other => Err(invalid(format!(
                        "halftone /HalftoneType {other} is outside 1..=5"
                    ))),
                }
            }
            None if dict.get("DefaultHalftoneType").is_some() => {
                self.parse_composite(dict).map(HalftoneModel::Composite)
            }
            None => Err(invalid(
                "halftone dictionary requires /HalftoneType or /DefaultHalftoneType",
            )),
        }
    }

    fn parse_type1(&self, dict: &Dict) -> Result<Type1Halftone, GraphicsStateError> {
        let x_frequency = required_positive(self, dict, "XFrequency")?;
        let angle = required_finite(self, dict, "Angle")?;
        let spot_function = required_function(self, dict, "SpotFunction")?;
        let transfer_function = optional_function(self, dict, "TransferFunction")?;
        Ok(Type1Halftone {
            x_frequency,
            angle,
            spot_function,
            transfer_function,
        })
    }

    fn parse_type2(&self, dict: &Dict) -> Result<Type2Halftone, GraphicsStateError> {
        let frequency = required_positive(self, dict, "Frequency")?;
        let angle = required_finite(self, dict, "Angle")?;
        let width = required_non_negative(self, dict, "Width")?;
        let uncompensated = match dict.get("Uncompensated") {
            None => false,
            Some(value) => {
                let resolved = self.resolve_graphics_state_value(value)?;
                match resolved {
                    Value::Bool(flag) => flag,
                    _ => return Err(invalid("halftone /Uncompensated must be a boolean")),
                }
            }
        };
        let spot_function = required_function(self, dict, "SpotFunction")?;
        let transfer_function = optional_function(self, dict, "TransferFunction")?;
        Ok(Type2Halftone {
            frequency,
            angle,
            width,
            uncompensated,
            spot_function,
            transfer_function,
        })
    }

    fn parse_composite(&self, dict: &Dict) -> Result<CompositeHalftone, GraphicsStateError> {
        let default_type = match dict.get("DefaultHalftoneType") {
            None => None,
            Some(value) => {
                let kind = integer(self, value)?
                    .and_then(|value| u8::try_from(value).ok())
                    .filter(|kind| (1..=5).contains(kind))
                    .ok_or_else(|| {
                        invalid("halftone /DefaultHalftoneType must be an integer within 1..=5")
                    })?;
                Some(kind)
            }
        };
        let mut colorants = BTreeMap::new();
        let mut other = BTreeMap::new();
        for (key, value) in &dict.0 {
            match key.as_str() {
                "DefaultHalftoneType" | "Type" => {}
                "DefaultColorant" | "DefaultDecode" | "Duplex" | "DefaultDuplex" => {
                    other.insert(key.clone(), value.clone());
                }
                _ if key.ends_with("HalftoneType") => {
                    other.insert(key.clone(), value.clone());
                }
                _ => {
                    let resolved = self.resolve_graphics_state_value(value)?;
                    if resolved.as_dict().is_some() {
                        colorants.insert(key.clone(), self.parse_halftone_dict(value, value)?);
                    } else {
                        colorants.insert(key.clone(), HalftoneModel::Unmodeled(value.clone()));
                    }
                }
            }
        }
        Ok(CompositeHalftone {
            default_type,
            colorants,
            other,
        })
    }
}

fn integer(document: &Document, value: &Value) -> Result<Option<i64>, GraphicsStateError> {
    let resolved = document.resolve_graphics_state_value(value)?;
    Ok(match resolved {
        Value::Integer(value) => Some(value),
        Value::Float(value) if value.is_finite() && value.fract() == 0.0 => Some(value as i64),
        _ => None,
    })
}

fn number(document: &Document, value: &Value) -> Result<Option<f64>, GraphicsStateError> {
    let resolved = document.resolve_graphics_state_value(value)?;
    Ok(match resolved {
        Value::Integer(value) => Some(value as f64),
        Value::Float(value) if value.is_finite() => Some(value),
        _ => None,
    })
}

fn required_positive(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<f64, GraphicsStateError> {
    let value = required_number(document, dict, key)?;
    if value <= 0.0 {
        return Err(invalid(format!("halftone /{key} must be positive")));
    }
    Ok(value)
}

fn required_non_negative(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<f64, GraphicsStateError> {
    let value = required_number(document, dict, key)?;
    if value < 0.0 {
        return Err(invalid(format!("halftone /{key} must be non-negative")));
    }
    Ok(value)
}

fn required_finite(document: &Document, dict: &Dict, key: &str) -> Result<f64, GraphicsStateError> {
    required_number(document, dict, key)
}

fn required_number(document: &Document, dict: &Dict, key: &str) -> Result<f64, GraphicsStateError> {
    let value = dict
        .get(key)
        .ok_or_else(|| invalid(format!("halftone /{key} is required")))?;
    number(document, value)?
        .ok_or_else(|| invalid(format!("halftone /{key} must be a finite number")))
}

fn required_function(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<Value, GraphicsStateError> {
    optional_function(document, dict, key)?
        .ok_or_else(|| invalid(format!("halftone /{key} is required")))
}

fn optional_function(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<Option<Value>, GraphicsStateError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let resolved = document.resolve_graphics_state_value(value)?;
    match &resolved {
        Value::Dict(_) | Value::Stream(_, _) => {}
        _ => {
            return Err(invalid(format!(
                "halftone /{key} must be a function dictionary or stream"
            )));
        }
    }
    Ok(Some(resolved))
}
