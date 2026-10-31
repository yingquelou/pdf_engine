//! 逻辑结构写端（ISO 32000-1 14.7.1–14.7.4.4、Table 321–326）。
//!
//! 目录 `/StructTreeRoot`（Table 322）与 `/MarkInfo`（Table 321）、结构
//! 元素（Table 323）、MCR 内容项（Table 324）、父树（14.7.4.4）与
//! `/IDTree` 名称树都从现有最大对象号之后分配，不影响既有对象布局；
//! 页面字典追加 `/StructParents`（Table 326）。
//!
//! 校验先于任何对象号分配：深度、元素数、单元素子项数与映射条目上限
//! 直接复用读端常量；页索引越界、负 MCID、同页 MCID 重复、空结构类型、
//! 空角色映射键/目标、类映射取值非字典/流、空类名、ID 为空/重复/非
//! ASCII 一律返回 [`WriteError::InvalidObject`]。版本门禁：结构树 ≥1.3、
//! `/Lang` `/ActualText` ≥1.4、`/E` ≥1.5、`/MarkInfo` ≥1.4、
//! `/UserProperties` `/Suspects` ≥1.6，不满足返回
//! [`WriteError::UnsupportedVersion`]。两类失败均不写出任何字节。
//!
//! 本切片的范围限制：`/K` 只写出子元素引用与 MCR 字典（带显式 `/Pg` 与
//! `/MCID`），不写 OBJR、整数 MCID、`/A` 属性对象与 `/Stm` `/StmOwn`。

use std::collections::BTreeMap;

use crate::core::reader::structure::{
    MAX_STRUCTURE_CHILDREN, MAX_STRUCTURE_DEPTH, MAX_STRUCTURE_ELEMENTS, MAX_STRUCTURE_MAP_ENTRIES,
};
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::super::types::{
    StructureElement, StructureKid, StructureMarkInfo, StructureTree, text_string_value,
};
use super::invalid;

/// 写端 MCID 上限：父树数组按 MCID 作零基下标，与读端单元素子项上限
/// 取同值，避免单页父树数组无界膨胀（65535 为最大可用 MCID）。
const MAX_STRUCTURE_MCID: i64 = MAX_STRUCTURE_CHILDREN as i64 - 1;

/// 写出目录条目：结构树根引用与 `/MarkInfo` 字典（均为可选）。
#[derive(Debug, Default)]
pub(super) struct CatalogEntries {
    /// 目录 `/StructTreeRoot` 间接引用。
    pub(super) struct_tree_root: Option<Value>,
    /// 目录 `/MarkInfo` 直接字典。
    pub(super) mark_info: Option<Value>,
}

/// 展开后的结构元素（前序 arena）。
struct Flat<'a> {
    /// 原始结构元素。
    element: &'a StructureElement,
    /// 父元素 arena 下标；顶层为 `None`。
    parent: Option<usize>,
    /// 按文档顺序展开的子项（子元素已解析为 arena 下标）。
    kids: Vec<PlannedKid>,
}

/// 写端展开后的 `/K` 子项。
enum PlannedKid {
    /// 子结构元素 arena 下标。
    Element(usize),
    /// MCR：页索引 + MCID。
    MarkedContent { page: usize, mcid: i64 },
}

/// 校验并展开后的写出计划（不含对象号，尚未分配任何对象）。
struct Plan<'a> {
    flat: Vec<Flat<'a>>,
    /// 页面索引 → 父树键（按页升序 0..n-1）。
    page_keys: BTreeMap<usize, i64>,
    /// 父树键 → 按 MCID 索引的父元素 arena 下标（空位 `None`）。
    parents: BTreeMap<i64, Vec<Option<usize>>>,
    /// `/ID` 原始字节 → 元素 arena 下标（按字节升序）。
    ids: BTreeMap<Vec<u8>, usize>,
    /// 是否使用 `/Lang` 或 `/ActualText`（PDF 1.4）。
    uses_language: bool,
    /// 是否使用 `/E`（PDF 1.5）。
    uses_expanded: bool,
}

/// 把结构树与标记信息写入已组装的 `Document`，返回目录条目。
///
/// 所有校验在任何对象号分配之前完成。
pub(super) fn write(
    doc: &mut Document,
    version: PdfVersion,
    page_refs: &[(i64, i64)],
    tree: Option<&StructureTree>,
    mark_info: Option<&StructureMarkInfo>,
) -> Result<CatalogEntries, WriteError> {
    let mut entries = CatalogEntries::default();
    if let Some(info) = mark_info {
        if (info.user_properties || info.suspects)
            && !version.supports(PdfFeature::MarkInfoUserProperties)
        {
            return Err(WriteError::UnsupportedVersion(format!(
                "/MarkInfo /UserProperties and /Suspects require PDF {}, got {version}",
                PdfFeature::MarkInfoUserProperties.min_version()
            )));
        }
        entries.mark_info = Some(Value::Dict(mark_info_dict(info)));
        let mut features = doc.features.borrow_mut();
        features.insert(PdfFeature::MarkInfo);
        if info.user_properties {
            features.insert(PdfFeature::MarkInfoUserProperties);
        }
        if info.suspects {
            features.insert(PdfFeature::MarkInfoSuspects);
        }
    }
    let Some(tree) = tree else {
        return Ok(entries);
    };

    let plan = plan(tree, version, page_refs.len())?;

    // 全部校验通过后才分配对象号：根 + 前序元素。
    let mut next_num = doc
        .objects_for_write()
        .iter()
        .map(|object| object.num)
        .max()
        .unwrap_or(0)
        + 1;
    let root_num = next_num;
    next_num += 1;
    let element_nums: Vec<i64> = plan
        .flat
        .iter()
        .map(|_| {
            let num = next_num;
            next_num += 1;
            num
        })
        .collect();

    // `/ClassMap` 的流值不能内联（PDF 流必须是间接对象），逐个分配对象
    // 并以引用写回映射。
    let mut class_map = tree.class_map.clone();
    for value in class_map.values_mut() {
        if matches!(value, Value::Stream(..)) {
            let num = next_num;
            next_num += 1;
            doc.set_object(Object::new(num, 0, value.clone()));
            *value = Value::Ref(num, 0);
        }
    }

    let root_dict = root_dict(&plan, &element_nums, &tree.role_map, &class_map);
    doc.set_object(Object::new(root_num, 0, Value::Dict(root_dict)));
    entries.struct_tree_root = Some(Value::Ref(root_num, 0));

    write_elements(doc, &plan, &element_nums, root_num, page_refs);
    write_page_parents(doc, &plan, page_refs)?;

    let mut features = doc.features.borrow_mut();
    features.insert(PdfFeature::LogicalStructure);
    if plan.uses_language {
        features.insert(PdfFeature::StructureLanguageText);
    }
    if plan.uses_expanded {
        features.insert(PdfFeature::StructureExpandedText);
    }
    Ok(entries)
}

/// 校验结构树并展开为前序计划；版本门禁先于结构校验之后执行。
fn plan<'a>(
    tree: &'a StructureTree,
    version: PdfVersion,
    page_count: usize,
) -> Result<Plan<'a>, WriteError> {
    let mut flattener = Flattener {
        flat: Vec::new(),
        ids: BTreeMap::new(),
        mcids: BTreeMap::new(),
        page_count,
        uses_language: false,
        uses_expanded: false,
    };
    if tree.role_map.len() > MAX_STRUCTURE_MAP_ENTRIES {
        return Err(invalid(format!(
            "structure /RoleMap has {} entries (limit {MAX_STRUCTURE_MAP_ENTRIES})",
            tree.role_map.len()
        )));
    }
    if tree.class_map.len() > MAX_STRUCTURE_MAP_ENTRIES {
        return Err(invalid(format!(
            "structure /ClassMap has {} entries (limit {MAX_STRUCTURE_MAP_ENTRIES})",
            tree.class_map.len()
        )));
    }
    for (key, target) in &tree.role_map {
        validate_name(key, "structure role map key")?;
        validate_name(target, &format!("structure role map target for {key:?}"))?;
    }
    for (key, value) in &tree.class_map {
        validate_name(key, "structure class map key")?;
        if !matches!(value, Value::Dict(_) | Value::Stream(..)) {
            return Err(invalid(format!(
                "structure class map /{key} must be an attribute dictionary or stream ({})",
                value.type_name()
            )));
        }
    }
    for element in &tree.elements {
        flattener.add(element, None, 0)?;
    }
    if flattener.uses_language && !version.supports(PdfFeature::StructureLanguageText) {
        return Err(WriteError::UnsupportedVersion(format!(
            "structure element /Lang and /ActualText require PDF {}, got {version}",
            PdfFeature::StructureLanguageText.min_version()
        )));
    }
    if flattener.uses_expanded && !version.supports(PdfFeature::StructureExpandedText) {
        return Err(WriteError::UnsupportedVersion(format!(
            "structure element /E requires PDF {}, got {version}",
            PdfFeature::StructureExpandedText.min_version()
        )));
    }

    let mut page_keys = BTreeMap::new();
    let mut parents = BTreeMap::new();
    for (key, (page, entries)) in flattener.mcids.iter().enumerate() {
        let key = key as i64;
        page_keys.insert(*page, key);
        let max = entries.keys().next_back().copied().unwrap_or(0);
        let mut array = vec![None; (max + 1) as usize];
        for (mcid, index) in entries {
            array[*mcid as usize] = Some(*index);
        }
        parents.insert(key, array);
    }

    Ok(Plan {
        flat: flattener.flat,
        page_keys,
        parents,
        ids: flattener.ids,
        uses_language: flattener.uses_language,
        uses_expanded: flattener.uses_expanded,
    })
}

/// 前序展开与逐元素校验。
struct Flattener<'a> {
    flat: Vec<Flat<'a>>,
    ids: BTreeMap<Vec<u8>, usize>,
    /// 页面索引 → MCID → 父元素 arena 下标（同页 MCID 唯一）。
    mcids: BTreeMap<usize, BTreeMap<i64, usize>>,
    page_count: usize,
    uses_language: bool,
    uses_expanded: bool,
}

impl<'a> Flattener<'a> {
    /// 记录一个结构元素并递归展开其 `/K`。
    fn add(
        &mut self,
        element: &'a StructureElement,
        parent: Option<usize>,
        depth: usize,
    ) -> Result<(), WriteError> {
        if depth >= MAX_STRUCTURE_DEPTH {
            return Err(invalid(format!(
                "structure tree depth exceeds {MAX_STRUCTURE_DEPTH}"
            )));
        }
        if self.flat.len() >= MAX_STRUCTURE_ELEMENTS {
            return Err(invalid(format!(
                "structure tree has more than {MAX_STRUCTURE_ELEMENTS} elements"
            )));
        }
        if element.kids.len() > MAX_STRUCTURE_CHILDREN {
            return Err(invalid(format!(
                "structure element has {} children (limit {MAX_STRUCTURE_CHILDREN})",
                element.kids.len()
            )));
        }
        validate_name(&element.structure_type, "structure element /S")?;
        if let Some(id) = &element.id {
            if id.is_empty() {
                return Err(invalid("structure element /ID must not be empty"));
            }
            if id.iter().any(|byte| *byte > 0x7F) {
                return Err(invalid(
                    "structure element /ID must contain ASCII bytes only (0x00-0x7F)",
                ));
            }
            if self.ids.contains_key(id) {
                return Err(invalid(format!(
                    "structure element /ID {id:?} is declared more than once"
                )));
            }
        }
        for (name, _) in &element.classes {
            validate_name(name, "structure class name")?;
        }
        if element.lang.is_some() || element.actual_text.is_some() {
            self.uses_language = true;
        }
        if element.expanded.is_some() {
            self.uses_expanded = true;
        }

        let index = self.flat.len();
        self.flat.push(Flat {
            element,
            parent,
            kids: Vec::new(),
        });
        if let Some(id) = &element.id {
            self.ids.insert(id.clone(), index);
        }

        let mut kids = Vec::with_capacity(element.kids.len());
        for kid in &element.kids {
            match kid {
                StructureKid::Element(child) => {
                    let child_index = self.flat.len();
                    self.add(child, Some(index), depth + 1)?;
                    kids.push(PlannedKid::Element(child_index));
                }
                StructureKid::MarkedContent { page, mcid } => {
                    if *page >= self.page_count {
                        return Err(invalid(format!(
                            "structure MCR page {page} is out of range (page count {})",
                            self.page_count
                        )));
                    }
                    if *mcid < 0 {
                        return Err(invalid(format!(
                            "structure MCR MCID must be non-negative, got {mcid}"
                        )));
                    }
                    if *mcid > MAX_STRUCTURE_MCID {
                        return Err(invalid(format!(
                            "structure MCR MCID {mcid} exceeds the limit {MAX_STRUCTURE_MCID}"
                        )));
                    }
                    if self
                        .mcids
                        .entry(*page)
                        .or_default()
                        .insert(*mcid, index)
                        .is_some()
                    {
                        return Err(invalid(format!(
                            "page {page} MCID {mcid} is claimed by more than one structure element"
                        )));
                    }
                    kids.push(PlannedKid::MarkedContent {
                        page: *page,
                        mcid: *mcid,
                    });
                }
            }
        }
        self.flat[index].kids = kids;
        Ok(())
    }
}

/// 结构树根字典（Table 322）。
fn root_dict(
    plan: &Plan<'_>,
    element_nums: &[i64],
    role_map: &BTreeMap<String, String>,
    class_map: &BTreeMap<String, Value>,
) -> Dict {
    let mut top = Array::new();
    for (index, flat) in plan.flat.iter().enumerate() {
        if flat.parent.is_none() {
            top = top.push(Value::Ref(element_nums[index], 0));
        }
    }
    let mut nums = Array::new();
    for (key, array) in &plan.parents {
        nums = nums.push(Value::Integer(*key));
        let mut values = Array::new();
        for slot in array {
            values = values.push(match slot {
                Some(index) => Value::Ref(element_nums[*index], 0),
                None => Value::Null,
            });
        }
        nums = nums.push(Value::Array(values));
    }
    let mut dict = Dict::new()
        .insert("Type", Value::Name("StructTreeRoot".into()))
        .insert("K", Value::Array(top))
        .insert(
            "ParentTree",
            Value::Dict(Dict::new().insert("Nums", Value::Array(nums))),
        )
        .insert(
            "ParentTreeNextKey",
            Value::Integer(plan.parents.len() as i64),
        );
    if !role_map.is_empty() {
        let mut map = Dict::new();
        for (structure_type, standard_type) in role_map {
            map = map.insert(structure_type, Value::Name(standard_type.clone()));
        }
        dict = dict.insert("RoleMap", Value::Dict(map));
    }
    if !class_map.is_empty() {
        let mut map = Dict::new();
        for (name, value) in class_map {
            map = map.insert(name, value.clone());
        }
        dict = dict.insert("ClassMap", Value::Dict(map));
    }
    if !plan.ids.is_empty() {
        let mut names = Array::new();
        for (id, index) in &plan.ids {
            names = names
                .push(Value::HexString(hex_encode(id)))
                .push(Value::Ref(element_nums[*index], 0));
        }
        dict = dict.insert(
            "IDTree",
            Value::Dict(Dict::new().insert("Names", Value::Array(names))),
        );
    }
    dict
}

/// 写出全部结构元素对象；`/K` 混合子元素引用与 MCR 字典。
fn write_elements(
    doc: &mut Document,
    plan: &Plan<'_>,
    element_nums: &[i64],
    root_num: i64,
    page_refs: &[(i64, i64)],
) {
    for (index, flat) in plan.flat.iter().enumerate() {
        let element = flat.element;
        let parent_num = flat.parent.map_or(root_num, |parent| element_nums[parent]);
        let mut dict = Dict::new()
            .insert("Type", Value::Name("StructElem".into()))
            .insert("S", Value::Name(element.structure_type.clone()))
            .insert("P", Value::Ref(parent_num, 0));
        if let Some(id) = &element.id {
            dict = dict.insert("ID", Value::HexString(hex_encode(id)));
        }
        if let Some(title) = &element.title {
            dict = dict.insert("T", text_string_value(title));
        }
        if let Some(lang) = &element.lang {
            dict = dict.insert("Lang", text_string_value(lang));
        }
        if let Some(alt) = &element.alt {
            dict = dict.insert("Alt", text_string_value(alt));
        }
        if let Some(expanded) = &element.expanded {
            dict = dict.insert("E", text_string_value(expanded));
        }
        if let Some(actual_text) = &element.actual_text {
            dict = dict.insert("ActualText", text_string_value(actual_text));
        }
        if !element.classes.is_empty() {
            dict = dict.insert("C", classes_value(&element.classes));
        }
        if element.revision != 0 {
            dict = dict.insert("R", Value::Integer(i64::from(element.revision)));
        }
        let mut kids = Array::new();
        for kid in &flat.kids {
            kids = kids.push(match kid {
                PlannedKid::Element(child) => Value::Ref(element_nums[*child], 0),
                PlannedKid::MarkedContent { page, mcid } => {
                    let (page_num, generation) = page_refs[*page];
                    Value::Dict(
                        Dict::new()
                            .insert("Type", Value::Name("MCR".into()))
                            .insert("Pg", Value::Ref(page_num, generation))
                            .insert("MCID", Value::Integer(*mcid)),
                    )
                }
            });
        }
        dict = dict.insert("K", Value::Array(kids));
        doc.set_object(Object::new(element_nums[index], 0, Value::Dict(dict)));
    }
}

/// `/C`：单类名且无修订号时写名字，否则写 `[name revision? ...]`。
fn classes_value(classes: &[(String, Option<u32>)]) -> Value {
    if let [(name, None)] = classes {
        return Value::Name(name.clone());
    }
    let mut array = Array::new();
    for (name, revision) in classes {
        array = array.push(Value::Name(name.clone()));
        if let Some(revision) = revision {
            array = array.push(Value::Integer(i64::from(*revision)));
        }
    }
    Value::Array(array)
}

/// 页面字典追加 `/StructParents`（Table 326）。
fn write_page_parents(
    doc: &mut Document,
    plan: &Plan<'_>,
    page_refs: &[(i64, i64)],
) -> Result<(), WriteError> {
    for (page, key) in &plan.page_keys {
        let (num, generation) = page_refs[*page];
        let objects = doc.objects_for_write();
        let object = objects
            .iter()
            .find(|object| object.num == num && object.generation == generation)
            .ok_or_else(|| invalid(format!("page object {num} {generation} is missing")))?;
        let mut dict = object.value.as_dict().cloned().ok_or_else(|| {
            invalid(format!(
                "page object {num} {generation} is not a dictionary"
            ))
        })?;
        dict = dict.insert("StructParents", Value::Integer(*key));
        doc.set_object(Object::new(num, generation, Value::Dict(dict)));
    }
    Ok(())
}

/// `/MarkInfo` 字典（Table 321）：`/Marked` 显式写出，另两个布尔条目
/// 只在 true 时物化（缺省 false）。
fn mark_info_dict(info: &StructureMarkInfo) -> Dict {
    let mut dict = Dict::new().insert("Marked", Value::Bool(info.marked));
    if info.user_properties {
        dict = dict.insert("UserProperties", Value::Bool(true));
    }
    if info.suspects {
        dict = dict.insert("Suspects", Value::Bool(true));
    }
    dict
}

/// 结构类型、类名与角色映射的名字：非空且不含空白/名字定界符。
fn validate_name(value: &str, context: &str) -> Result<(), WriteError> {
    if value.is_empty() {
        return Err(invalid(format!("{context} must not be empty")));
    }
    if value
        .bytes()
        .any(|byte| byte <= 0x20 || byte >= 0x7F || b"()<>[]{}/%#".contains(&byte))
    {
        return Err(invalid(format!(
            "{context} {value:?} is not a valid PDF name"
        )));
    }
    Ok(())
}

/// 原始字节 → 大写十六进制串（`Value::HexString` 内容）。
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}
