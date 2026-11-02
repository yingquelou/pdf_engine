//! 颜色空间数组的分发与各家族解析器（§8.6.4–8.6.7）：CIE-Based、ICCBased、
//! Indexed 调色板、Separation / DeviceN tint 变换与 Pattern 基空间。

use crate::core::functions::Function;
use crate::core::types::{Document, Value};

use super::error::{ColorSpaceError, Result, malformed, unsupported};
use super::model::{
    CalGraySpace, CalRGBSpace, ColorSpaceModel, DeviceNSpace, IccBasedSpace, IndexedSpace,
    LabSpace, SeparationSpace,
};

/// 颜色空间嵌套解析最大深度（Indexed 基空间、Separation 备用空间等），防环。
const MAX_COLOR_SPACE_DEPTH: usize = 8;

impl Document {
    pub(super) fn color_space_depth(&self, value: &Value, depth: usize) -> Result<ColorSpaceModel> {
        if depth > MAX_COLOR_SPACE_DEPTH {
            return Err(unsupported(
                "color space nesting exceeds 8 levels (possible cycle)",
            ));
        }
        let value = self.resolve_one(value)?;
        match &value {
            Value::Name(name) => match name.as_str() {
                "DeviceGray" => Ok(ColorSpaceModel::DeviceGray),
                "DeviceRGB" => Ok(ColorSpaceModel::DeviceRGB),
                "DeviceCMYK" => Ok(ColorSpaceModel::DeviceCMYK),
                "Pattern" => Ok(ColorSpaceModel::Pattern { base: None }),
                other => Err(unsupported(format!("color space name /{other}"))),
            },
            Value::Array(array) => {
                let items = &array.0;
                let Some(first) = items.first() else {
                    return Err(malformed("empty color space array"));
                };
                let keyword = self
                    .resolve_one(first)?
                    .as_name()
                    .ok_or_else(|| malformed("color space family must be a name"))?
                    .to_string();
                match keyword.as_str() {
                    "CalGray" => self.parse_cal_gray(items, depth),
                    "CalRGB" => self.parse_cal_rgb(items, depth),
                    "Lab" => self.parse_lab(items, depth),
                    "ICCBased" => self.parse_icc_based(items, depth),
                    "Indexed" | "Index" => self.parse_indexed(items, depth),
                    "Separation" => self.parse_separation(items, depth),
                    "DeviceN" => self.parse_device_n(items, depth),
                    "Pattern" => self.parse_pattern(items, depth),
                    "NChannel" => Err(unsupported("NChannel color space")),
                    other => Err(unsupported(format!("color space family /{other}"))),
                }
            }
            other => Err(malformed(format!(
                "color space must be a name or array, got {}",
                other.type_name()
            ))),
        }
    }

    fn parse_cal_gray(&self, items: &[Value], _depth: usize) -> Result<ColorSpaceModel> {
        let dict = self.dict_item(items, 1, "CalGray")?;
        Ok(ColorSpaceModel::CalGray(CalGraySpace {
            white_point: self.real_array(&dict, "WhitePoint", 3, None)?,
            black_point: self.real_array(&dict, "BlackPoint", 3, Some(&[0.0, 0.0, 0.0]))?,
            gamma: match dict.get("Gamma") {
                Some(value) => {
                    let resolved = self.resolve_one(value)?;
                    match resolved {
                        Value::Integer(i) => i as f64,
                        Value::Float(f) => f,
                        other => {
                            return Err(malformed(format!(
                                "/Gamma must be a number, got {}",
                                other.type_name()
                            )));
                        }
                    }
                }
                None => 1.0,
            },
        }))
    }

    fn parse_cal_rgb(&self, items: &[Value], _depth: usize) -> Result<ColorSpaceModel> {
        let dict = self.dict_item(items, 1, "CalRGB")?;
        Ok(ColorSpaceModel::CalRGB(CalRGBSpace {
            white_point: self.real_array(&dict, "WhitePoint", 3, None)?,
            black_point: self.real_array(&dict, "BlackPoint", 3, Some(&[0.0, 0.0, 0.0]))?,
            gamma: self.real_array(&dict, "Gamma", 3, Some(&[1.0, 1.0, 1.0]))?,
            matrix: self.real_array(
                &dict,
                "Matrix",
                9,
                Some(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
            )?,
        }))
    }

    fn parse_lab(&self, items: &[Value], _depth: usize) -> Result<ColorSpaceModel> {
        let dict = self.dict_item(items, 1, "Lab")?;
        let range = self.ordered_pairs(&dict, "Range", 4, &[-100.0, 100.0, -100.0, 100.0])?;
        Ok(ColorSpaceModel::Lab(LabSpace {
            white_point: self.real_array(&dict, "WhitePoint", 3, None)?,
            black_point: self.real_array(&dict, "BlackPoint", 3, Some(&[0.0, 0.0, 0.0]))?,
            range,
        }))
    }

    fn parse_icc_based(&self, items: &[Value], depth: usize) -> Result<ColorSpaceModel> {
        let Some(specified) = items.get(1) else {
            return Err(malformed(
                "ICCBased color space is missing its stream reference",
            ));
        };
        let resolved = self.resolve_one(specified)?;
        let Value::Stream(dict, _) = &resolved else {
            return Err(malformed(format!(
                "ICCBased parameter must be a stream, got {}",
                resolved.type_name()
            )));
        };
        let n = dict
            .get("N")
            .and_then(Value::as_integer)
            .ok_or_else(|| malformed("ICCBased stream dictionary missing /N"))?;
        if !(1..=4).contains(&n) {
            return Err(malformed(format!("ICCBased /N must be 1..=4, got {n}")));
        }
        let n = n as usize;
        let default_range = [0.0f64, 1.0].repeat(n);
        let range = self.ordered_pairs(dict, "Range", 2 * n, &default_range)?;
        let profile = self
            .decode_stream_bytes(&resolved)
            .map_err(|error| malformed(format!("ICCBased profile stream: {error}")))?;
        let alternate = match dict.get("Alternate") {
            Some(value) => Box::new(self.color_space_depth(value, depth + 1)?),
            None => Box::new(default_device_for_channels(n)),
        };
        Ok(ColorSpaceModel::IccBased(IccBasedSpace {
            n,
            range,
            alternate,
            profile,
        }))
    }

    fn parse_indexed(&self, items: &[Value], depth: usize) -> Result<ColorSpaceModel> {
        let missing = |index: usize| {
            malformed(format!(
                "Indexed color space is missing element {index} (base, hival, lookup)"
            ))
        };
        let base_value = items.get(1).ok_or_else(|| missing(1))?;
        let base = Box::new(self.color_space_depth(base_value, depth + 1)?);
        if matches!(
            *base,
            ColorSpaceModel::Indexed(_)
                | ColorSpaceModel::Separation(_)
                | ColorSpaceModel::DeviceN(_)
                | ColorSpaceModel::Pattern { .. }
        ) {
            return Err(unsupported(
                "Indexed base must be device, CIE-based or ICCBased",
            ));
        }
        let hival_value = items.get(2).ok_or_else(|| missing(2))?;
        let hival = self
            .resolve_one(hival_value)?
            .as_integer()
            .ok_or_else(|| malformed("Indexed hival must be an integer"))?;
        if !(0..=255).contains(&hival) {
            return Err(malformed(format!(
                "Indexed hival must be 0..=255, got {hival}"
            )));
        }
        let lookup_value = items.get(3).ok_or_else(|| missing(3))?;
        let lookup = self.color_lookup_bytes(lookup_value)?;
        let stride = base
            .native_component_count()
            .ok_or_else(|| unsupported("Indexed base has no sample components"))?;
        let required = (hival as usize + 1)
            .checked_mul(stride)
            .ok_or_else(|| malformed("Indexed palette length overflow"))?;
        if lookup.len() < required {
            return Err(malformed(format!(
                "Indexed palette has {} bytes, needs at least {required} for hival {hival}",
                lookup.len()
            )));
        }
        Ok(ColorSpaceModel::Indexed(IndexedSpace {
            base,
            hival: hival as usize,
            lookup,
        }))
    }

    /// 调色板：文本串（字节按字符低 8 位还原）、十六进制串或流对象。
    fn color_lookup_bytes(&self, value: &Value) -> Result<Vec<u8>> {
        let resolved = self.resolve_one(value)?;
        match &resolved {
            Value::String(text) => Ok(text.chars().map(|c| c as u8).collect()),
            Value::HexString(text) => hex_to_bytes(text),
            Value::Stream(_, _) => self
                .decode_stream_bytes(&resolved)
                .map_err(|error| malformed(format!("palette stream: {error}"))),
            other => Err(malformed(format!(
                "Indexed lookup must be a string or stream, got {}",
                other.type_name()
            ))),
        }
    }

    fn parse_separation(&self, items: &[Value], depth: usize) -> Result<ColorSpaceModel> {
        if items.len() < 4 {
            return Err(malformed(
                "Separation color space needs [family colorant alternate tint]",
            ));
        }
        let colorant = self
            .resolve_one(&items[1])?
            .as_name()
            .ok_or_else(|| malformed("Separation colorant must be a name"))?
            .to_string();
        let alternate = Box::new(self.restricted_alternate(&items[2], depth)?);
        let tint = self.parse_tint_function(&items[3])?;
        Ok(ColorSpaceModel::Separation(SeparationSpace {
            colorant,
            alternate,
            tint,
        }))
    }

    fn parse_device_n(&self, items: &[Value], depth: usize) -> Result<ColorSpaceModel> {
        if items.len() < 4 {
            return Err(malformed(
                "DeviceN color space needs [family names alternate tint]",
            ));
        }
        let names_value = self.resolve_one(&items[1])?;
        let names = names_value
            .as_array()
            .ok_or_else(|| malformed("DeviceN colorant list must be an array"))?;
        let mut colorants = Vec::with_capacity(names.0.len());
        for item in &names.0 {
            let name = self
                .resolve_one(item)?
                .as_name()
                .ok_or_else(|| malformed("DeviceN colorant must be a name"))?
                .to_string();
            colorants.push(name);
        }
        if colorants.is_empty() || colorants.len() > 8 {
            return Err(malformed(format!(
                "DeviceN needs 1..=8 colorants, got {}",
                colorants.len()
            )));
        }
        let alternate = Box::new(self.restricted_alternate(&items[2], depth)?);
        let tint = self.parse_tint_function(&items[3])?;
        Ok(ColorSpaceModel::DeviceN(DeviceNSpace {
            colorants,
            alternate,
            tint,
        }))
    }

    /// Separation / DeviceN 的备用空间：不允许再嵌套 tint/索引/图案类空间。
    fn restricted_alternate(&self, value: &Value, depth: usize) -> Result<ColorSpaceModel> {
        let alternate = self.color_space_depth(value, depth + 1)?;
        if matches!(
            alternate,
            ColorSpaceModel::Indexed(_)
                | ColorSpaceModel::Separation(_)
                | ColorSpaceModel::DeviceN(_)
                | ColorSpaceModel::Pattern { .. }
        ) {
            return Err(unsupported(
                "Separation/DeviceN alternate must be device, CIE-based or ICCBased",
            ));
        }
        Ok(alternate)
    }

    fn parse_tint_function(&self, value: &Value) -> Result<Option<Function>> {
        if matches!(value, Value::Null) {
            return Ok(None);
        }
        let resolved = self.resolve_one(value)?;
        let built = match &resolved {
            Value::Stream(dict, _) => {
                let bytes = self
                    .decode_stream_bytes(&resolved)
                    .map_err(|error| malformed(format!("tint function stream: {error}")))?;
                Function::from_dict(dict, Some(&bytes))
            }
            Value::Dict(_) => Function::from_value(&resolved, None),
            other => {
                return Err(malformed(format!(
                    "tint transform must be a function dictionary or stream, got {}",
                    other.type_name()
                )));
            }
        };
        built
            .map(Some)
            .map_err(|error| ColorSpaceError::Function(error.to_string()))
    }

    fn parse_pattern(&self, items: &[Value], depth: usize) -> Result<ColorSpaceModel> {
        let base = match items.get(1) {
            Some(value) => {
                let base = Box::new(self.color_space_depth(value, depth + 1)?);
                if matches!(*base, ColorSpaceModel::Pattern { .. }) {
                    return Err(malformed("Pattern base must not be Pattern"));
                }
                Some(base)
            }
            None => None,
        };
        Ok(ColorSpaceModel::Pattern { base })
    }
}

fn default_device_for_channels(n: usize) -> ColorSpaceModel {
    match n {
        1 => ColorSpaceModel::DeviceGray,
        3 => ColorSpaceModel::DeviceRGB,
        _ => ColorSpaceModel::DeviceCMYK,
    }
}

/// 解码十六进制串（跳过空白，奇数位按规范以 0 补尾）。
fn hex_to_bytes(text: &str) -> Result<Vec<u8>> {
    let mut nibbles = Vec::with_capacity(text.len());
    for c in text.chars() {
        if c.is_whitespace() {
            continue;
        }
        let digit = c
            .to_digit(16)
            .ok_or_else(|| malformed(format!("invalid hex digit '{c}' in palette string")))?;
        nibbles.push(digit);
    }
    if nibbles.len() % 2 == 1 {
        nibbles.push(0);
    }
    Ok(nibbles
        .chunks(2)
        .map(|pair| ((pair[0] << 4) | pair[1]) as u8)
        .collect())
}
