//! 附录 F 线性化 PDF 的**写端**：hint 表位编码写出与读端往返一致性。
//!
//! 本模块闭合矩阵「附录 F」行的写端 P0 缺口第一步——此前本库**只能读 hint 表、
//! 完全不能写**。这里交付的是「hint 表位编码写出 + 与既有读端的往返一致性」，
//! **不**产出真实可分页的线性化文件：文件级对象重排、精确字节偏移（Table F.1
//! `/L`/`/H`/`/T`/`/E` 须指向真实位置）、`/H` 区间与 `/E` 预测端点等属剩余缺口
//! （见模块末「剩余缺口」与本切片报告）。
//!
//! 结构：
//! - [`bits`]：与读端 [`BitReader`](crate::core::reader::linearization::hints::bits::BitReader)
//!   对偶的位**写**原语 [`HintBitWriter`]（MSB-first、按位追加、末尾补零到字节
//!   边界）与写端类型化错误 [`HintEncodeError`]；宽度门（无符号 64 位上限、
//!   给定宽度下的值溢出）与读端同口径，保证写出的字节读端一定解得开、且逐项
//!   往返相等；
//! - [`hints`]：各表族的「模型 → 字节」编码器，复用读端已定义的类型化模型
//!   （[`SharedObjectHintTable`](crate::core::reader::linearization::hints::SharedObjectHintTable)
//!   等），落地 F.4.2 共享对象 hint 表与 F.4.4 generic hint 表两家。
//!
//! 公开面（additive，不改任何现有签名）：[`HintTableEncoder`] 是 crate 内可用、
//! 亦可对外调用的编码入口；详见各方法文档。
//!
//! **剩余缺口（后续切片复用本模块）**：
//! 1. 表级编码已能独立产出合法位流，但要写「真实线性化文件」还须把各表按 Table
//!    F.2 键定位拼接进主 hint 流、并对页偏移表（F.4.1）先建索引——本模块只提供
//!    单表编码，装配多表与偏移计算未做；
//! 2. 对象号/位置重排、`/L`（文件总长）、`/H`（主流对象范围）、`/T`（主引用表
//!    定位）、`/E`（首页末尾偏移）的**真实字节**求值，以及两段交叉引用表与两个
//!    trailer 的写出（F.3.1/F.3.4/F.3.11）——本模块完全不碰文件布局；
//! 3. F.4 六家的**表级**编码器已全部落地（F.4.1 页偏移、F.4.2 共享对象、
//!    F.4.3 缩略图、F.4.4 generic、F.4.5 extended 与本轮的 F.4.6 内嵌文件流，
//!    见 [`encode_embedded_file_table`]），剩下的不是「哪张表还写不出」而是门面
//!    与装配：本类型挂了 F.4.1/F.4.2/F.4.4/F.4.6 四家，缩略图 F.4.3 与 extended
//!    F.4.5 还须直调 `hints` 子模块的自由函数；而内嵌文件流表 Table F.12 项 5 的
//!    宽度是**文档级**硬依赖（取本页偏移表 Table F.3 项 11，见
//!    [`encode_page_offset_table`]），其真实位置又须由 Table F.2 键 `B` 给出——
//!    两者都要等缺口 1 的多表装配切片才在同一次调用里算得出（generic F.4.4 与
//!    页偏移 F.4.1 已落地，见 [`encode_generic_table`]）。

pub mod bits;
pub mod hints;

pub use bits::{HintBitWriter, HintEncodeError, MAX_UNSIGNED_BITS};
pub use hints::embedded_file::{
    EmbeddedFileEncodeError, EmbeddedFileTableInput, encode_embedded_file_table,
};
pub use hints::encode_shared_object_table;
pub use hints::generic::encode_generic_table;
pub use hints::page_offset::{PageOffsetTableInput, encode_page_offset_table};

use crate::core::reader::linearization::hints::{GenericHintTable, SharedObjectHintTable};

/// 附录 F.4 hint 表的编码入口（crate 内可用、对外的新增公开类型）。
///
/// 把读端已定义的类型化模型逐位还原成 F.4 前言「高位在前」的字节流，与既有读端
/// 解析器精确对偶——每个 `encode_*` 的产物都可交回同名模型的 `parse` 解出、逐项
/// 相等。方法一律在写出任何位之前完成自洽校验，返回类型化 [`HintEncodeError`]，
/// 绝不 panic、绝不产出半成品字节。
#[derive(Debug, Clone, Copy, Default)]
pub struct HintTableEncoder;

impl HintTableEncoder {
    /// 新建编码器（无状态）。
    pub fn new() -> Self {
        Self
    }

    /// F.4.2 共享对象 hint 表（Table F.5 头部 + Table F.6 组条目）→ 字节。
    ///
    /// 见 [`encode_shared_object_table`] 的编码口径与自洽约束。
    pub fn encode_shared_object_table(
        &self,
        table: &SharedObjectHintTable,
    ) -> Result<Vec<u8>, HintEncodeError> {
        encode_shared_object_table(table)
    }

    /// F.4.4 generic hint 表（Table F.9 四项 × 32 位）→ 16 字节。
    ///
    /// 见 [`encode_generic_table`] 的编码口径；四项任一超出 32 位定宽即类型化拒绝。
    pub fn encode_generic_table(
        &self,
        table: &GenericHintTable,
    ) -> Result<Vec<u8>, HintEncodeError> {
        encode_generic_table(table)
    }

    /// F.4.1 页偏移 hint 表（Table F.3 头部 + Table F.4 交错列）→ 位流字节。
    ///
    /// 见 [`encode_page_offset_table`] 的编码口径与自洽约束；声明宽度越界、
    /// 首页 shall 违规、计数与向量不符、数值溢出一律类型化拒绝且零字节产出。
    pub fn encode_page_offset_table(
        &self,
        input: &PageOffsetTableInput,
    ) -> Result<Vec<u8>, HintEncodeError> {
        encode_page_offset_table(input)
    }

    /// F.4.6 内嵌文件流 hint 表（Table F.11 头部 7 项 + Table F.12 的项主序五列）
    /// → 位流字节。
    ///
    /// 见 [`encode_embedded_file_table`] 的编码口径与自洽约束；声明宽度越界、
    /// 组数与组向量不符、项 4 与项 5 列表不符、项 2/项 3/项 4 的互斥 shall、
    /// 数值溢出一律类型化拒绝且零字节产出。项 5 的宽度是**文档级**入参
    /// （[`EmbeddedFileTableInput::shared_object_id_bits`]，取 Table F.3 项 11），
    /// 由调用方从页偏移表带来。
    pub fn encode_embedded_file_table(
        &self,
        input: &EmbeddedFileTableInput,
    ) -> Result<Vec<u8>, EmbeddedFileEncodeError> {
        encode_embedded_file_table(input)
    }
}
