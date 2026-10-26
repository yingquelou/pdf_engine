//! 原始像素图片数据与色彩空间。

use super::function::Function;

/// 图片色彩空间。
///
/// 设备空间（DeviceGray/DeviceRGB/DeviceCMYK）以名字引用；CIE-Based
/// 空间按 ISO 32000-1 §8.6.5 写成 `[名称 字典]` 数组；ICCBased 写成
/// `[/ICCBased 流引用]`，配置文件作为独立流对象输出；Indexed 写成
/// `[/Indexed base hival 调色板]`；Separation 与 DeviceN（§8.6.6）
/// 写成 `[名称 … 备用空间 tint变换]`。
/// 作为 [`ImageData::color_space`] 的取值，决定每个像素的通道数。
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSpace {
    /// 设备灰度，每个像素 1 个样本。
    DeviceGray,
    /// 设备 RGB，每个像素 3 个样本。
    DeviceRGB,
    /// 设备 CMYK，每个像素 4 个样本。
    DeviceCMYK,
    /// 校准灰度（CalGray，Table 63）。
    CalGray(CalGray),
    /// 校准 RGB（CalRGB，Table 64）。
    CalRGB(CalRGB),
    /// CIE L*a*b*（Lab，Table 65）。
    Lab(Lab),
    /// ICC 配置文件（ICCBased，Table 66）。
    ICCBased(ICCBased),
    /// 索引色（Indexed，§8.6.6.3）：像素为调色板下标，每像素 1 个样本。
    Indexed(Indexed),
    /// 分色（Separation，§8.6.6.4）：单一 tint 分量，每像素 1 个样本。
    Separation(Separation),
    /// 多分量专色（DeviceN，§8.6.6.5）：每色料一个 tint 分量。
    DeviceN(DeviceN),
}

/// CalGray 颜色空间（Table 63）。
///
/// `white_point` 必填；`black_point` 与 `gamma` 省略时使用规范默认值
/// （`[0 0 0]` 与 `1.0`）。
#[derive(Debug, Clone, PartialEq)]
pub struct CalGray {
    /// CIE XYZ 白点，三个分量。
    pub white_point: [f64; 3],
    /// 黑点，省略时为 `[0 0 0]`。
    pub black_point: Option<[f64; 3]>,
    /// gamma 值，省略时为 `1.0`；必须为有限正数。
    pub gamma: Option<f64>,
}

impl CalGray {
    pub fn new(white_point: [f64; 3]) -> Self {
        Self {
            white_point,
            black_point: None,
            gamma: None,
        }
    }
}

/// CalRGB 颜色空间（Table 64）。
///
/// 省略的条目使用规范默认值：黑点 `[0 0 0]`、gamma `[1 1 1]`、
/// 矩阵为单位矩阵。
#[derive(Debug, Clone, PartialEq)]
pub struct CalRGB {
    /// CIE XYZ 白点，三个分量。
    pub white_point: [f64; 3],
    /// 黑点，省略时为 `[0 0 0]`。
    pub black_point: Option<[f64; 3]>,
    /// 三个分量的 gamma，省略时为 `[1 1 1]`；各分量必须为有限正数。
    pub gamma: Option<[f64; 3]>,
    /// 3×3 变换矩阵，按行展开 9 个数，省略时为单位矩阵。
    pub matrix: Option<[f64; 9]>,
}

impl CalRGB {
    pub fn new(white_point: [f64; 3]) -> Self {
        Self {
            white_point,
            black_point: None,
            gamma: None,
            matrix: None,
        }
    }
}

/// Lab 颜色空间（Table 65）。
///
/// 省略的条目使用规范默认值：黑点 `[0 0 0]`、范围
/// `[-100 100 -100 100]`。
#[derive(Debug, Clone, PartialEq)]
pub struct Lab {
    /// CIE XYZ 白点，三个分量。
    pub white_point: [f64; 3],
    /// 黑点，省略时为 `[0 0 0]`。
    pub black_point: Option<[f64; 3]>,
    /// `[a_min a_max b_min b_max]`，省略时为 `[-100 100 -100 100]`。
    pub range: Option<[f64; 4]>,
}

impl Lab {
    pub fn new(white_point: [f64; 3]) -> Self {
        Self {
            white_point,
            black_point: None,
            range: None,
        }
    }
}

/// ICCBased 颜色空间（Table 66）。
///
/// 配置文件字节作为独立流对象写出；`n` 必须为 1、3 或 4，并与图片
/// 像素样本的通道数一致。
#[derive(Debug, Clone, PartialEq)]
pub struct ICCBased {
    /// ICC 配置文件的颜色分量数：1（灰度）、3（RGB）或 4（CMYK）。
    pub n: u8,
    /// ICC 配置文件字节（不压缩写出）。
    pub data: Vec<u8>,
    /// 备用颜色空间，分量数必须与 `n` 相同。
    pub alternate: Option<Box<ColorSpace>>,
    /// 各分量的有效范围（2 × `n` 个数），省略时由规范隐含默认值。
    pub range: Option<Vec<f64>>,
}

impl ICCBased {
    pub fn new(n: u8, data: impl Into<Vec<u8>>) -> Self {
        Self {
            n,
            data: data.into(),
            alternate: None,
            range: None,
        }
    }
}

/// 索引色颜色空间（§8.6.6.3）。
///
/// 像素样本是调色板下标，范围为 `0..=hival`；调色板为按 `base`
/// 分量连续排列的字节序列，长度必须为 `(hival + 1) × base 通道数`。
#[derive(Debug, Clone, PartialEq)]
pub struct Indexed {
    /// 调色板条目的基础颜色空间，不能是另一个 Indexed。
    pub base: Box<ColorSpace>,
    /// 调色板最大下标，取值 `0..=255`。
    pub hival: u8,
    /// 调色板字节：每个条目按基础空间的通道顺序排列。
    pub lookup: Vec<u8>,
}

impl Indexed {
    pub fn new(base: ColorSpace, hival: u8, lookup: impl Into<Vec<u8>>) -> Self {
        Self {
            base: Box::new(base),
            hival,
            lookup: lookup.into(),
        }
    }
}

/// Separation 颜色空间（§8.6.6.4）。
///
/// 单一 tint 分量控制给定色料；`All` 与 `None` 是规范保留的特殊色料名。
/// `tint_transform` 必须接受 1 个输入，并输出备用颜色空间的通道数。
#[derive(Debug, Clone, PartialEq)]
pub struct Separation {
    /// 色料名称（非空；`All`/`None` 允许）。
    pub colorant: String,
    /// 备用颜色空间，须为设备或 CIE-Based 空间。
    pub alternate: Box<ColorSpace>,
    /// tint 变换函数。
    pub tint_transform: Function,
}

impl Separation {
    pub fn new(
        colorant: impl Into<String>,
        alternate: ColorSpace,
        tint_transform: Function,
    ) -> Self {
        Self {
            colorant: colorant.into(),
            alternate: Box::new(alternate),
            tint_transform,
        }
    }
}

/// DeviceN 颜色空间（§8.6.6.5）。
///
/// 分量数由 `colorants` 长度决定；除 `None` 外分量名不得重复，且不得
/// 使用保留名 `All`。`tint_transform` 必须接受 `colorants.len()` 个输入，
/// 并输出备用颜色空间的通道数。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceN {
    /// 色料名称数组，决定分量数（至少 1 个）。
    pub colorants: Vec<String>,
    /// 备用颜色空间，须为设备或 CIE-Based 空间。
    pub alternate: Box<ColorSpace>,
    /// tint 变换函数。
    pub tint_transform: Function,
}

impl DeviceN {
    pub fn new(
        colorants: impl Into<Vec<String>>,
        alternate: ColorSpace,
        tint_transform: Function,
    ) -> Self {
        Self {
            colorants: colorants.into(),
            alternate: Box::new(alternate),
            tint_transform,
        }
    }
}

impl ColorSpace {
    /// 色彩空间名称（数组形式的 CIE-Based 空间取其首项名称）。
    pub(crate) fn name(&self) -> &'static str {
        match self {
            ColorSpace::DeviceGray => "DeviceGray",
            ColorSpace::DeviceRGB => "DeviceRGB",
            ColorSpace::DeviceCMYK => "DeviceCMYK",
            ColorSpace::CalGray(_) => "CalGray",
            ColorSpace::CalRGB(_) => "CalRGB",
            ColorSpace::Lab(_) => "Lab",
            ColorSpace::ICCBased(_) => "ICCBased",
            ColorSpace::Indexed(_) => "Indexed",
            ColorSpace::Separation(_) => "Separation",
            ColorSpace::DeviceN(_) => "DeviceN",
        }
    }

    /// 设备空间的名字；CIE-Based 空间返回 `None`。
    pub(crate) fn device_name(&self) -> Option<&'static str> {
        match self {
            ColorSpace::DeviceGray => Some("DeviceGray"),
            ColorSpace::DeviceRGB => Some("DeviceRGB"),
            ColorSpace::DeviceCMYK => Some("DeviceCMYK"),
            _ => None,
        }
    }

    /// 每个像素的颜色分量数。
    pub(crate) fn channels(&self) -> usize {
        match self {
            ColorSpace::DeviceGray | ColorSpace::CalGray(_) => 1,
            ColorSpace::DeviceRGB | ColorSpace::CalRGB(_) | ColorSpace::Lab(_) => 3,
            ColorSpace::DeviceCMYK => 4,
            ColorSpace::ICCBased(icc) => icc.n as usize,
            ColorSpace::Indexed(_) => 1,
            ColorSpace::Separation(_) => 1,
            ColorSpace::DeviceN(devicen) => devicen.colorants.len(),
        }
    }

    /// 校验 CIE-Based 参数；设备空间恒通过。
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            ColorSpace::DeviceGray | ColorSpace::DeviceRGB | ColorSpace::DeviceCMYK => Ok(()),
            ColorSpace::CalGray(cal) => {
                validate_triplet("CalGray WhitePoint", &cal.white_point)?;
                if let Some(black_point) = cal.black_point {
                    validate_triplet("CalGray BlackPoint", &black_point)?;
                }
                if let Some(gamma) = cal.gamma
                    && (!gamma.is_finite() || gamma <= 0.0)
                {
                    return Err("CalGray Gamma must be a finite positive number".into());
                }
                Ok(())
            }
            ColorSpace::CalRGB(cal) => {
                validate_triplet("CalRGB WhitePoint", &cal.white_point)?;
                if let Some(black_point) = cal.black_point {
                    validate_triplet("CalRGB BlackPoint", &black_point)?;
                }
                if let Some(gamma) = cal.gamma
                    && gamma.iter().any(|g| !g.is_finite() || *g <= 0.0)
                {
                    return Err("CalRGB Gamma must contain 3 finite positive numbers".into());
                }
                if let Some(matrix) = cal.matrix
                    && matrix.iter().any(|v| !v.is_finite())
                {
                    return Err("CalRGB Matrix must contain 9 finite numbers".into());
                }
                Ok(())
            }
            ColorSpace::Lab(lab) => {
                validate_triplet("Lab WhitePoint", &lab.white_point)?;
                if let Some(black_point) = lab.black_point {
                    validate_triplet("Lab BlackPoint", &black_point)?;
                }
                if let Some(range) = lab.range {
                    if range.iter().any(|v| !v.is_finite()) {
                        return Err("Lab Range must contain 4 finite numbers".into());
                    }
                    if range[0] > range[1] || range[2] > range[3] {
                        return Err("Lab Range minimum must not exceed maximum".into());
                    }
                }
                Ok(())
            }
            ColorSpace::ICCBased(icc) => {
                if !matches!(icc.n, 1 | 3 | 4) {
                    return Err(format!("ICCBased N must be 1, 3 or 4, got {}", icc.n));
                }
                if icc.data.is_empty() {
                    return Err("ICCBased profile data must not be empty".into());
                }
                if let Some(range) = &icc.range {
                    let expected = icc.n as usize * 2;
                    if range.len() != expected {
                        return Err(format!(
                            "ICCBased Range must contain {expected} numbers, got {}",
                            range.len()
                        ));
                    }
                    if range.iter().any(|v| !v.is_finite()) {
                        return Err("ICCBased Range must contain finite numbers".into());
                    }
                    if range
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .any(|pair| pair[0] > pair[1])
                    {
                        return Err("ICCBased Range minimum must not exceed maximum".into());
                    }
                }
                if let Some(alternate) = &icc.alternate {
                    if matches!(**alternate, ColorSpace::ICCBased(_)) {
                        return Err(
                            "ICCBased Alternate must not be another ICCBased color space".into(),
                        );
                    }
                    if alternate.channels() != icc.n as usize {
                        return Err(format!(
                            "ICCBased Alternate has {} components, expected {}",
                            alternate.channels(),
                            icc.n
                        ));
                    }
                    alternate
                        .validate()
                        .map_err(|message| format!("ICCBased Alternate: {message}"))?;
                }
                Ok(())
            }
            ColorSpace::Indexed(indexed) => {
                if matches!(*indexed.base, ColorSpace::Indexed(_)) {
                    return Err("Indexed base must not be another Indexed color space".into());
                }
                indexed
                    .base
                    .validate()
                    .map_err(|message| format!("Indexed base: {message}"))?;
                let expected = (indexed.hival as usize + 1) * indexed.base.channels();
                if indexed.lookup.len() != expected {
                    return Err(format!(
                        "Indexed lookup must contain {expected} bytes for hival {} and a {}-component base, got {}",
                        indexed.hival,
                        indexed.base.channels(),
                        indexed.lookup.len()
                    ));
                }
                Ok(())
            }
            ColorSpace::Separation(separation) => {
                if separation.colorant.is_empty() {
                    return Err("Separation colorant must not be empty".into());
                }
                validate_alternate("Separation", &separation.alternate)?;
                validate_tint_transform(
                    "Separation",
                    &separation.tint_transform,
                    1,
                    separation.alternate.channels(),
                )
            }
            ColorSpace::DeviceN(devicen) => {
                if devicen.colorants.is_empty() {
                    return Err("DeviceN must have at least one colorant".into());
                }
                let mut seen = std::collections::BTreeSet::new();
                for colorant in &devicen.colorants {
                    if colorant.is_empty() {
                        return Err("DeviceN colorant names must not be empty".into());
                    }
                    if colorant == "All" {
                        return Err("DeviceN must not use the reserved colorant name All".into());
                    }
                    if colorant != "None" && !seen.insert(colorant.as_str()) {
                        return Err(format!(
                            "DeviceN colorant names must be distinct, got duplicate {colorant}"
                        ));
                    }
                }
                validate_alternate("DeviceN", &devicen.alternate)?;
                validate_tint_transform(
                    "DeviceN",
                    &devicen.tint_transform,
                    devicen.colorants.len(),
                    devicen.alternate.channels(),
                )
            }
        }
    }
}

/// Separation/DeviceN 的备用空间只能是设备或 CIE-Based 空间。
fn validate_alternate(role: &str, alternate: &ColorSpace) -> Result<(), String> {
    match alternate {
        ColorSpace::DeviceGray
        | ColorSpace::DeviceRGB
        | ColorSpace::DeviceCMYK
        | ColorSpace::CalGray(_)
        | ColorSpace::CalRGB(_)
        | ColorSpace::Lab(_)
        | ColorSpace::ICCBased(_) => alternate
            .validate()
            .map_err(|message| format!("{role} alternate color space: {message}")),
        ColorSpace::Indexed(_) | ColorSpace::Separation(_) | ColorSpace::DeviceN(_) => Err(
            format!("{role} alternate color space must be a device or CIE-based color space"),
        ),
    }
}

/// tint 变换的输入/输出维数必须与色料数和备用空间通道数一致。
fn validate_tint_transform(
    role: &str,
    transform: &Function,
    inputs: usize,
    outputs: usize,
) -> Result<(), String> {
    let actual_inputs = transform.input_count();
    if actual_inputs != inputs {
        return Err(format!(
            "{role} tint transform must take {inputs} input(s), got {actual_inputs}"
        ));
    }
    if let Some(actual_outputs) = transform.output_count()
        && actual_outputs != outputs
    {
        return Err(format!(
            "{role} tint transform must produce {outputs} output(s) for the alternate color space, got {actual_outputs}"
        ));
    }
    Ok(())
}

fn validate_triplet(name: &str, values: &[f64; 3]) -> Result<(), String> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(format!("{name} must contain 3 finite numbers"))
    }
}

/// 原始像素图片数据。
///
/// 流字节即未压缩的逐行像素样本，由 [`crate::core::builder::PdfBuilder::image`]
/// 消费并翻译为 XObject Image 流。关键字段见 [`ImageData::width`]、
/// [`ImageData::color_space`]、[`ImageData::data`]。
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    /// 图像宽度（像素）。
    pub width: u32,
    /// 图像高度（像素）。
    pub height: u32,
    /// 色彩空间（设备空间、CIE-Based/ICCBased 或 Indexed）。
    pub color_space: ColorSpace,
    /// 每个颜色分量的位数（`1`、`2`、`4`、`8` 或 `16`）。
    pub bits: u8,
    /// 原始像素字节，按行对齐排列：每行
    /// `ceil(width * 通道数 * bits / 8)` 字节，总长度为该值乘以 `height`。
    /// 构建阶段校验长度，不匹配时返回 `WriteError::InvalidObject`。
    pub data: Vec<u8>,
    /// `/Decode` 数组（Table 89）：每个颜色分量一对 `[min max]`，
    /// 必须为 `2 × 通道数` 个有限数；反向区间表示反相。省略时按位深
    /// 使用规范默认值。
    pub decode: Option<Vec<f64>>,
    /// `/Interpolate`：是否对图像做插值采样，省略即 false。
    pub interpolate: bool,
    /// 显式软蒙版（`/SMask`，PDF 1.4）：独立的 DeviceGray 图像，
    /// 尺寸必须与本图一致，且自身不得再带软蒙版。
    pub smask: Option<Box<ImageData>>,
    /// `/ImageMask`（§8.9.6.2）：为 true 时本图是 1 位模板蒙版，
    /// 样本决定用当前非描边颜色绘制还是遮罩；`color_space` 仅作
    /// `DeviceGray` 占位，不会写出。
    pub image_mask: bool,
    /// `/Mask`（PDF 1.3）：显式蒙版图像或颜色键范围。
    pub mask: Option<ImageMask>,
}

/// 图像 `/Mask` 条目（Table 89，PDF 1.3）。
#[derive(Debug, Clone, PartialEq)]
pub enum ImageMask {
    /// 显式蒙版：独立图像蒙版 XObject（`/ImageMask true`，1 位样本）。
    /// 按 §8.9.6.3，分辨率可与基图不同。
    Explicit(Box<ImageData>),
    /// 颜色键蒙版：`2 × n` 个整数范围 `[min1 max1 … minn maxn]`，
    /// 落在范围内的样本不绘制（§8.9.6.4）。
    ColorKey(Vec<i64>),
}

impl ImageMask {
    /// 用一张图像蒙版构造显式蒙版。
    pub fn explicit(image: ImageData) -> Self {
        Self::Explicit(Box::new(image))
    }

    /// 用 `2 × 通道数` 个样本值范围构造颜色键蒙版。
    pub fn color_key(ranges: impl Into<Vec<i64>>) -> Self {
        Self::ColorKey(ranges.into())
    }
}

impl ImageData {
    /// 设置 `/Decode` 数组。
    pub fn with_decode(mut self, decode: impl Into<Vec<f64>>) -> Self {
        self.decode = Some(decode.into());
        self
    }

    /// 设置 `/Interpolate`。
    pub fn with_interpolate(mut self, interpolate: bool) -> Self {
        self.interpolate = interpolate;
        self
    }

    /// 附加显式软蒙版图像。
    pub fn with_smask(mut self, smask: ImageData) -> Self {
        self.smask = Some(Box::new(smask));
        self
    }

    /// 把本图标记为图像蒙版（`/ImageMask true`）。
    ///
    /// 要求 `bits == 1` 且 `color_space` 为 `DeviceGray`；`/ColorSpace`
    /// 与 `/Mask` 不会写出，`/Decode` 只允许 `[0 1]` 或 `[1 0]`。
    pub fn with_image_mask(mut self) -> Self {
        self.image_mask = true;
        self
    }

    /// 附加 `/Mask` 条目（显式蒙版或颜色键蒙版）。
    pub fn with_mask(mut self, mask: ImageMask) -> Self {
        self.mask = Some(mask);
        self
    }

    /// 按行对齐计算的样本字节数；维度或位深非法时返回 `None`。
    pub(crate) fn expected_data_len(&self) -> Option<usize> {
        if self.width == 0 || self.height == 0 || !matches!(self.bits, 1 | 2 | 4 | 8 | 16) {
            return None;
        }
        let row_bits = (self.width as usize)
            .checked_mul(self.color_space.channels())?
            .checked_mul(self.bits as usize)?;
        row_bits.div_ceil(8).checked_mul(self.height as usize)
    }

    /// 校验颜色空间、样本长度、`/Decode`、软蒙版与图像蒙版；
    /// 非法时返回可读原因。
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.color_space
            .validate()
            .map_err(|message| format!("color space: {message}"))?;
        if self.image_mask {
            if !matches!(self.color_space, ColorSpace::DeviceGray) {
                return Err("image mask color space must be DeviceGray".into());
            }
            if self.bits != 1 {
                return Err(format!(
                    "image mask requires BitsPerComponent 1, got {}",
                    self.bits
                ));
            }
            if self.smask.is_some() {
                return Err("image mask must not have an SMask".into());
            }
            if self.mask.is_some() {
                return Err("image mask must not have a Mask entry".into());
            }
        }
        let expected = self.expected_data_len().ok_or_else(|| {
            format!(
                "invalid dimensions {}x{} or bits per component {}",
                self.width, self.height, self.bits
            )
        })?;
        if self.data.len() != expected {
            return Err(format!(
                "data length {} does not match expected {expected} for {} {}x{} at {} bits",
                self.data.len(),
                self.color_space.name(),
                self.width,
                self.height,
                self.bits
            ));
        }
        if let Some(decode) = &self.decode {
            let expected = self.color_space.channels() * 2;
            if decode.len() != expected {
                return Err(format!(
                    "Decode must contain {expected} numbers for a {}-component image, got {}",
                    self.color_space.channels(),
                    decode.len()
                ));
            }
            if decode.iter().any(|v| !v.is_finite()) {
                return Err("Decode must contain finite numbers".into());
            }
            if self.image_mask
                && !(decode.len() == 2
                    && ((decode[0] == 0.0 && decode[1] == 1.0)
                        || (decode[0] == 1.0 && decode[1] == 0.0)))
            {
                return Err("image mask Decode must be [0 1] or [1 0]".into());
            }
        }
        if let Some(smask) = &self.smask {
            if smask.width != self.width || smask.height != self.height {
                return Err(format!(
                    "SMask dimensions {}x{} do not match image {}x{}",
                    smask.width, smask.height, self.width, self.height
                ));
            }
            if !matches!(smask.color_space, ColorSpace::DeviceGray) {
                return Err("SMask color space must be DeviceGray".into());
            }
            if smask.smask.is_some() {
                return Err("SMask must not contain another SMask".into());
            }
            smask
                .validate()
                .map_err(|message| format!("SMask: {message}"))?;
        }
        match &self.mask {
            None => {}
            Some(ImageMask::Explicit(mask)) => {
                if !mask.image_mask {
                    return Err("Mask image must be an image mask (ImageMask true)".into());
                }
                mask.validate()
                    .map_err(|message| format!("Mask: {message}"))?;
            }
            Some(ImageMask::ColorKey(ranges)) => {
                let components = self.color_space.channels();
                let expected = components * 2;
                if ranges.len() != expected {
                    return Err(format!(
                        "color key Mask must contain {expected} integers for a \
                         {components}-component image, got {}",
                        ranges.len()
                    ));
                }
                let limit = (1i64 << self.bits) - 1;
                for pair in ranges.as_chunks::<2>().0 {
                    if pair[0] < 0 || pair[0] > pair[1] || pair[1] > limit {
                        return Err(format!(
                            "color key Mask ranges must satisfy 0 <= min <= max <= {limit}"
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
