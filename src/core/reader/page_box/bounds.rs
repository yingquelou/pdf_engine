//! 一页边界声明与有效框缺省链（14.11.2 Table 30）。

use crate::core::types::Dict;

use super::{BoxColorInfo, Rectangle};

/// 一页的边界声明（14.11.2 Table 30）。
///
/// 只含页面实际声明的值；仅 `/MediaBox` / `/CropBox` 沿 `/Parent` 链
/// 继承，`/BleedBox` `/TrimBox` `/ArtBox` 只取页面字典自身。
#[derive(Debug, Clone, PartialEq)]
pub struct PageBounds {
    /// `/MediaBox`：介质框（必填、可继承）。
    pub media: Rectangle,
    /// `/CropBox`：裁剪框（可继承，缺省等于 MediaBox）。
    pub crop: Option<Rectangle>,
    /// `/BleedBox`：出血框（PDF 1.3，不可继承，缺省等于 CropBox）。
    pub bleed: Option<Rectangle>,
    /// `/TrimBox`：裁切框（PDF 1.3，不可继承，缺省等于 CropBox）。
    pub trim: Option<Rectangle>,
    /// `/ArtBox`：作品框（PDF 1.3，不可继承，缺省等于 CropBox）。
    pub art: Option<Rectangle>,
    /// `/BoxColorInfo`：框颜色信息（PDF 1.4）。
    pub box_color_info: Option<BoxColorInfo>,
    /// 原始页面字典（保留未建模条目）。
    pub dict: Dict,
}

impl PageBounds {
    /// 有效 CropBox：声明值或 MediaBox，再与 MediaBox 求交（14.11.2.1）。
    pub fn effective_crop(&self) -> Rectangle {
        self.crop.unwrap_or(self.media).intersect(self.media)
    }

    /// 有效 BleedBox：声明值或 CropBox，再与 MediaBox 求交。
    pub fn effective_bleed(&self) -> Rectangle {
        self.bleed
            .unwrap_or_else(|| self.effective_crop())
            .intersect(self.media)
    }

    /// 有效 TrimBox：声明值或 CropBox，再与 MediaBox 求交。
    pub fn effective_trim(&self) -> Rectangle {
        self.trim
            .unwrap_or_else(|| self.effective_crop())
            .intersect(self.media)
    }

    /// 有效 ArtBox：声明值或 CropBox，再与 MediaBox 求交。
    pub fn effective_art(&self) -> Rectangle {
        self.art
            .unwrap_or_else(|| self.effective_crop())
            .intersect(self.media)
    }
}
