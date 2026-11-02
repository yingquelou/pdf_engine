use crate::core::builder::assemble::annotation_markup::write_markup_entries;
use crate::core::builder::assemble::annotation_mk::appearance_characteristics_dict;
use crate::core::builder::assemble::annotation_popup::PopupPairs;
use crate::core::builder::assemble::hex_bytes;
use crate::core::builder::assemble::real_array;
use crate::core::builder::assemble::sound::annotation_sound;
use crate::core::builder::assemble::three_d::write_annotation_entries;
use crate::core::builder::navigation::destination_value;
use crate::core::builder::types::text_string_value;
use crate::core::builder::types::{Action, AnnotationDestination, PdfAnnotation};
use crate::core::reader::navigation::Destination;
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

use super::AnnotationRefs;

/// 把单个注解写成字典；对象号已由 assemble 主流程按句柄分配。
///
/// `page_refs` 用于把 `/Dest` 与 `GoTo` 动作中的 0 基页号解析为
/// Page 字典间接引用；`popup_pairs` 提供按单侧声明补出的另一端；
/// `refs` 提供注解、外观流、嵌入文件与可选内容组的对象号映射。
pub(in crate::core::builder::assemble) fn annotation_dict(
    annot: &PdfAnnotation,
    handle: usize,
    popup_pairs: &PopupPairs,
    page_refs: &[(i64, i64)],
    refs: &AnnotationRefs<'_>,
) -> Result<Dict, WriteError> {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Annot".into()))
        .insert("Subtype", Value::Name(annot.kind().name().into()))
        .insert("Rect", real_array(&annot.rect()));
    // Printer's mark 与 Trap 网络注解的 `/F` 恒为 Print|ReadOnly（68，
    // 14.11.3 / 14.11.6），即使调用方未显式设置标志位也必须写出。
    let flags = annot.written_flags();
    if flags != 0 {
        dict = dict.insert("F", Value::Integer(i64::from(flags)));
    }
    if let Some(contents) = annot.contents() {
        dict = dict.insert("Contents", text_string_value(contents));
    }
    if let Some(title) = annot.title() {
        dict = dict.insert("T", text_string_value(title));
    }
    if let Some(name) = annot.name() {
        dict = dict.insert("NM", text_string_value(name));
    }
    // `/MN` 是 Printer's mark 的标记名称（Table 362，name 而非文本串）。
    if let Some(mark) = annot.printer_mark()
        && let Some(name) = mark.mark_name()
    {
        dict = dict.insert("MN", Value::Name(name.into()));
    }
    if let Some(date) = annot.modified_date() {
        dict = dict.insert("M", text_string_value(date));
    }
    if let Some(date) = annot.creation_date() {
        dict = dict.insert("CreationDate", text_string_value(date));
    }
    // Trap 网络跟踪条目（Table 366）：`/LastModified` 或 `/Version` +
    // `/AnnotStates`，二者由构建期校验保证互斥且恰有其一。
    if let Some(trap) = annot.trap_network() {
        if let Some(date) = trap.last_modified() {
            dict = dict.insert("LastModified", text_string_value(&date.to_pdf_string()));
        }
        if let Some(version) = trap.version() {
            dict = dict.insert("Version", Value::Array(Array(version.to_vec())));
        }
        if let Some(states) = trap.annot_states() {
            let states = states
                .iter()
                .map(|state| match state {
                    Some(name) => Value::Name(name.clone()),
                    None => Value::Null,
                })
                .collect();
            dict = dict.insert("AnnotStates", Value::Array(Array(states)));
        }
    }
    if let Some(color) = annot.color() {
        dict = dict.insert("C", real_array(color));
    }
    if let Some(color) = annot.interior_color() {
        dict = dict.insert("IC", real_array(color));
    }
    if let Some(opacity) = annot.opacity() {
        dict = dict.insert("CA", Value::Float(opacity));
    }
    if let Some(mode) = annot.blend_mode() {
        dict = dict.insert("BM", Value::Name(mode.into()));
    }
    if let Some(bs) = border_style_dict(annot) {
        dict = dict.insert("BS", Value::Dict(bs));
    }
    if let Some(border) = annot.border_array() {
        dict = dict.insert("Border", Value::Array(border.to_array()));
    }
    if let Some(quad) = annot.quad_points() {
        dict = dict.insert("QuadPoints", real_array(quad));
    }
    if let Some(icon) = annot.icon_name() {
        dict = dict.insert("Name", Value::Name(icon.into()));
    }
    if let Some(open) = annot.is_open() {
        dict = dict.insert("Open", Value::Bool(open));
    }
    if let Some(state) = annot.state() {
        dict = dict.insert("State", text_string_value(state));
    }
    if let Some(model) = annot.state_model() {
        dict = dict.insert("StateModel", text_string_value(model));
    }
    if let Some(symbol) = annot.symbol() {
        dict = dict.insert("Symbol", Value::Name(symbol.into()));
    }
    if !annot.ink_paths().is_empty() {
        let paths = annot
            .ink_paths()
            .iter()
            .map(|path| real_array(path))
            .collect();
        dict = dict.insert("InkList", Value::Array(Array(paths)));
    }
    let line = annot.line();
    if let Some(endpoints) = line.endpoints() {
        dict = dict.insert("L", real_array(&endpoints));
    }
    if let Some(endings) = line.endings() {
        let names = endings
            .iter()
            .map(|style| Value::Name(style.name().into()))
            .collect();
        dict = dict.insert("LE", Value::Array(Array(names)));
    }
    if let Some(length) = line.leader_length() {
        dict = dict.insert("LL", Value::Float(length));
    }
    if let Some(extension) = line.leader_extension() {
        dict = dict.insert("LLE", Value::Float(extension));
    }
    if let Some(caption) = line.caption() {
        dict = dict.insert("Cap", Value::Bool(caption));
    }
    if let Some(intent) = line.intent() {
        dict = dict.insert("IT", Value::Name(intent.name().into()));
    }
    if let Some(offset) = line.leader_line_offset() {
        dict = dict.insert("LLO", Value::Float(offset));
    }
    if let Some(position) = line.caption_position() {
        dict = dict.insert("CP", Value::Name(position.name().into()));
    }
    if let Some(offset) = line.caption_offset() {
        dict = dict.insert("CO", real_array(&offset));
    }
    let free_text = annot.free_text();
    if let Some(default_appearance) = free_text.default_appearance() {
        dict = dict.insert("DA", text_string_value(default_appearance));
    }
    if let Some(rich_content) = free_text.rich_content() {
        dict = dict.insert("RC", text_string_value(rich_content));
    }
    if let Some(quadding) = free_text.quadding() {
        dict = dict.insert("Q", Value::Integer(i64::from(quadding)));
    }
    if let Some(default_style) = free_text.default_style() {
        dict = dict.insert("DS", text_string_value(default_style));
    }
    if let Some(callout_line) = free_text.callout_line() {
        dict = dict.insert("CL", real_array(callout_line.values()));
    }
    if let Some(intent) = free_text.intent() {
        dict = dict.insert("IT", Value::Name(intent.name().into()));
    }
    if let Some(effect) = free_text.border_effect() {
        let mut border_effect = Dict::new().insert("S", Value::Name(effect.style.name().into()));
        if let Some(intensity) = effect.intensity {
            border_effect = border_effect.insert("I", Value::Float(intensity));
        }
        dict = dict.insert("BE", Value::Dict(border_effect));
    }
    if let Some(text_rectangle) = free_text.text_rectangle() {
        dict = dict.insert("RD", real_array(&text_rectangle));
    }
    if let Some(style) = free_text.callout_ending() {
        dict = dict.insert("LE", Value::Name(style.name().into()));
    }
    let geometry = annot.geometry();
    if let Some(vertices) = geometry.vertices() {
        dict = dict.insert("Vertices", real_array(vertices));
    }
    if let Some(intent) = geometry.intent() {
        dict = dict.insert("IT", Value::Name(intent.name().into()));
    }
    if let Some(measure) = annot.measure() {
        dict = dict.insert("Measure", Value::Dict(measure.to_dict()));
    }
    let link = annot.link();
    if let Some(highlight) = link.highlight() {
        dict = dict.insert("H", Value::Name(highlight.name().into()));
    }
    if let Some(action) = link.pressed_action() {
        dict = dict.insert("PA", Value::Dict(action.to_dict(&refs.action_refs())?));
    }
    let redact = annot.redact();
    if let Some(text) = redact.overlay_text() {
        dict = dict.insert("OverlayText", text_string_value(text));
    }
    if let Some(appearance) = redact.appearance_string() {
        dict = dict.insert("DA", text_string_value(appearance));
    }
    if let Some(quadding) = redact.quadding() {
        dict = dict.insert("Q", Value::Integer(i64::from(quadding)));
    }
    if let Some(repeat) = redact.repeat() {
        dict = dict.insert("Repeat", Value::Bool(repeat));
    }
    if let Some(form) = redact.overlay_form() {
        let num = refs.form_nums.get(form).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /RO references unknown form handle {form}"
            ))
        })?;
        dict = dict.insert("RO", Value::Ref(*num, 0));
    }
    if let Some(characteristics) = annot.appearance_characteristics() {
        let mk = appearance_characteristics_dict(characteristics, handle, refs.form_nums)?;
        dict = dict.insert("MK", Value::Dict(mk));
    }
    // `/FixedPrint` 写成直接字典（Table 191）；缺省条目不物化。
    if let Some(fixed_print) = annot.fixed_print() {
        let mut fixed = Dict::new().insert("Type", Value::Name("FixedPrint".into()));
        if let Some(matrix) = fixed_print.matrix() {
            fixed = fixed.insert("Matrix", real_array(&matrix));
        }
        if let Some(horizontal) = fixed_print.horizontal() {
            fixed = fixed.insert("H", Value::Float(horizontal));
        }
        if let Some(vertical) = fixed_print.vertical() {
            fixed = fixed.insert("V", Value::Float(vertical));
        }
        dict = dict.insert("FixedPrint", Value::Dict(fixed));
    }
    // `/ExData` 3D 标记外部数据字典（13.6.6 Table 313）：/3DA 指向目标
    // 3D 注解，/3DV 为内联视图字典。/Type 按 Table 313 写 ExData；ISO
    // 13.6.6 的示例写作 Markup3D，读端两者都接受。
    if let Some(markup) = annot.ex_data() {
        let num = refs.annotation_nums.get(markup.annotation()).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /ExData /3DA references unknown annotation handle {}",
                markup.annotation()
            ))
        })?;
        let mut ex_data = Dict::new()
            .insert("Type", Value::Name("ExData".into()))
            .insert("Subtype", Value::Name("Markup3D".into()))
            .insert("3DA", Value::Ref(*num, 0))
            .insert("3DV", Value::Dict(markup.view().to_dict(refs.form_nums)));
        if let Some(md5) = markup.md5() {
            ex_data = ex_data.insert("MD5", Value::HexString(hex_bytes(md5)));
        }
        dict = dict.insert("ExData", Value::Dict(ex_data));
    }
    // 标记类注解的 Table 170 附加条目 `/Subj` `/IRT` `/RT`；`/IRT` 的对象号
    // 由 annotation_markup 的跨注解前序校验保证有效。
    write_markup_entries(annot, handle, refs.annotation_nums, &mut dict)?;
    if let Some(action) = annot.action() {
        // `GoTo` 的 `/D` 页号在这里解析为间接引用；其余动作沿用
        // `to_dict` 的直接构造。
        let action_dict = match action {
            Action::GoTo(destination) => {
                let value = explicit_destination_value(destination, page_refs)?;
                Dict::new()
                    .insert("S", Value::Name("GoTo".into()))
                    .insert("D", value)
            }
            other => other.to_dict(&refs.action_refs())?,
        };
        dict = dict.insert("A", Value::Dict(action_dict));
    }
    if let Some(destination) = annot.destination() {
        let value = annotation_destination_value(destination, page_refs)?;
        dict = dict.insert("Dest", value);
    }
    if let Some(popup) = annot.popup().or_else(|| popup_pairs.popup_of(handle)) {
        let num = refs.annotation_nums.get(popup).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /Popup references unknown annotation handle {popup}"
            ))
        })?;
        dict = dict.insert("Popup", Value::Ref(*num, 0));
    }
    if let Some(parent) = annot
        .popup_parent()
        .or_else(|| popup_pairs.parent_of(handle))
    {
        if parent == handle {
            return Err(WriteError::InvalidObject(format!(
                "annotation handle {handle}: /Parent must not reference the annotation itself"
            )));
        }
        let num = refs.annotation_nums.get(parent).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /Parent references unknown annotation handle {parent}"
            ))
        })?;
        dict = dict.insert("Parent", Value::Ref(*num, 0));
    }
    if let Some(target) = annot.filespec() {
        let num = refs.embedded_nums.get(target).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /FS references unknown embedded file handle {target}"
            ))
        })?;
        dict = dict.insert("FS", Value::Ref(*num, 0));
    }
    if let Some(sound) = annotation_sound(annot, handle, refs.sound_nums)? {
        dict = dict.insert("Sound", sound);
    }
    write_annotation_entries(
        annot,
        handle,
        refs.three_d_nums,
        refs.three_d_artworks,
        &mut dict,
        refs.form_nums,
    )?;
    if let Some(movie) = annot.movie() {
        dict = dict.insert("Movie", Value::Dict(movie.to_dict()));
    }
    if let Some(playback) = annot.movie_playback() {
        dict = dict.insert("A", playback.to_value());
    }
    let mut appearance = Dict::new();
    if !annot.appearance_states().is_empty() {
        // `/N` 为状态名 → 外观流引用（Widget 多状态外观，Table 168）。
        let mut states = Dict::new();
        for (state, form) in annot.appearance_states() {
            let num = refs.form_nums.get(*form).ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "annotation handle {handle}: /AP /N state {state} references unknown form handle {form}"
                ))
            })?;
            states = states.insert(state.clone(), Value::Ref(*num, 0));
        }
        appearance = appearance.insert("N", Value::Dict(states));
    } else if let Some(form) = annot.appearance_normal() {
        let num = refs.form_nums.get(form).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /AP /N references unknown form handle {form}"
            ))
        })?;
        appearance = appearance.insert("N", Value::Ref(*num, 0));
    }
    for (key, form) in [
        ("D", annot.appearance_down()),
        ("R", annot.appearance_rollover()),
    ] {
        if let Some(form) = form {
            let num = refs.form_nums.get(form).ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "annotation handle {handle}: /AP /{key} references unknown form handle {form}"
                ))
            })?;
            appearance = appearance.insert(key, Value::Ref(*num, 0));
        }
    }
    if !appearance.0.is_empty() {
        dict = dict.insert("AP", Value::Dict(appearance));
    }
    if let Some(state) = annot.appearance_state() {
        dict = dict.insert("AS", Value::Name(state.into()));
    }
    Ok(dict)
}

/// `/BS` 边框样式字典：`/W` `/S` `/D` 仅在提供时写出；三项全空返回 `None`。
fn border_style_dict(annot: &PdfAnnotation) -> Option<Dict> {
    if annot.border_width().is_none()
        && annot.border_style().is_none()
        && annot.border_dash().is_empty()
    {
        return None;
    }
    let mut dict = Dict::new();
    if let Some(width) = annot.border_width() {
        dict = dict.insert("W", Value::Float(width));
    }
    if let Some(style) = annot.border_style() {
        dict = dict.insert("S", Value::Name(style.name().into()));
    }
    if !annot.border_dash().is_empty() {
        dict = dict.insert("D", real_array(annot.border_dash()));
    }
    Some(dict)
}

/// 把 [`AnnotationDestination`] 写成 `/Dest` 值（显式数组或命名文本串）。
fn annotation_destination_value(
    destination: &AnnotationDestination,
    page_refs: &[(i64, i64)],
) -> Result<Value, WriteError> {
    match destination {
        AnnotationDestination::Explicit(destination) => {
            explicit_destination_value(destination, page_refs)
        }
        AnnotationDestination::Named(name) => Ok(text_string_value(name)),
    }
}

/// 显式目标数组：先校验 0 基页号在页集合内，再交给导航层解析为引用。
fn explicit_destination_value(
    destination: &Destination,
    page_refs: &[(i64, i64)],
) -> Result<Value, WriteError> {
    if destination.page() >= page_refs.len() {
        return Err(WriteError::InvalidObject(format!(
            "annotation destination references unknown page {}",
            destination.page()
        )));
    }
    destination_value(destination, page_refs)
}
