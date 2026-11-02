//! 注解字典写出（12.5.1–12.5.6）：通用条目 + 子类型专有条目（Text `/Name`
//! `/Open` `/State` `/StateModel`、Caret `/Symbol`、Ink `/InkList`、
//! Line `/L` `/LE` `/LL` `/LLE` `/Cap` `/IT` `/LLO` `/CP` `/CO`、
//! FreeText `/DA` `/RC` `/Q` `/DS` `/CL` `/IT` `/BE` `/RD` `/LE`、
//! Polygon/PolyLine `/Vertices` `/IT`、Link `/H` `/PA` `/QuadPoints` `/BS`、
//! Popup `/Parent` `/Open`、Redact `/OverlayText` `/DA` `/Q` `/Repeat` `/RO`
//! （Table 192）、Screen `/MK`（Table 189，见 `annotation_mk`）、
//! Line/Polygon/PolyLine `/Measure`、标记注解 `/ExData` 3D 标记
//! （Table 170/313，见 `annotation_exdata`）、标记注解附加条目 `/Subj` `/IRT`
//! `/RT`（Table 170，见 `annotation_markup`））+
//! `/A` `/Dest` `/Popup` `/FS` 引用解析，以及各子类型条目的版本门禁。
//! `/Popup` 与 `/Parent` 只需声明一侧，另一侧由 `annotation_popup::pair_popups`
//! 补出（12.5.2、Table 163、Table 183）。
//!
//! 本模块按主题目录化：
//! - `dict`：单个注解写成字典（通用与子类型专有条目、`/A` `/Dest` `/Popup`
//!   `/FS` 引用解析）及其 `/BS`、`/Dest` 私有取值辅助；
//! - `geometry_gates`：Square/Circle/Polygon/PolyLine 专有条目与 `/Measure`、
//!   `/ExData` 3D 标记的版本门禁及特性集合；
//! - `link_gates`：Link 专有条目与旧式 `/Border` 虚线项的版本门禁及特性集合；
//! - `text_gates`：Line/FreeText/TrapNet 专有条目的版本门禁。
//!
//! 跨文件共享项 `AnnotationRefs`（按句柄解析的对象号映射）与其
//! `action_refs` 保留在 `mod.rs`，其余对外可见项由本文件门面重导出。

use crate::core::builder::types::{ActionRefs, ThreeDArtwork};
use std::cell::Cell;

mod dict;
mod geometry_gates;
mod link_gates;
mod text_gates;

pub(super) use dict::annotation_dict;
pub(super) use geometry_gates::{
    geometry_feature_gate, geometry_features, markup_3d_feature_gate, measure_feature_gate,
};
pub(super) use link_gates::{border_feature_gate, link_feature_gate, link_features};
pub(super) use text_gates::{free_text_feature_gate, line_feature_gate, trap_network_feature_gate};

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
