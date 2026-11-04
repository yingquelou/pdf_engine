use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

use super::functions::GraphicsStateFunctions;
use super::model::GraphicsState;

impl GraphicsState {
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
        if let Some(value) = &self.hybrid_color_space {
            validate_hybrid_color_space(value)?;
            dict = dict.insert("CS", value.clone());
        }
        if let Some(style) = &self.hybrid_color_space_style {
            if self.hybrid_color_space.is_none() {
                return Err(invalid(
                    "/CSS requires /CS to be present on the same graphics state",
                ));
            }
            if style != "Knockout" && style != "Native" {
                return Err(invalid(
                    "/CSS must be /Knockout or /Native, got the string given",
                ));
            }
            dict = dict.insert("CSS", Value::Name(style.clone()));
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

/// `/CS` 形态校验：一个合法的 PDF 名，或一个恰好两元素的数组，每个
/// 元素为合法的 PDF 名或有限数。setter 生成的形态均满足此约束；此处
/// 二次校验保证调用方手工构造 `GraphicsState` 并直接设置 `Option<Value>`
/// 字段时也走同一条契约。
fn validate_hybrid_color_space(value: &Value) -> Result<(), WriteError> {
    match value {
        Value::Name(name) => {
            if !is_valid_pdf_name(name) {
                return Err(invalid("/CS name must be a valid PDF name"));
            }
            Ok(())
        }
        Value::Array(items) => {
            if items.0.len() != 2 {
                return Err(invalid("/CS array form must have exactly two elements"));
            }
            for item in &items.0 {
                match item {
                    Value::Name(name) => {
                        if !is_valid_pdf_name(name) {
                            return Err(invalid("/CS array elements must be valid PDF names"));
                        }
                    }
                    Value::Integer(number) => {
                        if *number == i64::MIN {
                            return Err(invalid(
                                "/CS array numbers must convert to finite floating point",
                            ));
                        }
                    }
                    Value::Float(number) if number.is_finite() => {}
                    other => {
                        return Err(invalid(format!(
                            "/CS array elements must be names or finite numbers, got {}",
                            other.type_name()
                        )));
                    }
                }
            }
            Ok(())
        }
        other => Err(invalid(format!(
            "/CS must be a name or a two-element array, got {}",
            other.type_name()
        ))),
    }
}
