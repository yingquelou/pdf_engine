//! `assemble` 各阶段的中间结果：写前门禁产物与对象号分配计划。

use std::collections::BTreeSet;

use crate::core::builder::types::AcroForm;
use crate::core::builder::{CidFont, Font, GraphicsState, ImageData};
use crate::core::types::PdfFeature;

use super::embedded_file::EmbeddedSlot;
use super::form_field::FieldSlot;
use super::images::ImageObjectNums;
use super::three_d::ThreeDObjectSlots;

/// 写前门禁（任何对象号分配与字节写出之前）的产物。
///
/// 各字段是被门禁确认过的输入快照：内容流扫描得到的特性标记、克隆出的
/// AcroForm 配置、分色页映射与动作特性需求，供后续对象号分配与写出阶段
/// 复用，避免再次借用 `self`。
pub(super) struct Preflight {
    /// 克隆出的 AcroForm 配置；字段树对象号以其为借用源。
    pub(super) acro_form: Option<AcroForm>,
    /// 表单数据动作（12.7.5.2–12.7.5.4）的版本特性需求。
    pub(super) action_features: Vec<PdfFeature>,
    /// 分色页组映射（`页号 → 组下标`），供页写出挂 `/SeparationInfo`。
    pub(super) separation_page_map: Vec<Option<usize>>,
    /// 3D 视图是否带 PDF 1.7 专有条目（13.6.4.1 Table 304）。
    pub(super) three_d_view_extensions: bool,
    /// Span 属性表条目（14.9.2.1、14.9.3–14.9.5）扫描结果。
    pub(super) span_property_lists: bool,
    /// Span `/Alt` 与 `/ActualText` 扫描结果。
    pub(super) span_alternate_text: bool,
    /// Artifact 属性表条目（14.8.2.2.2 Table 330）扫描结果。
    pub(super) artifact_property_lists: bool,
    /// Artifact `/Type /Background` 扫描结果。
    pub(super) artifact_background: bool,
    /// Artifact `/Subtype` 扫描结果。
    pub(super) artifact_subtype: bool,
    /// Tag suspects（14.8.2.3.1）扫描结果。
    pub(super) tag_suspects: bool,
    /// 反序显示串（14.8.2.3.3）扫描结果。
    pub(super) reversed_characters: bool,
}

/// 页面与文档级资源去重池的对象号分配结果。
pub(super) struct PoolPlan {
    /// 每页的 (Page 字典号, 内容流号)。
    pub(super) page_slots: Vec<(i64, i64)>,
    /// 字体去重池（含表单自有资源并入的字体）。
    pub(super) font_pool: Vec<(Font, i64)>,
    /// 每页字体句柄 → 全局字体对象号。
    pub(super) page_font_nums: Vec<Vec<i64>>,
    /// 图片去重池及其从属对象号。
    pub(super) image_pool: Vec<(ImageData, ImageObjectNums)>,
    /// 每页图片句柄 → 基图对象号。
    pub(super) page_image_nums: Vec<Vec<i64>>,
    /// 每页 `/Thumb` 的基图对象号。
    pub(super) page_thumbnail_nums: Vec<Option<i64>>,
    /// CIDFont 去重池（Type 0 父对象号）。
    pub(super) cid_font_pool: Vec<(CidFont, i64)>,
    /// 每页 CIDFont 句柄 → Type 0 父对象号。
    pub(super) page_cid_font_nums: Vec<Vec<i64>>,
    /// ExtGState 去重池。
    pub(super) gs_pool: Vec<(GraphicsState, i64)>,
    /// 每页 ExtGState 句柄 → 对象号。
    pub(super) page_gs_nums: Vec<Vec<i64>>,
}

/// Form XObject 引用集与自有资源的对象号分配结果。
pub(super) struct FormPlan {
    /// 实际被引用（页面绘制、外观、软蒙版、动作等）的表单句柄。
    pub(super) used_forms: BTreeSet<usize>,
    /// 表单句柄 → 流对象号；未引用的句柄保持 0。
    pub(super) form_nums: Vec<i64>,
    /// 各表单自有字体资源 → 全局字体对象号。
    pub(super) form_font_nums: Vec<Vec<i64>>,
    /// 各表单自有图片资源 → 全局基图对象号。
    pub(super) form_image_nums: Vec<Vec<i64>>,
}

/// 可选内容组、渐变与图案的对象号分配结果。
pub(super) struct GroupPlan {
    /// 实际使用的可选内容组句柄。
    pub(super) used_ocgs: BTreeSet<usize>,
    /// 组句柄 → OCG 对象号。
    pub(super) ocg_nums: Vec<i64>,
    /// 实际使用的渐变句柄。
    pub(super) used_shadings: BTreeSet<usize>,
    /// 渐变句柄 → 对象号。
    pub(super) shading_nums: Vec<i64>,
    /// 实际使用的图案句柄。
    pub(super) used_patterns: BTreeSet<usize>,
    /// 图案句柄 → 对象号。
    pub(super) pattern_nums: Vec<i64>,
}

/// 注解、字段树与各文档级对象池的对象号分配结果。
///
/// 借用生命周期 `'a` 指向 [`Preflight`] 克隆出的 AcroForm 配置：字段树
/// 节点按前序分配对象号，其 `/T` 链引用仍指向配置里的字段。
pub(super) struct AnnotPlan<'a> {
    /// 注解句柄 → 间接对象号。
    pub(super) annotation_nums: Vec<i64>,
    /// 注解句柄 → 绑定页索引。
    pub(super) annotation_pages: Vec<usize>,
    /// 每页的注解句柄列表（注册顺序）。
    pub(super) page_annotation_handles: Vec<Vec<usize>>,
    /// 每页并入 `/Annots` 的 Widget 对象号。
    pub(super) page_widget_nums: Vec<Vec<i64>>,
    /// AcroForm 字段树扁平节点表（前序）。
    pub(super) field_slots: Vec<FieldSlot<'a>>,
    /// 顶层字段在 `field_slots` 中的下标。
    pub(super) field_roots: Vec<usize>,
    /// 嵌入文件对象号槽位。
    pub(super) embedded_slots: Vec<EmbeddedSlot>,
    /// 嵌入文件句柄 → 文件规格对象号。
    pub(super) embedded_nums: Vec<i64>,
    /// 声音句柄 → 声音流对象号。
    pub(super) sound_nums: Vec<i64>,
    /// 3D artwork 的流 / 引用字典 / `/OnInstantiate` 对象号槽位。
    pub(super) three_d_slots: ThreeDObjectSlots,
}
