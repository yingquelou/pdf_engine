//! 页面树索引：按顺序收集页面对象引用，供目标解析与命名页面使用。

use std::collections::HashSet;

use crate::core::types::{Document, Value};

use super::{MAX_TREE_DEPTH, NavigationError, catalog, malformed, resolve};

/// 页面树中各页面的间接引用（直接嵌入的页面字典保留 `None`）。
#[derive(Debug)]
pub(crate) struct PageTreeIndex {
    pages: Vec<Option<(i64, i64)>>,
}

impl PageTreeIndex {
    pub(crate) fn len(&self) -> usize {
        self.pages.len()
    }

    pub(crate) fn index_of(&self, num: i64, generation: i64) -> Option<usize> {
        self.pages
            .iter()
            .position(|entry| *entry == Some((num, generation)))
    }
}

/// 遍历目录 `/Pages` 树，按文档顺序收集页面引用。
pub(crate) fn page_tree_index(document: &Document) -> Result<PageTreeIndex, NavigationError> {
    let catalog = catalog(document)?;
    let pages = catalog
        .get("Pages")
        .ok_or_else(|| malformed("catalog has no /Pages"))?;
    let root_key = pages.as_ref();
    let root = resolve(document, pages)?;
    let root = root
        .as_dict()
        .ok_or_else(|| malformed("/Pages is not a dictionary"))?;

    let mut visited: HashSet<(i64, i64)> = HashSet::new();
    if let Some(key) = root_key {
        visited.insert(key);
    }

    let mut pages_out = Vec::new();
    let mut stack = vec![(Value::Dict(root.clone()), 0usize)];
    while let Some((value, depth)) = stack.pop() {
        if depth > MAX_TREE_DEPTH {
            return Err(NavigationError::LimitExceeded("page tree depth"));
        }
        let node = resolve(document, &value)?;
        let node = node
            .as_dict()
            .ok_or_else(|| malformed("page tree node is not a dictionary"))?;
        match node.get("Type").and_then(Value::as_name) {
            Some("Page") => pages_out.push(value.as_ref()),
            Some("Pages") => {
                let Some(kids) = node.get("Kids") else {
                    continue;
                };
                let kids = resolve(document, kids)?;
                let kids = kids
                    .as_array()
                    .ok_or_else(|| malformed("/Kids is not an array"))?;
                for kid in kids.0.iter().rev() {
                    if let Some(key) = kid.as_ref()
                        && !visited.insert(key)
                    {
                        return Err(malformed("page tree /Kids cycle"));
                    }
                    stack.push((kid.clone(), depth + 1));
                }
            }
            other => {
                return Err(malformed(format!(
                    "page tree node has unexpected /Type {other:?}"
                )));
            }
        }
    }
    Ok(PageTreeIndex { pages: pages_out })
}
