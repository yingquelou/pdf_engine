//! PDF 名称树（Name Tree）与数值树（Number Tree）。
//!
//! 两种树结构用于将字符串或整数映射到 PDF 对象，常见于文档目录的
//! `/Names`（嵌入文件、JavaScript 等）和 `/Dests`（命名目标）。
//!
//! 树节点可以是叶子（含 `/Names` 或 `/Nums` 数组）或中间节点
//! （含 `/Kids` 数组指向子节点）。每个节点都有 `/Limits` 指定其
//! 键值范围。

use crate::core::types::{Array, Dict, Document, Value};
use std::collections::BTreeMap;

/// 解析间接引用：如果值是 `Ref`，则从文档获取对应对象；否则直接返回。
///
/// 按对象号取最新修订，忽略引用中的代号：增量更新会把变更对象升代
/// 写出，而历史引用仍写旧代号。
fn resolve_value(document: &Document, value: &Value) -> Result<Value, String> {
    match value {
        Value::Ref(num, generation) => document
            .get_object(*num)
            .map_err(|e| format!("failed to get object {num}: {e}"))?
            .map(|o| o.value)
            .ok_or_else(|| format!("object {num} generation {generation} not found")),
        _ => Ok(value.clone()),
    }
}

/// 名称树：字符串键 → 值。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NameTree {
    entries: BTreeMap<String, Value>,
}

impl NameTree {
    /// 创建空名称树。
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// 从 PDF 字典解析名称树。
    ///
    /// 支持叶子节点（`/Names` 数组）和中间节点（`/Kids` 数组递归）。
    /// `/Limits` 用于验证但不影响解析逻辑。
    pub fn parse(document: &Document, dict: &Dict) -> Result<Self, String> {
        let mut entries = BTreeMap::new();
        Self::parse_node(document, dict, &mut entries)?;
        Ok(Self { entries })
    }

    fn parse_node(
        document: &Document,
        dict: &Dict,
        entries: &mut BTreeMap<String, Value>,
    ) -> Result<(), String> {
        // 中间节点：递归处理 /Kids
        if let Some(kids_value) = dict.get("Kids") {
            let kids_value = resolve_value(document, kids_value)?;
            let kids = kids_value
                .as_array()
                .ok_or_else(|| "/Kids is not an array".to_string())?;
            for kid_value in kids.iter() {
                let kid = resolve_value(document, kid_value)?;
                let kid_dict = kid
                    .as_dict()
                    .ok_or_else(|| "kid is not a dictionary".to_string())?;
                Self::parse_node(document, kid_dict, entries)?;
            }
            return Ok(());
        }

        // 叶子节点：解析 /Names 数组 [key1 value1 key2 value2 ...]
        if let Some(names_value) = dict.get("Names") {
            let names_value = resolve_value(document, names_value)?;
            let names = names_value
                .as_array()
                .ok_or_else(|| "/Names is not an array".to_string())?;
            let items = &names.0;
            if items.len() % 2 != 0 {
                return Err("/Names array length is not even".to_string());
            }
            for chunk in items.chunks(2) {
                let key = match &chunk[0] {
                    Value::String(s) => s.clone(),
                    Value::HexString(s) => s.clone(),
                    _ => return Err("/Names key is not a string".to_string()),
                };
                entries.insert(key, chunk[1].clone());
            }
            return Ok(());
        }

        Err("tree node has neither /Kids nor /Names".to_string())
    }

    /// 按键查找值。
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.entries.get(name)
    }

    /// 迭代所有 (键, 值) 对，按键排序。
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.entries.iter()
    }

    /// 条目数量。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 插入或更新条目（用于构造）。
    pub fn insert(&mut self, name: impl Into<String>, value: Value) {
        self.entries.insert(name.into(), value);
    }

    /// 转换为 PDF 字典（叶子节点形式）。
    ///
    /// 生成的字典含 `/Names` 数组，按键排序。
    pub fn to_dict(&self) -> Dict {
        let mut names = Array::new();
        for (key, value) in &self.entries {
            names = names.push(Value::String(key.clone()));
            names = names.push(value.clone());
        }
        Dict::new().insert("Names", Value::Array(names))
    }
}

/// 数值树：整数键 → 值。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NumberTree {
    entries: BTreeMap<i32, Value>,
}

impl NumberTree {
    /// 创建空数值树。
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// 从 PDF 字典解析数值树。
    ///
    /// 支持叶子节点（`/Nums` 数组）和中间节点（`/Kids` 数组递归）。
    pub fn parse(document: &Document, dict: &Dict) -> Result<Self, String> {
        let mut entries = BTreeMap::new();
        Self::parse_node(document, dict, &mut entries)?;
        Ok(Self { entries })
    }

    fn parse_node(
        document: &Document,
        dict: &Dict,
        entries: &mut BTreeMap<i32, Value>,
    ) -> Result<(), String> {
        // 中间节点：递归处理 /Kids
        if let Some(kids_value) = dict.get("Kids") {
            let kids_value = resolve_value(document, kids_value)?;
            let kids = kids_value
                .as_array()
                .ok_or_else(|| "/Kids is not an array".to_string())?;
            for kid_value in kids.iter() {
                let kid = resolve_value(document, kid_value)?;
                let kid_dict = kid
                    .as_dict()
                    .ok_or_else(|| "kid is not a dictionary".to_string())?;
                Self::parse_node(document, kid_dict, entries)?;
            }
            return Ok(());
        }

        // 叶子节点：解析 /Nums 数组 [key1 value1 key2 value2 ...]
        if let Some(nums_value) = dict.get("Nums") {
            let nums_value = resolve_value(document, nums_value)?;
            let nums = nums_value
                .as_array()
                .ok_or_else(|| "/Nums is not an array".to_string())?;
            let items = &nums.0;
            if items.len() % 2 != 0 {
                return Err("/Nums array length is not even".to_string());
            }
            for chunk in items.chunks(2) {
                let key = chunk[0]
                    .as_integer()
                    .ok_or_else(|| "/Nums key is not an integer".to_string())?;
                entries.insert(key as i32, chunk[1].clone());
            }
            return Ok(());
        }

        Err("tree node has neither /Kids nor /Nums".to_string())
    }

    /// 按键查找值。
    pub fn get(&self, num: i32) -> Option<&Value> {
        self.entries.get(&num)
    }

    /// 迭代所有 (键, 值) 对，按键排序。
    pub fn iter(&self) -> impl Iterator<Item = (&i32, &Value)> {
        self.entries.iter()
    }

    /// 条目数量。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 插入或更新条目（用于构造）。
    pub fn insert(&mut self, num: i32, value: Value) {
        self.entries.insert(num, value);
    }

    /// 转换为 PDF 字典（叶子节点形式）。
    ///
    /// 生成的字典含 `/Nums` 数组，按键排序。
    pub fn to_dict(&self) -> Dict {
        let mut nums = Array::new();
        for (key, value) in &self.entries {
            nums = nums.push(Value::Integer(*key as i64));
            nums = nums.push(value.clone());
        }
        Dict::new().insert("Nums", Value::Array(nums))
    }
}
