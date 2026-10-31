//! 逻辑结构写端（ISO 32000-1 14.7.1–14.7.5、14.8.5.2、14.8.5.5–14.8.5.7、
//! Table 321–329、341、347–349）。
//!
//! 目录 `/StructTreeRoot`（Table 322）与 `/MarkInfo`（Table 321）、结构
//! 元素（Table 323）、MCR 内容项（Table 324）、父树（14.7.4.4）与
//! `/IDTree` 名称树都从现有最大对象号之后分配，不影响既有对象布局；
//! 页面字典追加 `/StructParents`（Table 326）。结构属性对象 `/A` 与
//! `/ClassMap`（14.7.5）按 Table 327–329 写出，属性流提升为间接对象。
//!
//! 标准属性所有者（14.8.5.2 Table 341）与 List/PrintField/Table 条目
//! （14.8.5.5–14.8.5.7 Table 347–349）由
//! [`structure_standard`] 做值形态、元素适用性与 `/Headers` ID 交叉校验；
//! Layout（14.8.5.4 Table 342–346）条目暂不校验或建模。
//!
//! 校验先于任何对象号分配：深度、元素数、单元素子项数与映射条目上限
//! 直接复用读端常量；页索引越界、负 MCID、同页 MCID 重复、空结构类型、
//! 空角色映射键/目标、类映射/属性对象取值非字典/流或 `/O` 非法、空类名、
//! ID 为空/重复/非 ASCII 一律返回 [`WriteError::InvalidObject`]。版本门禁：
//! 结构树 ≥1.3、`/Lang` `/ActualText` ≥1.4、`/E` ≥1.5、`/MarkInfo` ≥1.4、
//! `/UserProperties` `/Suspects` ≥1.6、结构属性用户属性 ≥1.6、标准属性
//! 所有者 ≥1.4、`/Headers` `/Scope` ≥1.5、PrintField 与 `/Summary` ≥1.7，
//! 不满足返回 [`WriteError::UnsupportedVersion`]。使用用户属性时还要求
//! `/MarkInfo /UserProperties true`（14.7.5.4）。两类失败均不写出任何字节。
//!
//! 14.8.4 的 PDF 1.5 扩展标准结构类型（THead/TBody/TFoot/Annot/Ruby/
//! RB/RT/RP/Warichu/WT/WP）同样设门禁：元素 `/S` 经 `/RoleMap` 解析后的
//! 终点、或 `/RoleMap` 任一目标为这些名字时要求 ≥1.5（`/RoleMap` 目标
//! 即使未被元素使用也计入，避免写出带 1.5 类型名的 1.3/1.4 文件）。
//!
//! 本切片的范围限制：`/K` 只写出子元素引用与 MCR 字典（带显式 `/Pg` 与
//! `/MCID`），不写 OBJR、整数 MCID 与 `/Stm` `/StmOwn`。

use std::collections::BTreeMap;

use crate::core::reader::accessibility::LanguageIdentifier;
use crate::core::reader::structure::{
    MAX_STRUCTURE_CHILDREN, MAX_STRUCTURE_DEPTH, MAX_STRUCTURE_ELEMENTS, MAX_STRUCTURE_MAP_ENTRIES,
};
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::super::types::{
    StructureAttribute, StructureElement, StructureKid, StructureMarkInfo, StructureTree,
    text_string_value,
};
use super::invalid;
use super::structure_standard::{self, StandardUsage};

/// 写端 MCID 上限：父树数组按 MCID 作零基下标，与读端单元素子项上限
/// 取同值，避免单页父树数组无界膨胀（65535 为最大可用 MCID）。
const MAX_STRUCTURE_MCID: i64 = MAX_STRUCTURE_CHILDREN as i64 - 1;

/// PDF 1.5 引入的标准结构类型（14.8.4）：`THead` `TBody` `TFoot`
/// （Table 337）、`Annot`（Table 338）与 Ruby/Warichu 家族（Table 339）。
const EXTENDED_STRUCTURE_TYPES: &[&str] = &[
    "THead", "TBody", "TFoot", "Annot", "Ruby", "RB", "RT", "RP", "Warichu", "WT", "WP",
];

/// 名字是否为 PDF 1.5 引入的标准结构类型（14.8.4）。
fn is_extended_structure_type(name: &str) -> bool {
    EXTENDED_STRUCTURE_TYPES.contains(&name)
}

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
    /// 是否使用用户属性属性对象（PDF 1.6，14.7.5.3）。
    uses_user_properties: bool,
    /// 是否使用 PDF 1.5 扩展标准结构类型（14.8.4）。
    uses_extended_types: bool,
    /// 标准属性使用情况（14.8.5 的版本门禁与特性画像）。
    standard: StandardUsage,
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

    // 14.7.5.4：含用户属性的文档必须在 `/MarkInfo` 声明 `/UserProperties
    // true`。读端已在结构树之前校验过标记信息，这里只做交叉检查，仍在
    // 任何对象号分配之前。
    if plan.uses_user_properties && !mark_info.is_some_and(|info| info.user_properties) {
        return Err(invalid(
            "structure user properties require /MarkInfo /UserProperties true",
        ));
    }

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
    // 并以引用写回映射；数组形式同样只提升其中的流项。
    let mut class_map = tree.class_map.clone();
    for value in class_map.values_mut() {
        promote_attribute_streams(doc, value, &mut next_num);
    }

    // 元素 `/A` 的属性流同样提升为间接对象；其余值（字典）内联写出。
    let element_attributes: Vec<Vec<StructureAttribute>> = plan
        .flat
        .iter()
        .map(|flat| {
            flat.element
                .attributes
                .iter()
                .map(|attribute| {
                    let mut attribute = attribute.clone();
                    promote_attribute_streams(doc, &mut attribute.value, &mut next_num);
                    attribute
                })
                .collect()
        })
        .collect();

    let root_dict = root_dict(&plan, &element_nums, &tree.role_map, &class_map);
    doc.set_object(Object::new(root_num, 0, Value::Dict(root_dict)));
    entries.struct_tree_root = Some(Value::Ref(root_num, 0));

    write_elements(
        doc,
        &plan,
        &element_nums,
        &element_attributes,
        root_num,
        page_refs,
    );
    write_page_parents(doc, &plan, page_refs)?;

    let mut features = doc.features.borrow_mut();
    features.insert(PdfFeature::LogicalStructure);
    if plan.uses_language {
        features.insert(PdfFeature::StructureLanguageText);
    }
    if plan.uses_expanded {
        features.insert(PdfFeature::StructureExpandedText);
    }
    if plan.uses_user_properties {
        features.insert(PdfFeature::StructureUserProperties);
    }
    if plan.uses_extended_types {
        features.insert(PdfFeature::StructureTypeExtensions);
    }
    plan.standard.apply_features(&mut features);
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
        parents.insert(key, array);
    }

    Ok(Plan {
        flat: flattener.flat,
        page_keys,
        parents,
        ids: flattener.ids,
        uses_language: flattener.uses_language,
        uses_expanded: flattener.uses_expanded,
        uses_user_properties: flattener.uses_user_properties,
        uses_extended_types: flattener.uses_extended_types,
        standard: flattener.standard,
    })
}

/// `/ClassMap` 的单个值：属性对象或属性对象数组（14.7.5.2）。
///
/// 类属性可能被不同类型的元素复用，标准条目只做值形态检查，不做元素
/// 适用性检查（适用性只对元素 `/A`）。
fn validate_class_attributes(
    value: &Value,
    key: &str,
    uses_user_properties: &mut bool,
    standard: &mut StandardUsage,
) -> Result<(), WriteError> {
    let context = format!("structure class map /{key}");
    match value {
        Value::Array(items) => {
            if items.0.len() > MAX_STRUCTURE_CHILDREN {
                return Err(invalid(format!(
                    "{context} has {} attribute objects (limit {MAX_STRUCTURE_CHILDREN})",
                    items.0.len()
                )));
            }
            for item in &items.0 {
                validate_attribute_object(item, &context, uses_user_properties)?;
                structure_standard::validate_class_attribute(item, &context, standard)?;
            }
        }
        single => {
            validate_attribute_object(single, &context, uses_user_properties)?;
            structure_standard::validate_class_attribute(single, &context, standard)?;
        }
    }
    Ok(())
}

/// 单个属性对象（Table 327）：直接字典或流，`/O` 为合法非空名字。
fn validate_attribute_object(
    value: &Value,
    context: &str,
    uses_user_properties: &mut bool,
) -> Result<(), WriteError> {
    let dict = match value {
        Value::Dict(dict) | Value::Stream(dict, _) => dict,
        other => {
            return Err(invalid(format!(
                "{context} must be an attribute dictionary or stream ({})",
                other.type_name()
            )));
        }
    };
    let owner = match dict.get("O") {
        Some(Value::Name(name)) => name,
        Some(other) => {
            return Err(invalid(format!(
                "{context} /O must be a name ({})",
                other.type_name()
            )));
        }
        None => return Err(invalid(format!("{context} /O is required (Table 327)"))),
    };
    validate_name(owner, &format!("{context} /O"))?;
    if owner == "UserProperties" {
        validate_user_properties(dict, context)?;
        *uses_user_properties = true;
    }
    Ok(())
}

/// 用户属性属性对象（14.7.5.3 Table 328/329）：`/P` 为属性字典数组。
fn validate_user_properties(dict: &Dict, context: &str) -> Result<(), WriteError> {
    let properties = match dict.get("P") {
        Some(Value::Array(items)) => items,
        Some(other) => {
            return Err(invalid(format!(
                "{context} /P must be an array of user property dictionaries ({})",
                other.type_name()
            )));
        }
        None => return Err(invalid(format!("{context} /P is required (Table 328)"))),
    };
    if properties.0.len() > MAX_STRUCTURE_CHILDREN {
        return Err(invalid(format!(
            "{context} /P has {} entries (limit {MAX_STRUCTURE_CHILDREN})",
            properties.0.len()
        )));
    }
    for (index, item) in properties.0.iter().enumerate() {
        let Value::Dict(property) = item else {
            return Err(invalid(format!(
                "{context} /P entry {index} must be a user property dictionary ({})",
                item.type_name()
            )));
        };
        match property.get("N") {
            Some(Value::String(_) | Value::HexString(_)) => {}
            Some(other) => {
                return Err(invalid(format!(
                    "{context} /P entry {index} /N must be a text string ({})",
                    other.type_name()
                )));
            }
            None => {
                return Err(invalid(format!(
                    "{context} /P entry {index} /N is required (Table 329)"
                )));
            }
        }
        if !property.0.contains_key("V") {
            return Err(invalid(format!(
                "{context} /P entry {index} /V is required (Table 329)"
            )));
        }
        if let Some(formatted) = property.get("F")
            && !matches!(formatted, Value::String(_) | Value::HexString(_))
        {
            return Err(invalid(format!(
                "{context} /P entry {index} /F must be a text string ({})",
                formatted.type_name()
            )));
        }
        if let Some(hidden) = property.get("H")
            && !matches!(hidden, Value::Bool(_))
        {
            return Err(invalid(format!(
                "{context} /P entry {index} /H must be a boolean ({})",
                hidden.type_name()
            )));
        }
    }
    Ok(())
}

/// 把属性值中的流提升为间接对象并写回引用；数组只处理直接元素
/// （ClassMap 数组项本身就是属性对象）。校验已保证数组项为字典/流。
fn promote_attribute_streams(doc: &mut Document, value: &mut Value, next_num: &mut i64) {
    match value {
        Value::Stream(..) => {
            let num = *next_num;
            *next_num += 1;
            doc.set_object(Object::new(num, 0, value.clone()));
            *value = Value::Ref(num, 0);
        }
        Value::Array(items) => {
            for item in &mut items.0 {
                if matches!(item, Value::Stream(..)) {
                    let num = *next_num;
                    *next_num += 1;
                    doc.set_object(Object::new(num, 0, item.clone()));
                    *item = Value::Ref(num, 0);
                }
            }
        }
        _ => {}
    }
}

/// 前序展开与逐元素校验。
struct Flattener<'a> {
    flat: Vec<Flat<'a>>,
    ids: BTreeMap<Vec<u8>, usize>,
    /// 页面索引 → MCID → 父元素 arena 下标（同页 MCID 唯一）。
    mcids: BTreeMap<usize, BTreeMap<i64, usize>>,
    page_count: usize,
    /// 结构类型 → 标准类型的角色映射（元素适用性判定用）。
    role_map: &'a BTreeMap<String, String>,
    /// 标准属性使用情况与待核对的 `/Headers` 引用。
    standard: StandardUsage,
    uses_language: bool,
    uses_expanded: bool,
    uses_user_properties: bool,
    /// 是否使用 PDF 1.5 扩展标准结构类型（14.8.4）。
    uses_extended_types: bool,
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
        // 14.8.4.1：/S 先沿 /RoleMap 一或多级解析，终点为 PDF 1.5 扩展
        // 标准类型时置位；版本门禁在 plan 末尾统一执行（仍在对象号分配前）。
        let resolved = structure_standard::resolved_type(self.role_map, &element.structure_type);
        if is_extended_structure_type(&resolved) {
            self.uses_extended_types = true;
        }
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
        if element.attributes.len() > MAX_STRUCTURE_CHILDREN {
            return Err(invalid(format!(
                "structure element has {} attributes (limit {MAX_STRUCTURE_CHILDREN})",
                element.attributes.len()
            )));
        }
        for (index, attribute) in element.attributes.iter().enumerate() {
            let context = format!("structure element /A attribute {index}");
            validate_attribute_object(&attribute.value, &context, &mut self.uses_user_properties)?;
            structure_standard::validate_element_attribute(
                &attribute.value,
                &context,
                &element.structure_type,
                self.role_map,
                &mut self.standard,
            )?;
        }
        if element.lang.is_some() || element.actual_text.is_some() {
            self.uses_language = true;
        }
        // 14.9.2.2：结构元素 `/Lang` 必须是空串或 RFC 3066 Language-Tag。
        // 语法校验先于任何对象号分配；错误信息带上前序下标与原串。
        if let Some(lang) = &element.lang {
            LanguageIdentifier::parse(lang).map_err(|error| {
                invalid(format!(
                    "structure element {} /Lang {lang:?} is not a valid language tag: {error}",
                    self.flat.len()
                ))
            })?;
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
    element_attributes: &[Vec<StructureAttribute>],
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
        let attributes = &element_attributes[index];
        if !attributes.is_empty() {
            dict = dict.insert("A", attributes_value(attributes));
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

/// `/A`（14.7.5.3）：单个无修订号属性直接写对象，否则写
/// `[object revision? ...]`，顺序与声明一致。
fn attributes_value(attributes: &[StructureAttribute]) -> Value {
    if attributes.len() == 1 && attributes[0].revision.is_none() {
        return attributes[0].value.clone();
    }
    let mut array = Array::new();
    for attribute in attributes {
        array = array.push(attribute.value.clone());
        if let Some(revision) = attribute.revision {
            array = array.push(Value::Integer(i64::from(revision)));
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
