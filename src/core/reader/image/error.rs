//! 图像解析或样本读取的失败原因 [`ImageError`] 与模块内 `Result` 别名。

use std::fmt;

use crate::core::reader::ParseError;
use crate::core::reader::color_space::ColorSpaceError;
use crate::core::reader::page_content::PageContentError;

pub(super) type Result<T> = std::result::Result<T, ImageError>;

/// 图像解析或样本读取的失败原因。
#[derive(Debug)]
pub enum ImageError {
    /// 间接引用无法解析。
    Parse(ParseError),
    /// 页面定位或资源解析失败。
    PageContent(PageContentError),
    /// `/ColorSpace` 解析失败。
    ColorSpace(ColorSpaceError),
    /// 图像字典或样本数据不满足规范。
    Invalid(String),
    /// 暂不支持的图像特性（如 Indexed 的 `/Decode`、DCT 原始样本）。
    Unsupported(String),
    /// 流过滤链解码失败。
    Filter(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::PageContent(error) => write!(f, "{error}"),
            Self::ColorSpace(error) => write!(f, "invalid image color space: {error}"),
            Self::Invalid(message) => write!(f, "invalid image: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported image feature: {message}"),
            Self::Filter(message) => write!(f, "image stream decode failed: {message}"),
        }
    }
}

impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::PageContent(error) => Some(error),
            Self::ColorSpace(error) => Some(error),
            Self::Invalid(_) | Self::Unsupported(_) | Self::Filter(_) => None,
        }
    }
}

impl From<ParseError> for ImageError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<PageContentError> for ImageError {
    fn from(error: PageContentError) -> Self {
        Self::PageContent(error)
    }
}

impl From<ColorSpaceError> for ImageError {
    fn from(error: ColorSpaceError) -> Self {
        Self::ColorSpace(error)
    }
}

pub(super) fn invalid(message: impl Into<String>) -> ImageError {
    ImageError::Invalid(message.into())
}

pub(super) fn unsupported(message: impl Into<String>) -> ImageError {
    ImageError::Unsupported(message.into())
}
