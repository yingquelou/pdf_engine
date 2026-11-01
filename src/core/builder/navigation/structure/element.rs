//! 结构对象写出：`/StructTreeRoot` 根字典（Table 322）与全部
//! `StructElem` 元素对象（Table 323，`/K` 混合子元素引用、MCR 字典、
//! 整数简写、OBJR 字典与流形态 MCR 字典）。

use std::collections::BTreeMap;

use crate::core::builder::types::{StructureAttribute, text_string_value};
use crate::core::types::{Array, Dict, Document, Object, Value};

use super::flatten::PlannedKid;
use super::plan::{ParentEntry, Plan};

/// 结构树根字典（Table 322）。
pub(super) fn root_dict(
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
    for (key, entry) in &plan.parents {
        nums = nums.push(Value::Integer(*key));
        nums = nums.push(match entry {
            // OBJR 单元条目：裸间接引用（14.7.4.4），不是数组。
            ParentEntry::Object(index) => Value::Ref(element_nums[*index], 0),
            // 内容流容器条目：MCID 零基下标数组（页面或 `/Stm` 宿主）。
            ParentEntry::McidArray(slots) => {
                let mut values = Array::new();
                for slot in slots {
                    values = values.push(match slot {
                        Some(index) => Value::Ref(element_nums[*index], 0),
                        None => Value::Null,
                    });
                }
                Value::Array(values)
            }
        });
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

/// 写出全部结构元素对象；`/K` 混合子元素引用、MCR 字典与 OBJR 字典。
#[allow(clippy::too_many_arguments)]
pub(super) fn write_elements(
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
        // Table 323 `/Pg`：可选间接页引用；已校验页索引在文档页表内。
        if let Some(page) = element.page {
            let (page_num, generation) = page_refs[page];
            dict = dict.insert("Pg", Value::Ref(page_num, generation));
        }
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
                PlannedKid::InlineMarkedContent { mcid, .. } => {
                    // 14.7.4.2：整数值即 MCID，所在页由元素 `/Pg` 指定；
                    // 页一致性已在 `Flattener::add` 校验。
                    Value::Integer(*mcid)
                }
                PlannedKid::ObjectReference { object_num, page } => {
                    // Table 325：`/Pg` 与 `/Obj` 均为间接引用；页取目标
                    // 自身的渲染页，独立于元素 `/Pg`。
                    let (page_num, generation) = page_refs[*page];
                    Value::Dict(
                        Dict::new()
                            .insert("Type", Value::Name("OBJR".into()))
                            .insert("Pg", Value::Ref(page_num, generation))
                            .insert("Obj", Value::Ref(*object_num, 0)),
                    )
                }
                PlannedKid::StreamMarkedContent {
                    stream_num,
                    owner_num,
                    page,
                    write_pg,
                    mcid,
                } => {
                    // Table 324 流形态（14.7.4.2 EXAMPLE 5）：`/Stm` 与
                    // `/StmOwn` 均为间接引用；`/Pg` 只在引用显式给页或经
                    // `/StmOwn` 解析时写出，继承元素 `/Pg` 时省略。
                    // `/MCID` 必填（Table 324 标注 Required）。
                    let mut dict = Dict::new().insert("Type", Value::Name("MCR".into()));
                    if *write_pg {
                        let (page_num, generation) = page_refs[*page];
                        dict = dict.insert("Pg", Value::Ref(page_num, generation));
                    }
                    dict = dict.insert("Stm", Value::Ref(*stream_num, 0));
                    if let Some(owner_num) = owner_num {
                        dict = dict.insert("StmOwn", Value::Ref(*owner_num, 0));
                    }
                    Value::Dict(dict.insert("MCID", Value::Integer(*mcid)))
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
/// 原始字节 → 大写十六进制串（`Value::HexString` 内容）。
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}
