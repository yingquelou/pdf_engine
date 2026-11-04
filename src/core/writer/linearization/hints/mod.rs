//! 附录 F.4 hint 表写端编码的**表族**模块。
//!
//! 目前落地 F.4.2 共享对象 hint 表（[`shared`]）与 F.4.4 generic hint 表
//! （[`generic`]）两家——前者是 F.4 六个家族里头部项最少、语义最自足的（宽度
//! 全在 Table F.5 头部自描述，不依赖文档级页偏移表），后者是六家里唯一「长度
//! 与声明无关」的定宽表（Table F.9 四项 × 32 位），两家合起来把写端往返闭环的
//! 「声明宽度」与「定宽」两种形态都覆盖到。其余家族（页偏移 F.4.1 / 缩略图
//! F.4.3 / extended F.4.5 / 内嵌文件流 F.4.6）的编码器留待后续切片复用本目录的
//! [`HintBitWriter`](super::bits::HintBitWriter) 与校验骨架补齐。

pub mod generic;
pub mod page_offset;
pub mod shared;

pub use generic::encode_generic_table;
pub use page_offset::{PageOffsetTableInput, encode_page_offset_table};
pub use shared::encode_shared_object_table;
