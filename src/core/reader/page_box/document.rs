//! 页面边界读端入口：Document::page_bounds 与框继承解析。

use std::collections::HashSet;

use crate::core::reader::navigation::{MAX_TREE_DEPTH, resolve};
use crate::core::types::{Dict, Document};

use super::{
    PageBounds, PageBoxError, Rectangle, malformed, parse_box_color_info, parse_rectangle,
};

impl Document {
    /// 读取 `page_index`（0 基）页面的边界声明（14.11.2 Table 30）。
    ///
    /// `/MediaBox` 沿 `/Parent` 链继承（深度上限 64，引用成环报错），
    /// 整条链都缺失时为 [`PageBoxError::Malformed`]；`/CropBox` 取最近
    /// 声明者；`/BleedBox` `/TrimBox` `/ArtBox` 只取页面字典自身。
    /// 各框须为 4 个有限数字的数组（7.9.5，任意对角顺序均归一化）。
    /// `/BoxColorInfo` 须为字典，其条目按 Table 360/361 解析。
    pub fn page_bounds(&self, page_index: usize) -> Result<PageBounds, PageBoxError> {
        let page_value = self.locate_page_node(page_index)?;
        let page = page_value
            .as_dict()
            .ok_or_else(|| malformed("page node is not a dictionary"))?;

        let media = inherited_box(self, page, "MediaBox")?
            .ok_or_else(|| malformed("page has no inheritable /MediaBox"))?;
        let crop = inherited_box(self, page, "CropBox")?;
        let bleed = page_box(self, page, "BleedBox")?;
        let trim = page_box(self, page, "TrimBox")?;
        let art = page_box(self, page, "ArtBox")?;
        let box_color_info = match page.get("BoxColorInfo") {
            Some(value) => Some(parse_box_color_info(self, value)?),
            None => None,
        };

        Ok(PageBounds {
            media,
            crop,
            bleed,
            trim,
            art,
            box_color_info,
            rotate: inherited_rotate(self, page)?,
            dict: page.clone(),
        })
    }
}

/// 沿页面字典与 `/Parent` 链查找可继承的框（MediaBox/CropBox）。
fn inherited_box(
    document: &Document,
    page: &Dict,
    key: &str,
) -> Result<Option<Rectangle>, PageBoxError> {
    let mut current = page.clone();
    let mut visited: HashSet<(i64, i64)> = HashSet::new();
    let mut depth = 0usize;
    loop {
        if let Some(value) = current.get(key) {
            return parse_rectangle(document, value, &format!("page /{key}")).map(Some);
        }
        if depth > MAX_TREE_DEPTH {
            return Err(PageBoxError::LimitExceeded("page boundary inheritance"));
        }
        let Some(parent) = current.get("Parent") else {
            return Ok(None);
        };
        if let Some(reference) = parent.as_ref()
            && !visited.insert(reference)
        {
            return Err(malformed(format!(
                "page /Parent chain contains a cycle while resolving /{key}"
            )));
        }
        let parent = resolve(document, parent)
            .map_err(|error| malformed(format!("page /Parent: {error}")))?;
        current = parent
            .as_dict()
            .cloned()
            .ok_or_else(|| malformed("page /Parent is not a dictionary"))?;
        depth += 1;
    }
}

/// 沿 `/Parent` 链取可继承的 `/Rotate`（Table 30）：整数且为 90 的倍数，
/// 整条链缺失时为缺省 0；与 [`inherited_box`] 同款深度/环防护。
fn inherited_rotate(document: &Document, page: &Dict) -> Result<i32, PageBoxError> {
    let mut current = page.clone();
    let mut visited: HashSet<(i64, i64)> = HashSet::new();
    let mut depth = 0usize;
    loop {
        if let Some(value) = current.get("Rotate") {
            let value = resolve(document, value)
                .map_err(|error| malformed(format!("page /Rotate: {error}")))?;
            let degrees = value
                .as_integer()
                .ok_or_else(|| malformed("page /Rotate is not an integer"))?;
            if degrees % 90 != 0 {
                return Err(malformed("page /Rotate must be a multiple of 90"));
            }
            return i32::try_from(degrees).map_err(|_| malformed("page /Rotate is out of range"));
        }
        if depth > MAX_TREE_DEPTH {
            return Err(PageBoxError::LimitExceeded("page boundary inheritance"));
        }
        let Some(parent) = current.get("Parent") else {
            return Ok(0);
        };
        if let Some(reference) = parent.as_ref()
            && !visited.insert(reference)
        {
            return Err(malformed(
                "page /Parent chain contains a cycle while resolving /Rotate",
            ));
        }
        let parent = resolve(document, parent)
            .map_err(|error| malformed(format!("page /Parent: {error}")))?;
        current = parent
            .as_dict()
            .cloned()
            .ok_or_else(|| malformed("page /Parent is not a dictionary"))?;
        depth += 1;
    }
}

/// 读取页面字典自身（不继承）的框。
fn page_box(
    document: &Document,
    page: &Dict,
    key: &str,
) -> Result<Option<Rectangle>, PageBoxError> {
    match page.get(key) {
        Some(value) => parse_rectangle(document, value, &format!("page /{key}")).map(Some),
        None => Ok(None),
    }
}
