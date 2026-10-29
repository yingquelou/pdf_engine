//! 页面视口字典（12.9.2，Table 260）与页面 `/VP` 数组的类型化读端模型。
//!
//! 视口把页面上一块矩形区域（`/BBox`）与测量字典（`/Measure`，Table 261–263）
//! 关联起来，使同一页的不同区域可以使用不同测量比例。`/VP` 是页面字典条目
//! （Table 30，PDF 1.6 引入），数组按绘制顺序排列。
//!
//! 校验按规范执行：`/Type` 出现时必须是 `/Viewport`；`/BBox` 必填，且为
//! 左下、右上顺序规范化的四个有限数值（规范以该顺序确定测量坐标系方向）；
//! `/Name` 为文本串；`/Measure` 复用注解测量字典解析器
//! （[`parse_measure_value`]）。未建模条目原样保留在 [`Viewport::other`]。

use std::collections::BTreeMap;
use std::fmt;

use crate::core::reader::annotation::{AnnotationError, Measure, parse_measure_value};
use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::reader::page_content::PageContentError;
use crate::core::types::{Dict, Document, Value};

#[cfg(test)]
mod tests;

/// 单页视口数量上限（防护不可信文件）。
pub const MAX_VIEWPORTS: usize = 4096;

type Result<T> = std::result::Result<T, ViewportError>;

/// 视口解析的失败原因。
#[derive(Debug)]
pub enum ViewportError {
    /// 页面节点或页面树无法解析。
    PageContent(PageContentError),
    /// 视口 `/Measure` 测量字典解析失败。
    Measure(AnnotationError),
    /// 视口字典本身不满足 Table 260。
    Invalid(String),
}

impl fmt::Display for ViewportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PageContent(error) => write!(f, "{error}"),
            Self::Measure(error) => write!(f, "viewport /Measure: {error}"),
            Self::Invalid(message) => write!(f, "invalid viewport: {message}"),
        }
    }
}

impl std::error::Error for ViewportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PageContent(error) => Some(error),
            Self::Measure(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<PageContentError> for ViewportError {
    fn from(error: PageContentError) -> Self {
        Self::PageContent(error)
    }
}

fn invalid(message: impl Into<String>) -> ViewportError {
    ViewportError::Invalid(message.into())
}

/// 解析后的视口字典（Table 260）。
#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    /// `/BBox`：左下、右上两个坐标对（规范要求已规范化）。
    pub bbox: [f64; 4],
    /// `/Name` 描述名；省略即 `None`。
    pub name: Option<String>,
    /// `/Measure` 测量字典（Table 261）；省略即 `None`。
    pub measure: Option<Measure>,
    /// 不建模的其余条目，原值保留。
    pub other: BTreeMap<String, Value>,
}

impl Document {
    /// 解析一个视口字典值。
    ///
    /// `value` 可为字典或指向字典的间接引用。
    pub fn viewport(&self, value: &Value) -> Result<Viewport> {
        let resolved = resolve(self, value).map_err(|err| invalid(err.to_string()))?;
        let dict = resolved.as_dict().ok_or_else(|| {
            invalid(format!(
                "viewport must be a dictionary, got {}",
                resolved.type_name()
            ))
        })?;
        self.parse_viewport(dict)
    }

    /// 解析 `page_index`（0 基）页面字典的 `/VP` 视口数组。
    ///
    /// 页面无 `/VP` 时返回空数组；数组元素的间接引用逐项解引用。
    pub fn page_viewports(&self, page_index: usize) -> Result<Vec<Viewport>> {
        let page_value = self.locate_page_node(page_index)?;
        let page = page_value
            .as_dict()
            .ok_or_else(|| invalid("page node is not a dictionary"))?;
        let Some(value) = page.get("VP") else {
            return Ok(Vec::new());
        };
        let resolved = resolve(self, value).map_err(|err| invalid(err.to_string()))?;
        let array = resolved
            .as_array()
            .ok_or_else(|| invalid("page /VP is not an array"))?;
        if array.len() > MAX_VIEWPORTS {
            return Err(invalid(format!(
                "page /VP has more than {MAX_VIEWPORTS} viewports"
            )));
        }
        let mut viewports = Vec::with_capacity(array.len());
        for item in array.iter() {
            viewports.push(self.viewport(item)?);
        }
        Ok(viewports)
    }

    fn parse_viewport(&self, dict: &Dict) -> Result<Viewport> {
        match dict.get("Type") {
            None => {}
            Some(value) => match resolve(self, value).map_err(|err| invalid(err.to_string()))? {
                Value::Name(name) if name == "Viewport" => {}
                Value::Name(name) => {
                    return Err(invalid(format!(
                        "viewport /Type must be /Viewport, got /{name}"
                    )));
                }
                _ => return Err(invalid("viewport /Type must be a name")),
            },
        }

        let bbox_value = dict
            .get("BBox")
            .ok_or_else(|| invalid("viewport /BBox is required"))?;
        let resolved = resolve(self, bbox_value).map_err(|err| invalid(err.to_string()))?;
        let array = resolved
            .as_array()
            .ok_or_else(|| invalid("viewport /BBox is not an array"))?;
        if array.len() != 4 {
            return Err(invalid("viewport /BBox must hold four numbers"));
        }
        let mut bbox = [0.0f64; 4];
        for (slot, item) in bbox.iter_mut().zip(array.iter()) {
            *slot = match item {
                Value::Integer(value) => *value as f64,
                Value::Float(value) => *value,
                _ => return Err(invalid("viewport /BBox must hold four numbers")),
            };
            if !slot.is_finite() {
                return Err(invalid("viewport /BBox must hold finite numbers"));
            }
        }
        if bbox[0] > bbox[2] || bbox[1] > bbox[3] {
            return Err(invalid(
                "viewport /BBox must be normalized (lower-left then upper-right)",
            ));
        }

        let name = match dict.get("Name") {
            None => None,
            Some(value) => {
                if !matches!(value, Value::String(_) | Value::HexString(_)) {
                    return Err(invalid("viewport /Name must be a text string"));
                }
                Some(text_value(value).ok_or_else(|| invalid("viewport /Name is not valid text"))?)
            }
        };

        let measure = match dict.get("Measure") {
            None => None,
            Some(value) => Some(parse_measure_value(self, value).map_err(ViewportError::Measure)?),
        };

        const VIEWPORT_ENTRIES: [&str; 4] = ["Type", "BBox", "Name", "Measure"];
        let mut other = BTreeMap::new();
        for (key, value) in &dict.0 {
            if !VIEWPORT_ENTRIES.contains(&key.as_str()) {
                other.insert(key.clone(), value.clone());
            }
        }

        Ok(Viewport {
            bbox,
            name,
            measure,
            other,
        })
    }
}
