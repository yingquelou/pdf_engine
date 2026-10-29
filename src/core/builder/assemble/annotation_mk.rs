//! 外观特性字典 `/MK`（Table 189）与图标适配字典 `/IF`（Table 247）的写出。
//!
//! 挂在 Screen 注解的字典上（Widget 端的 `/MK` 归 E4 的字段写路径）；
//! `/I` `/RI` `/IX` 按 Table 189 要求写成 Form XObject 流引用的间接引用，
//! 句柄取自 `annotation_dict` 的 `form_nums` 表，越界句柄报
//! `WriteError::InvalidObject`（构建在任何字节写出前失败）。

use crate::core::builder::types::{AppearanceCharacteristics, text_string_value};
use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

use super::real_array;

/// 构造 `/MK` 字典。
pub(super) fn appearance_characteristics_dict(
    characteristics: &AppearanceCharacteristics,
    handle: usize,
    form_nums: &[i64],
) -> Result<Dict, WriteError> {
    let mut mk = Dict::new();
    if let Some(rotation) = characteristics.rotation() {
        mk = mk.insert("R", Value::Integer(rotation));
    }
    if let Some(color) = characteristics.border_color() {
        mk = mk.insert("BC", real_array(color));
    }
    if let Some(color) = characteristics.background_color() {
        mk = mk.insert("BG", real_array(color));
    }
    for (key, caption) in [
        ("CA", characteristics.caption()),
        ("RC", characteristics.rollover_caption()),
        ("AC", characteristics.alternate_caption()),
    ] {
        if let Some(caption) = caption {
            mk = mk.insert(key, text_string_value(caption));
        }
    }
    let icon_handles = characteristics.icon_handles();
    for (key, form) in [
        ("I", icon_handles[0]),
        ("RI", icon_handles[1]),
        ("IX", icon_handles[2]),
    ] {
        let Some(form) = form else { continue };
        let num = form_nums.get(form).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /MK /{key} references unknown form handle {form}"
            ))
        })?;
        mk = mk.insert(key, Value::Ref(*num, 0));
    }
    if let Some(icon_fit) = characteristics.icon_fit() {
        let mut fit = Dict::new();
        if let Some(scale_when) = icon_fit.scale_when() {
            fit = fit.insert("SW", Value::Name(scale_when.name().into()));
        }
        if let Some(scaling) = icon_fit.scaling() {
            fit = fit.insert("S", Value::Name(scaling.name().into()));
        }
        if let Some(leftover) = icon_fit.leftover() {
            fit = fit.insert("A", real_array(&leftover));
        }
        if let Some(fit_border) = icon_fit.fit_border() {
            fit = fit.insert("FB", Value::Bool(fit_border));
        }
        mk = mk.insert("IF", Value::Dict(fit));
    }
    if let Some(position) = characteristics.text_position() {
        mk = mk.insert("TP", Value::Integer(i64::from(position)));
    }
    Ok(mk)
}
