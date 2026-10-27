//! 名称树（`/Names`）与数值树（`/Nums`）的受控遍历。
//!
//! 与 `types::NameTree`/`NumberTree` 的通用解析不同，这里对 `/Kids`
//! 施加深度、节点数与环限制，返回按键排序的原始值供语义层解析。

use std::collections::{BTreeMap, HashSet};

use crate::core::types::{Dict, Document, Value};

use super::{MAX_TREE_DEPTH, MAX_TREE_NODES, NavigationError, Result, malformed, resolve};

struct TreeWalker<'a> {
    document: &'a Document,
    visited: HashSet<(i64, i64)>,
    nodes: usize,
}

/// 读取名称树所有条目；键为 PDF 字符串的原始字节（每字节一个 `char`）。
pub(crate) fn name_tree_entries(
    document: &Document,
    root: &Dict,
) -> Result<BTreeMap<String, Value>> {
    let mut walker = TreeWalker {
        document,
        visited: HashSet::new(),
        nodes: 0,
    };
    let mut entries = BTreeMap::new();
    walker.walk_name(root, 0, &mut entries)?;
    Ok(entries)
}

/// 读取数值树所有条目。
pub(crate) fn number_tree_entries(
    document: &Document,
    root: &Dict,
) -> Result<BTreeMap<i64, Value>> {
    let mut walker = TreeWalker {
        document,
        visited: HashSet::new(),
        nodes: 0,
    };
    let mut entries = BTreeMap::new();
    walker.walk_number(root, 0, &mut entries)?;
    Ok(entries)
}

impl TreeWalker<'_> {
    fn enter(&mut self, kind: &str, depth: usize) -> Result<()> {
        if depth > MAX_TREE_DEPTH {
            return Err(NavigationError::LimitExceeded(match kind {
                "name" => "name tree depth",
                _ => "number tree depth",
            }));
        }
        self.nodes += 1;
        if self.nodes > MAX_TREE_NODES {
            return Err(NavigationError::LimitExceeded(match kind {
                "name" => "name tree node count",
                _ => "number tree node count",
            }));
        }
        Ok(())
    }

    fn walk_name(
        &mut self,
        node: &Dict,
        depth: usize,
        entries: &mut BTreeMap<String, Value>,
    ) -> Result<()> {
        self.enter("name", depth)?;
        if let Some(kids) = node.get("Kids") {
            let kids = resolve(self.document, kids)?;
            let kids = kids
                .as_array()
                .ok_or_else(|| malformed("/Kids is not an array"))?;
            for kid in &kids.0 {
                if let Some(key) = kid.as_ref()
                    && !self.visited.insert(key)
                {
                    return Err(malformed("name tree /Kids cycle"));
                }
                let kid = resolve(self.document, kid)?;
                let kid = kid
                    .as_dict()
                    .ok_or_else(|| malformed("name tree kid is not a dictionary"))?;
                self.walk_name(kid, depth + 1, entries)?;
            }
            return Ok(());
        }
        let Some(names) = node.get("Names") else {
            return Err(malformed("name tree node has neither /Kids nor /Names"));
        };
        let names = resolve(self.document, names)?;
        let names = names
            .as_array()
            .ok_or_else(|| malformed("/Names is not an array"))?;
        if names.len() % 2 != 0 {
            return Err(malformed("/Names array length is not even"));
        }
        for pair in names.0.chunks(2) {
            let key =
                string_key(&pair[0]).ok_or_else(|| malformed("name tree key is not a string"))?;
            entries.insert(key, pair[1].clone());
        }
        Ok(())
    }

    fn walk_number(
        &mut self,
        node: &Dict,
        depth: usize,
        entries: &mut BTreeMap<i64, Value>,
    ) -> Result<()> {
        self.enter("number", depth)?;
        if let Some(kids) = node.get("Kids") {
            let kids = resolve(self.document, kids)?;
            let kids = kids
                .as_array()
                .ok_or_else(|| malformed("/Kids is not an array"))?;
            for kid in &kids.0 {
                if let Some(key) = kid.as_ref()
                    && !self.visited.insert(key)
                {
                    return Err(malformed("number tree /Kids cycle"));
                }
                let kid = resolve(self.document, kid)?;
                let kid = kid
                    .as_dict()
                    .ok_or_else(|| malformed("number tree kid is not a dictionary"))?;
                self.walk_number(kid, depth + 1, entries)?;
            }
            return Ok(());
        }
        let Some(nums) = node.get("Nums") else {
            return Err(malformed("number tree node has neither /Kids nor /Nums"));
        };
        let nums = resolve(self.document, nums)?;
        let nums = nums
            .as_array()
            .ok_or_else(|| malformed("/Nums is not an array"))?;
        if nums.len() % 2 != 0 {
            return Err(malformed("/Nums array length is not even"));
        }
        for pair in nums.0.chunks(2) {
            let key = pair[0]
                .as_integer()
                .ok_or_else(|| malformed("number tree key is not an integer"))?;
            entries.insert(key, pair[1].clone());
        }
        Ok(())
    }
}

/// PDF 字符串值 → 原始字节串（每字节一个 `char`）；十六进制奇数位补 0。
pub(crate) fn string_key(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::HexString(hex) => {
            hex_bytes(hex).map(|bytes| bytes.iter().map(|&b| b as char).collect())
        }
        _ => None,
    }
}

fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let padded;
    let digits = if digits.len() % 2 == 1 {
        padded = {
            let mut value = digits.clone();
            value.push(b'0');
            value
        };
        &padded[..]
    } else {
        &digits[..]
    };
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        bytes.push((high * 16 + low) as u8);
    }
    Some(bytes)
}
