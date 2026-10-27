//! Type 3 字体字典与字符过程流。

use crate::core::builder::{Font, Type3Font};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
use crate::core::writer::WriteError;
use std::collections::{BTreeMap, BTreeSet};

use super::cmap::to_unicode_cmap;

/// 构造 Type 3 字体字典并写出各字符过程流（PDF 32000-1 §9.6.5）。
///
/// `/CharProcs` 的流对象号按 `type3.glyphs` 的顺序从 `num` 连续分配，
/// 保证同一字体定义总是得到同一对象号布局。字典不含 `/BaseFont` 与
/// 字体程序；`/Encoding` 必须存在（Type 3 字体没有 `/BaseFont`
/// 可供推导内建编码）。Table 112 的可选条目：`/Name`（来自
/// [`Type3Font::name`]）、`/FontDescriptor`（来自 `Font::descriptor`，
/// 补齐必需默认值，不写 `/FontFile*`）与 `/ToUnicode`（来自
/// [`Type3Font::to_unicode`]，经 ToUnicode CMap 生成器写出）。
///
/// 以下情况返回 [`WriteError::InvalidObject`]：嵌入字体程序、请求
/// `subset_glyphs`/`subset_type1_glyphs`/`subset_cff_glyphs`、显式设置
/// `Font::widths`、未知编码名、字形列表为空、字符码或字形名重复、
/// `/Encoding` 缺失、`/Resources` 不是字典或间接引用、`/Name` 不是
/// 合法的 PDF Name。
pub(super) fn type3_font_dict(
    doc: &mut Document,
    font: &Font,
    type3: &Type3Font,
    num: &mut i64,
) -> Result<Dict, WriteError> {
    if font.font_program.is_some() {
        return Err(WriteError::InvalidObject(format!(
            "Type 3 font {} must not embed a font program",
            font.name
        )));
    }
    // 未知编码名无法写出合法的 `/Encoding`：构建期即报错。
    if let Some(name) = font.encoding.unknown_name() {
        return Err(WriteError::InvalidObject(format!(
            "/Encoding /{name} for Type 3 font {} is not a supported predefined encoding",
            font.name
        )));
    }
    if font.subset_glyphs.is_some() {
        return Err(WriteError::InvalidObject(format!(
            "Type 3 font {} cannot be subset with subset_glyphs: it has no font program",
            font.name
        )));
    }
    if font.subset_type1_glyphs.is_some() {
        return Err(WriteError::InvalidObject(format!(
            "Type 3 font {} cannot be subset with subset_type1_glyphs: it has no font program",
            font.name
        )));
    }
    if font.subset_cff_glyphs.is_some() {
        return Err(WriteError::InvalidObject(format!(
            "Type 3 font {} cannot be subset with subset_cff_glyphs: it has no font program",
            font.name
        )));
    }
    if font.widths.is_some() {
        return Err(WriteError::InvalidObject(format!(
            "Type 3 font {} cannot use explicit widths: /Widths is derived from its glyphs",
            font.name
        )));
    }
    if type3.glyphs.is_empty() {
        return Err(WriteError::InvalidObject(format!(
            "Type 3 font {} requires at least one glyph",
            font.name
        )));
    }
    let mut codes = BTreeSet::new();
    let mut names = BTreeSet::new();
    for glyph in &type3.glyphs {
        if !codes.insert(glyph.code) {
            return Err(WriteError::InvalidObject(format!(
                "Type 3 font {} has duplicate character code {}",
                font.name, glyph.code
            )));
        }
        if !names.insert(glyph.name.as_str()) {
            return Err(WriteError::InvalidObject(format!(
                "Type 3 font {} has duplicate glyph name {:?}",
                font.name, glyph.name
            )));
        }
        // /Widths 由字形宽度直接写出（PDF 32000-1 Table 112），
        // 非有限或负值无法表示为有效数值对象。
        if !glyph.width.is_finite() || glyph.width < 0.0 {
            return Err(WriteError::InvalidObject(format!(
                "Type 3 font {} has an invalid width for glyph {:?}: {}",
                font.name, glyph.name, glyph.width
            )));
        }
    }
    for (label, values) in [
        ("FontBBox", type3.font_bbox.as_slice()),
        ("FontMatrix", type3.font_matrix.as_slice()),
    ] {
        for value in values {
            if !value.is_finite() {
                return Err(WriteError::InvalidObject(format!(
                    "Type 3 font {} has an invalid {label} value: {value}",
                    font.name
                )));
            }
        }
    }
    let Some(encoding) = font.encoding.to_value() else {
        return Err(WriteError::InvalidObject(format!(
            "Type 3 font {} requires /Encoding",
            font.name
        )));
    };

    // `/Resources` 原样写出：直接字典或指向字典的间接引用均可，引用目标
    // 的有效性由调用方保证（builder 的对象号在构建期分配，不自动注册）。
    if let Some(resources) = &type3.resources
        && !matches!(resources, Value::Dict(_) | Value::Ref(_, _))
    {
        return Err(WriteError::InvalidObject(format!(
            "Type 3 font {} /Resources must be a dictionary or an indirect reference",
            font.name
        )));
    }

    // `/Name`（Table 112）必须是可原样写出的 PDF Name。
    if let Some(name) = &type3.name
        && !super::cid::is_valid_pdf_name(name)
    {
        return Err(WriteError::InvalidObject(format!(
            "Type 3 font {} has an invalid /Name {name:?}",
            font.name
        )));
    }

    // 字符过程流：按字形列表顺序分配对象号（与字形的书写顺序一致）。
    let mut char_procs = Dict::new();
    for glyph in &type3.glyphs {
        let proc_num = *num;
        *num += 1;
        char_procs = char_procs.insert(glyph.name.clone(), Value::Ref(proc_num, 0));
        doc.set_object(Object::new(
            proc_num,
            0,
            Value::Stream(
                Dict::new().insert("Length", Value::Integer(glyph.char_proc.len() as i64)),
                StreamData::Owned(glyph.char_proc.clone()),
            ),
        ));
    }

    // `/FontDescriptor`（Table 112；Tagged PDF 必需）：与普通字体共用描述符
    // 结构与必需项兜底，但不挂 `/FontFile*`（Type 3 无字体程序）。
    let descriptor_ref = if font.descriptor.is_some() {
        let descriptor_ref = *num;
        *num += 1;
        let mut descriptor = font.descriptor.clone().expect("checked above");
        descriptor.fill_required_defaults(&font.name);
        doc.set_object(Object::new(descriptor_ref, 0, descriptor.to_value()));
        Some(descriptor_ref)
    } else {
        None
    };

    // `/ToUnicode`（Table 112 可选）：单字节字符码经 ToUnicode CMap 生成器写出。
    let to_unicode_ref = if type3.to_unicode.is_empty() {
        None
    } else {
        let cmap_num = *num;
        *num += 1;
        let byte_keys: BTreeMap<Vec<u8>, String> = type3
            .to_unicode
            .iter()
            .map(|(code, text)| (vec![*code], text.clone()))
            .collect();
        let cmap = to_unicode_cmap(&BTreeMap::new(), &byte_keys, None);
        let cmap_len = cmap.len();
        doc.set_object(Object::new(
            cmap_num,
            0,
            Value::Stream(
                Dict::new().insert("Length", Value::Integer(cmap_len as i64)),
                StreamData::Owned(cmap),
            ),
        ));
        Some(cmap_num)
    };

    // `/Widths` 覆盖 FirstChar..=LastChar；缺号位置写 0。
    let first_char = type3
        .glyphs
        .iter()
        .map(|glyph| glyph.code)
        .min()
        .expect("glyph list is non-empty");
    let last_char = type3
        .glyphs
        .iter()
        .map(|glyph| glyph.code)
        .max()
        .expect("glyph list is non-empty");
    let mut widths = Array::new();
    for code in first_char..=last_char {
        let width = type3
            .glyphs
            .iter()
            .find(|glyph| glyph.code == code)
            .map_or(0.0, |glyph| glyph.width);
        widths = widths.push(Value::Float(width));
    }

    let bbox = type3
        .font_bbox
        .iter()
        .fold(Array::new(), |arr, value| arr.push(Value::Float(*value)));
    let matrix = type3
        .font_matrix
        .iter()
        .fold(Array::new(), |arr, value| arr.push(Value::Float(*value)));

    let mut font_dict = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name("Type3".into()))
        .insert("FontBBox", Value::Array(bbox))
        .insert("FontMatrix", Value::Array(matrix))
        .insert("CharProcs", Value::Dict(char_procs))
        .insert("FirstChar", Value::Integer(i64::from(first_char)))
        .insert("LastChar", Value::Integer(i64::from(last_char)))
        .insert("Widths", Value::Array(widths))
        .insert("Encoding", encoding);
    // 未设置 `/Resources` 时不写该条目，保持既有输出不变。
    if let Some(resources) = &type3.resources {
        font_dict = font_dict.insert("Resources", resources.clone());
    }
    if let Some(name) = &type3.name {
        font_dict = font_dict.insert("Name", Value::Name(name.clone()));
    }
    if let Some(descriptor_ref) = descriptor_ref {
        font_dict = font_dict.insert("FontDescriptor", Value::Ref(descriptor_ref, 0));
    }
    if let Some(to_unicode_ref) = to_unicode_ref {
        font_dict = font_dict.insert("ToUnicode", Value::Ref(to_unicode_ref, 0));
    }
    Ok(font_dict)
}
