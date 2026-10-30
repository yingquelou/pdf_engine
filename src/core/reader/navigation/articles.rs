//! 文章线读取（ISO 32000-1 12.4.3、Table 160/161）。
//!
//! 目录 `/Threads` 数组给出线程字典（Table 160）；每个线程经 `/F`
//! 指向首个 bead，再沿 bead 的 `/N` 链展开。读端校验 `/N` `/V` 双向链
//! 闭合、`/T` 归属线程与 `/P` 页面引用，并还原 bead 所在页索引与矩形。

use std::collections::HashSet;

use crate::core::types::{Dict, Document, Value};

use super::{NavigationError, PageTreeIndex, catalog, malformed, page_tree_index, resolve};

/// 文章线上的一个 bead（12.4.3 Table 161）。
#[derive(Debug, Clone, PartialEq)]
pub struct ArticleBead {
    /// bead 所在页面（0 基索引）。
    pub page: usize,
    /// `/R` 矩形，按声明顺序保留 `[x0, y0, x1, y1]`。
    pub rect: [f64; 4],
}

/// 一条文章线（12.4.3 Table 160）。
#[derive(Debug, Clone, PartialEq)]
pub struct ArticleThread {
    /// 从首 bead 沿 `/N` 展开的 bead 序列。
    pub beads: Vec<ArticleBead>,
    /// 线程信息字典 `/I`（14.3.3 语法，原样保留）。
    pub info: Option<Dict>,
}

/// 单条线程的 bead 链长上限（与读端大纲条目上限一致）。
pub(crate) const MAX_ARTICLE_BEADS: usize = 10_000;

impl Document {
    /// 读取文档全部文章线（目录 `/Threads`，12.4.3）。
    ///
    /// 目录无 `/Threads` 返回空列表。每条线程的 bead 按 `/N` 链展开并
    /// 校验：`/V` 与前一 bead 一致（首 bead 的 `/V` 指向末 bead）、`/T`
    /// 归属当前线程、`/P` 指向页面树中的页面。链断裂、成环或越界返回
    /// 类型化错误；链长上限 10,000。
    pub fn article_threads(&self) -> Result<Vec<ArticleThread>, NavigationError> {
        read_article_threads(self)
    }
}

pub(crate) fn read_article_threads(
    document: &Document,
) -> Result<Vec<ArticleThread>, NavigationError> {
    let catalog = catalog(document)?;
    let Some(threads) = catalog.get("Threads") else {
        return Ok(Vec::new());
    };
    let threads = resolve(document, threads)?;
    let array = threads
        .as_array()
        .ok_or_else(|| malformed("/Threads is not an array"))?;
    let pages = page_tree_index(document)?;
    let mut result = Vec::with_capacity(array.len());
    let mut seen: HashSet<(i64, i64)> = HashSet::new();
    for entry in &array.0 {
        let Some((thread_num, thread_generation)) = entry.as_ref() else {
            return Err(malformed("article thread is not an indirect reference"));
        };
        if !seen.insert((thread_num, thread_generation)) {
            return Err(malformed(format!(
                "duplicate article thread object {thread_num} {thread_generation}"
            )));
        }
        let thread = resolve(document, entry)?;
        let thread = thread
            .as_dict()
            .ok_or_else(|| malformed("article thread is not a dictionary"))?;
        let first = thread
            .get("F")
            .ok_or_else(|| malformed(format!("article thread object {thread_num} has no /F")))?;
        let first = first
            .as_ref()
            .ok_or_else(|| malformed("article thread /F is not an indirect reference"))?;
        let info = match thread.get("I") {
            Some(value) => {
                let value = resolve(document, value)?;
                Some(
                    value
                        .as_dict()
                        .cloned()
                        .ok_or_else(|| malformed("article thread /I is not a dictionary"))?,
                )
            }
            None => None,
        };
        let beads = read_beads(document, &pages, thread_num, first)?;
        result.push(ArticleThread { beads, info });
    }
    Ok(result)
}

fn read_beads(
    document: &Document,
    pages: &PageTreeIndex,
    thread_num: i64,
    first: (i64, i64),
) -> Result<Vec<ArticleBead>, NavigationError> {
    let mut beads = Vec::new();
    let mut refs = Vec::new();
    let mut previous: Vec<Option<(i64, i64)>> = Vec::new();
    let mut visited: HashSet<(i64, i64)> = HashSet::new();
    let mut current = first;
    loop {
        if beads.len() >= MAX_ARTICLE_BEADS {
            return Err(NavigationError::LimitExceeded("article beads"));
        }
        if !visited.insert(current) {
            return Err(malformed(format!(
                "article bead chain revisits object {} {}",
                current.0, current.1
            )));
        }
        let value = resolve(document, &Value::Ref(current.0, current.1))?;
        let dict = value.as_dict().ok_or_else(|| {
            malformed(format!(
                "article bead object {} {} is not a dictionary",
                current.0, current.1
            ))
        })?;
        match dict.get("T") {
            Some(thread) => {
                let (num, _generation) = thread
                    .as_ref()
                    .ok_or_else(|| malformed("article bead /T is not an indirect reference"))?;
                if num != thread_num {
                    return Err(malformed(format!(
                        "article bead belongs to thread object {num}, expected {thread_num}"
                    )));
                }
            }
            None if beads.is_empty() => {
                return Err(malformed("first article bead has no /T"));
            }
            None => {}
        }
        let page_ref = dict
            .get("P")
            .ok_or_else(|| malformed("article bead has no /P"))?;
        let (page_num, page_generation) = page_ref
            .as_ref()
            .ok_or_else(|| malformed("article bead /P is not an indirect reference"))?;
        let page = pages
            .index_of(page_num, page_generation)
            .ok_or(NavigationError::PageNotFound)?;
        let rect = read_rect(dict.get("R"))?;
        let prev = match dict.get("V") {
            Some(value) => Some(
                value
                    .as_ref()
                    .ok_or_else(|| malformed("article bead /V is not an indirect reference"))?,
            ),
            None => None,
        };
        let next = dict
            .get("N")
            .ok_or_else(|| malformed("article bead has no /N"))?;
        let next = next
            .as_ref()
            .ok_or_else(|| malformed("article bead /N is not an indirect reference"))?;

        beads.push(ArticleBead { page, rect });
        refs.push(current);
        previous.push(prev);
        if next == first {
            break;
        }
        current = next;
    }

    let last = refs[refs.len() - 1];
    for (index, prev) in previous.iter().enumerate() {
        let expected = if index == 0 { last } else { refs[index - 1] };
        match prev {
            None => return Err(malformed("article bead has no /V")),
            Some(actual) if actual.0 != expected.0 => {
                return Err(malformed(format!(
                    "article bead /V points to object {}, expected {}",
                    actual.0, expected.0
                )));
            }
            Some(_) => {}
        }
    }
    Ok(beads)
}

fn read_rect(value: Option<&Value>) -> Result<[f64; 4], NavigationError> {
    let value = value.ok_or_else(|| malformed("article bead has no /R"))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("article bead /R is not an array"))?;
    if array.len() != 4 {
        return Err(malformed("article bead /R must have four numbers"));
    }
    let mut rect = [0.0f64; 4];
    for (slot, item) in rect.iter_mut().zip(&array.0) {
        let number = match item {
            Value::Integer(number) => *number as f64,
            Value::Float(number) => *number,
            _ => return Err(malformed("article bead /R must be numeric")),
        };
        if !number.is_finite() {
            return Err(malformed("article bead /R must be finite"));
        }
        *slot = number;
    }
    Ok(rect)
}

#[cfg(test)]
mod tests;
