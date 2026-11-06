//! 附录 F.4 hint 表写端编码的**表族**模块。
//!
//! F.4 六家已全部落地：F.4.2 共享对象 hint 表（[`shared`]）、F.4.4 generic hint
//! 表（[`generic`]）、F.4.1 页偏移 hint 表（[`page_offset`]）、F.4.5 extended
//! generic hint 表（[`extended`]）、F.4.3 缩略图 hint 表（[`thumbnail`]）与 F.4.6
//! 内嵌文件流 hint 表（[`embedded_file`]）。六家合起来把写端往返闭环的形态都覆盖
//! 到：**声明宽度**（`shared` 的 Table F.5 头部自描述全部列宽）、**定宽**
//! （`generic` 是六家里唯一「长度与声明无关」的表，Table F.9 四项 × 32 位＝16
//! 字节整），**逐页交错列 ＋ 增量**（`page_offset` 的 Table F.3 头部 ＋ Table F.4
//! 七列，跨页累加属读端派生），以及**变长表补零到字节边界**（`thumbnail` 与
//! `extended` 的列宽与列长都装在本表自己的头部里，故表长完全由本表算出）。
//! `extended` 是 `generic` 的**超集**（Table F.10 项 1—4 与 Table F.9 逐字同形，
//! 故编码产物的前 16 字节与 [`encode_generic_table`] 逐字节相等），另加一项组内
//! 位宽声明与一个**变长**标识符列——与 `page_offset` 的列长取字典 `/N` 不同，本表
//! 的列长与列宽**都装在本表自己的头部**（项 5 × 项 6）。`thumbnail` 与
//! `page_offset` 同为「头部 + 交错增量列」形态（Table F.7 项 4/8/6 给出 Table F.8
//! 三列的宽度、项 3 给出列长），差别在列长**不**来自字典、而来自本表头部，故
//! `pages_with_thumbnails` 与条目向量的自洽校验由本目录自己承担。
//!
//! [`embedded_file`] 收口最后一种形态：**逐组重复的五列（项主序/转置）＋
//! 文档级列宽**。Table F.12 的五项按 35797—35799 的 shall 逐列铺开（同
//! [`page_offset`] 与 [`thumbnail`] 的交错序），但其中第五列的宽度**不在本表头部**
//! ——它是 Table F.3 项 11 的文档级声明值（35847 的「See Table F.3, item 11」），
//! 故模型的 [`shared_object_id_bits`](embedded_file::EmbeddedFileTableInput::shared_object_id_bits)
//! 与读端 `parse` 的第三形参一样是**显式入参**：本目录由此确立「跨表硬依赖由调用方
//! 注入、写端不推导也不冒充」的口径（`extended` 用组内项 6 自描述，无此依赖）。

pub mod embedded_file;
pub mod extended;
pub mod generic;
pub mod page_offset;
pub mod shared;
pub mod thumbnail;

pub use embedded_file::{
    EmbeddedFileEncodeError, EmbeddedFileTableInput, encode_embedded_file_table,
};
pub use extended::{ExtendedHintEncodeError, ExtendedHintTableInput, encode_extended_table};
pub use generic::encode_generic_table;
pub use page_offset::{PageOffsetTableInput, encode_page_offset_table};
pub use shared::encode_shared_object_table;
pub use thumbnail::{ThumbnailEncodeError, ThumbnailTableInput, encode_thumbnail_table};
