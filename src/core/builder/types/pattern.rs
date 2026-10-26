//! 平铺图案（Tiling Pattern，PatternType 1）：ISO 32000-1 §8.7.3。
//!
//! 支持彩色图案（`/PaintType 1`，图块内容流自带颜色操作符）与无色图案
//! （`/PaintType 2`，颜色由使用图案时的 `scn`/`SCN` 操作数给出，图案字典
//! 的 `/ColorSpace` 定义这些分量的解释空间）。图案资源仍仅接受调用方
//! 给定的直接字典，构建器不代管其中的字体/图片对象号。

use super::image::ColorSpace;
use super::shading::ShadingPattern;
use crate::core::content::ContentStream;
use crate::core::types::Dict;

/// 平铺类型 `/TilingType`（§8.7.3.3 Table 77）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TilingType {
    /// 1：恒定间距，图案单元按设备空间等距排列。
    ConstantSpacing,
    /// 2：无畸变，图案单元不缩放以吸收舍入误差。
    NoDistortion,
    /// 3：恒定间距与速度，允许近似间距以提高绘制速度。
    ConstantSpacingAndSpeed,
}

impl TilingType {
    pub(crate) fn as_i64(self) -> i64 {
        match self {
            Self::ConstantSpacing => 1,
            Self::NoDistortion => 2,
            Self::ConstantSpacingAndSpeed => 3,
        }
    }
}

/// 图案着色方式 `/PaintType`（§8.7.3 Table 75）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintType {
    /// 1：彩色图案，图块内容流自带颜色操作符。
    Colored,
    /// 2：无色图案，颜色由使用图案时的 `scn`/`SCN` 操作数给出。
    Uncolored,
}

impl PaintType {
    pub(crate) fn as_i64(self) -> i64 {
        match self {
            Self::Colored => 1,
            Self::Uncolored => 2,
        }
    }
}

/// PatternType 1 平铺图案：以小图块重复填充（§8.7.3.3 Table 77）。
///
/// 图案本身是流对象：内容流在图案坐标系中绘制单个图块，`BBox` 给出
/// 图块边界，`XStep`/`YStep` 给出相邻图块原点间距。写端总是写出
/// `/Resources`（未提供时为空字典），因为图案内容流独立于页面资源。
#[derive(Debug, Clone)]
pub struct TilingPattern {
    /// 图案空间中的图块边界 `[llx lly urx ury]`。
    pub bbox: [f64; 4],
    /// 图块水平间距，不得为 0。
    pub x_step: f64,
    /// 图块垂直间距，不得为 0。
    pub y_step: f64,
    /// 平铺类型 `/TilingType`。
    pub tiling_type: TilingType,
    /// 着色方式 `/PaintType`。
    pub paint_type: PaintType,
    /// 无色图案的颜色空间；彩色图案忽略此字段。
    pub color_space: Option<ColorSpace>,
    /// 从图案空间到默认坐标系的变换矩阵；省略时为单位矩阵。
    pub matrix: Option<[f64; 6]>,
    /// 图案资源字典；省略时写空字典。
    pub resources: Option<Dict>,
    /// 单个图块的内容流。
    pub content: ContentStream,
}

impl TilingPattern {
    /// 以图块边界、间距与内容流创建彩色平铺图案。
    pub fn new(bbox: [f64; 4], x_step: f64, y_step: f64, content: ContentStream) -> Self {
        Self {
            bbox,
            x_step,
            y_step,
            tiling_type: TilingType::ConstantSpacing,
            paint_type: PaintType::Colored,
            color_space: None,
            matrix: None,
            resources: None,
            content,
        }
    }

    /// 创建无色平铺图案（`/PaintType 2`）。
    ///
    /// 图块内容流不应包含颜色操作符；颜色在填充时由
    /// [`crate::core::builder::PdfBuilder::fill_rect_with_uncolored_pattern`]
    /// 以 `scn` 操作数提供，分量按 `color_space` 解释。
    pub fn uncolored(
        bbox: [f64; 4],
        x_step: f64,
        y_step: f64,
        color_space: ColorSpace,
        content: ContentStream,
    ) -> Self {
        Self {
            bbox,
            x_step,
            y_step,
            tiling_type: TilingType::ConstantSpacing,
            paint_type: PaintType::Uncolored,
            color_space: Some(color_space),
            matrix: None,
            resources: None,
            content,
        }
    }

    /// 设置平铺类型。
    pub fn with_tiling_type(mut self, tiling_type: TilingType) -> Self {
        self.tiling_type = tiling_type;
        self
    }

    /// 设置图案矩阵。
    pub fn with_matrix(mut self, matrix: [f64; 6]) -> Self {
        self.matrix = Some(matrix);
        self
    }

    /// 设置图案资源字典。
    pub fn with_resources(mut self, resources: Dict) -> Self {
        self.resources = Some(resources);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.bbox.iter().any(|v| !v.is_finite()) {
            return Err("tiling pattern BBox must contain finite numbers".into());
        }
        if self.bbox[0] > self.bbox[2] || self.bbox[1] > self.bbox[3] {
            return Err("tiling pattern BBox minimum must not exceed maximum".into());
        }
        if !self.x_step.is_finite() || !self.y_step.is_finite() {
            return Err("tiling pattern XStep and YStep must be finite".into());
        }
        if self.x_step == 0.0 || self.y_step == 0.0 {
            return Err("tiling pattern XStep and YStep must be nonzero".into());
        }
        if let Some(matrix) = self.matrix
            && matrix.iter().any(|v| !v.is_finite())
        {
            return Err("tiling pattern Matrix must contain finite numbers".into());
        }
        if self.paint_type == PaintType::Uncolored {
            let Some(color_space) = &self.color_space else {
                return Err("uncolored tiling pattern requires a color space".into());
            };
            color_space
                .validate()
                .map_err(|message| format!("uncolored tiling pattern color space: {message}"))?;
        }
        Ok(())
    }
}

/// 文档级图案池的元素：着色图案（PatternType 2）或平铺图案（PatternType 1）。
#[derive(Debug, Clone)]
pub(crate) enum Pattern {
    Shading(Box<ShadingPattern>),
    Tiling(Box<TilingPattern>),
}

impl Pattern {
    /// 校验图案；返回的消息已带图案类别前缀。
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Shading(pattern) => pattern
                .validate()
                .map_err(|message| format!("shading pattern: {message}")),
            Self::Tiling(pattern) => pattern
                .validate()
                .map_err(|message| format!("tiling pattern: {message}")),
        }
    }
}
