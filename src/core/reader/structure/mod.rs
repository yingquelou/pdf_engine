//! 逻辑结构（ISO 32000-1 14.7）读端模型。
//!
//! 覆盖目录 `/MarkInfo`（14.7.1 Table 321）、结构树根 `/StructTreeRoot`
//! （14.7.2 Table 322）、结构元素（Table 323）、标记内容引用 MCR
//! （Table 324）与对象引用 OBJR（Table 325），以及父树（14.7.4.4
//! Table 326）的页面/对象键映射。
//!
//! 结构元素按前序展开到 arena：元素之间用 arena 下标互相引用，父树与
//! ID 树在遍历完成后按对象号解析为 arena 下标。所有引用经
//! [`crate::core::reader::navigation::resolve`] 解引用并按对象号取最新
//! 修订（忽略 generation），与库内既有语义一致。

use std::collections::BTreeMap;
use std::fmt;

use crate::core::types::{Dict, Document, Value};

mod entries;
mod parse;
#[cfg(test)]
mod tests;

/// 结构树最大嵌套深度（最多 64 层结构元素）。
const MAX_STRUCTURE_DEPTH: usize = 64;
/// 结构元素总数上限。
const MAX_STRUCTURE_ELEMENTS: usize = 4096;
/// 单个结构元素的 `/K` 子项总数上限。
const MAX_STRUCTURE_CHILDREN: usize = 65536;
/// 结构树各映射（ID 树、父树、RoleMap、ClassMap）条目上限。
const MAX_STRUCTURE_MAP_ENTRIES: usize = 4096;

/// 逻辑结构读取的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum StructureError {
    /// 条目缺失、类型错误或结构约束不满足。
    Malformed(String),
    /// 超出遍历深度、元素数或映射条目数上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for StructureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "invalid logical structure: {message}"),
            Self::LimitExceeded(what) => {
                write!(f, "logical structure limit exceeded: {what}")
            }
        }
    }
}

impl std::error::Error for StructureError {}

fn malformed(message: impl Into<String>) -> StructureError {
    StructureError::Malformed(message.into())
}

fn resolve_value(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Value, StructureError> {
    crate::core::reader::navigation::resolve(document, value)
        .map_err(|error| malformed(format!("{context}: {error}")))
}

/// 目录 `/MarkInfo`（14.7.1 Table 321）。
#[derive(Debug, Clone, PartialEq)]
pub struct MarkInfo {
    /// `/Marked`：文档是否符合标记内容约定（缺省 false）。
    pub marked: bool,
    /// `/UserProperties`：是否含用户属性（PDF 1.6，缺省 false）。
    pub user_properties: bool,
    /// `/Suspects`：是否含可疑标记内容（PDF 1.6，缺省 false）。
    pub suspects: bool,
    /// 原始字典（保留未建模条目）。
    pub dict: Dict,
}

/// 结构树根（14.7.2 Table 322）。
#[derive(Debug, Clone, PartialEq)]
pub struct StructureTreeRoot {
    /// `/K`：顶层结构元素或内容项。
    pub kids: Vec<StructureKid>,
    /// `/IDTree`：元素 ID → arena 下标。
    pub id_tree: BTreeMap<Vec<u8>, usize>,
    /// `/ParentTree`：父树键 → 父元素或 MCID 数组。
    pub parent_tree: BTreeMap<i64, ParentTreeEntry>,
    /// `/ParentTreeNextKey`：父树下一个可用键。
    pub parent_tree_next_key: Option<i64>,
    /// `/RoleMap`：结构类型 → 标准类型映射。
    pub role_map: BTreeMap<String, String>,
    /// `/ClassMap`：类名 → 属性对象，值原样保留。
    pub class_map: BTreeMap<String, Value>,
    /// 原始根字典（保留未建模条目）。
    pub dict: Dict,
}

/// 结构树中的内容项或子元素。
#[derive(Debug, Clone, PartialEq)]
pub enum StructureKid {
    /// 结构元素（arena 下标）。
    Element(usize),
    /// 整数 MCID（非负）。
    Mcid(i64),
    /// 标记内容引用字典（MCR，Table 324）。
    MarkedContent(MarkedContentReference),
    /// 对象引用字典（OBJR，Table 325）。
    ObjectReference(ObjectReference),
}

/// 标记内容引用（MCR，Table 324）。
#[derive(Debug, Clone, PartialEq)]
pub struct MarkedContentReference {
    /// `/Pg`：标记内容所在页面的间接引用。
    pub page: Option<(i64, i64)>,
    /// `/Stm`：内容流对象的间接引用。
    pub stream: Option<(i64, i64)>,
    /// `/StmOwn`：内容流所有者对象的间接引用。
    pub stream_own: Option<(i64, i64)>,
    /// `/MCID`：标记内容标识符。
    pub mcid: i64,
    /// 原始 MCR 字典。
    pub dict: Dict,
}

/// 对象引用（OBJR，Table 325）。
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectReference {
    /// `/Pg`：对象所在页面的间接引用。
    pub page: Option<(i64, i64)>,
    /// `/Obj`：被引用对象的间接引用。
    pub object: (i64, i64),
    /// 原始 OBJR 字典。
    pub dict: Dict,
}

/// 结构属性（Table 323 `/A` 条目）。
#[derive(Debug, Clone, PartialEq)]
pub struct StructureAttribute {
    /// 属性对象（已解引用的字典或流）。
    pub value: Value,
    /// 属性字典后的修订号。
    pub revision: Option<u32>,
}

/// 结构类（Table 323 `/C` 条目）。
#[derive(Debug, Clone, PartialEq)]
pub struct StructureClass {
    /// 类名。
    pub name: String,
    /// 类名后的修订号。
    pub revision: Option<u32>,
}

/// 结构元素（Table 323）。
#[derive(Debug, Clone, PartialEq)]
pub struct StructureElement {
    /// 以间接引用到达时记录的对象号与 generation；直接字典为 `None`。
    pub object: Option<(i64, i64)>,
    /// `/S`：结构类型。
    pub structure_type: String,
    /// 父元素 arena 下标；顶层元素为 `None`。
    pub parent: Option<usize>,
    /// `/Pg`：页面间接引用。
    pub page: Option<(i64, i64)>,
    /// `/ID`：元素标识（原始字节）。
    pub id: Option<Vec<u8>>,
    /// `/T`：标题。
    pub title: Option<String>,
    /// `/Lang`：语言。
    pub lang: Option<String>,
    /// `/Alt`：替代文本。
    pub alt: Option<String>,
    /// `/E`：展开缩写文本。
    pub expanded: Option<String>,
    /// `/ActualText`：实际文本。
    pub actual_text: Option<String>,
    /// `/C`：结构类列表。
    pub classes: Vec<StructureClass>,
    /// `/A`：属性列表。
    pub attributes: Vec<StructureAttribute>,
    /// `/R`：修订号（缺省 0）。
    pub revision: u32,
    /// `/K`：子元素与内容项。
    pub kids: Vec<StructureKid>,
    /// 原始结构元素字典（保留未建模条目）。
    pub dict: Dict,
}

/// 父树条目（14.7.4.4 Table 326）。
#[derive(Debug, Clone, PartialEq)]
pub enum ParentTreeEntry {
    /// OBJR 型内容项对象的父元素（arena 下标）。
    Object(usize),
    /// 页面/内容流的 MCID 下标数组；`None` 表示该 MCID 无父元素。
    Page(Vec<Option<usize>>),
}

/// 结构树（结构树根 + 结构元素 arena）。
#[derive(Debug, Clone, PartialEq)]
pub struct StructureTree {
    root: StructureTreeRoot,
    elements: Vec<StructureElement>,
    by_object: BTreeMap<(i64, i64), usize>,
}

impl StructureTree {
    /// 结构树根。
    pub fn root(&self) -> &StructureTreeRoot {
        &self.root
    }

    /// 全部结构元素（前序 arena）。
    pub fn elements(&self) -> &[StructureElement] {
        &self.elements
    }

    /// 按 arena 下标取结构元素。
    pub fn element(&self, index: usize) -> Option<&StructureElement> {
        self.elements.get(index)
    }

    /// 按对象号取结构元素；generation 不参与比较（容错旧引用）。
    pub fn element_by_object(&self, object: (i64, i64)) -> Option<&StructureElement> {
        if let Some(&index) = self.by_object.get(&object) {
            return self.elements.get(index);
        }
        self.by_object
            .iter()
            .find(|((number, _), _)| *number == object.0)
            .and_then(|(_, &index)| self.elements.get(index))
    }

    /// 按 `/ID` 字节串取结构元素。
    pub fn element_by_id(&self, id: &[u8]) -> Option<&StructureElement> {
        self.root
            .id_tree
            .get(id)
            .and_then(|&index| self.elements.get(index))
    }

    /// 按父树键取条目。
    pub fn parent_tree_entry(&self, key: i64) -> Option<&ParentTreeEntry> {
        self.root.parent_tree.get(&key)
    }

    /// 页面数组型父树条目中 `mcid` 对应的父元素 arena 下标。
    ///
    /// OBJR 单元型条目或越界/空位返回 `None`。
    pub fn mcid_parent(&self, parents_key: i64, mcid: usize) -> Option<usize> {
        match self.root.parent_tree.get(&parents_key)? {
            ParentTreeEntry::Page(parents) => parents.get(mcid).copied().flatten(),
            ParentTreeEntry::Object(_) => None,
        }
    }

    /// OBJR 单元型父树条目中的父元素 arena 下标。
    ///
    /// 页面数组型条目返回 `None`。
    pub fn object_parent(&self, parent_key: i64) -> Option<usize> {
        match self.root.parent_tree.get(&parent_key)? {
            ParentTreeEntry::Object(index) => Some(*index),
            ParentTreeEntry::Page(_) => None,
        }
    }

    /// 沿 `/RoleMap` 解析结构类型；链终止时返回最后一个映射名。
    ///
    /// 名字没有映射时返回 `None`。映射成环时停在环上最后一个尚未重复的
    /// 名字，不会 panic 或无限循环。
    pub fn resolved_role(&self, structure_type: &str) -> Option<&str> {
        let mut current = structure_type.to_owned();
        let mut last_key: Option<String> = None;
        let mut seen = std::collections::BTreeSet::new();
        seen.insert(current.clone());
        loop {
            let Some(next) = self.root.role_map.get(&current) else {
                return last_key
                    .as_ref()
                    .and_then(|key| self.root.role_map.get_key_value(key))
                    .map(|(_, value)| value.as_str());
            };
            if seen.contains(next) {
                return self
                    .root
                    .role_map
                    .get_key_value(&current)
                    .map(|(key, _)| key.as_str());
            }
            seen.insert(next.clone());
            last_key = Some(current.clone());
            current = next.clone();
        }
    }
}

/// 页面的结构父信息（页面 `/StructParents` 与父树页面数组）。
#[derive(Debug, Clone, PartialEq)]
pub struct PageStructureParents {
    /// `/StructParents`：父树键。
    pub key: i64,
    /// 父树数组：按 MCID 作零基下标，空位为 `None`。
    pub parents: Vec<Option<usize>>,
}

impl Document {
    /// 读取目录 `/MarkInfo`（14.7.1 Table 321）。
    ///
    /// 目录没有 `/MarkInfo` 时返回 `Ok(None)`；三个布尔条目缺省 false。
    pub fn mark_info(&self) -> Result<Option<MarkInfo>, StructureError> {
        parse::mark_info(self)
    }

    /// 读取目录 `/StructTreeRoot` 并展开逻辑结构树（14.7.2–14.7.4.4）。
    ///
    /// 目录没有 `/StructTreeRoot` 时返回 `Ok(None)`。
    pub fn structure_tree(&self) -> Result<Option<StructureTree>, StructureError> {
        parse::structure_tree(self)
    }

    /// 读取 `page_index`（0 基）页面的 `/StructParents` 并映射到父树数组。
    ///
    /// 页码越界或页面树损坏返回 [`StructureError::Malformed`]；页面没有
    /// `/StructParents` 时返回 `Ok(None)`；文档没有结构树或父树缺少该键
    /// 时也返回 `Malformed`。
    pub fn page_struct_parents(
        &self,
        page_index: usize,
    ) -> Result<Option<PageStructureParents>, StructureError> {
        entries::page_struct_parents(self, page_index)
    }
}
