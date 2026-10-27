//! Shading 与 Pattern 读端模型（ISO 32000-1 §8.7）。
//!
//! 从 [`Document`] 解析 Shading 字典/流（Type 1–7，含网格位流的
//! `BitsPerCoordinate`/`BitsPerComponent`/`BitsPerFlag`/`Decode` 解码）、
//! PatternType 1 平铺图案（流 + 资源 + 无色基色空间）与 PatternType 2
//! 着色图案（内联字典、间接引用或命名 `/Shading` 资源）。函数维数、
//! 坐标与定义域按规范校验，颜色空间复用
//! [`crate::core::reader::color_space`] 模型。
//!
//! Type 4 网格顶点按规范保留原始记录（标志 + 坐标 + 分量），不做
//! “复用前序顶点”的三角形展开——展开属几何合成，由调用方按标志
//! 语义完成。非法结构返回类型化 [`ShadingError`]。

use crate::core::functions::Function;
use crate::core::reader::color_space::{ColorSpaceError, ColorSpaceModel};
use crate::core::types::{Dict, Document, Value};

/// Shading/Pattern 解析失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum ShadingError {
    /// 字典、流数据或位流结构非法（缺键、类型错误、长度或维数不符）。
    Malformed(String),
    /// 暂不支持的着色/图案类型（如 PatternType 6/7 之外的扩展）。
    Unsupported(String),
    /// 颜色函数构造、维数校验或求值失败。
    Function(String),
}

impl std::fmt::Display for ShadingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "malformed shading: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported shading: {message}"),
            Self::Function(message) => write!(f, "shading function error: {message}"),
        }
    }
}

impl std::error::Error for ShadingError {}

type Result<T> = std::result::Result<T, ShadingError>;

fn malformed(message: impl Into<String>) -> ShadingError {
    ShadingError::Malformed(message.into())
}

fn unsupported(message: impl Into<String>) -> ShadingError {
    ShadingError::Unsupported(message.into())
}

fn from_color_space(error: ColorSpaceError) -> ShadingError {
    match error {
        ColorSpaceError::Malformed(message) => malformed(message),
        ColorSpaceError::Unsupported(message) => unsupported(message),
        ColorSpaceError::Function(message) => ShadingError::Function(message),
    }
}

/// 网格位流记录总数上限，防止损坏输入造成内存膨胀。
const MAX_MESH_RECORDS: usize = 1_000_000;
/// 平铺图案内容流字节上限（与文本提取/页面内容一致）。
const MAX_PATTERN_CONTENT: usize = 64 * 1024 * 1024;

/// 网格顶点记录：标志位、坐标与颜色分量（带 `/Function` 时为单个参数变量）。
#[derive(Debug, Clone, PartialEq)]
pub struct MeshVertex {
    pub flag: u8,
    pub x: f64,
    pub y: f64,
    pub components: Vec<f64>,
}

/// 网格渐变的可选 `/Function`（单函数或按分量的函数数组）。
#[derive(Debug, Clone, PartialEq)]
pub enum MeshFunction {
    /// 单个 1 输入、n 输出函数。
    Single(Function),
    /// n 个 1 输入 1 输出函数，按颜色分量排列。
    Components(Vec<Function>),
}

/// 渐变共有可选条目（Table 72 末三行）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShadingOptions {
    pub background: Option<Vec<f64>>,
    pub bbox: Option<[f64; 4]>,
    pub anti_alias: Option<bool>,
}

/// Shading Type 1：函数渐变。
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionShading {
    pub color_space: ColorSpaceModel,
    pub function: Function,
    pub matrix: Option<[f64; 6]>,
    pub options: ShadingOptions,
}

/// Shading Type 2：轴向渐变。
#[derive(Debug, Clone, PartialEq)]
pub struct AxialShading {
    pub color_space: ColorSpaceModel,
    pub function: Function,
    pub coords: [f64; 4],
    /// `/Domain`（缺省 `[(0, 1)]`）。
    pub domain: Vec<(f64, f64)>,
    pub extend: [bool; 2],
    pub options: ShadingOptions,
}

/// Shading Type 3：径向渐变。
#[derive(Debug, Clone, PartialEq)]
pub struct RadialShading {
    pub color_space: ColorSpaceModel,
    pub function: Function,
    pub coords: [f64; 6],
    pub domain: Vec<(f64, f64)>,
    pub extend: [bool; 2],
    pub options: ShadingOptions,
}

/// Shading Type 4：自由形态三角网格（流）。
#[derive(Debug, Clone, PartialEq)]
pub struct FreeFormMeshShading {
    pub color_space: ColorSpaceModel,
    pub function: Option<MeshFunction>,
    pub bits_per_coordinate: usize,
    pub bits_per_components: usize,
    pub bits_per_flag: usize,
    pub decode: Vec<(f64, f64)>,
    /// 原始顶点记录（每 3 个一组构成一个三角形；标志 1/2 表示复用前序顶点）。
    pub vertices: Vec<MeshVertex>,
    pub options: ShadingOptions,
}

/// Shading Type 5：四边形条带网格（流）。
#[derive(Debug, Clone, PartialEq)]
pub struct LatticeMeshShading {
    pub color_space: ColorSpaceModel,
    pub function: Option<MeshFunction>,
    pub bits_per_coordinate: usize,
    pub bits_per_components: usize,
    pub vertices_per_row: usize,
    pub decode: Vec<(f64, f64)>,
    pub vertices: Vec<MeshVertex>,
    pub options: ShadingOptions,
}

/// Shading Type 6：Coons 补丁网格（流，每片 12 条记录）。
#[derive(Debug, Clone, PartialEq)]
pub struct CoonsPatchShading {
    pub color_space: ColorSpaceModel,
    pub function: Option<MeshFunction>,
    pub bits_per_coordinate: usize,
    pub bits_per_components: usize,
    pub bits_per_flag: usize,
    pub decode: Vec<(f64, f64)>,
    pub patches: Vec<[MeshVertex; 12]>,
    pub options: ShadingOptions,
}

/// Shading Type 7：张量积补丁网格（流，每片 16 条记录）。
#[derive(Debug, Clone, PartialEq)]
pub struct TensorProductPatchShading {
    pub color_space: ColorSpaceModel,
    pub function: Option<MeshFunction>,
    pub bits_per_coordinate: usize,
    pub bits_per_components: usize,
    pub bits_per_flag: usize,
    pub decode: Vec<(f64, f64)>,
    pub patches: Vec<[MeshVertex; 16]>,
    pub options: ShadingOptions,
}

/// 类型化 Shading 模型（Type 1–7）。
#[derive(Debug, Clone, PartialEq)]
pub enum ShadingModel {
    Function(FunctionShading),
    Axial(AxialShading),
    Radial(RadialShading),
    FreeFormMesh(FreeFormMeshShading),
    LatticeMesh(LatticeMeshShading),
    CoonsPatch(CoonsPatchShading),
    TensorProductPatch(TensorProductPatchShading),
}

impl ShadingModel {
    /// 着色类型号（1–7）。
    pub fn shading_type(&self) -> u8 {
        match self {
            Self::Function(_) => 1,
            Self::Axial(_) => 2,
            Self::Radial(_) => 3,
            Self::FreeFormMesh(_) => 4,
            Self::LatticeMesh(_) => 5,
            Self::CoonsPatch(_) => 6,
            Self::TensorProductPatch(_) => 7,
        }
    }

    /// 渐变颜色空间。
    pub fn color_space(&self) -> &ColorSpaceModel {
        match self {
            Self::Function(s) => &s.color_space,
            Self::Axial(s) => &s.color_space,
            Self::Radial(s) => &s.color_space,
            Self::FreeFormMesh(s) => &s.color_space,
            Self::LatticeMesh(s) => &s.color_space,
            Self::CoonsPatch(s) => &s.color_space,
            Self::TensorProductPatch(s) => &s.color_space,
        }
    }

    /// 共有可选条目。
    pub fn options(&self) -> &ShadingOptions {
        match self {
            Self::Function(s) => &s.options,
            Self::Axial(s) => &s.options,
            Self::Radial(s) => &s.options,
            Self::FreeFormMesh(s) => &s.options,
            Self::LatticeMesh(s) => &s.options,
            Self::CoonsPatch(s) => &s.options,
            Self::TensorProductPatch(s) => &s.options,
        }
    }
}

/// 平铺图案 `/PaintType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintType {
    /// 1：图案内容流自设颜色。
    Colored,
    /// 2：无色，颜色取自 `scn`/`SCN` 分量。
    Uncolored,
}

/// 平铺图案 `/TilingType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TilingType {
    /// 1：固定间距。
    ConstantSpacing,
    /// 2：非自适应可变间距。
    NonAdaptiveVariableSpacing,
    /// 3：自适应可变间距。
    AdaptiveVariableSpacing,
}

/// PatternType 1：平铺图案（流对象 + 图块内容）。
#[derive(Debug, Clone, PartialEq)]
pub struct TilingPattern {
    pub paint_type: PaintType,
    pub tiling_type: TilingType,
    pub bbox: [f64; 4],
    pub x_step: f64,
    pub y_step: f64,
    pub matrix: Option<[f64; 6]>,
    /// 图块自有 `/Resources`（原样保留的字典）。
    pub resources: Option<Dict>,
    /// 无色图案（PaintType 2）的基颜色空间；彩色图案为 `None`。
    pub color_space: Option<ColorSpaceModel>,
    /// 解码后的图块内容流字节。
    pub content: Vec<u8>,
}

/// PatternType 2：着色图案（字典）。
#[derive(Debug, Clone, PartialEq)]
pub struct ShadingPattern {
    pub matrix: Option<[f64; 6]>,
    pub shading: ShadingModel,
}

/// 类型化 Pattern 模型。
#[derive(Debug, Clone, PartialEq)]
pub enum PatternModel {
    Tiling(TilingPattern),
    Shading(ShadingPattern),
}

impl Document {
    /// 从 Shading 资源值（字典、流或间接引用）构造类型化模型。
    pub fn shading(&self, value: &Value) -> Result<ShadingModel> {
        self.shading_model(value)
    }

    /// 解析页面 `/Resources/Shading` 中的命名着色；资源不存在返回 `None`。
    pub fn page_shading(&self, page_index: usize, name: &str) -> Result<Option<ShadingModel>> {
        let value = self
            .page_resource(page_index, "Shading", name)
            .map_err(|error| malformed(error.to_string()))?;
        match value {
            Some(value) => self.shading_model(&value).map(Some),
            None => Ok(None),
        }
    }

    /// 从 Pattern 资源值构造类型化模型。
    ///
    /// PatternType 2 的 `/Shading` 若为名称，需要资源字典上下文——请用
    /// [`Document::page_pattern`]，否则报 [`ShadingError::Malformed`]。
    pub fn pattern(&self, value: &Value) -> Result<PatternModel> {
        self.pattern_model(value, None)
    }

    /// 解析页面 `/Resources/Pattern` 中的命名图案；资源不存在返回 `None`。
    ///
    /// 页面资源字典同时用于解析 PatternType 2 中名称形式的 `/Shading`。
    pub fn page_pattern(&self, page_index: usize, name: &str) -> Result<Option<PatternModel>> {
        let value = self
            .page_resource(page_index, "Pattern", name)
            .map_err(|error| malformed(error.to_string()))?;
        match value {
            Some(value) => {
                let resources = self
                    .locate_page_resources(page_index)
                    .map_err(|error| malformed(error.to_string()))?;
                self.pattern_model(&value, resources.as_ref()).map(Some)
            }
            None => Ok(None),
        }
    }

    fn shading_model(&self, value: &Value) -> Result<ShadingModel> {
        let resolved = self.resolve(value)?;
        let (dict, data): (Dict, Option<Vec<u8>>) = match &resolved {
            Value::Dict(_) => (resolved.as_dict().cloned().expect("dict"), None),
            Value::Stream(_, _) => {
                let dict = resolved.as_dict().cloned().expect("stream dict");
                let bytes = self
                    .decode_stream_bytes(&resolved)
                    .map_err(|error| malformed(format!("shading stream: {error}")))?;
                (dict, Some(bytes))
            }
            other => {
                return Err(malformed(format!(
                    "shading must be a dictionary or stream, got {}",
                    other.type_name()
                )));
            }
        };
        let kind = dict
            .get("ShadingType")
            .and_then(Value::as_integer)
            .ok_or_else(|| malformed("missing required key /ShadingType"))?;
        match kind {
            1..=3 => {
                let model = self.parse_simple_shading(kind, &dict)?;
                Ok(model)
            }
            4..=7 => {
                let bytes = data.ok_or_else(|| {
                    malformed(format!("shading type {kind} requires stream data"))
                })?;
                self.parse_mesh_shading(kind, &dict, &bytes)
            }
            other => Err(unsupported(format!("unsupported /ShadingType {other}"))),
        }
    }

    fn parse_simple_shading(&self, kind: i64, dict: &Dict) -> Result<ShadingModel> {
        let color_space = self.shading_color_space(dict)?;
        let components = color_space
            .component_count()
            .ok_or_else(|| unsupported("shading color space has no fixed component count"))?;
        let function = self.shading_function(dict)?;
        let inputs = function.domain().len();
        let expected_inputs = if kind == 1 { 2 } else { 1 };
        if inputs != expected_inputs {
            return Err(malformed(format!(
                "shading type {kind} function must take {expected_inputs} input, got {inputs}"
            )));
        }
        if let Some(outputs) = function.output_count()
            && outputs != components
        {
            return Err(malformed(format!(
                "shading function produces {outputs} outputs, color space has {components} components"
            )));
        }
        let options = self.shading_options(dict, components)?;
        match kind {
            1 => {
                let matrix = self.optional_matrix(dict)?;
                Ok(ShadingModel::Function(FunctionShading {
                    color_space,
                    function,
                    matrix,
                    options,
                }))
            }
            2 => {
                let coords = self.required_real_array(dict, "Coords", 4)?;
                let coords: [f64; 4] = [coords[0], coords[1], coords[2], coords[3]];
                Ok(ShadingModel::Axial(AxialShading {
                    domain: self.shading_domain(dict)?,
                    extend: self.shading_extend(dict)?,
                    color_space,
                    function,
                    coords,
                    options,
                }))
            }
            _ => {
                let coords = self.required_real_array(dict, "Coords", 6)?;
                if coords[2] < 0.0 || coords[5] < 0.0 {
                    return Err(malformed("radial shading radii must be non-negative"));
                }
                let coords: [f64; 6] = [
                    coords[0], coords[1], coords[2], coords[3], coords[4], coords[5],
                ];
                Ok(ShadingModel::Radial(RadialShading {
                    domain: self.shading_domain(dict)?,
                    extend: self.shading_extend(dict)?,
                    color_space,
                    function,
                    coords,
                    options,
                }))
            }
        }
    }

    fn parse_mesh_shading(&self, kind: i64, dict: &Dict, data: &[u8]) -> Result<ShadingModel> {
        let color_space = self.shading_color_space(dict)?;
        let components = color_space
            .component_count()
            .ok_or_else(|| unsupported("shading color space has no fixed component count"))?;
        let indexed = matches!(color_space, ColorSpaceModel::Indexed(_));
        let function = match dict.get("Function") {
            None | Some(Value::Null) => None,
            Some(_) if indexed => {
                return Err(malformed(
                    "mesh /Function must not be combined with an Indexed color space",
                ));
            }
            Some(value) => Some(self.mesh_function(value, components)?),
        };
        let per_record = if function.is_some() { 1 } else { components };
        let bits_per_coordinate =
            self.bit_depth(dict, "BitsPerCoordinate", &[1, 2, 4, 8, 12, 16, 24, 32])?;
        let bits_per_components =
            self.bit_depth(dict, "BitsPerComponent", &[1, 2, 4, 8, 12, 16])?;
        let has_flag = kind == 4 || kind == 6 || kind == 7;
        let bits_per_flag = if has_flag {
            self.bit_depth(dict, "BitsPerFlag", &[2, 4, 8])?
        } else {
            0
        };
        let decode = match self.decode_pairs(dict, 2 + per_record)? {
            Some(pairs) => pairs,
            None if kind == 4 || kind == 5 => {
                return Err(malformed("shading types 4 and 5 require /Decode"));
            }
            None => vec![(0.0, 1.0); 2 + per_record],
        };
        let record_bits =
            bits_per_flag + 2 * bits_per_coordinate + per_record * bits_per_components;
        let total_bits = data.len() * 8;
        if total_bits / record_bits > MAX_MESH_RECORDS {
            return Err(malformed("mesh vertex data exceeds the record limit"));
        }
        let vertices = decode_mesh(
            data,
            record_bits,
            bits_per_flag,
            bits_per_coordinate,
            bits_per_components,
            &decode,
        )?;
        match kind {
            4 => {
                if vertices.len() % 3 != 0 {
                    return Err(malformed(
                        "type 4 mesh vertex records must come in groups of 3",
                    ));
                }
                Ok(ShadingModel::FreeFormMesh(FreeFormMeshShading {
                    color_space,
                    function,
                    bits_per_coordinate,
                    bits_per_components,
                    bits_per_flag,
                    decode,
                    vertices,
                    options: self.shading_options(dict, components)?,
                }))
            }
            5 => {
                let vertices_per_row = dict
                    .get("VerticesPerRow")
                    .and_then(Value::as_integer)
                    .ok_or_else(|| malformed("missing required key /VerticesPerRow"))?;
                if vertices_per_row < 2 {
                    return Err(malformed("/VerticesPerRow must be at least 2"));
                }
                let per_row = vertices_per_row as usize;
                if vertices.len() % per_row != 0 || vertices.is_empty() {
                    return Err(malformed(
                        "type 5 mesh vertex records must fill complete rows",
                    ));
                }
                Ok(ShadingModel::LatticeMesh(LatticeMeshShading {
                    color_space,
                    function,
                    bits_per_coordinate,
                    bits_per_components,
                    vertices_per_row: per_row,
                    decode,
                    vertices,
                    options: self.shading_options(dict, components)?,
                }))
            }
            6 => {
                let patches = group_patches(vertices, 12)?;
                Ok(ShadingModel::CoonsPatch(CoonsPatchShading {
                    color_space,
                    function,
                    bits_per_coordinate,
                    bits_per_components,
                    bits_per_flag,
                    decode,
                    patches,
                    options: self.shading_options(dict, components)?,
                }))
            }
            _ => {
                let patches = group_tensor_patches(vertices)?;
                Ok(ShadingModel::TensorProductPatch(
                    TensorProductPatchShading {
                        color_space,
                        function,
                        bits_per_coordinate,
                        bits_per_components,
                        bits_per_flag,
                        decode,
                        patches,
                        options: self.shading_options(dict, components)?,
                    },
                ))
            }
        }
    }

    fn pattern_model(&self, value: &Value, resources: Option<&Dict>) -> Result<PatternModel> {
        let resolved = self.resolve(value)?;
        let pattern_type = match &resolved {
            Value::Dict(_) | Value::Stream(_, _) => resolved
                .as_dict()
                .and_then(|dict| dict.get("PatternType"))
                .and_then(Value::as_integer)
                .unwrap_or(1),
            other => {
                return Err(malformed(format!(
                    "pattern must be a dictionary or stream, got {}",
                    other.type_name()
                )));
            }
        };
        match pattern_type {
            1 => self.tiling_pattern(&resolved).map(PatternModel::Tiling),
            2 => self
                .shading_pattern_dict(&resolved, resources)
                .map(PatternModel::Shading),
            other => Err(unsupported(format!("unsupported /PatternType {other}"))),
        }
    }

    fn tiling_pattern(&self, value: &Value) -> Result<TilingPattern> {
        let Value::Stream(_, _) = value else {
            return Err(malformed("tiling pattern (PatternType 1) must be a stream"));
        };
        let dict = value.as_dict().expect("stream dict");
        let paint_type = match dict.get("PaintType").and_then(Value::as_integer) {
            Some(1) => PaintType::Colored,
            Some(2) => PaintType::Uncolored,
            Some(other) => return Err(malformed(format!("invalid /PaintType {other}"))),
            None => return Err(malformed("missing required key /PaintType")),
        };
        let tiling_type = match dict.get("TilingType").and_then(Value::as_integer) {
            Some(1) => TilingType::ConstantSpacing,
            Some(2) => TilingType::NonAdaptiveVariableSpacing,
            Some(3) => TilingType::AdaptiveVariableSpacing,
            Some(other) => return Err(malformed(format!("invalid /TilingType {other}"))),
            None => return Err(malformed("missing required key /TilingType")),
        };
        let bbox_values = self.required_real_array(dict, "BBox", 4)?;
        let bbox = [
            bbox_values[0],
            bbox_values[1],
            bbox_values[2],
            bbox_values[3],
        ];
        let x_step = number(dict, "XStep")?;
        let y_step = number(dict, "YStep")?;
        if !x_step.is_finite() || x_step == 0.0 || !y_step.is_finite() || y_step == 0.0 {
            return Err(malformed("/XStep and /YStep must be finite and non-zero"));
        }
        let matrix = self.optional_matrix(dict)?;
        let resources = match dict.get("Resources") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                self.resolve(value)?
                    .as_dict()
                    .cloned()
                    .ok_or_else(|| malformed("/Resources must be a dictionary"))?,
            ),
        };
        let color_space = match dict.get("ColorSpace") {
            None | Some(Value::Null) => None,
            Some(value) => Some(self.color_space(value).map_err(from_color_space)?),
        };
        if paint_type == PaintType::Uncolored && color_space.is_none() {
            return Err(malformed(
                "uncolored tiling pattern (PaintType 2) requires /ColorSpace",
            ));
        }
        let content = self
            .decode_stream_bytes(value)
            .map_err(|error| malformed(format!("tiling pattern stream: {error}")))?;
        if content.len() > MAX_PATTERN_CONTENT {
            return Err(malformed("tiling pattern content exceeds the size limit"));
        }
        Ok(TilingPattern {
            paint_type,
            tiling_type,
            bbox,
            x_step,
            y_step,
            matrix,
            resources,
            color_space,
            content,
        })
    }

    fn shading_pattern_dict(
        &self,
        value: &Value,
        resources: Option<&Dict>,
    ) -> Result<ShadingPattern> {
        let dict = value
            .as_dict()
            .ok_or_else(|| malformed("shading pattern (PatternType 2) must be a dictionary"))?;
        let matrix = self.optional_matrix(dict)?;
        let shading_value = dict
            .get("Shading")
            .ok_or_else(|| malformed("missing required key /Shading"))?;
        let shading = match shading_value.as_name() {
            Some(name) => {
                let category = resources
                    .and_then(|resources| resources.get("Shading"))
                    .and_then(|value| match value {
                        Value::Dict(dict) => Some(dict),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        malformed("named /Shading requires a shading resource dictionary")
                    })?;
                let target = category
                    .get(name)
                    .ok_or_else(|| malformed(format!("shading resource /{name} missing")))?;
                self.shading_model(target)?
            }
            None => self.shading_model(shading_value)?,
        };
        Ok(ShadingPattern { matrix, shading })
    }

    fn shading_color_space(&self, dict: &Dict) -> Result<ColorSpaceModel> {
        let value = dict
            .get("ColorSpace")
            .ok_or_else(|| malformed("missing required key /ColorSpace"))?;
        let model = self.color_space(value).map_err(from_color_space)?;
        if matches!(model, ColorSpaceModel::Pattern { .. }) {
            return Err(unsupported("shading color space must not be Pattern"));
        }
        Ok(model)
    }

    fn shading_function(&self, dict: &Dict) -> Result<Function> {
        let value = dict
            .get("Function")
            .ok_or_else(|| malformed("missing required key /Function"))?;
        self.build_function(value)
    }

    fn mesh_function(&self, value: &Value, components: usize) -> Result<MeshFunction> {
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

    fn shading_domain(&self, dict: &Dict) -> Result<Vec<(f64, f64)>> {
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

    fn shading_extend(&self, dict: &Dict) -> Result<[bool; 2]> {
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

    fn shading_options(&self, dict: &Dict, components: usize) -> Result<ShadingOptions> {
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

    fn optional_matrix(&self, dict: &Dict) -> Result<Option<[f64; 6]>> {
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

    fn bit_depth(&self, dict: &Dict, key: &str, allowed: &[usize]) -> Result<usize> {
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

    fn decode_pairs(&self, dict: &Dict, expected: usize) -> Result<Option<Vec<(f64, f64)>>> {
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

    fn required_real_array(&self, dict: &Dict, key: &str, len: usize) -> Result<Vec<f64>> {
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

    fn resolve(&self, value: &Value) -> Result<Value> {
        match value {
            Value::Ref(num, generation) => self
                .get_object(*num)
                .map_err(|error| malformed(error.to_string()))?
                .filter(|object| object.generation == *generation)
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

fn number(dict: &Dict, key: &str) -> Result<f64> {
    let Some(value) = dict.get(key) else {
        return Err(malformed(format!("missing required key /{key}")));
    };
    number_value(value)
}

fn group_patches(vertices: Vec<MeshVertex>, per_patch: usize) -> Result<Vec<[MeshVertex; 12]>> {
    if vertices.is_empty() || !vertices.len().is_multiple_of(per_patch) {
        return Err(malformed(format!(
            "patch vertex records must come in complete groups of {per_patch}"
        )));
    }
    let mut out = Vec::new();
    for chunk in vertices.chunks(per_patch) {
        let array: [MeshVertex; 12] = chunk
            .to_vec()
            .try_into()
            .map_err(|_| malformed("internal patch grouping mismatch"))?;
        out.push(array);
    }
    Ok(out)
}

fn group_tensor_patches(vertices: Vec<MeshVertex>) -> Result<Vec<[MeshVertex; 16]>> {
    if vertices.is_empty() || !vertices.len().is_multiple_of(16) {
        return Err(malformed(
            "type 7 patch vertex records must come in complete groups of 16",
        ));
    }
    let mut out = Vec::new();
    for chunk in vertices.chunks(16) {
        let array: [MeshVertex; 16] = chunk
            .to_vec()
            .try_into()
            .map_err(|_| malformed("internal patch grouping mismatch"))?;
        out.push(array);
    }
    Ok(out)
}

/// 从位流按 MSB-first 读取 `bits` 位（≤32）。
fn read_bits(data: &[u8], offset: usize, bits: usize) -> u32 {
    let mut value = 0u32;
    for i in 0..bits {
        let bit = offset + i;
        let on = (data[bit / 8] >> (7 - bit % 8)) & 1 == 1;
        value = (value << 1) | on as u32;
    }
    value
}

fn map_code(code: u32, bits: usize, range: (f64, f64)) -> f64 {
    let max = if bits >= 32 {
        u32::MAX as f64
    } else {
        (1u64 << bits) as f64 - 1.0
    };
    let t = f64::from(code) / max;
    range.0 + (range.1 - range.0) * t
}

fn decode_mesh(
    data: &[u8],
    record_bits: usize,
    flag_bits: usize,
    coord_bits: usize,
    component_bits: usize,
    decode: &[(f64, f64)],
) -> Result<Vec<MeshVertex>> {
    let component_count = decode.len() - 2;
    let total_bits = data.len() * 8;
    let mut vertices = Vec::new();
    let mut pos = 0usize;
    while pos + record_bits <= total_bits {
        let flag = if flag_bits == 0 {
            0
        } else {
            read_bits(data, pos, flag_bits) as u8
        };
        pos += flag_bits;
        let x = map_code(read_bits(data, pos, coord_bits), coord_bits, decode[0]);
        pos += coord_bits;
        let y = map_code(read_bits(data, pos, coord_bits), coord_bits, decode[1]);
        pos += coord_bits;
        let mut components = Vec::with_capacity(component_count);
        for pair in &decode[2..] {
            components.push(map_code(
                read_bits(data, pos, component_bits),
                component_bits,
                *pair,
            ));
            pos += component_bits;
        }
        vertices.push(MeshVertex {
            flag,
            x,
            y,
            components,
        });
        if vertices.len() > MAX_MESH_RECORDS {
            return Err(malformed("mesh vertex data exceeds the record limit"));
        }
    }
    if vertices.is_empty() {
        return Err(malformed("mesh shading stream contains no vertex records"));
    }
    Ok(vertices)
}

#[cfg(test)]
mod tests;
