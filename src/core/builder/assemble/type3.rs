//! Type 3 字体字典与字符过程流。

use crate::core::builder::{Font, Type3Font};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
use crate::core::writer::WriteError;
use std::collections::BTreeSet;

/// 构造 Type 3 字体字典并写出各字符过程流（PDF 32000-1 §9.6.5）。
///
/// `/CharProcs` 的流对象号按 `type3.glyphs` 的顺序从 `num` 连续分配，
/// 保证同一字体定义总是得到同一对象号布局。字典不含 `/BaseFont`、
/// `/FontDescriptor` 或字体程序；`/Encoding` 必须存在（Type 3 字体没有
/// `/BaseFont` 可供推导内建编码）。
///
/// 以下情况返回 [`WriteError::InvalidObject`]：嵌入字体程序、请求
/// `subset_glyphs`/`subset_type1_glyphs`/`subset_cff_glyphs`、字形列表为空、
/// 字符码或字形名重复、`/Encoding` 缺失、`/Resources` 不是字典或间接引用。
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
    Ok(font_dict)
}
