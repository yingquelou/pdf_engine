//! Polygon/PolyLine 注解专有条目：构建态、构造方法与构建期校验（12.5.6.9，Table 178）。
//!
//! `/Vertices` 是两种子类型的必填条目；`/IT` 意图取值与子类型一一对应——
//! `PolygonCloud`（PDF 1.6）与 `PolygonDimension`（PDF 1.7）只属于 Polygon，
//! `PolyLineDimension`（PDF 1.7）只属于 PolyLine。
//! `/LE`（端点样式数组，仅 PolyLine 有意义）、`/BE`（边框效果）与 `/RD`
//! （矩形差值）是跨子类型共用键，归属矩阵分别在 `subtype.rs` 与
//! `free_text.rs` 中校验，此处不重复。

use super::{AnnotationKind, PdfAnnotation};
use crate::core::types::PdfVersion;

/// Polygon/PolyLine `/Vertices` 的坐标数值上限，与读端一致。
const MAX_VERTEX_VALUES: usize = 8192;

/// Polygon/PolyLine 注解意图（`/IT`，Table 178）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolygonIntent {
    /// 云状多边形（`/PolygonCloud`，PDF 1.6，仅 Polygon）。
    PolygonCloud,
    /// 折线标注（`/PolyLineDimension`，PDF 1.7，仅 PolyLine）。
    PolyLineDimension,
    /// 多边形标注（`/PolygonDimension`，PDF 1.7，仅 Polygon）。
    PolygonDimension,
}

impl PolygonIntent {
    /// 意图名（`/IT` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::PolygonCloud => "PolygonCloud",
            Self::PolyLineDimension => "PolyLineDimension",
            Self::PolygonDimension => "PolygonDimension",
        }
    }

    /// 该意图引入的最低 PDF 版本（Table 178）。
    pub fn min_version(self) -> PdfVersion {
        match self {
            Self::PolygonCloud => PdfVersion::V1_6,
            Self::PolyLineDimension | Self::PolygonDimension => PdfVersion::V1_7,
        }
    }

    /// 该意图允许的子类型。
    fn allows(self, kind: AnnotationKind) -> bool {
        match self {
            Self::PolygonCloud | Self::PolygonDimension => kind == AnnotationKind::Polygon,
            Self::PolyLineDimension => kind == AnnotationKind::PolyLine,
        }
    }
}

/// Polygon/PolyLine 注解专有条目（Table 178）的构建态。
#[derive(Debug, Clone, Default)]
pub(crate) struct GeometryEntries {
    /// `/Vertices`：x/y 交替的顶点坐标（必填）。
    vertices: Option<Vec<f64>>,
    /// `/IT`：注解意图（PDF 1.6/1.7）。
    intent: Option<PolygonIntent>,
}

impl GeometryEntries {
    /// 顶点坐标 `/Vertices`。
    pub(crate) fn vertices(&self) -> Option<&[f64]> {
        self.vertices.as_deref()
    }

    /// 意图 `/IT`。
    pub(crate) fn intent(&self) -> Option<PolygonIntent> {
        self.intent
    }

    /// 是否声明了任意几何专有条目；用于子类型归属校验。
    fn is_declared(&self) -> bool {
        self.vertices.is_some() || self.intent.is_some()
    }

    /// 校验取值与子类型配对；`/Vertices` 的必填性由子类型归属处判断。
    fn validate(&self, kind: AnnotationKind) -> Result<(), String> {
        if let Some(vertices) = &self.vertices {
            if vertices.len() % 2 != 0 || vertices.len() < 4 {
                return Err(
                    "Polygon annotation /Vertices must hold an even number of values for at least two vertices"
                        .into(),
                );
            }
            if vertices.len() > MAX_VERTEX_VALUES {
                return Err("Polygon annotation /Vertices has too many values".into());
            }
            if vertices.iter().any(|v| !v.is_finite()) {
                return Err("Polygon annotation /Vertices entries must be finite".into());
            }
        }
        if let Some(intent) = self.intent
            && !intent.allows(kind)
        {
            return Err(format!(
                "annotation /IT /{} is not meaningful for {} annotations",
                intent.name(),
                kind.name()
            ));
        }
        Ok(())
    }
}

impl PdfAnnotation {
    /// Polygon/PolyLine 顶点 `/Vertices`（x/y 交替，至少两个顶点；两种子类型必填）。
    pub fn with_vertices(mut self, vertices: Vec<f64>) -> Self {
        self.geometry.vertices = Some(vertices);
        self
    }

    /// Polygon/PolyLine 意图 `/IT`（`PolygonCloud` PDF 1.6，标注类 PDF 1.7）。
    pub fn with_polygon_intent(mut self, intent: PolygonIntent) -> Self {
        self.geometry.intent = Some(intent);
        self
    }
}

impl PdfAnnotation {
    /// Polygon/PolyLine 注解专有条目（Table 178）。
    pub(crate) fn geometry(&self) -> &GeometryEntries {
        &self.geometry
    }

    /// 几何专有条目的子类型归属与必填项校验；由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_geometry_entries(&self) -> Result<(), String> {
        if self.geometry.is_declared() {
            self.require_kind_in(
                &[AnnotationKind::Polygon, AnnotationKind::PolyLine],
                "Polygon/PolyLine annotation entries",
            )?;
        }
        if matches!(
            self.kind,
            AnnotationKind::Polygon | AnnotationKind::PolyLine
        ) && self.geometry.vertices().is_none()
        {
            return Err(format!(
                "{} annotation requires /Vertices (vertex coordinates)",
                self.kind.name()
            ));
        }
        self.geometry.validate(self.kind)
    }
}
