//! 逻辑结构树的解析实现（ISO 32000-1 14.7.2–14.7.4.4）。
//!
//! 结构元素按前序展开到 arena：解析一个元素时先占位，再解析 `/K`，
//! 使子元素能记录父元素的 arena 下标。父树与 ID 树在遍历完成后读取，
//! 引用按对象号匹配 arena（generation 容错）。所有引用经
//! [`crate::core::reader::navigation::resolve`] 解引用。

use std::collections::{BTreeMap, HashSet};

use crate::core::reader::navigation::{catalog, name_tree_entries, number_tree_entries};
use crate::core::types::{Dict, Document, Value};

use super::entries::{
    id_bytes, optional_text, parse_attributes, parse_class_map, parse_classes, parse_revision,
    parse_role_map,
};
use super::{
    MAX_STRUCTURE_CHILDREN, MAX_STRUCTURE_DEPTH, MAX_STRUCTURE_ELEMENTS, MAX_STRUCTURE_MAP_ENTRIES,
    MarkInfo, MarkedContentReference, ObjectReference, ParentTreeEntry, StructureElement,
    StructureError, StructureKid, StructureTree, StructureTreeRoot, malformed, resolve_value,
};

/// 目录 `/MarkInfo`（Table 321）。
pub(super) fn mark_info(document: &Document) -> Result<Option<MarkInfo>, StructureError> {
    let catalog = catalog(document).map_err(|error| malformed(error.to_string()))?;
    let Some(value) = catalog.get("MarkInfo") else {
        return Ok(None);
    };
    let value = resolve_value(document, value, "/MarkInfo")?;
    let dict = value
        .as_dict()
        .cloned()
        .ok_or_else(|| malformed("/MarkInfo is not a dictionary"))?;
    Ok(Some(MarkInfo {
        marked: boolean_entry(&dict, "Marked")?,
        user_properties: boolean_entry(&dict, "UserProperties")?,
        suspects: boolean_entry(&dict, "Suspects")?,
        dict,
    }))
}

/// 可选布尔条目，缺省 false；非布尔为 `Malformed`。
fn boolean_entry(dict: &Dict, key: &str) -> Result<bool, StructureError> {
    match dict.get(key) {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(other) => Err(malformed(format!(
            "/MarkInfo /{key} must be a boolean ({})",
            other.type_name()
        ))),
    }
}

/// 目录 `/StructTreeRoot`（Table 322）。
pub(super) fn structure_tree(document: &Document) -> Result<Option<StructureTree>, StructureError> {
    let catalog = catalog(document).map_err(|error| malformed(error.to_string()))?;
    let Some(root_value) = catalog.get("StructTreeRoot") else {
        return Ok(None);
    };
    let root_ref = root_value.as_ref();
    let resolved = resolve_value(document, root_value, "/StructTreeRoot")?;
    let root_dict = resolved
        .as_dict()
        .cloned()
        .ok_or_else(|| malformed("/StructTreeRoot is not a dictionary"))?;
    if let Some(value) = root_dict.get("Type")
        && value.as_name() != Some("StructTreeRoot")
    {
        return Err(malformed(format!(
            "/StructTreeRoot /Type is not StructTreeRoot ({})",
            value.type_name()
        )));
    }

    let role_map = parse_role_map(document, &root_dict)?;
    let class_map = parse_class_map(document, &root_dict)?;
    let parent_tree_next_key = match root_dict.get("ParentTreeNextKey") {
        None => None,
        Some(Value::Integer(key)) => Some(*key),
        Some(other) => {
            return Err(malformed(format!(
                "/ParentTreeNextKey must be an integer ({})",
                other.type_name()
            )));
        }
    };

    let mut builder = TreeBuilder {
        document,
        elements: Vec::new(),
        by_object: BTreeMap::new(),
        visited: HashSet::new(),
        root_ref,
    };
    let kids = builder.parse_kids(root_dict.get("K"), None, None, None, 0)?;
    let id_tree = parse_id_tree(document, &root_dict, &builder)?;
    let parent_tree = parse_parent_tree(document, &root_dict, &builder)?;

    // 类属性在元素解析完成后按类名回填：`/ClassMap` 与元素可以任意顺序
    // 出现；类名缺失时不报错，只留空列表（14.7.5.2 类属性没有继承）。
    let mut elements = builder.elements;
    for element in &mut elements {
        for class in &mut element.classes {
            class.attributes = class_map.get(&class.name).cloned().unwrap_or_default();
        }
    }

    Ok(Some(StructureTree {
        root: StructureTreeRoot {
            kids,
            id_tree,
            parent_tree,
            parent_tree_next_key,
            role_map,
            class_map,
            dict: root_dict,
        },
        elements,
        by_object: builder.by_object,
    }))
}

/// 结构元素展开状态。
struct TreeBuilder<'a> {
    document: &'a Document,
    elements: Vec<StructureElement>,
    by_object: BTreeMap<(i64, i64), usize>,
    visited: HashSet<i64>,
    root_ref: Option<(i64, i64)>,
}

impl TreeBuilder<'_> {
    /// 按对象号在 arena 中查找元素（generation 容错）。
    fn element_index_by_object(&self, object: (i64, i64)) -> Option<usize> {
        if let Some(&index) = self.by_object.get(&object) {
            return Some(index);
        }
        self.by_object
            .iter()
            .find(|((number, _), _)| *number == object.0)
            .map(|(_, &index)| index)
    }

    /// 解析 `/K`（缺省为空）；单值视作单条目，数组逐项分派。
    fn parse_kids(
        &mut self,
        value: Option<&Value>,
        parent: Option<usize>,
        parent_object: Option<(i64, i64)>,
        parent_page: Option<(i64, i64)>,
        depth: usize,
    ) -> Result<Vec<StructureKid>, StructureError> {
        let Some(value) = value else {
            return Ok(Vec::new());
        };
        let entries: Vec<&Value> = match value {
            Value::Array(array) => array.0.iter().collect(),
            other => vec![other],
        };
        if entries.len() > MAX_STRUCTURE_CHILDREN {
            return Err(StructureError::LimitExceeded("structure child count"));
        }
        let mut kids = Vec::with_capacity(entries.len());
        for entry in entries {
            kids.push(self.parse_kid(entry, parent, parent_object, parent_page, depth)?);
        }
        Ok(kids)
    }

    /// 分派一个 `/K` 条目。
    fn parse_kid(
        &mut self,
        value: &Value,
        parent: Option<usize>,
        parent_object: Option<(i64, i64)>,
        parent_page: Option<(i64, i64)>,
        depth: usize,
    ) -> Result<StructureKid, StructureError> {
        if matches!(value, Value::Array(_)) {
            return Err(malformed("nested /K arrays are not allowed"));
        }
        let (object, value) = match value {
            Value::Ref(number, generation) => (
                Some((*number, *generation)),
                resolve_value(self.document, value, "/K entry")?,
            ),
            other => (None, other.clone()),
        };
        match value {
            Value::Integer(mcid) => {
                if mcid < 0 {
                    return Err(malformed("/K MCID must be a non-negative integer"));
                }
                Ok(StructureKid::Mcid(mcid))
            }
            Value::Dict(dict) => {
                self.parse_kid_dict(&dict, object, parent, parent_object, parent_page, depth)
            }
            Value::Array(_) => Err(malformed("nested /K arrays are not allowed")),
            other => Err(malformed(format!(
                "/K entry must be an integer, structure element, MCR or OBJR dictionary ({})",
                other.type_name()
            ))),
        }
    }

    /// 按 `/Type` 分派 `/K` 字典：缺省与 `StructElem` 为结构元素。
    fn parse_kid_dict(
        &mut self,
        dict: &Dict,
        object: Option<(i64, i64)>,
        parent: Option<usize>,
        parent_object: Option<(i64, i64)>,
        parent_page: Option<(i64, i64)>,
        depth: usize,
    ) -> Result<StructureKid, StructureError> {
        let kind = match dict.get("Type") {
            None => None,
            Some(Value::Name(name)) => Some(name.as_str()),
            Some(other) => {
                return Err(malformed(format!(
                    "/K entry /Type must be a name ({})",
                    other.type_name()
                )));
            }
        };
        match kind {
            None | Some("StructElem") => self
                .parse_element(dict, object, parent, parent_object, depth)
                .map(StructureKid::Element),
            Some("MCR") => self
                .parse_mcr(dict, parent_page)
                .map(StructureKid::MarkedContent),
            Some("OBJR") => self.parse_objr(dict).map(StructureKid::ObjectReference),
            Some(other) => Err(malformed(format!("/K entry has unknown /Type /{other}"))),
        }
    }

    /// 解析结构元素（Table 323）并递归展开 `/K`。
    fn parse_element(
        &mut self,
        dict: &Dict,
        object: Option<(i64, i64)>,
        parent: Option<usize>,
        parent_object: Option<(i64, i64)>,
        depth: usize,
    ) -> Result<usize, StructureError> {
        if depth >= MAX_STRUCTURE_DEPTH {
            return Err(StructureError::LimitExceeded("structure tree depth"));
        }
        if self.elements.len() >= MAX_STRUCTURE_ELEMENTS {
            return Err(StructureError::LimitExceeded("structure element count"));
        }
        if let Some((number, _)) = object
            && !self.visited.insert(number)
        {
            return Err(malformed(format!(
                "structure element cycle at object {number}"
            )));
        }
        if let Some(value) = dict.get("Type")
            && value.as_name() != Some("StructElem")
        {
            return Err(malformed(format!(
                "structure element /Type is not StructElem ({})",
                value.type_name()
            )));
        }
        let structure_type = match dict.get("S") {
            Some(Value::Name(name)) if !name.is_empty() => name.clone(),
            Some(Value::Name(_)) => {
                return Err(malformed("structure element /S must be a non-empty name"));
            }
            Some(other) => {
                return Err(malformed(format!(
                    "structure element /S must be a name ({})",
                    other.type_name()
                )));
            }
            None => return Err(malformed("structure element /S is required")),
        };

        let parent_ref = required_ref(self.document, dict, "P", "structure element")?;
        let expected = if parent.is_some() {
            parent_object
        } else {
            self.root_ref
        };
        if let Some((expected_number, _)) = expected
            && parent_ref.0 != expected_number
        {
            return Err(malformed(format!(
                "structure element /P must reference its parent object {expected_number}"
            )));
        }

        let page = optional_ref(self.document, dict, "Pg", "structure element")?;
        let id = match dict.get("ID") {
            Some(value) => Some(id_bytes(value)?),
            None => None,
        };
        let title = optional_text(dict, "T")?;
        let lang = optional_text(dict, "Lang")?;
        let alt = optional_text(dict, "Alt")?;
        let expanded = optional_text(dict, "E")?;
        let actual_text = optional_text(dict, "ActualText")?;
        let classes = parse_classes(self.document, dict)?;
        let attributes = parse_attributes(self.document, dict)?;
        let revision = parse_revision(dict)?;

        let index = self.elements.len();
        self.elements.push(StructureElement {
            object,
            structure_type,
            parent,
            page,
            id,
            title,
            lang,
            alt,
            expanded,
            actual_text,
            classes,
            attributes,
            revision,
            kids: Vec::new(),
            dict: dict.clone(),
        });
        if let Some(reference) = object {
            self.by_object.insert(reference, index);
        }
        let kids = self.parse_kids(dict.get("K"), Some(index), object, page, depth + 1)?;
        self.elements[index].kids = kids;
        Ok(index)
    }

    /// 解析 MCR（Table 324）。
    fn parse_mcr(
        &self,
        dict: &Dict,
        parent_page: Option<(i64, i64)>,
    ) -> Result<MarkedContentReference, StructureError> {
        let page = optional_ref(self.document, dict, "Pg", "MCR")?;
        let stream = optional_ref(self.document, dict, "Stm", "MCR")?;
        let stream_own = optional_ref(self.document, dict, "StmOwn", "MCR")?;
        let mcid = match dict.get("MCID") {
            Some(Value::Integer(mcid)) => *mcid,
            Some(other) => {
                return Err(malformed(format!(
                    "MCR /MCID must be an integer ({})",
                    other.type_name()
                )));
            }
            None => return Err(malformed("MCR /MCID is required (Table 324)")),
        };
        if page.is_none() && parent_page.is_none() {
            return Err(malformed(
                "MCR has no /Pg and the parent structure element has no /Pg",
            ));
        }
        Ok(MarkedContentReference {
            page,
            stream,
            stream_own,
            mcid,
            dict: dict.clone(),
        })
    }

    /// 解析 OBJR（Table 325）。
    fn parse_objr(&self, dict: &Dict) -> Result<ObjectReference, StructureError> {
        let page = optional_ref(self.document, dict, "Pg", "OBJR")?;
        let object = required_ref(self.document, dict, "Obj", "OBJR")?;
        Ok(ObjectReference {
            page,
            object,
            dict: dict.clone(),
        })
    }
}

/// 必填间接引用：先校验 `Value::Ref`，再解引用验证目标存在。
fn required_ref(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<(i64, i64), StructureError> {
    let Some(value) = dict.get(key) else {
        return Err(malformed(format!("{context} /{key} is required")));
    };
    let Some(reference) = value.as_ref() else {
        return Err(malformed(format!(
            "{context} /{key} must be an indirect reference"
        )));
    };
    resolve_value(document, value, &format!("{context} /{key}"))?;
    Ok(reference)
}

/// 可选间接引用：存在时必须是 `Value::Ref` 并能解引用。
fn optional_ref(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<(i64, i64)>, StructureError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let Some(reference) = value.as_ref() else {
        return Err(malformed(format!(
            "{context} /{key} must be an indirect reference"
        )));
    };
    resolve_value(document, value, &format!("{context} /{key}"))?;
    Ok(Some(reference))
}

/// `/IDTree`：名称树，元素 ID 原始字节 → arena 下标。
fn parse_id_tree(
    document: &Document,
    root: &Dict,
    builder: &TreeBuilder<'_>,
) -> Result<BTreeMap<Vec<u8>, usize>, StructureError> {
    let Some(value) = root.get("IDTree") else {
        return Ok(BTreeMap::new());
    };
    let value = resolve_value(document, value, "/IDTree")?;
    let dict = value
        .as_dict()
        .ok_or_else(|| malformed("/IDTree is not a name tree dictionary"))?;
    let entries = name_tree_entries(document, dict)
        .map_err(|error| malformed(format!("/IDTree: {error}")))?;
    if entries.len() > MAX_STRUCTURE_MAP_ENTRIES {
        return Err(StructureError::LimitExceeded("structure ID tree entries"));
    }
    let mut tree = BTreeMap::new();
    for (raw_key, value) in entries {
        let key: Vec<u8> = raw_key
            .chars()
            .map(|character| character as u32 as u8)
            .collect();
        let Some(reference) = value.as_ref() else {
            return Err(malformed(
                "/IDTree entry value must be an indirect reference",
            ));
        };
        resolve_value(document, &value, "/IDTree entry")?;
        let index = builder.element_index_by_object(reference).ok_or_else(|| {
            malformed(format!(
                "/IDTree entry references object {} {} which is not a structure element",
                reference.0, reference.1
            ))
        })?;
        // 重复 ID 后者覆盖，不报错。
        tree.insert(key, index);
    }
    Ok(tree)
}

/// `/ParentTree`：数值树，键 → OBJR 父元素或页面 MCID 数组。
fn parse_parent_tree(
    document: &Document,
    root: &Dict,
    builder: &TreeBuilder<'_>,
) -> Result<BTreeMap<i64, ParentTreeEntry>, StructureError> {
    let Some(value) = root.get("ParentTree") else {
        return Ok(BTreeMap::new());
    };
    let value = resolve_value(document, value, "/ParentTree")?;
    let dict = value
        .as_dict()
        .ok_or_else(|| malformed("/ParentTree is not a number tree dictionary"))?;
    let entries = number_tree_entries(document, dict)
        .map_err(|error| malformed(format!("/ParentTree: {error}")))?;
    if entries.len() > MAX_STRUCTURE_MAP_ENTRIES {
        return Err(StructureError::LimitExceeded("parent tree entries"));
    }
    let mut tree = BTreeMap::new();
    for (key, value) in entries {
        let entry = match &value {
            Value::Ref(..) => {
                resolve_value(document, &value, "parent tree entry")?;
                let reference = value
                    .as_ref()
                    .expect("parent tree entry matched Value::Ref");
                let index = parent_element_index(builder, reference, "parent tree entry")?;
                ParentTreeEntry::Object(index)
            }
            Value::Array(items) => {
                let mut parents = Vec::with_capacity(items.0.len());
                for item in &items.0 {
                    match item {
                        Value::Null => parents.push(None),
                        Value::Ref(number, generation) => {
                            resolve_value(document, item, "parent tree array entry")?;
                            let index = parent_element_index(
                                builder,
                                (*number, *generation),
                                "parent tree array entry",
                            )?;
                            parents.push(Some(index));
                        }
                        other => {
                            return Err(malformed(format!(
                                "parent tree array entry must be an indirect reference or null ({})",
                                other.type_name()
                            )));
                        }
                    }
                }
                ParentTreeEntry::Page(parents)
            }
            other => {
                return Err(malformed(format!(
                    "parent tree entry must be an indirect reference or array ({})",
                    other.type_name()
                )));
            }
        };
        tree.insert(key, entry);
    }
    Ok(tree)
}

fn parent_element_index(
    builder: &TreeBuilder<'_>,
    reference: (i64, i64),
    context: &str,
) -> Result<usize, StructureError> {
    builder.element_index_by_object(reference).ok_or_else(|| {
        malformed(format!(
            "{context} references object {} {} which is not a structure element",
            reference.0, reference.1
        ))
    })
}
