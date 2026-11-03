//! hint 表读端单元测试：位原语、页偏移 hint 表（F.4.1）与共享对象 hint 表
//! （F.4.2）。
//!
//! 夹具驱动的文档级用例在 [`crate::core::reader::linearization::tests::hints`]。

mod bits;
mod page_offset;
mod shared;
