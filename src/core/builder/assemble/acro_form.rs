//! AcroForm `/DR` 默认资源字典（Table 218 / 资源字典 Table 34）。
//!
//! 字体经 `font_pool` 与页面字体去重共享对象；`/ExtGState` 写成直接
//! 参数子字典（只接受自包含状态），`/ColorSpace` 复用页面颜色空间
//! 值构造（ICCBased 与函数流分配独立对象），`/ProcSet` 写成名称数组。

use std::collections::BTreeSet;

use crate::core::builder::types::{AcroForm, Font, validate_name_value};
use crate::core::builder::{GraphicsStateFunctions, same_font};
use crate::core::types::{Array, Dict, Document, Value};
use crate::core::writer::WriteError;

use super::color_space_value;

/// 写前校验：各类资源名合法且同类不重名；`/ExtGState` 自包含；
/// `/ColorSpace` 取值合法。
pub(super) fn validate_default_resources(form: &AcroForm) -> Result<(), WriteError> {
    validate_names(form.default_fonts().iter().map(|(name, _)| name), "font")?;
    validate_names(
        form.default_ext_gstates().iter().map(|(name, _)| name),
        "ExtGState",
    )?;
    validate_names(
        form.default_color_spaces().iter().map(|(name, _)| name),
        "ColorSpace",
    )?;
    validate_names(form.proc_set().iter(), "ProcSet")?;
    for (name, state) in form.default_ext_gstates() {
        if !state.is_self_contained() {
            return Err(WriteError::InvalidObject(format!(
                "AcroForm /DR ExtGState {name:?} references external objects"
            )));
        }
        state
            .to_value(None, None, GraphicsStateFunctions::default())
            .map_err(|error| {
                WriteError::InvalidObject(format!("AcroForm /DR ExtGState {name:?}: {error}"))
            })?;
    }
    for (name, space) in form.default_color_spaces() {
        space.validate().map_err(|message| {
            WriteError::InvalidObject(format!("AcroForm /DR ColorSpace {name:?}: {message}"))
        })?;
    }
    Ok(())
}

fn validate_names<'a>(
    names: impl Iterator<Item = &'a String>,
    category: &str,
) -> Result<(), WriteError> {
    let mut seen = BTreeSet::new();
    for name in names {
        validate_name_value(name, &format!("AcroForm /DR {category} resource name"))
            .map_err(WriteError::InvalidObject)?;
        if !seen.insert(name.as_str()) {
            return Err(WriteError::InvalidObject(format!(
                "AcroForm /DR declares duplicate {category} resource name {name:?}"
            )));
        }
    }
    Ok(())
}

/// 构造 `/DR` 默认资源字典；未声明任何资源时返回 `None`。
pub(super) fn default_resources(
    form: &AcroForm,
    doc: &mut Document,
    num: &mut i64,
    font_pool: &mut Vec<(Font, i64)>,
) -> Result<Option<Value>, WriteError> {
    let mut dr = Dict::new();
    if !form.default_fonts().is_empty() {
        let mut fonts = Dict::new();
        for (name, font) in form.default_fonts() {
            let obj_num = match font_pool.iter().position(|(g, _)| same_font(g, font)) {
                Some(index) => font_pool[index].1,
                None => {
                    let n = *num;
                    *num += 1;
                    font_pool.push((font.clone(), n));
                    n
                }
            };
            fonts = fonts.insert(name.clone(), Value::Ref(obj_num, 0));
        }
        dr = dr.insert("Font", Value::Dict(fonts));
    }
    if !form.default_ext_gstates().is_empty() {
        let mut states = Dict::new();
        for (name, state) in form.default_ext_gstates() {
            let dict = state.to_value(None, None, GraphicsStateFunctions::default())?;
            states = states.insert(name.clone(), Value::Dict(dict));
        }
        dr = dr.insert("ExtGState", Value::Dict(states));
    }
    if !form.default_color_spaces().is_empty() {
        let mut spaces = Dict::new();
        for (name, space) in form.default_color_spaces() {
            spaces = spaces.insert(name.clone(), color_space_value(space, doc, num)?);
        }
        dr = dr.insert("ColorSpace", Value::Dict(spaces));
    }
    if !form.proc_set().is_empty() {
        let names = form
            .proc_set()
            .iter()
            .map(|name| Value::Name(name.clone()))
            .collect();
        dr = dr.insert("ProcSet", Value::Array(Array(names)));
    }
    Ok((!dr.0.is_empty()).then_some(Value::Dict(dr)))
}
