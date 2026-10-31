//! 分色字典读端模型（ISO 32000-1 14.11.4 Table 364）。
//!
//! 页面字典的 `/SeparationInfo`（PDF 1.3 引入，Table 30）指向一个分色
//! 字典，描述同一文档页的一组「分色页」。Table 364 要求 `/Pages` 数组
//! 自包含当前页，且每个被引用页的 `/SeparationInfo /Pages` 与本数组
//! 完全相同；`/DeviceColorant` 为设备色料名（名字或文本串），
//! `/ColorSpace` 可选，且只能是 Separation 或 DeviceN 颜色空间（8.6.6）。
//!
//! `/Pages` 的引用按**对象号**比较（忽略 generation），与库内
//! `resolve`/`resolve_value` 按对象号读取最新修订的语义一致。

use std::fmt;

use crate::core::reader::color_space::{ColorSpaceError, ColorSpaceModel};
use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::reader::page_content::PageContentError;
use crate::core::types::{Dict, Document, Value};

/// `/Pages` 数组条目数上限（一组分色页的规模上限）。
const MAX_SEPARATION_PAGES: usize = 1024;

/// 设备色料名（Table 364 `/DeviceColorant`）。
///
/// 名字型色料（如 `/Cyan`）用 [`DeviceColorant::Name`]；含空格的厂商
/// 色料名按规范写成文本串，读端保留为 [`DeviceColorant::Text`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceColorant {
    /// 名字型色料。
    Name(String),
    /// 文本串型色料。
    Text(String),
}

impl DeviceColorant {
    /// 构造名字型色料。
    pub fn name(value: impl Into<String>) -> Self {
        Self::Name(value.into())
    }

    /// 构造文本串型色料。
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// 色料名字符串（两种表示统一取值）。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Name(value) | Self::Text(value) => value,
        }
    }
}

/// 分色字典（Table 364）。
#[derive(Debug, Clone, PartialEq)]
pub struct SeparationInfo {
    /// `/Pages`：各分色页的间接引用（含 generation），原样保留。
    pub pages: Vec<(i64, i64)>,
    /// `/DeviceColorant`：设备色料名。
    pub device_colorant: DeviceColorant,
    /// `/ColorSpace`：Separation 或 DeviceN 颜色空间。
    pub color_space: Option<ColorSpaceModel>,
    /// 原始分色字典（保留未建模条目）。
    pub dict: Dict,
}

/// 分色字典读取的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum SeparationInfoError {
    /// 页面节点或页面树无法解析（含页码越界）。
    Page(PageContentError),
    /// `/ColorSpace` 解析失败。
    ColorSpace(ColorSpaceError),
    /// 条目缺失、类型错误或跨页一致性校验失败。
    Malformed(String),
    /// 超出 `/Pages` 数组条目数上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for SeparationInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Page(error) => write!(f, "invalid page separation info: {error}"),
            Self::ColorSpace(error) => write!(f, "invalid page separation info: {error}"),
            Self::Malformed(message) => write!(f, "invalid page separation info: {message}"),
            Self::LimitExceeded(what) => {
                write!(f, "invalid page separation info: limit exceeded: {what}")
            }
        }
    }
}

impl std::error::Error for SeparationInfoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Page(error) => Some(error),
            Self::ColorSpace(error) => Some(error),
            Self::Malformed(_) | Self::LimitExceeded(_) => None,
        }
    }
}

impl From<PageContentError> for SeparationInfoError {
    fn from(error: PageContentError) -> Self {
        Self::Page(error)
    }
}

impl From<ColorSpaceError> for SeparationInfoError {
    fn from(error: ColorSpaceError) -> Self {
        Self::ColorSpace(error)
    }
}

fn malformed(message: impl Into<String>) -> SeparationInfoError {
    SeparationInfoError::Malformed(message.into())
}

impl Document {
    /// 读取 `page_index`（0 基）页面的分色字典（14.11.4 Table 364）。
    ///
    /// 页面没有 `/SeparationInfo` 时返回 `Ok(None)`；条目可能是直接字典
    /// 或间接引用。`/Pages` 必填且每项须为间接引用，解引用后须是页
    /// 字典；当前页须在数组内，且所有被引用页的 `/SeparationInfo /Pages`
    /// 与本数组按对象号逐项相同（同序同长，忽略 generation）。
    /// `/DeviceColorant` 须为名字或可解码文本串；`/ColorSpace` 可选，
    /// 但只接受 Separation 或 DeviceN 颜色空间。
    pub fn page_separation_info(
        &self,
        page_index: usize,
    ) -> Result<Option<SeparationInfo>, SeparationInfoError> {
        let (page_value, page_ref) = self.locate_page_node_with_ref(page_index)?;
        let page = page_value
            .as_dict()
            .ok_or_else(|| malformed("page node is not a dictionary"))?;
        let Some(info_value) = page.get("SeparationInfo") else {
            return Ok(None);
        };
        let page_ref = page_ref.ok_or_else(|| {
            malformed("page with /SeparationInfo is not an indirect object (Table 364 /Pages)")
        })?;

        let info_value = resolve(self, info_value)
            .map_err(|error| malformed(format!("page /SeparationInfo: {error}")))?;
        let dict = info_value
            .as_dict()
            .ok_or_else(|| malformed("page /SeparationInfo is not a dictionary"))?
            .clone();

        let pages = parse_pages_references(self, &dict, "page /SeparationInfo")?;
        if !pages.iter().any(|(number, _)| *number == page_ref.0) {
            return Err(malformed(
                "page /SeparationInfo /Pages must include the page itself (Table 364)",
            ));
        }
        check_identical_pages(self, &pages)?;

        let Some(colorant_value) = dict.get("DeviceColorant") else {
            return Err(malformed(
                "page /SeparationInfo /DeviceColorant is required (Table 364)",
            ));
        };
        let colorant_value = resolve(self, colorant_value)
            .map_err(|error| malformed(format!("page /SeparationInfo /DeviceColorant: {error}")))?;
        let device_colorant = match &colorant_value {
            Value::Name(name) => DeviceColorant::Name(name.clone()),
            Value::String(_) | Value::HexString(_) => {
                let text = text_value(&colorant_value).ok_or_else(|| {
                    malformed("page /SeparationInfo /DeviceColorant text string cannot be decoded")
                })?;
                DeviceColorant::Text(text)
            }
            other => {
                return Err(malformed(format!(
                    "page /SeparationInfo /DeviceColorant must be a name or text string ({})",
                    other.type_name()
                )));
            }
        };

        let color_space = match dict.get("ColorSpace") {
            Some(value) => {
                let space = self.color_space(value)?;
                if !matches!(
                    space,
                    ColorSpaceModel::Separation(_) | ColorSpaceModel::DeviceN(_)
                ) {
                    return Err(malformed(
                        "page /SeparationInfo /ColorSpace must be a Separation or DeviceN colour space",
                    ));
                }
                Some(space)
            }
            None => None,
        };

        Ok(Some(SeparationInfo {
            pages,
            device_colorant,
            color_space,
            dict,
        }))
    }
}

/// 解析 `/Pages`：必填、数组、条目数受限，每项须为可解引用为字典的
/// 间接引用；返回原样引用（含 generation）。
fn parse_pages_references(
    document: &Document,
    info: &Dict,
    context: &str,
) -> Result<Vec<(i64, i64)>, SeparationInfoError> {
    let Some(value) = info.get("Pages") else {
        return Err(malformed(format!(
            "{context} /Pages is required (Table 364)"
        )));
    };
    let value = resolve(document, value)
        .map_err(|error| malformed(format!("{context} /Pages: {error}")))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed(format!("{context} /Pages is not an array")))?;
    if array.len() > MAX_SEPARATION_PAGES {
        return Err(SeparationInfoError::LimitExceeded(
            "page /SeparationInfo /Pages",
        ));
    }
    let mut pages = Vec::with_capacity(array.len());
    for (index, item) in array.iter().enumerate() {
        let reference = item.as_ref().ok_or_else(|| {
            malformed(format!(
                "{context} /Pages entry {index} must be an indirect reference (Table 364)"
            ))
        })?;
        let target = resolve(document, item)
            .map_err(|error| malformed(format!("{context} /Pages entry {index}: {error}")))?;
        if target.as_dict().is_none() {
            return Err(malformed(format!(
                "{context} /Pages entry {index} does not reference a page dictionary"
            )));
        }
        pages.push(reference);
    }
    Ok(pages)
}

/// Table 364：每个被引用页都须有 `/SeparationInfo`，且其 `/Pages` 与本
/// 数组按对象号逐项相同（同序同长）。
fn check_identical_pages(
    document: &Document,
    pages: &[(i64, i64)],
) -> Result<(), SeparationInfoError> {
    for &(number, generation) in pages {
        let page_value = resolve(document, &Value::Ref(number, generation))
            .map_err(|error| malformed(format!("page {number} {generation}: {error}")))?;
        let referenced_page = page_value.as_dict().ok_or_else(|| {
            malformed(format!(
                "page {number} {generation} does not reference a page dictionary with an identical /Pages"
            ))
        })?;
        let Some(info_value) = referenced_page.get("SeparationInfo") else {
            return Err(malformed(format!(
                "page {number} {generation} has no /SeparationInfo required for an identical /Pages (Table 364)"
            )));
        };
        let info_value = resolve(document, info_value).map_err(|error| {
            malformed(format!(
                "page {number} {generation} /SeparationInfo: {error}"
            ))
        })?;
        let info = info_value.as_dict().ok_or_else(|| {
            malformed(format!(
                "page {number} {generation} /SeparationInfo is not a dictionary for an identical /Pages"
            ))
        })?;
        let other = parse_pages_references(
            document,
            info,
            &format!("page {number} {generation} /SeparationInfo"),
        )?;
        let identical = other.len() == pages.len()
            && other
                .iter()
                .zip(pages)
                .all(|((other_number, _), (number, _))| other_number == number);
        if !identical {
            return Err(malformed(format!(
                "page {number} {generation} /SeparationInfo /Pages is not identical to this page's /Pages (Table 364)"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
