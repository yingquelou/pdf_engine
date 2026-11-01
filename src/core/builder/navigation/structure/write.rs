//! 入口编排：写出目录 `/StructTreeRoot` 引用与 `/MarkInfo` 字典。

use crate::core::builder::navigation::invalid;
use crate::core::builder::types::{StructureAttribute, StructureMarkInfo, StructureTree};
use crate::core::types::{Dict, Document, Object, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::attributes::promote_attribute_streams;
use super::element::{root_dict, write_elements};
use super::objr::{StructureTargets, validate_objr_targets, write_object_parents};
use super::plan::{plan, write_page_parents};
use super::stream_mcr::{validate_stream_mcr_targets, write_stream_parents};

/// 写出目录条目：结构树根引用与 `/MarkInfo` 字典（均为可选）。
#[derive(Debug, Default)]
pub(in crate::core::builder::navigation) struct CatalogEntries {
    /// 目录 `/StructTreeRoot` 间接引用。
    pub(in crate::core::builder::navigation) struct_tree_root: Option<Value>,
    /// 目录 `/MarkInfo` 直接字典。
    pub(in crate::core::builder::navigation) mark_info: Option<Value>,
}
/// 把结构树与标记信息写入已组装的 `Document`，返回目录条目。
///
/// `targets` 是 OBJR 目标的解析表（注解/Form XObject 句柄与页内图像序号 →
/// 对象号、所在页），供 Table 325 解析 `/Obj` 与 `/Pg`。
///
/// 所有句柄与页索引校验在任何对象号分配之前完成。
pub(in crate::core::builder::navigation) fn write(
    doc: &mut Document,
    version: PdfVersion,
    page_refs: &[(i64, i64)],
    targets: &StructureTargets<'_>,
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

    let plan = plan(tree, version, page_refs, targets)?;
    // 目标对象本身（注解字典 / XObject 流字典）的形态与 Table 326 的互斥
    // 条目在此核对：仍在结构对象号分配之前，失败即零字节。
    validate_objr_targets(doc, &plan, page_refs)?;
    // 流形态 MCR 的 `/Stm`/`/StmOwn` 对象形态与「所有者持有该外观流」
    // 一致性同样在结构对象号分配之前核对（Table 324）。
    validate_stream_mcr_targets(doc, &plan)?;

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
    write_object_parents(doc, &plan)?;
    write_stream_parents(doc, &plan)?;

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
