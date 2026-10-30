//! 页面缩略图（12.3.4 Thumbnail Images、7.7.3.4 Table 30 `/Thumb`）。
//!
//! `/Thumb` 是页面字典中的图像 XObject（可选、不参与页面属性继承、PDF
//! 1.0 起可用）。图像字典按 Table 89 解析，但 12.3.4 规定只有
//! `/Width` `/Height` `/ColorSpace` `/BitsPerComponent` `/Decode` 有意义，
//! 其余条目应被忽略；颜色空间限于 DeviceGray、DeviceRGB 或基于它们的
//! Indexed 空间。

use std::fmt;

use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::image::{ImageError, ImageModel};
use crate::core::reader::page_content::PageContentError;
use crate::core::types::Document;

#[cfg(test)]
mod tests;

/// 页面缩略图解析的失败原因。
#[derive(Debug)]
pub enum ThumbnailError {
    /// 页面节点或页面树无法解析。
    Page(PageContentError),
    /// `/Thumb` 图像 XObject 解析失败。
    Image(ImageError),
    /// 条目本身不满足 12.3.4 的约束。
    Invalid(String),
}

impl fmt::Display for ThumbnailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Page(error) => write!(f, "{error}"),
            Self::Image(error) => write!(f, "page /Thumb: {error}"),
            Self::Invalid(message) => write!(f, "invalid page thumbnail: {message}"),
        }
    }
}

impl std::error::Error for ThumbnailError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Page(error) => Some(error),
            Self::Image(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<PageContentError> for ThumbnailError {
    fn from(error: PageContentError) -> Self {
        Self::Page(error)
    }
}

fn invalid(message: impl Into<String>) -> ThumbnailError {
    ThumbnailError::Invalid(message.into())
}

impl Document {
    /// 解析 `page_index`（0 基）页面字典的 `/Thumb` 缩略图图像。
    ///
    /// 页面无 `/Thumb` 时返回 `None`；条目须为图像 XObject（间接引用或
    /// 直接流），且按 12.3.4 要求不得是图像蒙版，颜色空间须为
    /// DeviceGray、DeviceRGB 或基于二者的 Indexed。
    pub fn page_thumbnail(&self, page_index: usize) -> Result<Option<ImageModel>, ThumbnailError> {
        let page = self.locate_page_node(page_index)?;
        let dict = page
            .as_dict()
            .ok_or_else(|| invalid("page node is not a dictionary"))?;
        let Some(value) = dict.get("Thumb") else {
            return Ok(None);
        };
        let image = self.image(value).map_err(ThumbnailError::Image)?;
        if image.image_mask {
            return Err(invalid("page /Thumb must not be an image mask"));
        }
        if !thumbnail_color_space_supported(image.color_space.as_ref()) {
            return Err(invalid(
                "page /Thumb color space must be DeviceGray, DeviceRGB \
                 or Indexed based on them",
            ));
        }
        Ok(Some(image))
    }
}

/// 12.3.4：缩略图颜色空间限于 DeviceGray、DeviceRGB 或基于二者的 Indexed。
fn thumbnail_color_space_supported(space: Option<&ColorSpaceModel>) -> bool {
    match space {
        Some(ColorSpaceModel::DeviceGray | ColorSpaceModel::DeviceRGB) => true,
        Some(ColorSpaceModel::Indexed(indexed)) => matches!(
            indexed.base.as_ref(),
            ColorSpaceModel::DeviceGray | ColorSpaceModel::DeviceRGB
        ),
        _ => false,
    }
}
