//! 3D 视图字典（Table 304）与视图选择（`/3DV` `/DV`）。
//!
//! `/XN` 必填；`/MS` 缺失表示沿用 artwork 内的视图，`M` 要求恰好 12 个
//! 有限数的 `/C2W`，`U3D` 要求非空 `/U3DPath`。`/CO` 缺省 0、`/P` `/BG`
//! 缺省字典均不物化；`/RM` `/LS` `/SA` `/NA` 原样透传；`/O` 是 Form
//! XObject 流（须独立间接对象），本切片不支持。

use super::{ThreeDBackground, ThreeDMatrixSource, ThreeDProjection};
use crate::core::builder::types::text_string_value;
use crate::core::types::{Array, Dict, Value};

/// 3D 视图字典（Table 304）的构建态。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDView {
    external_name: String,
    internal_name: Option<String>,
    matrix_source: Option<ThreeDMatrixSource>,
    camera_to_world: Option<[f64; 12]>,
    u3d_path: Vec<String>,
    center_of_orbit: Option<f64>,
    projection: Option<ThreeDProjection>,
    background: Option<ThreeDBackground>,
    render_mode: Option<Value>,
    lighting: Option<Value>,
    cross_sections: Option<Value>,
    nodes: Option<Value>,
    restore_nodes: Option<bool>,
}

impl ThreeDView {
    /// 新建视图；`external_name` 为必填外部名 `/XN`。
    pub fn new(external_name: impl Into<String>) -> Self {
        Self {
            external_name: external_name.into(),
            internal_name: None,
            matrix_source: None,
            camera_to_world: None,
            u3d_path: Vec::new(),
            center_of_orbit: None,
            projection: None,
            background: None,
            render_mode: None,
            lighting: None,
            cross_sections: None,
            nodes: None,
            restore_nodes: None,
        }
    }

    /// 内部名 `/IN`（供 GoTo-3D-View 与按名视图选择引用）。
    pub fn with_internal_name(mut self, name: impl Into<String>) -> Self {
        self.internal_name = Some(name.into());
        self
    }

    /// 相机矩阵来源 `/MS`；缺省不写，表示使用 artwork 内的视图。
    pub fn with_matrix_source(mut self, source: ThreeDMatrixSource) -> Self {
        self.matrix_source = Some(source);
        self
    }

    /// 相机到世界矩阵 `/C2W`（12 个数，仅 `/MS /M` 时写出）。
    pub fn with_camera_to_world(mut self, matrix: [f64; 12]) -> Self {
        self.camera_to_world = Some(matrix);
        self
    }

    /// 视图节点路径 `/U3DPath`（非空文本串；仅 `/MS /U3D` 时写出）。
    pub fn with_u3d_path(mut self, path: Vec<String>) -> Self {
        self.u3d_path = path;
        self
    }

    /// 相机到轨道中心的 z 轴距离 `/CO`（非负；缺省 0 不写出）。
    pub fn with_center_of_orbit(mut self, distance: f64) -> Self {
        self.center_of_orbit = Some(distance);
        self
    }

    /// 投影字典 `/P`；缺省透视投影不写出。
    pub fn with_projection(mut self, projection: ThreeDProjection) -> Self {
        self.projection = Some(projection);
        self
    }

    /// 背景字典 `/BG`；缺省背景不写出。
    pub fn with_background(mut self, background: ThreeDBackground) -> Self {
        self.background = Some(background);
        self
    }

    /// 渲染模式字典 `/RM`（PDF 1.7，原样透传）。
    pub fn with_render_mode(mut self, render_mode: Value) -> Self {
        self.render_mode = Some(render_mode);
        self
    }

    /// 光照方案字典 `/LS`（PDF 1.7，原样透传）。
    pub fn with_lighting(mut self, lighting: Value) -> Self {
        self.lighting = Some(lighting);
        self
    }

    /// 截面字典数组 `/SA`（PDF 1.7，原样透传）。
    pub fn with_cross_sections(mut self, cross_sections: Value) -> Self {
        self.cross_sections = Some(cross_sections);
        self
    }

    /// 节点字典数组 `/NA`（PDF 1.7，原样透传）。
    pub fn with_nodes(mut self, nodes: Value) -> Self {
        self.nodes = Some(nodes);
        self
    }

    /// 应用 `/NA` 前是否恢复节点原状 `/NR`（PDF 1.7，缺省 false 不写出）。
    pub fn with_restore_nodes(mut self, restore: bool) -> Self {
        self.restore_nodes = Some(restore);
        self
    }

    /// 内部名（按名选择 `/3DV` 时与读端一致地比较）。
    pub(crate) fn internal_name(&self) -> Option<&str> {
        self.internal_name.as_deref()
    }

    /// 外部名 `/XN`（视图数组内的重名校验）。
    pub(crate) fn external_name(&self) -> &str {
        &self.external_name
    }

    /// 校验 Table 304 的必填项、矩阵来源依赖与数值范围。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.external_name.is_empty() {
            return Err("3D view /XN (external name) must not be empty".into());
        }
        match &self.matrix_source {
            None => {
                if self.camera_to_world.is_some() {
                    return Err("3D view /C2W requires /MS /M".into());
                }
                if !self.u3d_path.is_empty() {
                    return Err("3D view /U3DPath requires /MS /U3D".into());
                }
            }
            Some(ThreeDMatrixSource::M) => {
                let matrix = self
                    .camera_to_world
                    .as_ref()
                    .ok_or_else(|| "3D view /MS /M requires /C2W".to_string())?;
                if matrix.iter().any(|value| !value.is_finite()) {
                    return Err("3D view /C2W entries must be finite".into());
                }
                if !self.u3d_path.is_empty() {
                    return Err("3D view /U3DPath is meaningful only with /MS /U3D".into());
                }
            }
            Some(ThreeDMatrixSource::U3D) => {
                if self.u3d_path.is_empty() {
                    return Err("3D view /MS /U3D requires /U3DPath".into());
                }
                if self.u3d_path.iter().any(|segment| segment.is_empty()) {
                    return Err("3D view /U3DPath entries must not be empty".into());
                }
                if self.camera_to_world.is_some() {
                    return Err("3D view /C2W is meaningful only with /MS /M".into());
                }
            }
            Some(ThreeDMatrixSource::Other(name)) => {
                return Err(format!("3D view /MS {name:?} is not one of M, U3D"));
            }
        }
        if let Some(distance) = self.center_of_orbit
            && (!distance.is_finite() || distance < 0.0)
        {
            return Err("3D view /CO must be a finite nonnegative number".into());
        }
        if let Some(projection) = &self.projection {
            projection
                .validate()
                .map_err(|message| format!("3D view /P: {message}"))?;
        }
        if let Some(background) = &self.background {
            background
                .validate()
                .map_err(|message| format!("3D view /BG: {message}"))?;
        }
        Ok(())
    }

    /// 写成视图字典；缺省条目不物化。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("3DView".into()))
            .insert("XN", text_string_value(&self.external_name));
        if let Some(name) = &self.internal_name {
            dict = dict.insert("IN", text_string_value(name));
        }
        if let Some(source) = &self.matrix_source {
            let name = match source {
                ThreeDMatrixSource::M => "M",
                ThreeDMatrixSource::U3D => "U3D",
                ThreeDMatrixSource::Other(name) => name.as_str(),
            };
            dict = dict.insert("MS", Value::Name(name.to_string()));
        }
        if let Some(matrix) = &self.camera_to_world {
            let values = matrix.iter().map(|value| Value::Float(*value)).collect();
            dict = dict.insert("C2W", Value::Array(Array(values)));
        }
        if !self.u3d_path.is_empty() {
            let path = if self.u3d_path.len() == 1 {
                text_string_value(&self.u3d_path[0])
            } else {
                let values = self
                    .u3d_path
                    .iter()
                    .map(|segment| text_string_value(segment))
                    .collect();
                Value::Array(Array(values))
            };
            dict = dict.insert("U3DPath", path);
        }
        if let Some(distance) = self.center_of_orbit {
            dict = dict.insert("CO", Value::Float(distance));
        }
        if let Some(projection) = &self.projection {
            dict = dict.insert("P", Value::Dict(projection.to_dict()));
        }
        if let Some(background) = &self.background {
            dict = dict.insert("BG", Value::Dict(background.to_dict()));
        }
        for (key, value) in [
            ("RM", &self.render_mode),
            ("LS", &self.lighting),
            ("SA", &self.cross_sections),
            ("NA", &self.nodes),
        ] {
            if let Some(value) = value {
                dict = dict.insert(key, value.clone());
            }
        }
        if let Some(restore) = self.restore_nodes {
            dict = dict.insert("NR", Value::Bool(restore));
        }
        dict
    }
}

/// 视图选择（注解 `/3DV` 与流 `/DV`）。
#[derive(Debug, Clone, PartialEq)]
pub enum ThreeDViewSelection {
    /// 内联的 3D 视图字典。
    Inline(Box<ThreeDView>),
    /// `/VA` 数组下标（0 基）。
    Index(usize),
    /// 匹配某个视图 `/IN` 的文本串。
    Name(String),
    /// 名字 `F`：`/VA` 第一项。
    First,
    /// 名字 `L`：`/VA` 最后一项。
    Last,
}

impl ThreeDViewSelection {
    /// 校验选择目标存在；`context` 用于错误消息（`/3DV` 或 `/DV`）。
    pub(crate) fn validate(&self, views: &[ThreeDView], context: &str) -> Result<(), String> {
        match self {
            Self::Inline(view) => view
                .validate()
                .map_err(|message| format!("{context}: {message}")),
            Self::Index(index) if *index >= views.len() => Err(format!(
                "{context} index {index} is outside the view array ({} views)",
                views.len()
            )),
            Self::Index(_) => Ok(()),
            Self::Name(name)
                if !views
                    .iter()
                    .any(|view| view.internal_name() == Some(name.as_str())) =>
            {
                Err(format!(
                    "{context} name {name:?} does not match any view /IN"
                ))
            }
            Self::Name(_) => Ok(()),
            Self::First | Self::Last if views.is_empty() => Err(format!(
                "{context} First/Last selection requires a nonempty view array"
            )),
            Self::First | Self::Last => Ok(()),
        }
    }

    /// 校验并写成 `/3DV` 或 `/DV` 值。
    pub(crate) fn to_value(&self, views: &[ThreeDView], context: &str) -> Result<Value, String> {
        self.validate(views, context)?;
        Ok(match self {
            Self::Inline(view) => Value::Dict(view.to_dict()),
            Self::Index(index) => Value::Integer(*index as i64),
            Self::Name(name) => text_string_value(name),
            Self::First => Value::Name("F".into()),
            Self::Last => Value::Name("L".into()),
        })
    }
}
