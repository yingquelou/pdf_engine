//! FDF 字典 `/Differences`（Table 243 续表）与 `/EmbeddedFDFs`（同表）
//! 的类型化**只读**视图（12.7.7.3，两者均为 Optional; PDF 1.4 条目）。
//!
//! 本文件是模块门面：规范依据、防护上限、跨层条目预算与对外的
//! [`Document`] 入口方法。形态类型见 [`view`]，装配过程见
//! [`expand`]，内层字节的宽容扫描见 [`scan`]，Table 244 revision 1 的
//! 密钥派生与 RC4 解扰见 [`crypto`]。
//!
//! 规范依据（`pdftotext -raw docs/PDF32000_2008.pdf` 实测行号）：
//! - raw L24906–L24926 — `Differences`：**stream**，「A stream containing
//!   all the bytes in all incremental updates made to the underlying PDF
//!   document since it was opened (see 7.5.6, Incremental Updates)」；同段
//!   明文「Although a Fields or Annots entry (or both) may be present
//!   along with Differences, there is no requirement that their contents
//!   will be consistent with each other」，且 `/Differences` 含数字签名时
//!   只有该流里给出的字段值可信。⇒ 本视图**不做** Differences ↔ Fields
//!   一致性校验（规范不要求，做了反而是错误断言）。
//! - raw L24930–L24932 — `EmbeddedFDFs`：**array**，「An array of file
//!   specifications (see 7.11, File Specifications) representing other FDF
//!   files embedded within this one (7.11.4, Embedded File Streams)」。
//!   文件规范解析直接复用 [`crate::core::reader::embedded`]（7.11）的
//!   [`crate::core::reader::embedded::Filespec`] /
//!   [`crate::core::reader::embedded::EmbeddedFile`]，不另造一套。递归展开出的子 FDF 文档
//!   在**同一趟**递归里逐层类型化，子文档自己的 `/Differences` 也一并给出
//!   视图（[`FdfEmbeddedFdf::differences`]；子 [`Document`] 不落进视图，
//!   调用方拿到的 `FdfDictionary::differences` 原值只是一个无处解引用的
//!   对象号）。
//! - raw L24935–L24939 — Table 244：加密嵌入式 FDF 流的流字典**额外必需**
//!   条目 `EncryptionRevision`（integer，「Required if the FDF file is
//!   encrypted; PDF 1.4」，「This value shall be defined at the time of
//!   publication is 1」）。FDF 加密与 7.6 的 PDF 文件加密**无关**（raw
//!   L24893–L24899：独立机制，revision 1 用 40 位类 RC4 + MD5 派生密钥）。
//!   解密入口是 [`Document::fdf_embedded_fdfs_with_password`]（密钥派生与
//!   RC4 见 [`crypto`]，只走 7.6.3.3 的步 (a)/(b)）；`/EncryptionRevision`
//!   ≠ 1 不属已定义算法，只记诊断、不 Attempt 解密。
//!
//! **剩余缺口（如实记录，本车道不实现）**：
//! 1. 类型化结果以**方法**（[`Document::fdf_differences`] /
//!    [`Document::fdf_embedded_fdfs`]）而不是 `FdfDictionary` 字段暴露：
//!    加字段需要同时改动 `FdfDictionary` 的全字段字面构造点
//!    （`src/core/builder/tests/fdf/mod.rs`、`tests/main/fdf/{javascript,write}.rs`），
//!    这些文件超出本车道写集；且递归子树与字节解码本就依赖 `&Document`
//!    上下文，惰性方法形态与之匹配。字段化留给后续增量统一收口。
//! 2. `/Differences` 的增量更新字节**不参与**本文件的对象视图合并（7.5.6
//!    的增量重放属写端/签名车道范围）。
//!
//! 防护（库策略，非规范数值）：`/EmbeddedFDFs` 数组长度与内层流字节数都是
//! 不可信输入，故单次数组展开的条目数、递归层数、内层对象数与内联字节量
//! 各有上限；**并且**另有一道跨层共享的全局条目预算
//! （[`MAX_FDF_EMBEDDED_TOTAL_ENTRIES`]）：单数组上限与层数上限只约束
//! *一条链*或*一个数组*，扇出相乘后仍是 `breadth^depth` 量级，加上载体流
//! 可被压缩（解码字节远大于文件字节），故只看这两个上限不足以「防放大」。
//! 环检测按**同一份文档**记账（对象号只在所属文件内唯一，故
//! 子文档各自新建集合，父子同名的对象号不是环路；同层重复引用同一文件
//! 规范则判为环路并停止展开）。条目展开一律不预分配容量（禁
//! `Vec::with_capacity(外部计数)`），畸形项按「跳过 + 记录 + 保留原值」降级，
//! 与既有 `/JavaScript`、`/Target` 的读端宽容风格一致，绝不让整份 FDF
//! 解析失败。解密后的明文走的是**同一条**类型化路径，故这四道防护对加密
//! 条目逐层照常生效（`/EncryptionRevision 1` 不是绕过上限的通行证）。

use crate::core::types::Document;
use zeroize::Zeroizing;

use super::FdfError;

pub(crate) mod scan;

/// Table 244 revision 1 的密钥派生与 RC4 解扰（`pub(crate)` 仅供本模块
/// 与单元测试直接使用独立向量校验）。
pub(crate) mod crypto;

mod expand;
mod view;

type Result<T> = std::result::Result<T, FdfError>;

/// 嵌入式 FDF 递归展开的最大层数（**库策略**：`/EmbeddedFDFs` 自引用时
/// 不限层数会无限下降；规范未给上限）。
pub(crate) const MAX_FDF_EMBEDDED_DEPTH: usize = 4;
/// 单次 `/EmbeddedFDFs` 数组展开的条目上限（库策略，数组长度不可信）。
pub(crate) const MAX_FDF_EMBEDDED_ENTRIES: usize = 256;
/// **一次顶层展开**（含所有递归层）累计类型化条目的全局预算（库策略）。
/// [`MAX_FDF_EMBEDDED_ENTRIES`] 只约束一个数组、[`MAX_FDF_EMBEDDED_DEPTH`]
/// 只约束一条链，两者相乘仍是 `breadth^depth` 量级；载体流可压缩时解码字节
/// 也远超文件字节，故再加一道跨层共享的总条目预算兜住放大。
pub(crate) const MAX_FDF_EMBEDDED_TOTAL_ENTRIES: usize = 512;

/// 一次顶层 `/EmbeddedFDFs` 展开的全局条目预算（跨所有递归层共享，
/// 见 [`MAX_FDF_EMBEDDED_TOTAL_ENTRIES`]）。
#[derive(Debug, Default)]
pub(crate) struct ExpansionBudget {
    used: usize,
}

impl ExpansionBudget {
    /// 为本条目申请一个全局名额；预算耗尽返回 `false`，调用方记诊断并
    /// 停止展开所在数组的剩余项（已类型化的层不受影响）。
    pub(crate) fn admit(&mut self) -> bool {
        if self.used >= MAX_FDF_EMBEDDED_TOTAL_ENTRIES {
            return false;
        }
        self.used += 1;
        true
    }
}

pub use view::{FdfDifferences, FdfEmbeddedFdf, FdfEmbeddedFdfs};

impl Document {
    /// `/Differences` 的类型化读视图（Table 243 续表；stream；PDF 1.4）。
    ///
    /// 非 FDF 文件或该条目缺失返回 `Ok(None)`；条目存在时**总是**返回
    /// `Some`（形态不符只记诊断，不报错），口径与 [`Document::fdf`] 的
    /// 读端宽容一致。
    pub fn fdf_differences(&self) -> Result<Option<FdfDifferences>> {
        let Some(fdf) = self.fdf()? else {
            return Ok(None);
        };
        Ok(expand::differences_view(self, &fdf))
    }

    /// `/EmbeddedFDFs` 的类型化读视图（Table 243 续表；file specification
    /// 数组；PDF 1.4），含对可解出的嵌入式 FDF 流的**递归**子文档解析。
    ///
    /// 非 FDF 文件或该条目缺失返回 `Ok(None)`；数组级与条目级畸形都降级
    /// 为诊断记录，不让本文件解析失败。整棵递归树的展开共享一个
    /// [`MAX_FDF_EMBEDDED_TOTAL_ENTRIES`] 全局条目预算。
    ///
    /// **加密条目不解密**：本入口不带用户口令（12.7.7.3 的 FDF 侧解密
    /// 需要它），故 `/EncryptionRevision` 存在即记一条诊断并给出 `None`
    /// 子文档；需要读加密嵌入式 FDF 时用
    /// [`Document::fdf_embedded_fdfs_with_password`]。
    pub fn fdf_embedded_fdfs(&self) -> Result<Option<FdfEmbeddedFdfs>> {
        let Some(fdf) = self.fdf()? else {
            return Ok(None);
        };
        Ok(expand::embedded_view(
            self,
            &fdf,
            0,
            &mut ExpansionBudget::default(),
            None,
        ))
    }

    /// [`Document::fdf_embedded_fdfs`] 的**带口令**变体（12.7.7.3，
    /// Table 244 `/EncryptionRevision`）：额外对 `/EncryptionRevision 1`
    /// 的嵌入式 FDF 流做 FDF 侧解密。
    ///
    /// 密钥按规范原文派生：与 7.6.3.3「Algorithm 2」的步 (a)/(b) **完全
    /// 一致**（把口令截断/填充到 32 字节后取 MD5 摘要的**前 5 字节**），
    /// 不掺 `/O`、`/P`、`/ID`（raw L24893–L24900 + L3015–L3018 +
    /// L3054–L3059）；载体字节随后经 RC4 解扰，明文走**与未加密条目完全
    /// 相同**的类型化路径（递归子文档、`/Differences` 视图逐层照常给出）。
    ///
    /// 其余形态与本模块同口径：
    /// - 未声明 `/EncryptionRevision` 的条目行为与 [`Document::fdf_embedded_fdfs`]
    ///   逐字一致（明文路径）。
    /// - `/EncryptionRevision` ≠ 1（Table 244「唯一被定义的值是 1」）：
    ///   **不 Attempt 解密**，只记诊断、子文档为 `None`。
    /// - 解扰后无法解析（多为口令不符）：跳过 + 记录诊断 + 保留原值，
    ///   绝不 panic、绝不让整份 FDF 解析失败。
    /// - 口令与派生密钥只在本调用栈内以 [`Zeroizing`] 承载，解密明文同样
    ///   在装配完成后归零销毁；密钥字节不进入诊断、视图字段或日志。
    ///   口令在递归层逐层下传（子文档自己的加密条目同样可解）。
    pub fn fdf_embedded_fdfs_with_password(
        &self,
        password: impl AsRef<[u8]>,
    ) -> Result<Option<FdfEmbeddedFdfs>> {
        let Some(fdf) = self.fdf()? else {
            return Ok(None);
        };
        // 口令不驻留 Document（与 `reader/parser.rs` 的 `with_password`
        // 同一口径：只在本次解析期内存在）。
        let password = Zeroizing::new(password.as_ref().to_vec());
        Ok(expand::embedded_view(
            self,
            &fdf,
            0,
            &mut ExpansionBudget::default(),
            Some(password.as_slice()),
        ))
    }
}
