//! 附录 F.4 hint 表写端编码的**表族**模块。
//!
//! 目前落地 F.4 六家里的五家：F.4.2 共享对象 hint 表（[`shared`]）、F.4.4
//! generic hint 表（[`generic`]）、F.4.1 页偏移 hint 表（[`page_offset`]）、
//! F.4.5 extended generic hint 表（[`extended`]）与 F.4.3 缩略图 hint 表
//! （[`thumbnail`]）。五家合起来把写端往返闭环的形态都覆盖到：**声明宽度**
//! （`shared` 的 Table F.5 头部自描述全部列宽）、**定宽**（`generic` 是六家里
//! 唯一「长度与声明无关」的表，Table F.9 四项 × 32 位＝16 字节整），**逐页交错列
//! ＋ 增量**（`page_offset` 的 Table F.3 头部 ＋ Table F.4 七列，跨页累加属读端
//! 派生），以及**变长表补零到字节边界**（`thumbnail` 与 `extended` 的列宽与列长
//! 都装在本表自己的头部里，故表长完全由本表算出）。`extended` 是 `generic` 的
//! **超集**（Table F.10 项 1—4 与 Table F.9 逐字同形，故编码产物的前 16 字节与
//! [`encode_generic_table`] 逐字节相等），另加一项组内位宽声明与一个**变长**标识符
//! 列——与 `page_offset` 的列长取字典 `/N` 不同，本表的列长与列宽**都装在本表自己
//! 的头部**（项 5 × 项 6）。`thumbnail` 与 `page_offset` 同为「头部 + 交错增量列」
//! 形态（Table F.7 项 4/8/6 给出 Table F.8 三列的宽度、项 3 给出列长），差别在
//! 列长**不**来自字典、而来自本表头部，故 `pages_with_thumbnails` 与条目向量的
//! 自洽校验由本目录自己承担。
//!
//! 剩下唯一一家（内嵌文件流 F.4.6）的编码器留待后续切片复用本目录的
//! [`HintBitWriter`](super::bits::HintBitWriter) 与「校验先于任何位」骨架补齐；
//! F.4.6 的 Table F.12 逐组交错列可直接照 [`page_offset`] 的列序写法，其头部项
//! 5 的对象号宽度是**文档级**硬依赖（取本页偏移表 Table F.3 项 11）。

pub mod extended;
pub mod generic;
pub mod page_offset;
pub mod shared;
pub mod thumbnail;

pub use extended::{ExtendedHintEncodeError, ExtendedHintTableInput, encode_extended_table};
pub use generic::encode_generic_table;
pub use page_offset::{PageOffsetTableInput, encode_page_offset_table};
pub use shared::encode_shared_object_table;
pub use thumbnail::{ThumbnailEncodeError, ThumbnailTableInput, encode_thumbnail_table};
