//! 逻辑态 → `Document`：对象号分配、资源字典与字体/图片对象组装。
//!
//! 主流程保留在本模块；具体对象构造按类别拆分为子模块：
//! - `fonts`：普通字体字典与字体程序嵌入（含 TrueType / Type 1 子集）；
//! - `mmtype1`：Multiple Master（MMType1）实例字典的写端门禁与 `/BaseFont`
//!   命名规则（9.6.2.3）；
//! - `type3`：Type 3 字体字典与字符过程流；
//! - `cid`：CIDFont 后代字典、Type 0 父字体与相关流；
//! - `cmap`：嵌入编码 CMap 与 ToUnicode CMap；
//! - `annotation`：注解字典（`/Annots` 间接对象与目标引用解析）；
//! - `annotation_exdata`：`/ExData` 3D 标记（13.6.6 Table 313）的 `/3DA` 跨注解校验；
//! - `annotation_markup`：标记类注解 Table 170 附加条目（`/Subj` `/IRT` `/RT`）的写出、`/IRT` 跨注解同页校验与版本门禁；
//! - `annotation_popup`：`/Popup` 与 `/Parent` 的单侧声明补出与配对冲突校验；
//! - `embedded_file`：嵌入文件与文件规格（`/EmbeddedFiles` 名称树）；
//! - `form_field`：AcroForm 字段树对象分配与 Widget 字典写出；
//! - `acro_form`：AcroForm `/DR` 默认资源字典（字体/ExtGState/ColorSpace/ProcSet）；
//! - `sound`：声音对象流（13.3 Table 294）的分配、写出与注解 `/Sound` 引用；
//! - `three_d`：3D artwork 流（13.6 Table 300）、引用字典（Table 303）与 `/OnInstantiate` 流的分配、写出与注解 `/3DD` 等条目；
//! - `rendition`：media clip section 链（13.2.6 Table 277）的对象分配与写出；
//! - `marked_properties`：标记内容属性表（14.6）注册句柄的对象分配、页面内容
//!   占位替换与字典写出；
//! - `separation`：页面 `/SeparationInfo` 分色字典（14.11.4 Table 364）的校验与写出；
//! - `opi`：图像与表单 XObject 的 `/OPI` 代理字典（14.11.7 Table 368–370）校验与写出；
//! - `output_intent`：目录 `/OutputIntents` 输出意图（14.11.5 Table 365）的校验、配置文件流分配与写出。
//!
//! 主流程之外的装配阶段：
//! - `preflight`：写前门禁（版本与取值校验）与内容流扫描产物；
//! - `plan`：各阶段的中间结果（门禁产物与对象号分配计划）；
//! - `alloc_pools` / `alloc_forms` / `alloc_groups` / `alloc_annotations`：按主题分阶段的对象号分配；
//! - `feature_profile`：特征画像登记；
//! - `catalog_tree`：目录与 Pages 根节点写出；
//! - `page_tree`：每页 Page 字典与内容流写出；
//! - `indirect_objects`：注解/字段/嵌入文件/声音/3D/rendition 间接对象写出与对象号回填；
//! - `resource_objects`：字体、CIDFont、图片、可选内容组、渐变与图案对象写出；
//! - `form_stream`：Form XObject 流写出；
//! - `graphics_state`：函数条目与 ExtGState 对象写出；
//! - `images` / `colorspace` / `utils`：图像流、颜色空间值与共享小工具。

mod acro_form;
mod action;
mod alloc_annotations;
mod alloc_forms;
mod alloc_groups;
mod alloc_pools;
mod annotation;
mod annotation_exdata;
mod annotation_markup;
mod annotation_mk;
mod annotation_popup;
mod catalog_tree;
mod cid;
mod cmap;
mod colorspace;
mod embedded_file;
mod encryption;
mod feature_profile;
mod fonts;
mod form_field;
mod form_stream;
mod graphics_state;
mod images;
mod indirect_objects;
mod marked_properties;
mod mmtype1;
mod opi;
mod output_intent;
mod page_tree;
mod plan;
mod preflight;
mod rendition;
mod resource_objects;
mod separation;
mod shading;
mod sound;
mod three_d;
mod type3;
mod utils;

use super::PdfBuilder;
use crate::core::types::{Dict, Document, Object, Trailer, Value};
use crate::core::writer::WriteError;

pub(crate) use self::colorspace::color_space_value;
pub(crate) use self::utils::{hex_bytes, real_array, write_function};

// 图像类型经由本模块再导出，保持子模块既有的 `super::` 引用路径。
pub(crate) use crate::core::builder::{ImageData, ImageMask};

impl PdfBuilder {
    /// 把逻辑态翻译为底层 `Document`。
    ///
    /// CIDFont 的嵌入程序类型必须与子类型匹配，否则返回
    /// [`WriteError::InvalidObject`]。
    pub(super) fn assemble_document(&mut self) -> Result<Document, WriteError> {
        // —— 对象号分配 ——
        // Catalog = 1, Pages = 2 固定；其后按页、字体、图片依次递增。
        let catalog_num: i64 = 1;
        let pages_num: i64 = 2;
        let mut num = 3i64;

        // 写前门禁与对象号分配（顺序与拆分前完全一致）。
        let preflight = self.assemble_preflight()?;
        let mut pools = self.allocate_pool_objects(&mut num)?;
        // 图像对象号表回填：结构树 OBJR 的图像 XObject 目标（14.7.4.3）以
        // 「页索引 + 该页图像资源序号」定位，导航阶段据此解析 `/Obj`。
        self.image_nums = pools.page_image_nums.clone();
        let forms = self.allocate_form_objects(&preflight, &mut pools, &mut num)?;
        let groups = self.allocate_group_objects(&forms, &mut num)?;
        let annot_plan = self.allocate_annotation_objects(&preflight, &mut num)?;
        // 标记内容属性表（14.6）：注册句柄取号 + 把页面内容里的未定型占位
        // 替换成 `<号> 0 R`，必须早于 `assemble_page_tree` 序列化内容字节。
        let marked_properties = self.allocate_marked_property_objects(&mut num)?;

        // —— 构造 Document 并按主题写出各对象 ——
        let mut doc = self.assemble_feature_profile(&preflight, &groups);
        self.assemble_catalog_tree(
            &mut doc,
            &mut num,
            &preflight,
            &mut pools,
            &groups,
            &annot_plan,
            catalog_num,
            pages_num,
        )?;
        self.assemble_page_tree(
            &mut doc,
            &mut num,
            &preflight,
            &pools,
            &forms,
            &groups,
            &annot_plan,
            pages_num,
        )?;
        self.assemble_indirect_objects(&mut doc, &pools, &forms, &groups, &annot_plan)?;
        // 属性表字典作为间接对象落进对象表，与页面内容里回填的引用闭合。
        self.write_marked_property_objects(&mut doc, &marked_properties);
        self.assemble_font_and_image_objects(&mut doc, &mut num, &pools, &groups)?;
        self.assemble_form_streams(&mut doc, &mut num, &forms, &groups)?;
        self.assemble_document_group_objects(&mut doc, &mut num, &groups)?;
        self.assemble_graphics_objects(&mut doc, &mut num, &pools, &forms)?;

        // trailer：/Root -> Catalog(1)。/Size 由 writer 按**实际写出**的
        // 最大对象号 + 1 计算（经典表用 `ctx.max_num + 1`，XRef 流用自身
        // 分配到的对象号），此处显式给出以保证与对象号一致。
        // /Info（14.3.3）：非空时分配独立对象并以间接引用挂接。
        let mut trailer_dict = Dict::new().insert("Root", Value::Ref(catalog_num, 0));
        if !self.document_info.is_empty() {
            let info_num = num;
            num += 1;
            doc.set_object(Object::new(
                info_num,
                0,
                Value::Dict(self.document_info.to_dict(self.version)?),
            ));
            trailer_dict = trailer_dict.insert("Info", Value::Ref(info_num, 0));
        }
        // /ID（14.4）：显式设置时写 trailer 数组；不调用则保持缺省。
        if let Some(id) = &self.file_identifier {
            trailer_dict = trailer_dict.insert("ID", id.to_value());
        }
        let max_num = num - 1;
        doc.trailer = Trailer {
            dict: trailer_dict.insert("Size", Value::Integer(max_num + 1)),
        };
        Ok(doc)
    }
}
