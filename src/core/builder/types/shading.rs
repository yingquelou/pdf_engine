//! 渐变（Shading）与着色图案（Shading Pattern）：ISO 32000-1 §8.7.4.2、
//! §8.7.4.3 与 §8.7.2。
//!
//! 支持 Type 1（函数式）、Type 2（轴向）、Type 3（径向）、Type 4（自由
//! 三角网格）、Type 5（栅格三角网格）、Type 6（Coons 网格）与 Type 7
//! （张量积网格）渐变字典，以及引用渐变的 PatternType 2 图案字典。
//! 网格渐变暂不支持可选的 `/Function`；`/Background`、`/BBox` 与
//! `/AntiAlias` 尚未建模。

use super::function::Function;
use super::image::ColorSpace;

fn ensure_finite(values: &[f64], role: &str) -> Result<(), String> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(format!("{role} must contain finite numbers"));
    }
    Ok(())
}

fn validate_domain(domain: [f64; 2], role: &str) -> Result<(), String> {
    ensure_finite(&domain, role)?;
    if domain[0] >= domain[1] {
        return Err(format!("{role} minimum must be less than maximum"));
    }
    Ok(())
}

/// 校验渐变颜色函数：输入维数与颜色空间分量数必须匹配。
fn validate_color_function(
    role: &str,
    color_space: &ColorSpace,
    function: &Function,
    inputs: usize,
) -> Result<(), String> {
    color_space
        .validate()
        .map_err(|message| format!("{role} color space: {message}"))?;
    let actual_inputs = function.input_count();
    if actual_inputs != inputs {
        return Err(format!(
            "{role} function must take {inputs} input(s), got {actual_inputs}"
        ));
    }
    let channels = color_space.channels();
    if let Some(outputs) = function.output_count()
        && outputs != channels
    {
        return Err(format!(
            "{role} function must produce {channels} output(s) for its color space, got {outputs}"
        ));
    }
    Ok(())
}

/// Type 1 函数式渐变：以二维域上的函数直接给出颜色（`/ShadingType 1`）。
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionShading {
    pub color_space: ColorSpace,
    /// `[xmin xmax ymin ymax]`。
    pub domain: [f64; 4],
    /// 从渐变空间到用户空间的变换矩阵。
    pub matrix: Option<[f64; 6]>,
    pub function: Function,
}

impl FunctionShading {
    pub fn new(color_space: ColorSpace, domain: [f64; 4], function: Function) -> Self {
        Self {
            color_space,
            domain,
            matrix: None,
            function,
        }
    }

    pub fn with_matrix(mut self, matrix: [f64; 6]) -> Self {
        self.matrix = Some(matrix);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        ensure_finite(&self.domain, "type 1 shading Domain")?;
        if self.domain[0] >= self.domain[1] || self.domain[2] >= self.domain[3] {
            return Err("type 1 shading Domain minimum must be less than maximum".into());
        }
        if let Some(matrix) = self.matrix {
            ensure_finite(&matrix, "type 1 shading Matrix")?;
        }
        validate_color_function("type 1 shading", &self.color_space, &self.function, 2)
    }
}

/// Type 2 轴向渐变：沿两点连线插值（`/ShadingType 2`）。
#[derive(Debug, Clone, PartialEq)]
pub struct AxialShading {
    pub color_space: ColorSpace,
    /// `[x0 y0 x1 y1]`。
    pub coords: [f64; 4],
    /// 参数变量域；省略时按规范默认 `[0 1]`。
    pub domain: Option<[f64; 2]>,
    pub function: Function,
    /// 两端点之外的延伸行为；省略时默认 `[false false]`。
    pub extend: Option<[bool; 2]>,
}

impl AxialShading {
    pub fn new(color_space: ColorSpace, coords: [f64; 4], function: Function) -> Self {
        Self {
            color_space,
            coords,
            domain: None,
            function,
            extend: None,
        }
    }

    pub fn with_domain(mut self, domain: [f64; 2]) -> Self {
        self.domain = Some(domain);
        self
    }

    pub fn with_extend(mut self, extend: [bool; 2]) -> Self {
        self.extend = Some(extend);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        ensure_finite(&self.coords, "type 2 shading Coords")?;
        if let Some(domain) = self.domain {
            validate_domain(domain, "type 2 shading Domain")?;
        }
        validate_color_function("type 2 shading", &self.color_space, &self.function, 1)
    }
}

/// Type 3 径向渐变：两圆之间的径向插值（`/ShadingType 3`）。
#[derive(Debug, Clone, PartialEq)]
pub struct RadialShading {
    pub color_space: ColorSpace,
    /// `[x0 y0 r0 x1 y1 r1]`，半径不得为负。
    pub coords: [f64; 6],
    /// 参数变量域；省略时按规范默认 `[0 1]`。
    pub domain: Option<[f64; 2]>,
    pub function: Function,
    /// 两圆之外的延伸行为；省略时默认 `[false false]`。
    pub extend: Option<[bool; 2]>,
}

impl RadialShading {
    pub fn new(color_space: ColorSpace, coords: [f64; 6], function: Function) -> Self {
        Self {
            color_space,
            coords,
            domain: None,
            function,
            extend: None,
        }
    }

    pub fn with_domain(mut self, domain: [f64; 2]) -> Self {
        self.domain = Some(domain);
        self
    }

    pub fn with_extend(mut self, extend: [bool; 2]) -> Self {
        self.extend = Some(extend);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        ensure_finite(&self.coords, "type 3 shading Coords")?;
        if self.coords[2] < 0.0 || self.coords[5] < 0.0 {
            return Err("type 3 shading radii must not be negative".into());
        }
        if let Some(domain) = self.domain {
            validate_domain(domain, "type 3 shading Domain")?;
        }
        validate_color_function("type 3 shading", &self.color_space, &self.function, 1)
    }
}

/// 网格顶点：用户空间坐标与颜色分量。
///
/// `color` 按颜色空间分量顺序排列；多数空间的取值范围为 `0..1`，
/// Lab 为 `[0 100]` 与 `/Range` 指定的 a/b 范围，Indexed 为调色板下标。
#[derive(Debug, Clone, PartialEq)]
pub struct MeshVertex {
    pub x: f64,
    pub y: f64,
    pub color: Vec<f64>,
}

impl MeshVertex {
    pub fn new(x: f64, y: f64, color: impl Into<Vec<f64>>) -> Self {
        Self {
            x,
            y,
            color: color.into(),
        }
    }
}

/// 网格颜色分量的解码范围：对应 `/Decode` 中每个分量的 `[min max]`。
pub(crate) fn mesh_color_ranges(color_space: &ColorSpace) -> Vec<[f64; 2]> {
    match color_space {
        ColorSpace::Lab(lab) => {
            let [a_min, a_max, b_min, b_max] = lab.range.unwrap_or([-100.0, 100.0, -100.0, 100.0]);
            vec![[0.0, 100.0], [a_min, a_max], [b_min, b_max]]
        }
        ColorSpace::Indexed(indexed) => vec![[0.0, indexed.hival as f64]],
        ColorSpace::ICCBased(icc) => match &icc.range {
            Some(range) if range.len() == icc.n as usize * 2 => (0..icc.n as usize)
                .map(|i| [range[i * 2], range[i * 2 + 1]])
                .collect(),
            _ => vec![[0.0, 1.0]; icc.n as usize],
        },
        _ => vec![[0.0, 1.0]; color_space.channels()],
    }
}

/// 校验网格渐变的公共参数：位深、顶点坐标与颜色分量。
fn validate_mesh(
    role: &str,
    color_space: &ColorSpace,
    vertices: &[MeshVertex],
    decode: Option<[f64; 4]>,
    bits_per_coordinate: u8,
    bits_per_component: u8,
) -> Result<(), String> {
    color_space
        .validate()
        .map_err(|message| format!("{role} color space: {message}"))?;
    if !(1..=16).contains(&bits_per_coordinate) {
        return Err(format!("{role} BitsPerCoordinate must be between 1 and 16"));
    }
    if !(1..=16).contains(&bits_per_component) {
        return Err(format!("{role} BitsPerComponent must be between 1 and 16"));
    }
    if vertices.is_empty() {
        return Err(format!("{role} requires at least one vertex"));
    }
    if let Some(decode) = decode {
        ensure_finite(&decode, &format!("{role} Decode"))?;
        if decode[0] >= decode[1] || decode[2] >= decode[3] {
            return Err(format!("{role} Decode minimum must be less than maximum"));
        }
    }
    let ranges = mesh_color_ranges(color_space);
    for vertex in vertices {
        if !vertex.x.is_finite() || !vertex.y.is_finite() {
            return Err(format!("{role} vertex coordinates must be finite"));
        }
        if vertex.color.len() != ranges.len() {
            return Err(format!(
                "{role} vertex must have {} colour component(s), got {}",
                ranges.len(),
                vertex.color.len()
            ));
        }
        for (value, [min, max]) in vertex.color.iter().zip(&ranges) {
            if !value.is_finite() || *value < *min || *value > *max {
                return Err(format!(
                    "{role} colour component {value} is outside [{min}, {max}]"
                ));
            }
        }
    }
    Ok(())
}

/// Type 4 自由三角网格（Gouraud 着色，`/ShadingType 4`）。
///
/// 顶点按三角形分组（每 3 个一组）。每个顶点前写出 `BitsPerFlag` 位
/// 标志；本实现一律写 0，即每个三角形都独立给出三个顶点，不复用
/// 相邻三角形的共享边。
#[derive(Debug, Clone, PartialEq)]
pub struct FreeFormMeshShading {
    pub color_space: ColorSpace,
    pub vertices: Vec<MeshVertex>,
    /// 坐标采样范围 `[xmin xmax ymin ymax]`；省略时取顶点紧包围盒。
    pub decode: Option<[f64; 4]>,
    pub bits_per_coordinate: u8,
    pub bits_per_component: u8,
    pub bits_per_flag: u8,
}

impl FreeFormMeshShading {
    pub fn new(color_space: ColorSpace, vertices: Vec<MeshVertex>) -> Self {
        Self {
            color_space,
            vertices,
            decode: None,
            bits_per_coordinate: 16,
            bits_per_component: 8,
            bits_per_flag: 8,
        }
    }

    pub fn with_decode(mut self, decode: [f64; 4]) -> Self {
        self.decode = Some(decode);
        self
    }

    pub fn with_bits(mut self, coordinate: u8, component: u8, flag: u8) -> Self {
        self.bits_per_coordinate = coordinate;
        self.bits_per_component = component;
        self.bits_per_flag = flag;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.vertices.len().is_multiple_of(3) {
            return Err("type 4 shading vertices must come in groups of 3".into());
        }
        if !matches!(self.bits_per_flag, 2 | 4 | 8) {
            return Err("type 4 shading BitsPerFlag must be 2, 4 or 8".into());
        }
        validate_mesh(
            "type 4 shading",
            &self.color_space,
            &self.vertices,
            self.decode,
            self.bits_per_coordinate,
            self.bits_per_component,
        )
    }
}

/// Type 5 栅格三角网格（`/ShadingType 5`）：顶点按等长行排列。
///
/// 每行顶点数由 `vertices_per_row` 指定（至少 2），行数由顶点总数决定
/// （至少 2）；相邻两行之间按规范连线形成三角网格。
#[derive(Debug, Clone, PartialEq)]
pub struct LatticeMeshShading {
    pub color_space: ColorSpace,
    pub vertices_per_row: u16,
    /// 按行优先顺序展开的顶点。
    pub vertices: Vec<MeshVertex>,
    /// 坐标采样范围 `[xmin xmax ymin ymax]`；省略时取顶点紧包围盒。
    pub decode: Option<[f64; 4]>,
    pub bits_per_coordinate: u8,
    pub bits_per_component: u8,
}

impl LatticeMeshShading {
    pub fn new(color_space: ColorSpace, vertices_per_row: u16, vertices: Vec<MeshVertex>) -> Self {
        Self {
            color_space,
            vertices_per_row,
            vertices,
            decode: None,
            bits_per_coordinate: 16,
            bits_per_component: 8,
        }
    }

    pub fn with_decode(mut self, decode: [f64; 4]) -> Self {
        self.decode = Some(decode);
        self
    }

    pub fn with_bits(mut self, coordinate: u8, component: u8) -> Self {
        self.bits_per_coordinate = coordinate;
        self.bits_per_component = component;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.vertices_per_row < 2 {
            return Err("type 5 shading VerticesPerRow must be at least 2".into());
        }
        if !self
            .vertices
            .len()
            .is_multiple_of(self.vertices_per_row as usize)
        {
            return Err("type 5 shading vertices must fill whole rows".into());
        }
        let rows = self.vertices.len() / self.vertices_per_row as usize;
        if rows < 2 {
            return Err("type 5 shading requires at least two rows".into());
        }
        validate_mesh(
            "type 5 shading",
            &self.color_space,
            &self.vertices,
            self.decode,
            self.bits_per_coordinate,
            self.bits_per_component,
        )
    }
}

/// Coons 网格片：12 个控制点与 4 个角点颜色。
///
/// 控制点沿四条三次贝塞尔边界排列（`p0`–`p3`、`p3`–`p6`、`p6`–`p9`、
/// `p9`–`p0`）；颜色依次对应 `p0`、`p3`、`p6`、`p9`。
#[derive(Debug, Clone, PartialEq)]
pub struct CoonsPatch {
    pub points: [[f64; 2]; 12],
    pub colors: [Vec<f64>; 4],
}

impl CoonsPatch {
    pub fn new(points: [[f64; 2]; 12], colors: [Vec<f64>; 4]) -> Self {
        Self { points, colors }
    }
}

/// 张量积网格片：16 个控制点（4×4）与 4 个角点颜色。
///
/// 控制点按行优先排列；颜色依次对应 `p0`、`p3`、`p12`、`p15`。
#[derive(Debug, Clone, PartialEq)]
pub struct TensorProductPatch {
    pub points: [[f64; 2]; 16],
    pub colors: [Vec<f64>; 4],
}

impl TensorProductPatch {
    pub fn new(points: [[f64; 2]; 16], colors: [Vec<f64>; 4]) -> Self {
        Self { points, colors }
    }
}

fn validate_patch_common(
    role: &str,
    color_space: &ColorSpace,
    decode: Option<[f64; 4]>,
    bits_per_coordinate: u8,
    bits_per_component: u8,
) -> Result<(), String> {
    color_space
        .validate()
        .map_err(|message| format!("{role} color space: {message}"))?;
    if !(1..=16).contains(&bits_per_coordinate) {
        return Err(format!("{role} BitsPerCoordinate must be between 1 and 16"));
    }
    if !(1..=16).contains(&bits_per_component) {
        return Err(format!("{role} BitsPerComponent must be between 1 and 16"));
    }
    if let Some(decode) = decode {
        ensure_finite(&decode, &format!("{role} Decode"))?;
        if decode[0] >= decode[1] || decode[2] >= decode[3] {
            return Err(format!("{role} Decode minimum must be less than maximum"));
        }
    }
    Ok(())
}

fn validate_patch_points(role: &str, points: &[[f64; 2]]) -> Result<(), String> {
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return Err(format!("{role} control point coordinates must be finite"));
    }
    Ok(())
}

fn validate_patch_colors(
    role: &str,
    color_space: &ColorSpace,
    colors: &[Vec<f64>],
) -> Result<(), String> {
    let ranges = mesh_color_ranges(color_space);
    if colors.len() != 4 {
        return Err(format!(
            "{role} patch must have 4 corner colours, got {}",
            colors.len()
        ));
    }
    for color in colors {
        if color.len() != ranges.len() {
            return Err(format!(
                "{role} corner colour must have {} component(s), got {}",
                ranges.len(),
                color.len()
            ));
        }
        for (value, [min, max]) in color.iter().zip(&ranges) {
            if !value.is_finite() || *value < *min || *value > *max {
                return Err(format!(
                    "{role} colour component {value} is outside [{min}, {max}]"
                ));
            }
        }
    }
    Ok(())
}

/// Type 6 Coons 网格渐变（`/ShadingType 6`）。
///
/// 每个网格片独立写出（标志恒为 0，不复用相邻网格片的共享边）。
#[derive(Debug, Clone, PartialEq)]
pub struct CoonsPatchShading {
    pub color_space: ColorSpace,
    pub patches: Vec<CoonsPatch>,
    /// 坐标采样范围 `[xmin xmax ymin ymax]`；省略时取控制点紧包围盒。
    pub decode: Option<[f64; 4]>,
    pub bits_per_coordinate: u8,
    pub bits_per_component: u8,
    pub bits_per_flag: u8,
}

impl CoonsPatchShading {
    pub fn new(color_space: ColorSpace, patches: Vec<CoonsPatch>) -> Self {
        Self {
            color_space,
            patches,
            decode: None,
            bits_per_coordinate: 16,
            bits_per_component: 8,
            bits_per_flag: 8,
        }
    }

    pub fn with_decode(mut self, decode: [f64; 4]) -> Self {
        self.decode = Some(decode);
        self
    }

    pub fn with_bits(mut self, coordinate: u8, component: u8, flag: u8) -> Self {
        self.bits_per_coordinate = coordinate;
        self.bits_per_component = component;
        self.bits_per_flag = flag;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let role = "type 6 shading";
        if self.patches.is_empty() {
            return Err(format!("{role} requires at least one patch"));
        }
        if !matches!(self.bits_per_flag, 2 | 4 | 8) {
            return Err(format!("{role} BitsPerFlag must be 2, 4 or 8"));
        }
        validate_patch_common(
            role,
            &self.color_space,
            self.decode,
            self.bits_per_coordinate,
            self.bits_per_component,
        )?;
        for patch in &self.patches {
            validate_patch_points(role, &patch.points)?;
            validate_patch_colors(role, &self.color_space, &patch.colors)?;
        }
        Ok(())
    }
}

/// Type 7 张量积网格渐变（`/ShadingType 7`）。
///
/// 每个网格片独立写出（标志恒为 0，不复用相邻网格片的共享边）。
#[derive(Debug, Clone, PartialEq)]
pub struct TensorProductPatchShading {
    pub color_space: ColorSpace,
    pub patches: Vec<TensorProductPatch>,
    /// 坐标采样范围 `[xmin xmax ymin ymax]`；省略时取控制点紧包围盒。
    pub decode: Option<[f64; 4]>,
    pub bits_per_coordinate: u8,
    pub bits_per_component: u8,
    pub bits_per_flag: u8,
}

impl TensorProductPatchShading {
    pub fn new(color_space: ColorSpace, patches: Vec<TensorProductPatch>) -> Self {
        Self {
            color_space,
            patches,
            decode: None,
            bits_per_coordinate: 16,
            bits_per_component: 8,
            bits_per_flag: 8,
        }
    }

    pub fn with_decode(mut self, decode: [f64; 4]) -> Self {
        self.decode = Some(decode);
        self
    }

    pub fn with_bits(mut self, coordinate: u8, component: u8, flag: u8) -> Self {
        self.bits_per_coordinate = coordinate;
        self.bits_per_component = component;
        self.bits_per_flag = flag;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let role = "type 7 shading";
        if self.patches.is_empty() {
            return Err(format!("{role} requires at least one patch"));
        }
        if !matches!(self.bits_per_flag, 2 | 4 | 8) {
            return Err(format!("{role} BitsPerFlag must be 2, 4 or 8"));
        }
        validate_patch_common(
            role,
            &self.color_space,
            self.decode,
            self.bits_per_coordinate,
            self.bits_per_component,
        )?;
        for patch in &self.patches {
            validate_patch_points(role, &patch.points)?;
            validate_patch_colors(role, &self.color_space, &patch.colors)?;
        }
        Ok(())
    }
}

/// 渐变字典：Type 1–7 之一。
#[derive(Debug, Clone, PartialEq)]
pub enum Shading {
    Function(FunctionShading),
    Axial(AxialShading),
    Radial(RadialShading),
    FreeForm(FreeFormMeshShading),
    Lattice(LatticeMeshShading),
    Coons(CoonsPatchShading),
    TensorProduct(TensorProductPatchShading),
}

impl Shading {
    /// 渐变的 `/ShadingType` 编号。
    pub fn shading_type(&self) -> i64 {
        match self {
            Self::Function(_) => 1,
            Self::Axial(_) => 2,
            Self::Radial(_) => 3,
            Self::FreeForm(_) => 4,
            Self::Lattice(_) => 5,
            Self::Coons(_) => 6,
            Self::TensorProduct(_) => 7,
        }
    }

    pub(crate) fn color_space(&self) -> &ColorSpace {
        match self {
            Self::Function(shading) => &shading.color_space,
            Self::Axial(shading) => &shading.color_space,
            Self::Radial(shading) => &shading.color_space,
            Self::FreeForm(shading) => &shading.color_space,
            Self::Lattice(shading) => &shading.color_space,
            Self::Coons(shading) => &shading.color_space,
            Self::TensorProduct(shading) => &shading.color_space,
        }
    }

    /// 网格渐变（Type 4–7）只能写成流对象；其余类型是直接字典。
    pub(crate) fn is_stream(&self) -> bool {
        matches!(
            self,
            Self::FreeForm(_) | Self::Lattice(_) | Self::Coons(_) | Self::TensorProduct(_)
        )
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Function(shading) => shading.validate(),
            Self::Axial(shading) => shading.validate(),
            Self::Radial(shading) => shading.validate(),
            Self::FreeForm(shading) => shading.validate(),
            Self::Lattice(shading) => shading.validate(),
            Self::Coons(shading) => shading.validate(),
            Self::TensorProduct(shading) => shading.validate(),
        }
    }
}

/// PatternType 2 着色图案：以渐变填充图案空间。
#[derive(Debug, Clone, PartialEq)]
pub struct ShadingPattern {
    pub shading: Shading,
    /// 从图案空间到默认坐标系的变换矩阵。
    pub matrix: Option<[f64; 6]>,
}

impl ShadingPattern {
    pub fn new(shading: Shading) -> Self {
        Self {
            shading,
            matrix: None,
        }
    }

    pub fn with_matrix(mut self, matrix: [f64; 6]) -> Self {
        self.matrix = Some(matrix);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(matrix) = self.matrix {
            ensure_finite(&matrix, "shading pattern Matrix")?;
        }
        self.shading.validate()
    }
}
