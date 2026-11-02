//! CIDFont 描述符、程序嵌入与内容流写出测试。
//!
//! 按主题目录化拆分：
//! - `cff_subset`：CID 子集请求的 CFF/FontFile3 重写、CIDSet 裁剪与子集校验错误路径；
//! - `descriptor`：FontDescriptor 写出、Table 122 必需条目兜底与 `/Style /Lang /FD` 覆盖；
//! - `vertical_metrics`：`/DW2` `/W2` 纵向度量的写出、默认省略与非法取值拒绝；
//! - `program_embedding`：TrueType/CFF 程序嵌入、`Length1` 声明与 OpenType 轮廓门禁；
//! - `encoding`：Identity-H 双字节字符码、`/ToUnicode` 省略与 CIDToGIDMap 门禁；
//! - `truetype_subset`：CIDFontType2 子集化的 FontFile2 重写与 CIDToGIDMap 重排。
//!
//! 原 `cid.rs` 中的夹具 helper 均只服务单一主题，随主题迁入对应子文件，
//! 本文件仅保留子模块声明。

mod cff_subset;
mod descriptor;
mod encoding;
mod program_embedding;
mod truetype_subset;
mod vertical_metrics;
