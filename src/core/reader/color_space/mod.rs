//! 颜色空间读端模型（ISO 32000-1 §8.6.4–8.6.8）。
//!
//! 从 [`Document`] 解析设备、CIE-Based（CalGray / CalRGB / Lab / ICCBased）、
//! Indexed、Separation / DeviceN 与 Pattern 颜色空间，支持间接引用、
//! 流对象与（已解密上下文下的）过滤解码；Separation / DeviceN 的 tint
//! 变换复用 [`crate::core::functions::Function`] 求值器。
//!
//! [`ColorSpaceModel::decode_samples`] 把图像样本位流解码为规格化原生
//! 分量（Indexed 经调色板展开回基空间，Separation / DeviceN 经 tint
//! 函数映射到备用空间），不应用图像字典的 `/Decode` 矩阵——由调用方
//! 负责。非法颜色空间返回类型化 [`ColorSpaceError`]。

use crate::core::functions::Function;
use crate::core::types::{Dict, Document, Value};

/// 颜色空间解析与样本解码的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSpaceError {
    /// 颜色空间数组、字典或样本数据结构非法（缺键、类型错误、长度不足）。
    Malformed(String),
    /// 暂不支持的颜色空间类型或非法嵌套（如 Indexed 套 Indexed）。
    Unsupported(String),
    /// tint 变换函数构造或求值失败。
    Function(String),
}

impl std::fmt::Display for ColorSpaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "malformed color space: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported color space: {message}"),
            Self::Function(message) => write!(f, "tint function error: {message}"),
        }
    }
}

impl std::error::Error for ColorSpaceError {}

type Result<T> = std::result::Result<T, ColorSpaceError>;

fn malformed(message: impl Into<String>) -> ColorSpaceError {
    ColorSpaceError::Malformed(message.into())
}

fn unsupported(message: impl Into<String>) -> ColorSpaceError {
    ColorSpaceError::Unsupported(message.into())
}

/// 颜色空间嵌套解析最大深度（Indexed 基空间、Separation 备用空间等），防环。
const MAX_COLOR_SPACE_DEPTH: usize = 8;

/// CalGray（§8.6.5.2）。
#[derive(Debug, Clone, PartialEq)]
pub struct CalGraySpace {
    pub white_point: Vec<f64>,
    pub black_point: Vec<f64>,
    pub gamma: f64,
}

/// CalRGB（§8.6.5.3）。
#[derive(Debug, Clone, PartialEq)]
pub struct CalRGBSpace {
    pub white_point: Vec<f64>,
    pub black_point: Vec<f64>,
    pub gamma: Vec<f64>,
    pub matrix: Vec<f64>,
}

/// Lab（§8.6.5.4）。
#[derive(Debug, Clone, PartialEq)]
pub struct LabSpace {
    pub white_point: Vec<f64>,
    pub black_point: Vec<f64>,
    /// 原生值域，4 个数折叠为 2 个 `(min, max)`（a*、b*；L 固定 0..100）。
    pub range: Vec<(f64, f64)>,
}

/// ICCBased（§8.6.5.5）。
#[derive(Debug, Clone, PartialEq)]
pub struct IccBasedSpace {
    /// `/N`：通道数（1–4）。
    pub n: usize,
    /// 原生值域（缺 `/Range` 时为 n 个 `(0, 1)`）。
    pub range: Vec<(f64, f64)>,
    /// 备用设备空间（缺 `/Alternate` 时按 `/N` 取 DeviceGray/RGB/CMYK）。
    pub alternate: Box<ColorSpaceModel>,
    /// ICC 配置文件原始字节。
    pub profile: Vec<u8>,
}

/// Indexed（§8.6.6.1? 实为 §8.6.6 之前的 §8.6.6.1 索引色）：调色板颜色空间。
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedSpace {
    pub base: Box<ColorSpaceModel>,
    /// 最大调色板索引（hival）。
    pub hival: usize,
    /// 调色板字节，每条基空间分量宽度。
    pub lookup: Vec<u8>,
}

/// Separation（§8.6.6.2）：单色名空间。
#[derive(Debug, Clone, PartialEq)]
pub struct SeparationSpace {
    /// 色名（`/All`、`/None` 或专色名）。
    pub colorant: String,
    pub alternate: Box<ColorSpaceModel>,
    /// tint 变换；规范上应为函数，缺失/为空时按恒等处理。
    pub tint: Option<Function>,
}

/// DeviceN（§8.6.6.3）：多色名空间。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceNSpace {
    pub colorants: Vec<String>,
    pub alternate: Box<ColorSpaceModel>,
    pub tint: Option<Function>,
}

/// 类型化颜色空间模型。
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSpaceModel {
    DeviceGray,
    DeviceRGB,
    DeviceCMYK,
    CalGray(CalGraySpace),
    CalRGB(CalRGBSpace),
    Lab(LabSpace),
    IccBased(IccBasedSpace),
    Indexed(IndexedSpace),
    Separation(SeparationSpace),
    DeviceN(DeviceNSpace),
    /// Pattern（§8.6.7）：`/Pattern` 可选基空间。
    Pattern {
        base: Option<Box<ColorSpaceModel>>,
    },
}

impl ColorSpaceModel {
    /// 每像素的**样本**分量数（tint 域/索引/原生域解码前）；Pattern 无样本，
    /// 返回 `None`。
    pub fn component_count(&self) -> Option<usize> {
        match self {
            Self::DeviceGray | Self::CalGray(_) => Some(1),
            Self::DeviceRGB | Self::CalRGB(_) => Some(3),
            Self::DeviceCMYK => Some(4),
            Self::Lab(_) => Some(3),
            Self::IccBased(icc) => Some(icc.n),
            Self::Indexed(_) => Some(1),
            Self::Separation(_) => Some(1),
            Self::DeviceN(device_n) => Some(device_n.colorants.len()),
            Self::Pattern { .. } => None,
        }
    }

    /// 解码输出（原生空间）每像素分量数。
    pub fn native_component_count(&self) -> Option<usize> {
        match self {
            Self::Indexed(indexed) => indexed.base.native_component_count(),
            Self::Separation(separation) => separation.alternate.native_component_count(),
            Self::DeviceN(device_n) => device_n.alternate.native_component_count(),
            other => other.component_count(),
        }
    }

    /// 原生值域区间（解码输出未应用 `/Decode`，此处仅声明语义范围）。
    pub fn native_ranges(&self) -> Vec<(f64, f64)> {
        match self {
            Self::DeviceGray | Self::CalGray(_) => vec![(0.0, 1.0)],
            Self::DeviceRGB | Self::CalRGB(_) => vec![(0.0, 1.0); 3],
            Self::DeviceCMYK => vec![(0.0, 1.0); 4],
            Self::Lab(lab) => {
                let mut ranges = vec![(0.0, 100.0)];
                ranges.extend(lab.range.iter().copied());
                ranges
            }
            Self::IccBased(icc) => icc.range.clone(),
            Self::Indexed(indexed) => indexed.base.native_ranges(),
            Self::Separation(separation) => separation.alternate.native_ranges(),
            Self::DeviceN(device_n) => device_n.alternate.native_ranges(),
            Self::Pattern { base } => match base {
                Some(base) => base.native_ranges(),
                None => Vec::new(),
            },
        }
    }

    /// 把图像样本位流解码为规格化原生分量（像素优先、展平）。
    ///
    /// - `bits_per_component`：每样本位宽（设备/CIE/ICC/Separation/DeviceN
    ///   支持 1/2/4/8/16；Indexed 索引支持 1/2/4/8）；
    /// - `pixels_per_row`：每行像素数，行尾按字节补位；
    /// - Indexed：索引经调色板展开为基空间分量（调色板字节按 8 位归一）；
    /// - Separation / DeviceN：tint 规格值送 tint 函数求值到备用空间
    ///   （函数缺失时透传）。
    pub fn decode_samples(
        &self,
        bits_per_component: usize,
        pixels_per_row: usize,
        data: &[u8],
    ) -> Result<Vec<f64>> {
        let raw_n = self
            .component_count()
            .ok_or_else(|| unsupported("pattern color space carries no image samples"))?;
        if pixels_per_row == 0 {
            return Err(malformed("pixels_per_row must be positive"));
        }
        let allowed: &[usize] = if matches!(self, Self::Indexed(_)) {
            &[1, 2, 4, 8]
        } else {
            &[1, 2, 4, 8, 16]
        };
        if !allowed.contains(&bits_per_component) {
            return Err(malformed(format!(
                "unsupported bits per component {bits_per_component}"
            )));
        }
        let max_code = (1u64 << bits_per_component) - 1;
        let row_bytes = (bits_per_component * raw_n * pixels_per_row).div_ceil(8);
        if data.is_empty() || !data.len().is_multiple_of(row_bytes) {
            return Err(malformed(format!(
                "sample data length {} is not a whole number of {row_bytes}-byte rows",
                data.len()
            )));
        }

        let mut out = Vec::with_capacity(data.len() / row_bytes * pixels_per_row);
        for row in data.chunks(row_bytes) {
            for pixel in 0..pixels_per_row {
                let base_bit = pixel * raw_n * bits_per_component;
                let codes: Vec<u32> = (0..raw_n)
                    .map(|i| {
                        read_bits_msb(row, base_bit + i * bits_per_component, bits_per_component)
                    })
                    .collect();
                match self {
                    Self::Indexed(indexed) => {
                        let stride = indexed
                            .base
                            .native_component_count()
                            .ok_or_else(|| unsupported("Indexed base has no sample components"))?;
                        let index = (codes[0] as usize).min(indexed.hival);
                        let end = (index + 1)
                            .checked_mul(stride)
                            .ok_or_else(|| malformed("palette offset overflow"))?;
                        if indexed.lookup.len() < end {
                            return Err(malformed(format!(
                                "palette has {} bytes, index {index} needs {end}",
                                indexed.lookup.len()
                            )));
                        }
                        for byte in &indexed.lookup[index * stride..end] {
                            out.push(*byte as f64 / 255.0);
                        }
                    }
                    Self::Separation(separation) => {
                        let tints = normalize_codes(&codes, max_code);
                        out.extend(eval_tint(&separation.tint, &tints)?);
                    }
                    Self::DeviceN(device_n) => {
                        let tints = normalize_codes(&codes, max_code);
                        out.extend(eval_tint(&device_n.tint, &tints)?);
                    }
                    _ => out.extend(normalize_codes(&codes, max_code)),
                }
            }
        }
        Ok(out)
    }
}

fn normalize_codes(codes: &[u32], max_code: u64) -> Vec<f64> {
    codes
        .iter()
        .map(|code| *code as f64 / max_code as f64)
        .collect()
}

/// 从行位流按 MSB-first 读取 `bits` 位（1/2/4/8/16）。
fn read_bits_msb(row: &[u8], bit_offset: usize, bits: usize) -> u32 {
    let mut value = 0u32;
    for i in 0..bits {
        let bit = bit_offset + i;
        let on = (row[bit / 8] >> (7 - bit % 8)) & 1 == 1;
        value = (value << 1) | on as u32;
    }
    value
}

fn eval_tint(tint: &Option<Function>, tints: &[f64]) -> Result<Vec<f64>> {
    match tint {
        None => Ok(tints.to_vec()),
        Some(function) => function
            .eval(tints)
            .map_err(|error| ColorSpaceError::Function(error.to_string())),
    }
}

/// 页面默认颜色空间种类（资源字典 `/DefaultGray` 等）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultColorSpace {
    Gray,
    Rgb,
    Cmyk,
}

impl DefaultColorSpace {
    /// 资源字典中的键名。
    pub fn resource_key(self) -> &'static str {
        match self {
            Self::Gray => "DefaultGray",
            Self::Rgb => "DefaultRGB",
            Self::Cmyk => "DefaultCMYK",
        }
    }
}

impl Document {
    /// 从颜色空间值（名字或数组，可为间接引用）构造类型化模型。
    pub fn color_space(&self, value: &Value) -> Result<ColorSpaceModel> {
        self.color_space_depth(value, 0)
    }

    /// 解析页面 `/Resources` 中命名颜色空间资源；资源不存在时返回 `None`。
    pub fn page_color_space(
        &self,
        page_index: usize,
        name: &str,
    ) -> Result<Option<ColorSpaceModel>> {
        match self
            .page_resource(page_index, "ColorSpace", name)
            .map_err(|error| malformed(error.to_string()))?
        {
            Some(value) => self.color_space(&value).map(Some),
            None => Ok(None),
        }
    }

    /// 解析页面资源字典的 `/DefaultGray`、`/DefaultRGB` 或 `/DefaultCMYK`。
    ///
    /// 页面、资源或默认项不存在时返回 `None`（默认项是可选条目）。
    pub fn page_default_color_space(
        &self,
        page_index: usize,
        kind: DefaultColorSpace,
    ) -> Result<Option<ColorSpaceModel>> {
        let resources = match self.locate_page_resources(page_index) {
            Ok(Some(resources)) => resources,
            Ok(None) => return Ok(None),
            Err(error) => return Err(malformed(error.to_string())),
        };
        let Some(value) = resources.get(kind.resource_key()) else {
            return Ok(None);
        };
        self.color_space(value).map(Some)
    }

    fn color_space_depth(&self, value: &Value, depth: usize) -> Result<ColorSpaceModel> {
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

    /// 解一层间接引用；按对象号取最新修订，引用对象缺失报类型化错误。
    fn resolve_one(&self, value: &Value) -> Result<Value> {
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

    fn dict_item(&self, items: &[Value], index: usize, what: &str) -> Result<Dict> {
        let Some(value) = items.get(index) else {
            return Err(malformed(format!(
                "{what} color space is missing its dictionary"
            )));
        };
        self.resolve_one(value)?
            .as_dict()
            .cloned()
            .ok_or_else(|| malformed(format!("{what} color space parameter is not a dictionary")))
    }

    fn real_array(
        &self,
        dict: &Dict,
        key: &str,
        len: usize,
        default: Option<&[f64]>,
    ) -> Result<Vec<f64>> {
        let Some(value) = dict.get(key) else {
            return match default {
                Some(values) => Ok(values.to_vec()),
                None => Err(malformed(format!("missing required key /{key}"))),
            };
        };
        let resolved = self.resolve_one(value)?;
        let array = resolved
            .as_array()
            .ok_or_else(|| malformed(format!("/{key} must be an array")))?;
        let mut out = Vec::with_capacity(array.0.len());
        for item in &array.0 {
            let number = match item {
                Value::Integer(i) => Some(*i as f64),
                Value::Float(f) => Some(*f),
                _ => None,
            }
            .ok_or_else(|| malformed(format!("/{key} has a non-numeric element")))?;
            out.push(number);
        }
        if out.len() != len {
            return Err(malformed(format!(
                "/{key} must contain {len} numbers, got {}",
                out.len()
            )));
        }
        Ok(out)
    }

    fn ordered_pairs(
        &self,
        dict: &Dict,
        key: &str,
        len: usize,
        default: &[f64],
    ) -> Result<Vec<(f64, f64)>> {
        let numbers = self.real_array(dict, key, len, Some(default))?;
        Ok(numbers
            .chunks(2)
            .map(|pair| {
                let (lo, hi) = (pair[0], pair[1]);
                if lo <= hi { (lo, hi) } else { (hi, lo) }
            })
            .collect())
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

#[cfg(test)]
mod tests;
