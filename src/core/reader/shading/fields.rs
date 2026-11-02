//! Shading/Pattern 字典字段的抽取与规范校验（颜色空间、函数、域、
//! Extend、公共条目、Matrix、位宽、Decode、实数数组与间接引用解析），
//! 以及布尔/数值等值形态强转辅助。

use crate::core::functions::Function;
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::types::{Dict, Document, Value};

use super::error::{Result, ShadingError, from_color_space, malformed, unsupported};
use super::model::{MeshFunction, ShadingOptions};

impl Document {
    pub(super) fn shading_color_space(&self, dict: &Dict) -> Result<ColorSpaceModel> {
        let value = dict
            .get("ColorSpace")
            .ok_or_else(|| malformed("missing required key /ColorSpace"))?;
        let model = self.color_space(value).map_err(from_color_space)?;
        if matches!(model, ColorSpaceModel::Pattern { .. }) {
            return Err(unsupported("shading color space must not be Pattern"));
        }
        Ok(model)
    }

    pub(super) fn shading_function(&self, dict: &Dict) -> Result<Function> {
        let value = dict
            .get("Function")
            .ok_or_else(|| malformed("missing required key /Function"))?;
        self.build_function(value)
    }

    pub(super) fn mesh_function(&self, value: &Value, components: usize) -> Result<MeshFunction> {
        let resolved = self.resolve(value)?;
        if let Some(items) = resolved.as_array() {
            if items.0.len() != components {
                return Err(malformed(format!(
                    "shading function array must have {components} entries, got {}",
                    items.0.len()
                )));
            }
            let mut functions = Vec::with_capacity(items.0.len());
            for item in &items.0 {
                let function = self.build_function(item)?;
                if function.domain().len() != 1 || function.output_count() != Some(1) {
                    return Err(malformed(
                        "each shading component function must take 1 input and produce 1 output",
                    ));
                }
                functions.push(function);
            }
            return Ok(MeshFunction::Components(functions));
        }
        let function = self.build_function(&resolved)?;
        if function.domain().len() != 1 {
            return Err(malformed(
                "mesh shading function must take a single input parameter",
            ));
        }
        if let Some(outputs) = function.output_count()
            && outputs != components
        {
            return Err(malformed(format!(
                "shading function produces {outputs} outputs, color space has {components} components"
            )));
        }
        Ok(MeshFunction::Single(function))
    }

    fn build_function(&self, value: &Value) -> Result<Function> {
        let resolved = self.resolve(value)?;
        let built = match &resolved {
            Value::Stream(dict, _) => {
                let bytes = self
                    .decode_stream_bytes(&resolved)
                    .map_err(|error| malformed(format!("function stream: {error}")))?;
                Function::from_dict(dict, Some(&bytes))
            }
            Value::Dict(_) => Function::from_value(&resolved, None),
            other => {
                return Err(malformed(format!(
                    "function must be a dictionary or stream, got {}",
                    other.type_name()
                )));
            }
        };
        built.map_err(|error| ShadingError::Function(error.to_string()))
    }

    pub(super) fn shading_domain(&self, dict: &Dict) -> Result<Vec<(f64, f64)>> {
        let numbers = match dict.get("Domain") {
            None | Some(Value::Null) => vec![0.0, 1.0],
            Some(value) => {
                let numbers = self.real_numbers(value, "Domain")?;
                if numbers.len() != 2 && numbers.len() != 4 {
                    return Err(malformed("shading /Domain must contain 2 or 4 numbers"));
                }
                numbers
            }
        };
        let mut pairs = Vec::with_capacity(numbers.len() / 2);
        for pair in numbers.chunks(2) {
            if !pair[0].is_finite() || !pair[1].is_finite() || pair[0] > pair[1] {
                return Err(malformed(
                    "shading /Domain pairs must be finite and ordered",
                ));
            }
            pairs.push((pair[0], pair[1]));
        }
        Ok(pairs)
    }

    pub(super) fn shading_extend(&self, dict: &Dict) -> Result<[bool; 2]> {
        match dict.get("Extend") {
            None | Some(Value::Null) => Ok([false, false]),
            Some(value) => {
                let resolved = self.resolve(value)?;
                let array = resolved
                    .as_array()
                    .ok_or_else(|| malformed("/Extend must be an array"))?;
                if array.0.len() != 2 {
                    return Err(malformed("/Extend must contain 2 booleans"));
                }
                let first = bool_value(&array.0[0])
                    .ok_or_else(|| malformed("/Extend elements must be booleans"))?;
                let second = bool_value(&array.0[1])
                    .ok_or_else(|| malformed("/Extend elements must be booleans"))?;
                Ok([first, second])
            }
        }
    }

    pub(super) fn shading_options(&self, dict: &Dict, components: usize) -> Result<ShadingOptions> {
        let background = match dict.get("Background") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let numbers = self.real_numbers(value, "Background")?;
                if numbers.len() != components {
                    return Err(malformed(format!(
                        "/Background must have {components} components, got {}",
                        numbers.len()
                    )));
                }
                if numbers.iter().any(|v| !v.is_finite()) {
                    return Err(malformed("/Background components must be finite"));
                }
                Some(numbers)
            }
        };
        let bbox = match dict.get("BBox") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let numbers = self.real_numbers(value, "BBox")?;
                if numbers.len() != 4 {
                    return Err(malformed("/BBox must contain 4 numbers"));
                }
                if numbers.iter().any(|v| !v.is_finite())
                    || numbers[0] >= numbers[2]
                    || numbers[1] >= numbers[3]
                {
                    return Err(malformed(
                        "/BBox must be finite with the minimum corners below the maximum",
                    ));
                }
                Some([numbers[0], numbers[1], numbers[2], numbers[3]])
            }
        };
        let anti_alias = match dict.get("AntiAlias") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                bool_value(&self.resolve(value)?)
                    .ok_or_else(|| malformed("/AntiAlias must be a boolean"))?,
            ),
        };
        Ok(ShadingOptions {
            background,
            bbox,
            anti_alias,
        })
    }

    pub(super) fn optional_matrix(&self, dict: &Dict) -> Result<Option<[f64; 6]>> {
        match dict.get("Matrix") {
            None | Some(Value::Null) => Ok(None),
            Some(_) => {
                let numbers = self.required_real_array(dict, "Matrix", 6)?;
                Ok(Some([
                    numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
                ]))
            }
        }
    }

    pub(super) fn bit_depth(&self, dict: &Dict, key: &str, allowed: &[usize]) -> Result<usize> {
        let value = dict
            .get(key)
            .and_then(Value::as_integer)
            .ok_or_else(|| malformed(format!("missing required key /{key}")))?;
        if value <= 0 {
            return Err(malformed(format!("/{key} must be positive")));
        }
        let bits = value as usize;
        if !allowed.contains(&bits) {
            return Err(malformed(format!(
                "/{key} must be one of {allowed:?}, got {bits}"
            )));
        }
        Ok(bits)
    }

    pub(super) fn decode_pairs(
        &self,
        dict: &Dict,
        expected: usize,
    ) -> Result<Option<Vec<(f64, f64)>>> {
        let Some(value) = dict.get("Decode") else {
            return Ok(None);
        };
        let numbers = self.real_numbers(value, "Decode")?;
        if numbers.len() != expected * 2 {
            return Err(malformed(format!(
                "/Decode must contain {} numbers, got {}",
                expected * 2,
                numbers.len()
            )));
        }
        let mut pairs = Vec::with_capacity(expected);
        for pair in numbers.chunks(2) {
            if !pair[0].is_finite() || !pair[1].is_finite() {
                return Err(malformed("/Decode bounds must be finite"));
            }
            if pair[0] > pair[1] {
                pairs.push((pair[1], pair[0]));
            } else {
                pairs.push((pair[0], pair[1]));
            }
        }
        Ok(Some(pairs))
    }

    pub(super) fn required_real_array(
        &self,
        dict: &Dict,
        key: &str,
        len: usize,
    ) -> Result<Vec<f64>> {
        let value = dict
            .get(key)
            .ok_or_else(|| malformed(format!("missing required key /{key}")))?;
        let numbers = self.real_numbers(value, key)?;
        if numbers.len() != len {
            return Err(malformed(format!(
                "/{key} must contain {len} numbers, got {}",
                numbers.len()
            )));
        }
        if numbers.iter().any(|v| !v.is_finite()) {
            return Err(malformed(format!("/{key} values must be finite")));
        }
        Ok(numbers)
    }

    fn real_numbers(&self, value: &Value, key: &str) -> Result<Vec<f64>> {
        let resolved = self.resolve(value)?;
        let array = resolved
            .as_array()
            .ok_or_else(|| malformed(format!("/{key} must be an array")))?;
        array.0.iter().map(number_value).collect::<Result<Vec<_>>>()
    }

    pub(super) fn resolve(&self, value: &Value) -> Result<Value> {
        match value {
            Value::Ref(num, generation) => self
                .get_object(*num)
                .map_err(|error| malformed(error.to_string()))?
                .map(|object| object.value)
                .ok_or_else(|| {
                    malformed(format!("referenced object {num} {generation} R missing"))
                }),
            other => Ok(other.clone()),
        }
    }
}
fn bool_value(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

fn number_value(value: &Value) -> Result<f64> {
    match value {
        Value::Integer(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        other => Err(malformed(format!(
            "expected a number, got {}",
            other.type_name()
        ))),
    }
}

pub(super) fn number(dict: &Dict, key: &str) -> Result<f64> {
    let Some(value) = dict.get(key) else {
        return Err(malformed(format!("missing required key /{key}")));
    };
    number_value(value)
}
