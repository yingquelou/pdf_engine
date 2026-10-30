//! 注解字典写出（12.5.1–12.5.6）：通用条目 + 子类型专有条目（Text `/Name`
//! `/Open` `/State` `/StateModel`、Caret `/Symbol`、Ink `/InkList`、
//! Line `/L` `/LE` `/LL` `/LLE` `/Cap` `/IT` `/LLO` `/CP` `/CO`、
//! FreeText `/DA` `/RC` `/Q` `/DS` `/CL` `/IT` `/BE` `/RD` `/LE`、
//! Polygon/PolyLine `/Vertices` `/IT`、Link `/H` `/PA` `/QuadPoints` `/BS`、
//! Popup `/Parent` `/Open`、Redact `/OverlayText` `/DA` `/Q` `/Repeat` `/RO`
//! （Table 192）、Screen `/MK`（Table 189，见 `annotation_mk`）、
//! Line/Polygon/PolyLine `/Measure`）+
//! `/A` `/Dest` `/Popup` `/FS` 引用解析，以及各子类型条目的版本门禁。
//! `/Popup` 与 `/Parent` 只需声明一侧，另一侧由 `annotation_popup::pair_popups`
//! 补出（12.5.2、Table 163、Table 183）。

use crate::core::builder::navigation::destination_value;
use crate::core::builder::types::text_string_value;
use crate::core::builder::types::{
    Action, ActionRefs, AnnotationDestination, AnnotationKind, PdfAnnotation, PolygonIntent,
    ThreeDArtwork,
};
use crate::core::reader::navigation::Destination;
use crate::core::types::{Array, Dict, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;
use std::cell::Cell;

use super::annotation_mk::appearance_characteristics_dict;
use super::annotation_popup::PopupPairs;
use super::real_array;
use super::sound::annotation_sound;
use super::three_d::write_annotation_entries;

/// 注解写出时按句柄解析的间接对象号映射。
pub(super) struct AnnotationRefs<'a> {
    /// `/Popup`、`/Parent` 与 Hide 动作 `/T` 的注解目标。
    pub(super) annotation_nums: &'a [i64],
    /// `/AP`、`/RO` 与 `/MK` 的外观流。
    pub(super) form_nums: &'a [i64],
    /// `/FS` 关联的文件规格。
    pub(super) embedded_nums: &'a [i64],
    /// Set-OCG-State 动作 `/State` 的可选内容组。
    pub(super) ocg_nums: &'a [i64],
    /// Sound 动作与 Sound 注解 `/Sound` 的声音流对象。
    pub(super) sound_nums: &'a [i64],
    /// 3D 注解 `/3DD` 的 artwork 流对象。
    pub(super) three_d_nums: &'a [i64],
    /// 3D 注解 `/3DV` 索引/名称选择所依据的注册 artwork。
    pub(super) three_d_artworks: &'a [ThreeDArtwork],
    /// Rendition 动作内联 media clip section 的首级 `/D` 目标对象号。
    pub(super) rendition_nums: &'a [i64],
    /// 内联 section 的消费游标；与大纲写出共享。
    pub(super) rendition_cursor: Option<&'a Cell<usize>>,
}

impl AnnotationRefs<'_> {
    /// 动作字典写出所需的同一组对象号映射。
    fn action_refs(&self) -> ActionRefs<'_> {
        ActionRefs {
            annotation_nums: self.annotation_nums,
            ocg_nums: self.ocg_nums,
            sound_nums: self.sound_nums,
            form_nums: self.form_nums,
            embedded_nums: self.embedded_nums,
            rendition_nums: self.rendition_nums,
            rendition_cursor: self.rendition_cursor,
        }
    }
}

/// 把单个注解写成字典；对象号已由 assemble 主流程按句柄分配。
///
/// `page_refs` 用于把 `/Dest` 与 `GoTo` 动作中的 0 基页号解析为
/// Page 字典间接引用；`popup_pairs` 提供按单侧声明补出的另一端；
/// `refs` 提供注解、外观流、嵌入文件与可选内容组的对象号映射。
pub(super) fn annotation_dict(
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
    if annot.flags() != 0 {
        dict = dict.insert("F", Value::Integer(i64::from(annot.flags())));
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
    if let Some(date) = annot.modified_date() {
        dict = dict.insert("M", text_string_value(date));
    }
    if let Some(date) = annot.creation_date() {
        dict = dict.insert("CreationDate", text_string_value(date));
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

/// 几何类注解（Square/Circle/Polygon/PolyLine）子类型专有条目对应的特性集合。
///
/// `/BE` PDF 1.5（Square/Circle/Polygon）、`/RD` PDF 1.5（Square/Circle）、
/// `/IT /PolygonCloud` PDF 1.6、`/IT` 标注类 PDF 1.7。FreeText 的同名条目
/// 归 [`free_text_feature_gate`] 门禁，不在此重复。
pub(super) fn geometry_features(annot: &PdfAnnotation) -> Vec<PdfFeature> {
    let mut features = Vec::new();
    if let Some(intent) = annot.geometry().intent() {
        features.push(match intent {
            PolygonIntent::PolygonCloud => PdfFeature::PolygonCloudIntent,
            PolygonIntent::PolyLineDimension | PolygonIntent::PolygonDimension => {
                PdfFeature::PolygonDimensionIntent
            }
        });
    }
    let kind = annot.kind();
    let border = annot.free_text();
    if border.border_effect().is_some()
        && matches!(
            kind,
            AnnotationKind::Square | AnnotationKind::Circle | AnnotationKind::Polygon
        )
    {
        features.push(PdfFeature::AnnotationBorderEffect);
    }
    if border.text_rectangle().is_some()
        && matches!(kind, AnnotationKind::Square | AnnotationKind::Circle)
    {
        features.push(PdfFeature::AnnotationRectDifferences);
    }
    features
}

/// 几何类专有条目的版本门禁（Table 177/178）。
pub(super) fn geometry_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    for feature in geometry_features(annot) {
        if !version.supports(feature) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{feature} require PDF {}, got {}",
                feature.min_version().as_str(),
                version.as_str()
            )));
        }
    }
    Ok(())
}

/// 注解 `/Measure` 测量字典（Table 261–263）自 PDF 1.7 起可用（Table 175/178）。
pub(super) fn measure_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    if annot.measure().is_some() && !version.supports(PdfFeature::AnnotationMeasure) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::AnnotationMeasure,
            PdfFeature::AnnotationMeasure.min_version().as_str(),
            version.as_str()
        )));
    }
    Ok(())
}

/// Link 专有条目对应的特性集合（Table 173）：`/H` PDF 1.2、`/PA` PDF 1.3、
/// `/QuadPoints` 与 `/BS` PDF 1.6。
pub(super) fn link_features(annot: &PdfAnnotation) -> Vec<PdfFeature> {
    let link = annot.link();
    let mut features = Vec::new();
    if link.highlight().is_some() {
        features.push(PdfFeature::LinkAnnotationHighlighting);
    }
    if link.pressed_action().is_some() {
        features.push(PdfFeature::LinkAnnotationPressedAction);
    }
    if annot.kind() == AnnotationKind::Link {
        if annot.quad_points().is_some() {
            features.push(PdfFeature::LinkAnnotationQuadPoints);
        }
        if annot.border_width().is_some()
            || annot.border_style().is_some()
            || !annot.border_dash().is_empty()
        {
            features.push(PdfFeature::LinkAnnotationBorderStyle);
        }
    }
    features
}

/// Link 专有条目的版本门禁（Table 173）：`/H` PDF 1.2、`/PA` PDF 1.3、
/// `/QuadPoints` 与 `/BS` PDF 1.6。
pub(super) fn link_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    for feature in link_features(annot) {
        if !version.supports(feature) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{feature} require PDF {}, got {}",
                feature.min_version().as_str(),
                version.as_str()
            )));
        }
    }
    Ok(())
}

/// 旧式 `/Border` 虚线项（Table 164，PDF 1.1）的版本门禁。
pub(super) fn border_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    if let Some(border) = annot.border_array()
        && border.has_dash()
        && !version.supports(PdfFeature::AnnotationBorderDash)
    {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::AnnotationBorderDash,
            PdfFeature::AnnotationBorderDash.min_version().as_str(),
            version.as_str()
        )));
    }
    Ok(())
}

/// Line 专有条目的版本门禁（Table 175/176）。
///
/// `/LE` 与端点样式取值各自有版本门起：`/LE` 与 6 种基础样式 PDF 1.4，
/// `Butt`/`ROpenArrow`/`RClosedArrow` PDF 1.5，`Slash` PDF 1.6；
/// `/LL` `/LLE` `/Cap` `/IT` PDF 1.6，`/LLO` `/CP` `/CO` PDF 1.7。
pub(super) fn line_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    let line = annot.line();
    for feature in [
        (
            line.has_arrow_entries(),
            PdfFeature::LineAnnotationArrowEndings,
        ),
        (line.has_leader_entries(), PdfFeature::LineAnnotationLeader),
        (
            line.has_caption_entries(),
            PdfFeature::LineAnnotationCaption,
        ),
    ] {
        let (declared, feature) = feature;
        if declared && !version.supports(feature) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{feature} require PDF {}, got {}",
                feature.min_version().as_str(),
                version.as_str()
            )));
        }
    }
    if let Some(endings) = line.endings() {
        for style in endings.iter() {
            if version < style.min_version() {
                return Err(WriteError::UnsupportedVersion(format!(
                    "line annotation /LE /{} requires PDF {}, got {}",
                    style.name(),
                    style.min_version().as_str(),
                    version.as_str()
                )));
            }
        }
    }
    Ok(())
}

/// FreeText 专有条目的版本门禁（Table 174）。
///
/// `/Q` PDF 1.4；`/RC` `/DS` PDF 1.5；`/CL` `/IT` `/BE` `/RD` `/LE` PDF 1.6。
/// `/DA` 随 FreeText 子类型自 PDF 1.3 起可用，不单独门禁。`/BE` 与 `/RD`
/// 与几何类注解共用，但 FreeText 上仍属 Table 174 的 PDF 1.6 条目；
/// Square/Circle/Polygon 由 [`geometry_feature_gate`] 按 PDF 1.5 门禁。
pub(super) fn free_text_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    let free_text = annot.free_text();
    let has_shared_border_entries =
        free_text.border_effect().is_some() || free_text.text_rectangle().is_some();
    for (declared, feature) in [
        (free_text.has_quadding(), PdfFeature::FreeTextQuadding),
        (
            free_text.has_rich_content(),
            PdfFeature::FreeTextRichContent,
        ),
        (
            free_text.has_callout_entries()
                || (annot.kind() == AnnotationKind::FreeText && has_shared_border_entries),
            PdfFeature::FreeTextCallout,
        ),
    ] {
        if declared && !version.supports(feature) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{feature} require PDF {}, got {}",
                feature.min_version().as_str(),
                version.as_str()
            )));
        }
    }
    Ok(())
}
