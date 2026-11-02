//! JBIG2Decode 过滤器（ISO 32000-1 §7.4.7 / ITU-T T.88）的段语法层解析。
//!
//! 本模块**只遍历 JBIG2 段链**读取元数据，不解码任何像素：
//! - 段头（T.88 §7.2）：段号、段旗标（类型位 `& 0x3F`、4 字节页关联位
//!   `0x40`、延迟非保留位 `0x80`）、被引用段表（短形式 TP=0–6；TP=7 长
//!   形式以本字节起始的 u32 低 29 位为个数，随后 `ceil(count/8)` 字节
//!   保留位图）、段页关联（1 或 4 字节，字段宽度取决于当前段号：
//!   ≤256 为 1 字节、≤65536 为 2 字节、否则 4 字节）、未压缩数据长度；
//! - 页面信息段（T.88 §7.4.8）：宽高、分辨率、页面合成旗标（BPC 字节）
//!   与条带信息（BPIFlags/BPIExtension 组成 u16，bit15 条带编码）；
//! - PDF 规则（§7.4.7）：JBIG2 文件头、end-of-page 段与 end-of-file 段
//!   shall not be used，图像流内段页关联应为 1，全局段（页关联 0）应
//!   另置于 `/JBIG2Globals` 流。遇到违例一律记为类型化一致性发现
//!   [`Jbig2Finding`]（「存在但禁用」），不静默丢弃任何段。
//!
//! **实现边界（刻意的设计约束，与 JPX 同一立场）**：
//! - 不做像素解码：无算术/MR/MMR/Huffman 熵解码、无符号字典重建、无
//!   区域合成，[`super::ImageModel::samples`] 对 JBIG2 图像返回类型化
//!   `Unsupported`；
//! - 除页面信息段外，其余段类型的载荷按原始字节保留，不解释内部语法
//!   （文本区域 NBIL、半调网格等字段均未建模）；
//! - `0xFFFFFFFF` 未指定长度的终止符搜索与 TP=5/6 预留长形式不支持：
//!   前者记录 [`Jbig2Finding::SegmentLengthUnspecified`] 后干净停止；
//! - 多页共享/随机访问文件组织不属于 PDF 的嵌入式组织范围：文件头按
//!   签名与旗标字节识别、跳过并记录发现，不校验组织记录。
//!
//! 截断处理：段头不完整返回类型化 [`Jbig2Error::TruncatedSegmentHeader`]
//! （绝不 panic）；段数据不足记录 [`Jbig2Finding::SegmentDataTruncated`]
//! 并干净停止，已解析的段照常返回。

mod kinds;
mod model;
mod parse;

#[cfg(test)]
mod tests;

pub use kinds::Jbig2SegmentKind;
pub use model::{
    Jbig2Error, Jbig2Finding, Jbig2ImageInfo, Jbig2PageInfo, Jbig2Segment, Jbig2Stream,
};
pub(crate) use parse::is_jbig2_filter;
pub use parse::parse_jbig2_segments;
