//! hint 表读端单元测试：位原语、页偏移 hint 表（F.4.1）、共享对象 hint 表
//! （F.4.2）、缩略图 hint 表（F.4.3）、generic hint 表（F.4.4）、extended
//! generic hint 表（F.4.5）与内嵌文件流 hint 表（F.4.6）。
//!
//! 夹具驱动的文档级用例在 [`crate::core::reader::linearization::tests::hints`]。

mod bits;
mod embedded_file;
mod extended;
mod generic;
mod page_offset;
mod page_offset_derive;
mod shared;
mod thumbnail;
