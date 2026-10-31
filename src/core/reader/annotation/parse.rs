//! 单个注解字典的解析入口（12.5.1–12.5.6）：通用条目、子类型专有条目
//! 与 `/ExData` 3D 标记（13.6.6）在此汇合为 [`Annotation`]。
//!
//! 从 `mod.rs` 拆出以维持单文件规模约定；错误构造与类型化子模块仍在
//! 父模块中，本模块只负责按表组装字段。

use super::entries::{
    parse_appearance, parse_border, parse_number_array, parse_opacity, parse_rect,
    parse_text_string,
};
use super::{
    Annotation, AnnotationError, AnnotationFlags, AnnotationSubtype, Result, exdata, malformed,
    measure, mk, movie, printer_mark, redact, sound, subtype, trap_net, watermark,
};
use crate::core::reader::three_d::ThreeDError;
use crate::core::types::{Dict, Document, Value};

pub(super) fn parse_annotation(
    document: &Document,
    page_index: usize,
    reference: Option<(i64, i64)>,
    dict: &Dict,
) -> Result<Annotation> {
    match dict.get("Type").and_then(Value::as_name) {
        Some("Annot") => {}
        Some(other) => {
            return Err(malformed(format!(
                "annotation /Type must be Annot, found {other}"
            )));
        }
        None => return Err(malformed("annotation dictionary has no /Type")),
    }
    let subtype = AnnotationSubtype::from_name(
        dict.get("Subtype")
            .and_then(Value::as_name)
            .ok_or_else(|| malformed("annotation has no /Subtype"))?,
    );
    let rect = parse_rect(dict.get("Rect"))?;
    let flags = match dict.get("F") {
        Some(value) => match value {
            Value::Integer(bits) if *bits >= 0 && *bits <= u32::MAX as i64 => {
                AnnotationFlags::from_bits(*bits as u32)
            }
            _ => return Err(malformed("annotation /F is not a nonnegative integer")),
        },
        None => AnnotationFlags::default(),
    };
    let entries = subtype::parse_subtype_entries(dict)?;
    let redact = redact::parse_redact_entries(dict)?;
    let mk = mk::parse_appearance_characteristics(document, dict)?;
    let fixed_print = watermark::parse_fixed_print(document, dict)?;
    let sound = sound::parse_sound_entries(dict, &subtype)?;
    let movie = movie::parse_movie_entries(document, dict, &subtype)?;
    let printer_mark = printer_mark::parse_printer_mark(document, dict, &subtype)?;
    let trap_network = trap_net::parse_trap_network(document, dict, &subtype)?;
    let three_d = crate::core::reader::three_d::parse_three_d_entries(document, dict, &subtype)
        .map_err(|error| match error {
            ThreeDError::Malformed(message) => malformed(format!("3D annotation: {message}")),
            ThreeDError::LimitExceeded(what) => AnnotationError::LimitExceeded(what),
        })?;
    let three_d = three_d.unwrap_or_default();

    Ok(Annotation {
        page_index,
        reference,
        subtype,
        rect,
        flags,
        contents: parse_text_string(dict, "Contents")?,
        rich_contents: parse_text_string(dict, "RC")?,
        title: parse_text_string(dict, "T")?,
        name: parse_text_string(dict, "NM")?,
        modified_date: parse_text_string(dict, "M")?,
        creation_date: parse_text_string(dict, "CreationDate")?,
        color: parse_number_array(dict, "C", false)?,
        interior_color: parse_number_array(dict, "IC", false)?,
        opacity: parse_opacity(dict)?,
        border: parse_border(document, dict)?,
        appearance: parse_appearance(document, dict)?,
        appearance_state: dict
            .get("AS")
            .map(|value| match value.as_name() {
                Some(name) => Ok(name.to_string()),
                None => Err(malformed("annotation /AS must be a name")),
            })
            .transpose()?,
        printer_mark,
        trap_network,
        quad_points: parse_number_array(dict, "QuadPoints", true)?,
        icon_name: entries.icon_name,
        sound,
        is_open: entries.is_open,
        state: entries.state,
        state_model: entries.state_model,
        symbol: entries.symbol,
        ink_paths: entries.ink_paths,
        line_endpoints: entries.line_endpoints,
        line_endings: entries.line_endings,
        leader_length: entries.leader_length,
        leader_extension: entries.leader_extension,
        caption: entries.caption,
        line_intent: entries.line_intent,
        leader_line_offset: entries.leader_line_offset,
        caption_position: entries.caption_position,
        caption_offset: entries.caption_offset,
        default_appearance: entries.default_appearance,
        quadding: entries.quadding,
        default_style: entries.default_style,
        callout_line: entries.callout_line,
        free_text_intent: entries.free_text_intent,
        border_effect: entries.border_effect,
        text_rectangle: entries.text_rectangle,
        callout_ending: entries.callout_ending,
        vertices: entries.vertices,
        polygon_intent: entries.polygon_intent,
        link_highlight: entries.link.highlight,
        pressed_action: entries.link.pressed_action,
        overlay_text: redact.overlay_text,
        repeat_overlay_text: redact.repeat_overlay_text,
        overlay_appearance: redact.overlay_appearance,
        appearance_characteristics: mk,
        fixed_print,
        movie: movie.movie,
        movie_playback: movie.playback,
        three_d: three_d.artwork,
        three_d_view: three_d.view,
        three_d_activation: three_d.activation,
        three_d_interactive: three_d.interactive,
        three_d_view_box: three_d.view_box,
        action: dict.get("A").cloned(),
        destination: dict.get("Dest").cloned(),
        popup: match dict.get("Popup") {
            None => None,
            Some(value) => Some(
                value
                    .as_ref()
                    .ok_or_else(|| malformed("annotation /Popup must be an indirect reference"))?,
            ),
        },
        parent: match dict.get("Parent") {
            None => None,
            Some(value) => Some(
                value
                    .as_ref()
                    .ok_or_else(|| malformed("annotation /Parent must be an indirect reference"))?,
            ),
        },
        measure: measure::parse_measure(document, dict)?,
        ex_data: exdata::parse_external_data(document, dict.get("ExData"), "annotation /ExData")?,
        dict: dict.clone(),
    })
}
