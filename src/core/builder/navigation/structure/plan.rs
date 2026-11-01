//! 校验并展开后的写出计划：父树键分配（页面/OBJR/`/Stm` 容器三类
//! 容器键）与页面字典 `/StructParents` 写出。

use std::collections::BTreeMap;

use crate::core::builder::navigation::invalid;
use crate::core::builder::navigation::structure_content;
use crate::core::builder::navigation::structure_standard::{self, StandardUsage};
use crate::core::builder::types::{StructureObjectReference, StructureTree};
use crate::core::reader::structure::MAX_STRUCTURE_MAP_ENTRIES;
use crate::core::types::{Document, Object, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::attributes::validate_class_attributes;
use super::flatten::{Flat, Flattener};
use super::limits::{is_extended_structure_type, validate_name};
use super::objr::StructureTargets;

/// 父树条目的两种形态（14.7.4.4）。
pub(super) enum ParentEntry {
    /// 内容流容器数组：按 MCID 作零基下标，空位 `None`。容器是页字典
    /// （页面内容流，`/StructParents` 写在页上）或流形态 MCR 的
    /// `/Stm`/`/StmOwn` 宿主对象（14.7.4.4「the StructParents entry of the
    /// page object or other content stream in which the sequence resides」，
    /// raw 29722–29723 行）。
    McidArray(Vec<Option<usize>>),
    /// OBJR 内容项对象的单元条目：父元素 arena 下标（写出为裸引用）。
    Object(usize),
}

/// 校验并展开后的写出计划（不含对象号，尚未分配任何对象）。
pub(super) struct Plan<'a> {
    pub(super) flat: Vec<Flat<'a>>,
    /// 页面索引 → 父树键（按页升序 0..n-1）。
    pub(super) page_keys: BTreeMap<usize, i64>,
    /// 父树键 → 页面 MCID 数组或 OBJR 单元引用（空位 `None`）。
    pub(super) parents: BTreeMap<i64, ParentEntry>,
    /// OBJR 目标对象号 → 父树键（页面键之后按对象号升序连续分配）。
    pub(super) objr_keys: BTreeMap<i64, i64>,
    /// 流形态 MCR 容器对象号（`/StmOwn` 给出时为所有者，否则为 `/Stm`
    /// 流）→ 父树键（OBJR 键之后按容器对象号升序连续分配）。
    pub(super) stream_keys: BTreeMap<i64, i64>,
    /// OBJR 目标对象号 → 声称它的元素 arena 下标（判重与错误信息用）。
    pub(super) objrs: BTreeMap<i64, (usize, StructureObjectReference)>,
    /// `/ID` 原始字节 → 元素 arena 下标（按字节升序）。
    pub(super) ids: BTreeMap<Vec<u8>, usize>,
    /// 是否使用 `/Lang` 或 `/ActualText`（PDF 1.4）。
    pub(super) uses_language: bool,
    /// 是否使用 `/E`（PDF 1.5）。
    pub(super) uses_expanded: bool,
    /// 是否使用用户属性属性对象（PDF 1.6，14.7.5.3）。
    pub(super) uses_user_properties: bool,
    /// 是否使用 PDF 1.5 扩展标准结构类型（14.8.4）。
    pub(super) uses_extended_types: bool,
    /// 标准属性使用情况（14.8.5 的版本门禁与特性画像）。
    pub(super) standard: StandardUsage,
}
/// 校验结构树并展开为前序计划；版本门禁先于结构校验之后执行。
///
/// OBJR 子项按 `targets` 解析目标对象号与所在页，并对照 `page_refs` 的
/// 页对象号复核 Table 326 的 `/StructParent` ↔ `/StructParents` 互斥；
/// 这些检查与页面/MCID 检查一样先于任何对象号分配。
pub(super) fn plan<'a>(
    tree: &'a StructureTree,
    version: PdfVersion,
    page_refs: &[(i64, i64)],
    targets: &StructureTargets<'a>,
) -> Result<Plan<'a>, WriteError> {
    let mut flattener = Flattener {
        flat: Vec::new(),
        ids: BTreeMap::new(),
        mcids: BTreeMap::new(),
        objrs: BTreeMap::new(),
        stream_mcids: BTreeMap::new(),
        page_count: page_refs.len(),
        targets: *targets,
        role_map: &tree.role_map,
        standard: StandardUsage::default(),
        uses_language: false,
        uses_expanded: false,
        uses_user_properties: false,
        uses_extended_types: false,
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
        // 映射目标本身也是写出内容：未使用的 1.5 类型名同样抬高版本要求，
        // 避免写出 1.3/1.4 文件却带 1.5 标准结构类型（14.8.4）。
        if is_extended_structure_type(target) {
            flattener.uses_extended_types = true;
        }
    }
    for (key, value) in &tree.class_map {
        validate_name(key, "structure class map key")?;
        validate_class_attributes(
            value,
            key,
            &mut flattener.uses_user_properties,
            &mut flattener.standard,
        )?;
    }
    for element in &tree.elements {
        flattener.add(element, None, 0)?;
    }
    // `/Headers` 引用在全部元素展开后才能与 `/ID` 对照；仍在对象号分配前。
    let mut id_types = BTreeMap::new();
    for (id, index) in &flattener.ids {
        let structure_type = &flattener.flat[*index].element.structure_type;
        id_types.insert(
            id.clone(),
            structure_standard::resolved_type(&tree.role_map, structure_type),
        );
    }
    flattener.standard.validate_header_ids(&id_types)?;
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
    if flattener.uses_user_properties && !version.supports(PdfFeature::StructureUserProperties) {
        return Err(WriteError::UnsupportedVersion(format!(
            "structure user properties require PDF {}, got {version}",
            PdfFeature::StructureUserProperties.min_version()
        )));
    }
    if flattener.uses_extended_types && !version.supports(PdfFeature::StructureTypeExtensions) {
        return Err(WriteError::UnsupportedVersion(format!(
            "structure element types THead/TBody/TFoot/Annot/Ruby/Warichu require PDF {}, got {version}",
            PdfFeature::StructureTypeExtensions.min_version()
        )));
    }
    flattener.standard.check_version(version)?;

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
        parents.insert(key, ParentEntry::McidArray(array));
    }
    // 14.7.4.4：OBJR 内容项的父树条目是单元引用，键接在页面键之后按目标
    // 对象号升序连续分配，保证 0..n-1 无空洞。
    let mut objr_keys = BTreeMap::new();
    let first_objr_key = flattener.mcids.len() as i64;
    for (offset, (object_num, (index, _))) in flattener.objrs.iter().enumerate() {
        let key = first_objr_key + offset as i64;
        parents.insert(key, ParentEntry::Object(*index));
        objr_keys.insert(*object_num, key);
    }
    // 14.7.4.4：`/Stm` 序列的父树键由容器字典的 `/StructParents` 承载
    // （有 `/StmOwn` 时是所有者，否则是 `/Stm` 流自身）；键接在 OBJR 键
    // 之后按容器对象号升序连续分配，条目与页面数组同为 MCID 零基下标数组。
    let mut stream_keys = BTreeMap::new();
    let first_stream_key = first_objr_key + flattener.objrs.len() as i64;
    for (offset, (container, entries)) in flattener.stream_mcids.iter().enumerate() {
        let key = first_stream_key + offset as i64;
        let max = entries.keys().next_back().copied().unwrap_or(0);
        let mut array = vec![None; (max + 1) as usize];
        for (mcid, index) in entries {
            array[*mcid as usize] = Some(*index);
        }
        parents.insert(key, ParentEntry::McidArray(array));
        stream_keys.insert(*container, key);
    }
    // 14.7.4.4 shall：`/ParentTreeNextKey` 必须严格大于全部在用父树键。
    // 键按「新条目取当前 NextKey、随后自增」连续分配（页面键 0..p-1，
    // OBJR 键 p..），NextKey 写出为条目总数；在 `root_dict` 之前、仍在
    // 任何结构对象号分配时复核该不变量，键策略回归即零字节拒绝。
    let next_key = parents.len() as i64;
    let used_keys: Vec<i64> = parents.keys().copied().collect();
    structure_content::validate_next_key(&used_keys, next_key).map_err(invalid)?;

    let objrs = std::mem::take(&mut flattener.objrs);
    Ok(Plan {
        flat: flattener.flat,
        page_keys,
        parents,
        objr_keys,
        stream_keys,
        objrs,
        ids: flattener.ids,
        uses_language: flattener.uses_language,
        uses_expanded: flattener.uses_expanded,
        uses_user_properties: flattener.uses_user_properties,
        uses_extended_types: flattener.uses_extended_types,
        standard: flattener.standard,
    })
}
/// 页面字典追加 `/StructParents`（Table 326）。
pub(super) fn write_page_parents(
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
