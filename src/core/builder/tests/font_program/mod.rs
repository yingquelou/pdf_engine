//! 字体程序嵌入与 TrueType / Type 1 子集化测试。
//!
//! 按主题目录化拆分（与 `tests/cid/`、`tests/graphics_state/`、
//! `tests/structure_mcr_stream/` 的目录化形态一致）：
//! - `program_embedding`：Type1/TrueType/CFF 程序嵌入写出与 Table 127 的
//!   `Length1`/`Length2`/`Length3` 条目契约；
//! - `subset_tag`：`/BaseFont` 六字母子集前缀、标签确定性与字名/子集判别
//!   依赖，以及无程序时不加前缀；
//! - `truetype_subset`：`subset_glyphs` 的 FontFile2 重写、cmap 重编号、
//!   程序类型/越界 gid 拒绝与非去重契约；
//! - `type1_subset`：`subset_type1_glyphs` 的 FontFile 重写、CharString 保持、
//!   缺失/非 Type 1/未知字形名拒绝、与 `subset_glyphs` 互斥及非去重；
//! - `opentype_program`：OpenType 双风味的 FontFile2/FontFile3 写出、非法
//!   SFNT 版本标签与子集请求拒绝。
//!
//! 本文件的夹具与辅助（`render`、`sample_type1_program`、
//! `sample_truetype_program`、`sample_opentype_sfnt`、`find_font_object`、
//! `embedded_program`、`embedded_program_stream`）全部由 `tests/common.rs`
//! 定义并经 `tests/mod.rs` 再绑定，拆分未新增任何条目、未放宽任何可见性；
//! 与 `tests/cid/` 一样，本文件不含条目，子文件经 `use super::super::*;`
//! 取用 `tests` 作用域。

mod opentype_program;
mod program_embedding;
mod subset_tag;
mod truetype_subset;
mod type1_subset;
