//! 文档骨架：目录 `/OutputIntents`、`/OCProperties`、`/Lang`、`/Metadata`、`/AcroForm`、
//! `/Names /EmbeddedFiles` 与 `/Root` 对象，以及 Pages 根节点的 `Kids` 数组。

use crate::core::builder::OptionalContentGroup;
use crate::core::builder::PdfBuilder;
use crate::core::builder::types::text_string_value;
use crate::core::types::{
    Array, Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Value,
};
use crate::core::writer::WriteError;

use super::plan::{AnnotPlan, GroupPlan, PoolPlan, Preflight};
use super::{acro_form, embedded_file, form_field, output_intent};

impl PdfBuilder {
    /// 写出 Catalog 与 Pages 根对象；条目顺序与 XMP/输出意图的对象号分配
    /// 均与拆分前一致。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assemble_catalog_tree(
        &self,
        doc: &mut Document,
        num: &mut i64,
        preflight: &Preflight,
        pools: &mut PoolPlan,
        groups: &GroupPlan,
        annot_plan: &AnnotPlan<'_>,
        catalog_num: i64,
        pages_num: i64,
    ) -> Result<(), WriteError> {
        // 输出意图（14.11.5 Table 365）：每个 `/DestOutputProfile` 分配一个
        // 独立流对象号，目录 `/OutputIntents` 数组随后挂接。
        let output_intent_values = output_intent::write_objects(doc, &self.output_intents, num);

        // Catalog
        let mut catalog_dict = Dict::new()
            .insert("Type", Value::Name("Catalog".into()))
            .insert("Pages", Value::Ref(pages_num, 0));
        // 文档默认语言（14.9.2.1）：目录 `/Lang` 为文本串，空标识符写空串。
        if let Some(language) = &self.document_language {
            catalog_dict = catalog_dict.insert("Lang", text_string_value(language.as_str()));
            doc.features
                .borrow_mut()
                .insert(PdfFeature::DocumentLanguage);
        }
        if !output_intent_values.is_empty() {
            catalog_dict =
                catalog_dict.insert("OutputIntents", Value::Array(Array(output_intent_values)));
            doc.features.borrow_mut().insert(PdfFeature::OutputIntents);
        }
        if !groups.used_ocgs.is_empty() {
            catalog_dict = catalog_dict.insert(
                "OCProperties",
                Value::Dict(optional_content_properties_dict(
                    &groups.used_ocgs,
                    &self.optional_content_groups,
                    &groups.ocg_nums,
                )),
            );
        }
        // XMP 元数据流（14.3.2）：独立 `/Type /Metadata /Subtype /XML`
        // 流对象，目录 `/Metadata` 间接引用；要求目标版本 ≥1.4。
        // 原文透传与类型化包互斥，歧义按调用方错误处理，不静默择一。
        if self.xmp_metadata.is_some() && self.xmp_packet.is_some() {
            return Err(WriteError::InvalidObject(
                "raw XMP metadata and typed XMP packet are both set; use only one".into(),
            ));
        }
        if self.xmp_metadata.is_some() || self.xmp_packet.is_some() {
            if self.version < PdfVersion::V1_4 {
                return Err(WriteError::UnsupportedVersion(format!(
                    "XMP metadata (/Metadata) require PDF {}, got {}",
                    PdfVersion::V1_4.as_str(),
                    self.version.as_str()
                )));
            }
            let xml = match (&self.xmp_metadata, &self.xmp_packet) {
                (Some(raw), None) => raw.clone(),
                (None, Some(packet)) => packet
                    .to_xml()
                    .map_err(|error| {
                        WriteError::InvalidObject(format!("typed XMP packet: {error}"))
                    })?
                    .into_bytes(),
                _ => unreachable!("exactly one XMP source is set"),
            };
            let meta_num = *num;
            *num += 1;
            let meta_dict = Dict::new()
                .insert("Type", Value::Name("Metadata".into()))
                .insert("Subtype", Value::Name("XML".into()))
                .insert("Length", Value::Integer(xml.len() as i64));
            doc.set_object(Object::new(
                meta_num,
                0,
                Value::Stream(meta_dict, StreamData::Owned(xml)),
            ));
            catalog_dict = catalog_dict.insert("Metadata", Value::Ref(meta_num, 0));
        }
        // 交互表单目录条目（Table 218）：`/Fields` 引用顶层字段，
        // `/NeedAppearances` 与 `/DA` 仅在声明时写出。
        if let Some(form) = &preflight.acro_form {
            let fields: Vec<Value> = annot_plan
                .field_roots
                .iter()
                .map(|&index| Value::Ref(annot_plan.field_slots[index].num, 0))
                .collect();
            let mut acro = Dict::new().insert("Fields", Value::Array(Array(fields)));
            if form.need_appearances() {
                acro = acro.insert("NeedAppearances", Value::Bool(true));
            }
            if let Some(appearance) = form.default_appearance() {
                acro = acro.insert("DA", text_string_value(appearance));
            }
            // `/DR` 默认资源（Table 218/34）：字体与页面去重共享对象；
            // ExtGState/ColorSpace/ProcSet 由子模块构建。
            if let Some(dr) = acro_form::default_resources(form, doc, num, &mut pools.font_pool)? {
                acro = acro.insert("DR", dr);
            }
            // `/CO` 计算顺序：显式名录按声明顺序引用字段对象；缺省时按
            // 字段树前序自动列出全部带 `/C` 动作的字段。
            let order = form_field::calculation_order_nums(form, &annot_plan.field_slots)?;
            if !order.is_empty() {
                let refs: Vec<Value> = order.iter().map(|&n| Value::Ref(n, 0)).collect();
                acro = acro.insert("CO", Value::Array(Array(refs)));
            }
            catalog_dict = catalog_dict.insert("AcroForm", Value::Dict(acro));
        }
        // 嵌入文件目录条目（7.11.4）：`/Names /EmbeddedFiles` 名称树；
        // 导航层随后写 `/Dests` `/Pages` 时与本字典合并。
        if !annot_plan.embedded_slots.is_empty() {
            let names = Dict::new().insert(
                "EmbeddedFiles",
                embedded_file::embedded_files_node(&self.embedded, &annot_plan.embedded_slots),
            );
            catalog_dict = catalog_dict.insert("Names", Value::Dict(names));
        }
        doc.set_object(Object::new(catalog_num, 0, Value::Dict(catalog_dict)));

        // Pages Kids 数组：按页顺序引用各 Page 字典。
        let kids: Vec<Value> = pools
            .page_slots
            .iter()
            .map(|(pn, _)| Value::Ref(*pn, 0))
            .collect();
        let kids_arr = kids.iter().fold(Array::new(), |acc, v| acc.push(v.clone()));
        doc.set_object(Object::new(
            pages_num,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Pages".into()))
                    .insert("Kids", Value::Array(kids_arr))
                    .insert("Count", Value::Integer(self.pages.len() as i64)),
            ),
        ));
        Ok(())
    }
}

/// 目录 `/OCProperties`（Table 100）：使用组列表 + 默认配置（Table 101）。
///
/// `/BaseState` 固定为 `/ON`，每组初始可见性由 `/ON` / `/OFF` 精确表达；
/// `/Order` 按组注册顺序列出（仅实际使用的组）。
fn optional_content_properties_dict(
    used: &std::collections::BTreeSet<usize>,
    groups: &[OptionalContentGroup],
    ocg_nums: &[i64],
) -> Dict {
    let all: Vec<Value> = used
        .iter()
        .map(|&handle| Value::Ref(ocg_nums[handle], 0))
        .collect();
    let on: Vec<Value> = used
        .iter()
        .filter(|&&handle| groups[handle].default_visible)
        .map(|&handle| Value::Ref(ocg_nums[handle], 0))
        .collect();
    let off: Vec<Value> = used
        .iter()
        .filter(|&&handle| !groups[handle].default_visible)
        .map(|&handle| Value::Ref(ocg_nums[handle], 0))
        .collect();
    let default = Dict::new()
        .insert("BaseState", Value::Name("ON".into()))
        .insert("ON", Value::Array(Array(on)))
        .insert("OFF", Value::Array(Array(off)))
        .insert("Order", Value::Array(Array(all.clone())));
    Dict::new()
        .insert("OCGs", Value::Array(Array(all)))
        .insert("D", Value::Dict(default))
}
