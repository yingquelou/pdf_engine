//! 3D 视图字典（Table 304）与视图选择。
//!
//! `/XN` 必填文本串，`/IN` 可选；`/MS` 决定相机矩阵来源：`M` 要求
//! `/C2W` 恰为 12 个有限数，`U3D` 要求 `/U3DPath`（文本串或最多 64 段的
//! 文本串数组）。`/P` `/BG` 类型化为投影/背景字典；PDF 1.7 的 `/RM` `/LS`
//! `/SA` `/NA` 也类型化为渲染模式、光照方案、截面与节点模型，`/NR` 解析为
//! 可选布尔。
//!
//! 视图选择（注解 `/3DV` 与流 `/DV`）支持内联视图字典、`/VA` 整数下标、
//! 匹配 `/IN` 的文本串，以及名字 `F`/`L`；其他名字落 `Other`。

use crate::core::types::{Array, Dict, Document, Value};

use super::background::{ThreeDBackground, parse_background};
use super::cross_section::{ThreeDCrossSection, parse_cross_section};
use super::lighting::{ThreeDLightingScheme, parse_lighting_scheme};
use super::node::{ThreeDNode, parse_node};
use super::projection::{ThreeDProjection, parse_projection};
use super::render_mode::{ThreeDRenderMode, parse_render_mode};
use super::{
    MAX_CROSS_SECTIONS, MAX_NODES, MAX_PATH_SEGMENTS, ThreeDError, array_value, check_type,
    malformed, number_value, optional_bool, optional_name, optional_number, resolve_value,
    text_value, text_value_is_empty,
};

/// 相机到世界变换矩阵的来源（`/MS`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDMatrixSource {
    /// `M`：矩阵由 `/C2W` 显式给出。
    M,
    /// `U3D`：矩阵由 `/U3DPath` 选中的视图节点给出。
    U3D,
    /// 未识别的名字（原样保留）。
    Other(String),
}

/// 3D 视图字典（Table 304）。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDView {
    /// `/XN` 外部名称（必填文本串，原值保留）。
    pub external_name: Value,
    /// `/IN` 内部名称（文本串，供 GoTo-3D-View 等对象引用）。
    pub internal_name: Option<Value>,
    /// `/MS` 相机矩阵来源；缺失表示使用 artwork 内的视图。
    pub matrix_source: Option<ThreeDMatrixSource>,
    /// `/C2W` 12 元相机到世界变换矩阵（仅 `/MS /M` 时解析）。
    pub camera_to_world: Option<[f64; 12]>,
    /// `/U3DPath` 视图节点路径（仅 `/MS /U3D` 时解析）。
    pub u3d_path: Option<Vec<Value>>,
    /// `/CO` 相机到轨道中心的 z 轴距离（非负，仅 `/MS` 存在时有意义）。
    pub center_of_orbit: Option<f64>,
    /// `/P` 投影字典；`None` 表示缺省透视投影（FOV 90）。
    pub projection: Option<ThreeDProjection>,
    /// `/O` 覆盖用 Form XObject（仅 `/MS` 与 `/P` 都存在时有意义，原值保留）。
    pub overlay: Option<Value>,
    /// `/BG` 背景字典；`None` 表示全缺省背景。
    pub background: Option<ThreeDBackground>,
    /// `/RM` 渲染模式字典（PDF 1.7）。
    pub render_mode: Option<ThreeDRenderMode>,
    /// `/LS` 光照方案字典（PDF 1.7）。
    pub lighting: Option<ThreeDLightingScheme>,
    /// `/SA` 截面字典数组（PDF 1.7）；空数组合法。
    pub cross_sections: Option<Vec<ThreeDCrossSection>>,
    /// `/NA` 节点字典数组（PDF 1.7）；空数组合法。
    pub nodes: Option<Vec<ThreeDNode>>,
    /// `/NR` 应用 `/NA` 前是否恢复节点原状（PDF 1.7，缺省 false）。
    pub restore_nodes: Option<bool>,
}

impl ThreeDView {
    /// 把 `/C2W` 暴露为带 13.6.5 语义的矩阵。
    ///
    /// 解析阶段已校验 `/C2W` 为 12 个有限数，因此此处不重复校验。
    pub fn camera_to_world_matrix(&self) -> Option<ThreeDMatrix> {
        self.camera_to_world.map(ThreeDMatrix::from_entries)
    }
}

/// 3D 仿射变换矩阵（13.6.5）：`/C2W` 的 12 数表示及其坐标语义。
///
/// 12 个数按行展开 4×4 矩阵的前三列（最后一列恒为 `(0 0 0 1)`）：
///
/// ```text
/// a  b  c  0
/// d  e  f  0
/// g  h  i  0
/// tx ty tz 1
/// ```
///
/// 点按行向量左乘变换（13.6.5）：
/// `x' = a·x + d·y + g·z + tx`、`y' = b·x + e·y + h·z + ty`、
/// `z' = c·x + f·y + i·z + tz`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreeDMatrix {
    entries: [f64; 12],
}

impl ThreeDMatrix {
    /// 单位矩阵（恒等变换）。
    pub const IDENTITY: Self = Self {
        entries: [
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
        ],
    };

    /// 由 12 个数构造；任一项非有限时报 `Malformed`。
    pub fn new(entries: [f64; 12]) -> Result<Self, ThreeDError> {
        if !entries.iter().all(|entry| entry.is_finite()) {
            return Err(malformed("3D matrix entries must be finite"));
        }
        Ok(Self { entries })
    }

    /// 已由解析阶段校验有限的 12 个数，直接构造。
    fn from_entries(entries: [f64; 12]) -> Self {
        Self { entries }
    }

    /// 按 PDF 数组顺序返回 12 个数。
    pub fn entries(&self) -> [f64; 12] {
        self.entries
    }

    /// 以行向量约定变换一个点（13.6.5 的公式）。
    pub fn transform_point(&self, point: [f64; 3]) -> [f64; 3] {
        let [a, b, c, d, e, f, g, h, i, tx, ty, tz] = self.entries;
        let [x, y, z] = point;
        [
            a * x + d * y + g * z + tx,
            b * x + e * y + h * z + ty,
            c * x + f * y + i * z + tz,
        ]
    }

    /// 逆变换：13.6.5 步骤 b 的 `(cw)⁻¹`，把世界坐标映射回相机坐标。
    ///
    /// 线性部分行列式为 0（退化矩阵）或求逆结果溢出为无穷时报 `Malformed`，
    /// 不返回伪造值，也不 panic。
    pub fn inverse(&self) -> Result<Self, ThreeDError> {
        let [a, b, c, d, e, f, g, h, i, tx, ty, tz] = self.entries;
        let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        if det == 0.0 {
            return Err(malformed("3D matrix is singular and cannot be inverted"));
        }
        let linear = [
            (e * i - f * h) / det,
            (c * h - b * i) / det,
            (b * f - c * e) / det,
            (f * g - d * i) / det,
            (a * i - c * g) / det,
            (c * d - a * f) / det,
            (d * h - e * g) / det,
            (b * g - a * h) / det,
            (a * e - b * d) / det,
        ];
        let translation = [
            -(tx * linear[0] + ty * linear[3] + tz * linear[6]),
            -(tx * linear[1] + ty * linear[4] + tz * linear[7]),
            -(tx * linear[2] + ty * linear[5] + tz * linear[8]),
        ];
        let mut entries = [0.0; 12];
        entries[..9].copy_from_slice(&linear);
        entries[9..].copy_from_slice(&translation);
        Self::new(entries)
    }

    /// 组合变换：先应用 `self` 再应用 `next`（13.6.5 步骤 c 的 `aw × (cw)⁻¹`）。
    ///
    /// 结果非有限时报 `Malformed`。
    pub fn compose(&self, next: &Self) -> Result<Self, ThreeDError> {
        let left = self.entries;
        let right = next.entries;
        let mut entries = [0.0; 12];
        for row in 0..4 {
            for column in 0..3 {
                let mut sum = left[row * 3] * right[column]
                    + left[row * 3 + 1] * right[3 + column]
                    + left[row * 3 + 2] * right[6 + column];
                if row == 3 {
                    sum += right[9 + column];
                }
                entries[row * 3 + column] = sum;
            }
        }
        Self::new(entries)
    }
}

/// 视图选择（注解 `/3DV` 或流 `/DV`）。
#[derive(Debug, Clone, PartialEq)]
pub enum ThreeDViewSelection {
    /// 内联的 3D 视图字典。
    Inline(Box<ThreeDView>),
    /// `/VA` 数组下标（0 基）。
    Index(usize),
    /// 匹配某个视图 `/IN` 的文本串（原值保留）。
    Name(Value),
    /// 名字 `F`：`/VA` 第一项。
    First,
    /// 名字 `L`：`/VA` 最后一项。
    Last,
    /// 未识别的名字（原样保留）。
    Other(String),
}

/// 解析已解引用的 3D 视图字典。
///
/// 该入口同时服务于 3D 流 `/3DV` 与注解 `/ExData` 的 `/3DV`（13.6.6），
/// 因此对 crate 内可见。
pub(crate) fn parse_view(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<ThreeDView, ThreeDError> {
    check_type(dict, "3DView", context)?;
    let external_name = required_text(document, dict, "XN", context)?;
    let internal_name = optional_text(document, dict, "IN", context)?;
    let matrix_source =
        optional_name(document, dict, "MS", context)?.map(|name| match name.as_str() {
            "M" => ThreeDMatrixSource::M,
            "U3D" => ThreeDMatrixSource::U3D,
            _ => ThreeDMatrixSource::Other(name),
        });

    let camera_to_world = if matrix_source == Some(ThreeDMatrixSource::M) {
        let value = dict
            .get("C2W")
            .ok_or_else(|| malformed(format!("{context} /MS /M requires /C2W")))?;
        Some(parse_matrix(document, value, context)?)
    } else {
        None
    };
    let u3d_path = if matrix_source == Some(ThreeDMatrixSource::U3D) {
        let value = dict
            .get("U3DPath")
            .ok_or_else(|| malformed(format!("{context} /MS /U3D requires /U3DPath")))?;
        Some(parse_path(document, value, context)?)
    } else {
        None
    };

    let center_of_orbit = optional_number(document, dict, "CO", context)?;
    if let Some(center) = center_of_orbit
        && center < 0.0
    {
        return Err(malformed(format!("{context} /CO must not be negative")));
    }

    let projection = match dict.get("P") {
        None => None,
        Some(value) => {
            let resolved = resolve_value(document, value)?;
            let projection = resolved
                .as_dict()
                .ok_or_else(|| malformed(format!("{context} /P is not a dictionary")))?;
            Some(parse_projection(
                document,
                projection,
                &format!("{context} /P"),
            )?)
        }
    };
    let background = match dict.get("BG") {
        None => None,
        Some(value) => {
            let resolved = resolve_value(document, value)?;
            let background = resolved
                .as_dict()
                .ok_or_else(|| malformed(format!("{context} /BG is not a dictionary")))?;
            Some(parse_background(
                document,
                background,
                &format!("{context} /BG"),
            )?)
        }
    };
    let restore_nodes = optional_bool(document, dict, "NR", context)?;
    let render_mode = match dict.get("RM") {
        None => None,
        Some(value) => {
            let resolved = resolve_value(document, value)?;
            let Value::Dict(render_mode) = resolved else {
                return Err(malformed(format!("{context} /RM is not a dictionary")));
            };
            Some(parse_render_mode(
                document,
                &render_mode,
                &format!("{context} /RM"),
            )?)
        }
    };
    let lighting = match dict.get("LS") {
        None => None,
        Some(value) => {
            let resolved = resolve_value(document, value)?;
            let Value::Dict(lighting) = resolved else {
                return Err(malformed(format!("{context} /LS is not a dictionary")));
            };
            Some(parse_lighting_scheme(
                document,
                &lighting,
                &format!("{context} /LS"),
            )?)
        }
    };
    let cross_sections = match dict.get("SA") {
        None => None,
        Some(value) => Some(parse_cross_sections(
            document,
            value,
            &format!("{context} /SA"),
        )?),
    };
    let nodes = match dict.get("NA") {
        None => None,
        Some(value) => Some(parse_nodes(document, value, &format!("{context} /NA"))?),
    };

    Ok(ThreeDView {
        external_name,
        internal_name,
        matrix_source,
        camera_to_world,
        u3d_path,
        center_of_orbit,
        projection,
        overlay: dict.get("O").cloned(),
        background,
        render_mode,
        lighting,
        cross_sections,
        nodes,
        restore_nodes,
    })
}

/// `/SA`：截面字典数组；空数组合法，非字典条目报错。
fn parse_cross_sections(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Vec<ThreeDCrossSection>, ThreeDError> {
    let array = array_value(document, value, context)?;
    if array.len() > MAX_CROSS_SECTIONS {
        return Err(ThreeDError::LimitExceeded("3D view /SA"));
    }
    let mut out = Vec::with_capacity(array.len());
    for (index, item) in array.iter().enumerate() {
        let resolved = resolve_value(document, item)?;
        let Value::Dict(dict) = resolved else {
            return Err(malformed(format!("{context}[{index}] is not a dictionary")));
        };
        out.push(parse_cross_section(
            document,
            &dict,
            &format!("{context}[{index}]"),
        )?);
    }
    Ok(out)
}

/// `/NA`：节点字典数组；空数组合法，非字典条目报错。
fn parse_nodes(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Vec<ThreeDNode>, ThreeDError> {
    let array = array_value(document, value, context)?;
    if array.len() > MAX_NODES {
        return Err(ThreeDError::LimitExceeded("3D view /NA"));
    }
    let mut out = Vec::with_capacity(array.len());
    for (index, item) in array.iter().enumerate() {
        let resolved = resolve_value(document, item)?;
        let Value::Dict(dict) = resolved else {
            return Err(malformed(format!("{context}[{index}] is not a dictionary")));
        };
        out.push(parse_node(document, &dict, &format!("{context}[{index}]"))?);
    }
    Ok(out)
}

/// 解析视图选择；`view_count` 为 `Some(n)` 时表示流 `/VA` 存在且有 n 项。
pub(super) fn parse_view_selection(
    document: &Document,
    value: &Value,
    view_count: Option<usize>,
    context: &str,
) -> Result<ThreeDViewSelection, ThreeDError> {
    let resolved = resolve_value(document, value)?;
    match resolved {
        Value::Dict(dict) => Ok(ThreeDViewSelection::Inline(Box::new(parse_view(
            document, &dict, context,
        )?))),
        Value::Integer(index) => {
            let index = usize::try_from(index)
                .map_err(|_| malformed(format!("{context} index must be nonnegative")))?;
            if let Some(count) = view_count
                && index >= count
            {
                return Err(malformed(format!(
                    "{context} index {index} is outside the /VA array ({count} views)"
                )));
            }
            Ok(ThreeDViewSelection::Index(index))
        }
        // 文本串用于匹配 `/VA` 中某个视图的 `/IN`；匹配与否由调用方处理。
        Value::String(_) | Value::HexString(_) => Ok(ThreeDViewSelection::Name(resolved)),
        Value::Name(name) => Ok(match name.as_str() {
            "F" => ThreeDViewSelection::First,
            "L" => ThreeDViewSelection::Last,
            _ => ThreeDViewSelection::Other(name),
        }),
        _ => Err(malformed(format!(
            "{context} is not a view dictionary, index, text string or name"
        ))),
    }
}

/// 恰 12 个有限数值的 `/C2W` 矩阵。
fn parse_matrix(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<[f64; 12], ThreeDError> {
    let array = array_value(document, value, &format!("{context} /C2W"))?;
    if array.len() != 12 {
        return Err(malformed(format!(
            "{context} /C2W must hold exactly 12 numbers"
        )));
    }
    let mut out = [0.0; 12];
    for (index, item) in array.iter().enumerate() {
        let resolved = resolve_value(document, item)?;
        out[index] = number_value(&resolved, &format!("{context} /C2W entry"))?;
    }
    Ok(out)
}

/// `/U3DPath`：单个文本串或非空文本串数组（最多 64 段），统一存 `Vec`。
fn parse_path(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Vec<Value>, ThreeDError> {
    let resolved = resolve_value(document, value)?;
    let context = format!("{context} /U3DPath");
    if matches!(&resolved, Value::String(_) | Value::HexString(_)) {
        text_value(&resolved, &context)?;
        if text_value_is_empty(&resolved) {
            return Err(malformed(format!("{context} must not be empty")));
        }
        return Ok(vec![resolved]);
    }
    let Value::Array(Array(items)) = resolved else {
        return Err(malformed(format!(
            "{context} must be a text string or an array of text strings"
        )));
    };
    if items.is_empty() {
        return Err(malformed(format!(
            "{context} must contain at least one text string"
        )));
    }
    if items.len() > MAX_PATH_SEGMENTS {
        return Err(ThreeDError::LimitExceeded("3D view /U3DPath"));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in &items {
        let resolved = resolve_value(document, item)?;
        text_value(&resolved, &context)?;
        if text_value_is_empty(&resolved) {
            return Err(malformed(format!(
                "{context} entries must not be empty text strings"
            )));
        }
        out.push(resolved);
    }
    Ok(out)
}

fn required_text(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Value, ThreeDError> {
    let value = dict
        .get(key)
        .ok_or_else(|| malformed(format!("{context} is missing required /{key}")))?;
    let resolved = resolve_value(document, value)?;
    text_value(&resolved, &format!("{context} /{key}"))
}

fn optional_text(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<Value>, ThreeDError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let resolved = resolve_value(document, value)?;
    text_value(&resolved, &format!("{context} /{key}")).map(Some)
}
