//! builder 单元测试：对象布局、字体编码写出与内容操作符生成。
//!
//! 按主题拆分：
//! - `font`：简单字体编码与描述符；
//! - `widths`：简单字体 `/Widths` 与描述符必需项兜底；
//! - `font_program`：字体程序嵌入与 TrueType/Type 1 子集化；
//! - `font_cff`：name-keyed CFF 子集化写入；
//! - `type3`：Type 3 字体；
//! - `cid`：CIDFont 描述符与内容流；
//! - `to_unicode`：ToUnicode CMap；
//! - `encoding`：自定义 CID 编码 CMap；
//! - `annotation`：注解字典与 `/Annots` 通用条目与子类型专有条目；
//! - `annotation_border`：旧式 `/Border` 数组（Table 164）与通用 `/BS` 门禁；
//! - `annotation_line`：Line 注解专有条目（Table 175/176）与版本门禁；
//! - `annotation_free_text`：FreeText 注解专有条目（Table 174）与版本门禁；
//! - `annotation_geometry`：Square/Circle/Polygon/PolyLine 几何条目（Table 177/178）与版本门禁；
//! - `annotation_link`：Link 注解 `/H` `/PA` 条目（Table 173）与版本门禁；
//! - `annotation_subtype_table`：写端子类型集与读端 Table 169 的一致性与子类型版本门禁；
//! - `annotation_popup`：`/Popup` 与 `/Parent` 的单侧声明补出、配对冲突与必填校验（12.5.2）；
//! - `annotation_redact`：Redact 注解条目（Table 192）的取值契约与 PDF 1.7 门禁；
//! - `annotation_printer_mark`：Printer's mark 注解 `/F` `/MN` 与表单条目（Table 362/363）的写出、校验与 PDF 1.4 门禁；
//! - `annotation_trap_net`：Trap 网络注解跟踪条目（Table 366）与表单条目（Table 367）的写出、校验、每页唯一与 PDF 1.3/1.4 门禁；
//! - `annotation_exdata`：`/ExData` 3D 标记（13.6.6 Table 313）的 `/3DA` `/3DV` `/MD5` 写出、跨注解目标校验与 PDF 1.7 门禁；
//! - `annotation_markup`：标记类注解 Table 170 附加条目（`/Subj` `/IRT` `/RT`）的写出、`/IRT` 同页与分组校验、归属/依赖校验与 PDF 1.5/1.6 门禁；
//! - `annotation_mk`：外观特性字典 `/MK`（Table 189）与图标适配 `/IF`（Table 247）的取值契约、Screen 归属与 PDF 1.5 门禁；
//! - `hide_action`：Hide 动作 `/T` 目标解析与 `/H`（Table 210）的写出、校验与 PDF 1.2 门禁；
//! - `sound`：声音对象（13.3 Table 294）、Sound 动作（Table 208）与 Sound 注解（Table 185）的流对象分配、引用解析与 PDF 1.2 门禁；
//! - `action_ocg`：Set-OCG-State 动作 `/State`（Table 213）的组对象分配、写出与 PDF 1.5 门禁；
//! - `action_transition`：Transition 动作 `/Trans`（Table 215）的过渡字典写出、条目校验与 PDF 1.5 门禁；
//! - `action_thread`：Thread 动作 `/D` `/B` 索引（Table 205）的写出、越界校验与 PDF 1.1 门禁；
//! - `rendition`：多媒体 rendition 与 media clip（13.2 Table 266–278）、Rendition 动作（Table 214）的写出、Screen 目标校验与 PDF 1.5 门禁；
//! - `page_presentation`：页面 `/Dur` 与 `/Trans`（12.4.4 Table 30）的写出、PDF 1.1 门禁与 1.5 样式条目门禁；
//! - `page_box`：页面边界（14.11.2 Table 30）与框颜色信息（Table 360/361）的写出、PDF 1.3/1.4 门禁与取值契约；
//! - `separation`：分色字典（14.11.4 Table 364）的组共享 `/Pages`、色料与颜色空间写出、组契约与 PDF 1.3 门禁；
//! - `opi`：图像与表单 XObject 的 `/OPI` 代理字典（14.11.7 Table 368–370）写出、PDF 1.2 门禁与取值契约；
//! - `thumbnail`：页面缩略图 `/Thumb`（12.3.4 Table 30）的图像引用、去重与颜色空间契约；
//! - `articles`：文章线（12.4.3 Table 160/161）的线程/bead 对象、`/B` 与 PDF 1.1 门禁；
//! - `collection`：便携集合（12.3.5 Table 155–158）的模式、`/D` `/View` `/Sort` 与 PDF 1.7 门禁；
//! - `templates`：命名模板页（12.7.6 Table 31）的 `/Names /Templates` 名称树、`/Type /Template` 对象与 PDF 1.3 门禁；
//! - `alternate_presentation`：备用演示（13.5 Table 297）的幻灯片对象、`/Names /AlternatePresentations` 名称树、嵌入文件引用与 PDF 1.4 门禁；
//! - `structure`：逻辑结构树（14.7 Table 321–326）的目录条目、父树/`/IDTree`、版本门禁与输入校验；
//! - `structure_content`：结构标记内容通道（14.8.2，`/MCID` 的 BDC 属性表）的写出与页面/结构树构建期交叉校验、自动编号 MCID（14.7.4.4 NOTE）与 `/ParentTreeNextKey` 键策略；
//! - `structure_objr`：结构元素 OBJR 内容项（14.7.4.3–14.7.4.4 Table 325）的 `/K` 对象引用字典、父树单元条目与目标 `/StructParent`（Table 326）写出，覆盖注解、表单 XObject 与图像 XObject 三类目标的 `/Pg` 解析与句柄校验、`/StructParent`↔`/StructParents` 互斥与零字节契约；
//! - `structure_mcr_stream`：流形态 MCR（14.7.4.2 Table 324 `/Stm` `/StmOwn`）的 `/K` 写出、容器 `/StructParents` 父树键（14.7.4.4）、`/Pg` 三路回落、`/StmOwn`↔`/AP` 一致性核对、与 OBJR 声称的互斥（Table 326）与零字节契约；
//! - `structure_inline_mcid`：14.7.4.2 整数 MCID 简写与结构元素 `/Pg`（Table 323）的 `/K` 裸整数写出、页一致性零字节拒绝与父树数组归属；
//! - `structure_standard`：标准属性 owners 与 List/PrintField/Table 条目（14.8.5.2、14.8.5.5–14.8.5.7）的往返、适用性与版本门禁；
//! - `structure_types`：PDF 1.5 扩展标准结构类型（14.8.4）的直接/`/RoleMap` 到达方式与版本门禁；
//! - `structure_layout`：Layout 标准属性（14.8.5.4 Table 342–346）的往返、缺省省略、形态/适用性校验与 1.4/1.5/1.6 门禁；
//! - `accessibility`：文档默认语言（目录 `/Lang`）与结构元素 `/Lang` 的写出、RFC 3066 校验与 PDF 1.4 门禁；
//! - `span`：Span 属性表（`/Alt` `/ActualText` `/E` `/Lang`，14.9.3–14.9.5）的编码、BDC/EMC 写出与 PDF 1.4/1.5 门禁；
//! - `artifact`：Artifact 属性表（`/Type` `/BBox` `/Attached` `/Subtype`，14.8.2.2.2 Table 330）的编码、校验与 PDF 1.4/1.7 门禁；
//! - `tag_suspect`：tag suspects（标签为 `TagSuspect` 的标记内容，14.8.2.3.1）的 `/TagSuspect /Ordering` 属性表写出、`/MarkInfo /Suspects` 强制要求与 PDF 1.6 门禁；
//! - `reversed_chars`：反序显示串（标签为 `ReversedChars` 的标记内容，14.8.2.3.3）的属性表写出、逐串反转、空串/内部空格/非可打印内容/未闭合区段的零字节拒绝与 PDF 1.4 门禁；
//! - `three_d`：3D artwork 流（13.6 Table 300）与 3D 注解（Table 298）的流对象分配、视图选择、激活条目与 PDF 1.6 门禁；
//! - `three_d_view_extensions`：3D 视图 PDF 1.7 条目（`/RM` `/LS` `/SA` `/NA` `/NR`，13.6.4.1 Table 304）的版本门禁与 `/NA` → `/NR` 依赖；
//! - `three_d_payloads`：3D 视图负载字典（渲染模式/光照方案/截面/节点，Table 307–312）的类型化序列化、缺省省略与嵌套校验；
//! - `three_d_resources`：3D 流 `/Resources` 名称树（13.6.3.1 Table 300）的文本串键、排序写出与直接对象校验；
//! - `action_goto3d`：Go-To-3D-View 动作 `/TA` `/V`（12.6.4.15 Table 216）的写出、目标注解/`/VA` 相对校验与 PDF 1.6 门禁；
//! - `support`：注解字典取用的共享夹具；
//! - `form_action`：表单数据动作（SubmitForm/ResetForm/ImportData，12.7.5）与版本门禁；
//! - `form_appearance`：Widget `/AP /N` 状态子字典与 `/AS` 校验；
//! - `form_merged`：同全限定名字段多重表示的 `/V`、`/DV` 一致性（12.7.3.2）；
//! - `acro_form`：AcroForm 字段树、Widget 与 `/AP` 外观流；
//! - `acro_form_resources`：AcroForm `/DR` 非字体默认资源（ExtGState/ColorSpace/ProcSet）；
//! - `embedded_file`：嵌入文件流、文件规格与 `/EmbeddedFiles` 名称树；
//! - `extensions`：扩展字典（7.12、Table 50）的目录 `/Extensions` 直接对象写出、前缀/级别校验与 PDF 1.7 门禁；
//! - `form`：Form XObject 字典与资源；
//! - `page_group`：页字典组属性（`/Group`）；
//! - `optional_content`：可选内容组（`/OCG`）与目录 `/OCProperties`；
//! - `function`：PDF 函数（Type 0/2/3/4）与 ExtGState `/TR`；
//! - `shading`：渐变（Shading Type 1–5）与着色图案（PatternType 2）；
//! - `pattern`：平铺图案（PatternType 1）；
//! - `color_space`：页级命名颜色空间（`/ColorSpace` 资源与 `CS`/`cs`）；
//! - `image_xobject`：图像 XObject 字典条目（Decode/Interpolate）、软蒙版 `/SMask`、图像蒙版与显式/色彩键 `/Mask` 写出；
//! - `image_error`：图片数据长度、位深、Decode、SMask 与 Mask 非法参数的 `WriteError::InvalidObject` 零字节拒绝契约；
//! - `image_color_space`：图像内联颜色空间（CalGray/CalRGB/Lab/ICCBased/Indexed/Separation/DeviceN）的写出与非法参数拒绝；
//! - `object_layout`：页面树对象号分配：`/Count`、`/Kids` 间接引用与自定义 `/MediaBox`；
//! - `render`：最小文本内容流操作符骨架（BT/Tf/Td/Tj/ET）与字面串转义；
//! - `version`：头部版本覆写与图像 SMask、`/Mask` 的 PDF 1.3/1.4 门禁；
//! - `common`：各主题共享的辅助与夹具（整档渲染、字体程序夹具、数值断言、图片拒绝），由本文件 `use` 重新绑定到共享作用域；
//! - 本文件：子模块声明与跨主题共享作用域（`common` 与核心类型的再导出）。

mod accessibility;
mod acro_form;
mod acro_form_resources;
mod action_goto3d;
mod action_gotoe;
mod action_ocg;
mod action_thread;
mod action_transition;
mod alternate_presentation;
mod annotation;
mod annotation_appearance;
mod annotation_border;
mod annotation_contract;
mod annotation_exdata;
mod annotation_free_text;
mod annotation_geometry;
mod annotation_line;
mod annotation_link;
mod annotation_markup;
mod annotation_measure;
mod annotation_mk;
mod annotation_popup;
mod annotation_printer_mark;
mod annotation_redact;
mod annotation_subtype_table;
mod annotation_text_markup;
mod annotation_trap_net;
mod annotation_watermark;
mod articles;
mod artifact;
mod choice_indices;
mod cid;
mod cid_cmap_multibyte;
mod collection;
mod collection_item;
mod color_space;
mod common;
mod compression;
mod embedded_file;
mod encoding;
mod extensions;
mod font;
mod font_cff;
mod font_program;
mod form;
mod form_action;
mod form_appearance;
mod form_merged;
mod form_option;
mod form_resources;
mod function;
mod graphics_state;
mod hide_action;
mod image_color_space;
mod image_entries;
mod image_error;
mod image_xobject;
mod metadata;
mod movie;
mod navigation;
mod navigation_legacy_dests;
mod object_layout;
mod opi;
mod optional_content;
mod output_intent;
mod page_box;
mod page_group;
mod page_presentation;
mod pattern;
mod render;
mod rendition;
mod reversed_chars;
mod separation;
mod shading;
mod sound;
mod span;
mod structure;
mod structure_content;
mod structure_inline_mcid;
mod structure_layout;
mod structure_mcr_stream;
mod structure_objr;
mod structure_standard;
mod structure_types;
mod support;
mod tag_suspect;
mod templates;
mod three_d;
mod three_d_payloads;
mod three_d_resources;
mod three_d_view_extensions;
mod thumbnail;
mod to_unicode;
mod type3;
mod version;
mod viewer_preferences;
mod viewport;
mod widths;

use super::*;
use crate::core::types::{Dict, Document, StreamData, Value};
use common::{
    assert_image_rejected, embedded_program, embedded_program_stream, find_font_object,
    font_descriptor, numbers, render, sample_opentype_sfnt, sample_truetype_program,
    sample_type1_program, scalar,
};
