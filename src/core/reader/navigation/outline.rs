//! 文档大纲遍历（ISO 32000-1 12.3.4）。

use std::collections::HashSet;

use crate::core::reader::content::text_value;
use crate::core::types::{Document, Value};

use super::destination::parse_outline_destination;
use super::page_tree::PageTreeIndex;
use super::{
    MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS, NavigationError, catalog, malformed, page_tree_index,
    resolve,
};

/// 文档大纲的顶层条目。
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub items: Vec<OutlineItem>,
}

/// 大纲条目；`children` 按文档顺序展开。
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    /// 已按 PDF 文本字符串解码的标题。
    pub title: String,
    /// 目标（显式或命名）；条目可能只有动作而无目标。
    pub destination: Option<OutlineDestination>,
    /// `/A` 动作字典原样保留，语义由动作模型（E3）解析。
    pub action: Option<Value>,
    /// `/C` 标题颜色，各分量 0–1。
    pub color: Option<[f64; 3]>,
    /// `/F` 标志位：1 斜体、2 粗体。
    pub flags: u32,
    /// `/Count`：正数表示展开的可见子项数，负数表示折叠。
    pub count: i64,
    pub children: Vec<OutlineItem>,
}

/// 大纲目标：显式目的地或命名目的地。
#[derive(Debug, Clone, PartialEq)]
pub enum OutlineDestination {
    Explicit(super::Destination),
    Named(String),
}

struct OutlineReader<'a> {
    document: &'a Document,
    pages: &'a PageTreeIndex,
    visited: HashSet<(i64, i64)>,
    items: usize,
}

/// 读取目录 `/Outlines`；不存在时返回 `None`。
pub(crate) fn read_outline(document: &Document) -> Result<Option<Outline>, NavigationError> {
    let catalog = catalog(document)?;
    let Some(root) = catalog.get("Outlines") else {
        return Ok(None);
    };
    let root = resolve(document, root)?;
    let root = root
        .as_dict()
        .ok_or_else(|| malformed("/Outlines is not a dictionary"))?;
    let Some(first) = root.get("First") else {
        return Ok(Some(Outline { items: Vec::new() }));
    };
    let pages = page_tree_index(document)?;
    let mut reader = OutlineReader {
        document,
        pages: &pages,
        visited: HashSet::new(),
        items: 0,
    };
    let items = reader.read_siblings(first, 0)?;
    Ok(Some(Outline { items }))
}

impl OutlineReader<'_> {
    fn read_siblings(
        &mut self,
        first: &Value,
        depth: usize,
    ) -> Result<Vec<OutlineItem>, NavigationError> {
        if depth > MAX_OUTLINE_DEPTH {
            return Err(NavigationError::LimitExceeded("outline depth"));
        }
        let mut items = Vec::new();
        let mut current = Some(first.clone());
        while let Some(value) = current {
            if self.items >= MAX_OUTLINE_ITEMS {
                return Err(NavigationError::LimitExceeded("outline item count"));
            }
            if let Some(key) = value.as_ref()
                && !self.visited.insert(key)
            {
                return Err(malformed("outline item cycle"));
            }
            let node = resolve(self.document, &value)?;
            let dict = node
                .as_dict()
                .ok_or_else(|| malformed("outline item is not a dictionary"))?;
            self.items += 1;

            let title = dict
                .get("Title")
                .and_then(text_value)
                .ok_or_else(|| malformed("outline item is missing /Title"))?;
            let destination = match dict.get("Dest") {
                Some(value) => Some(parse_outline_destination(self.document, value, self.pages)?),
                None => None,
            };
            let action = dict.get("A").cloned();
            let color = parse_color(self.document, dict.get("C"))?;
            let flags = parse_flags(self.document, dict.get("F"))?;
            let count = parse_count(self.document, dict.get("Count"))?;
            let children = match dict.get("First") {
                Some(first) => self.read_siblings(first, depth + 1)?,
                None => Vec::new(),
            };
            items.push(OutlineItem {
                title,
                destination,
                action,
                color,
                flags,
                count,
                children,
            });
            current = dict.get("Next").cloned();
        }
        Ok(items)
    }
}

fn parse_color(
    document: &Document,
    value: Option<&Value>,
) -> Result<Option<[f64; 3]>, NavigationError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = resolve(document, value)?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("outline /C is not an array"))?;
    if array.len() != 3 {
        return Err(malformed("outline /C must contain 3 components"));
    }
    let mut color = [0.0; 3];
    for (slot, value) in color.iter_mut().zip(&array.0) {
        let value = resolve(document, value)?;
        let component = match value {
            Value::Integer(number) => number as f64,
            Value::Float(number) => number,
            other => {
                return Err(malformed(format!(
                    "outline /C component is not a number: {other:?}"
                )));
            }
        };
        if !(0.0..=1.0).contains(&component) {
            return Err(malformed("outline /C component is outside 0–1"));
        }
        *slot = component;
    }
    Ok(Some(color))
}

fn parse_flags(document: &Document, value: Option<&Value>) -> Result<u32, NavigationError> {
    let Some(value) = value else {
        return Ok(0);
    };
    let value = resolve(document, value)?;
    let flags = value
        .as_integer()
        .ok_or_else(|| malformed("outline /F is not an integer"))?;
    if !(0..=3).contains(&flags) {
        return Err(malformed("outline /F is outside 0–3"));
    }
    Ok(flags as u32)
}

fn parse_count(document: &Document, value: Option<&Value>) -> Result<i64, NavigationError> {
    let Some(value) = value else {
        return Ok(0);
    };
    let value = resolve(document, value)?;
    value
        .as_integer()
        .ok_or_else(|| malformed("outline /Count is not an integer"))
}
