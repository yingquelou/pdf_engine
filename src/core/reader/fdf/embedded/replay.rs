//! `/Differences` 增量更新字节的 **§7.5.6 对象重放**（读端类型化视图）。
//!
//! Table 243 续表（raw L24906–L24908）把 `/Differences` 定义为「A stream
//! containing all the bytes in all incremental updates made to the
//! underlying PDF document since it was opened (see 7.5.6, "Incremental
//! Updates")」。既有 [`FdfDifferences`] 只确证「它是一个流」并原值保留，
//! **没有**把这串增量字节重放成对象序列；本模块补上这条读端视图。
//!
//! §7.5.6 Incremental Updates（raw L2198–L2228）的语义要点：
//! - L2200 变更一律追加、原内容不动；
//! - L2208–L2210 增量的交叉引用段只含被改动/替换/删除的对象；
//! - L2211–L2213 追加的 trailer 须带 `Prev`、每个 trailer 自带 `%%EOF`；
//! - L2214–L2220 同一 (对象号, 代数) 可在文件里出现多份；
//! - **L2221–L2224 读者构造交叉信息时必须让最近一份副本生效**。
//!
//! 因此 [`FdfDifferencesReplay::entries`] 按**文件序**保留每一副本（较早
//! 副本不丢弃），[`FdfDifferencesReplay::latest`] /
//! [`FdfDifferencesReplay::effective`] 给出「最后一份生效」的口径。
//!
//! **不解析、也不解引用增量片段里的偏移**：L2221–L2224 的字节偏移是相对
//! **原 PDF 文档**的，而本流只是一串被剥离出来的更新片段、没有原文件可
//! 参照，故 [`FdfDifferencesReplay::xref_sections`] /
//! [`FdfDifferencesReplay::trailers`] / [`FdfDifferencesReplay::eof_markers`]
//! 只**报告存在性**（佐证载荷确为增量更新片段），不还原交叉引用、不据此
//! 定位对象，删除对象的 `f` 标记语义同理不在本轮（见模块门面 `embedded.rs`
//! 的「剩余缺口」）。
//!
//! 语法一律复用既有词法器与对象解析函数（[`Lexer`] /
//! `parse_indirect_object` / `parse_trailer` / `parse_startxref`），不新写
//! 语法、不加依赖。经典 `xref` 表用「跳到其后 `trailer`」的方式越过（与
//! `embedded/scan.rs` 定位 trailer 的既有做法一致），**不调用**
//! `parse_xref`——后者按声明条目数 `Vec::with_capacity(count)` 预分配，而
//! `count` 是不可信输入。对象内部嵌套深度由 `parse_object` 既有的
//! `MAX_NESTING_DEPTH` 护栏兜底，本模块不另设第二套递归预算。字节窗口与
//! 条目数上限走 [`MAX_FDF_DIFFERENCE_SCAN_BYTES`] /
//! [`MAX_FDF_DIFFERENCE_OBJECTS`] 常量风格。
//!
//! 解析失败一律降级为诊断并停止，绝不向上传播错误（读端宽容，与
//! `/JavaScript`、`/Target`、`/EmbeddedFDFs` 一致）。

use std::collections::HashSet;

use crate::core::reader::lexer::Lexer;
use crate::core::reader::object::{parse_indirect_object, parse_startxref, parse_trailer};
use crate::core::types::{Document, Object, Value};

use super::view::FdfDifferences;
use super::{MAX_FDF_DIFFERENCE_OBJECTS, MAX_FDF_DIFFERENCE_SCAN_BYTES};

/// 重放出的一个间接对象副本：增量链上的一次写入（同一标识可有多份）。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfReplayedObject {
    /// 对象号（增量片段内出现，语义上属于底层 PDF 文档的对象空间）。
    pub num: i64,
    /// 代数（generation）。同一 `(num, generation)` 的多副本见
    /// [`FdfDifferencesReplay::latest`]。
    pub generation: i64,
    /// 该对象在**已解码的 `/Differences` 流字节**里的起始偏移（文件序）。
    pub byte_offset: usize,
    /// 解析出的对象值原样保留（含未建模条目；流对象为
    /// [`crate::core::types::StreamData::Ref`]，其偏移相对本流、不可解引用）。
    pub value: Value,
}

impl FdfReplayedObject {
    /// 对象标识 `(num, generation)`（§7.5.6 raw L2215 的「object identifier」）。
    pub fn key(&self) -> (i64, i64) {
        (self.num, self.generation)
    }
}

/// `/Differences` 增量更新字节按 §7.5.6 重放后的类型化只读视图。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfDifferencesReplay {
    /// 文件序的**全部**对象副本（较早副本不丢弃，§7.5.6 raw L2221–L2224）。
    pub entries: Vec<FdfReplayedObject>,
    /// 流里出现的经典 `xref` 段数（>0 佐证载荷确为增量更新片段；只计数、
    /// 不解析条目）。
    pub xref_sections: usize,
    /// 流里出现的 `trailer` 数。
    pub trailers: usize,
    /// 流里出现的 `startxref` 数。
    pub startxrefs: usize,
    /// 流里出现的 `%%EOF` 标记数（按字面字节计数，可能含流载荷里的偶然
    /// 匹配，只作存在性参考）。
    pub eof_markers: usize,
    /// 实际扫描的解码字节数（超过 [`MAX_FDF_DIFFERENCE_SCAN_BYTES`] 时为
    /// 被截断的窗口长度）。
    pub scanned_bytes: usize,
    /// 降级与诊断记录（含畸形处的字节偏移）。
    pub issues: Vec<String>,
}

impl FdfDifferencesReplay {
    /// 副本总数（含同一标识的较早副本）。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否没有重放任何对象。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 该标识**生效的副本**：文件序里的最后一份（§7.5.6 raw L2221–L2224
    /// 「the most recent copy of each object shall be the one accessed」）。
    pub fn latest(&self, num: i64, generation: i64) -> Option<&FdfReplayedObject> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.num == num && entry.generation == generation)
    }

    /// 同一标识的全部副本，按文件序（较早 → 较新）；长度 >1 即增量链证据。
    pub fn copies(&self, num: i64, generation: i64) -> Vec<&FdfReplayedObject> {
        self.entries
            .iter()
            .filter(|entry| entry.num == num && entry.generation == generation)
            .collect()
    }

    /// 生效视图：每个 `(num, generation)` 取其最后一份副本，按该标识
    /// **首次出现**的文件序排列（较早副本仍在 [`Self::entries`] 保留）。
    pub fn effective(&self) -> Vec<&FdfReplayedObject> {
        let mut order: Vec<(i64, i64)> = Vec::new();
        let mut latest: std::collections::HashMap<(i64, i64), usize> =
            std::collections::HashMap::new();
        for (index, entry) in self.entries.iter().enumerate() {
            let key = entry.key();
            if !latest.contains_key(&key) {
                order.push(key);
            }
            latest.insert(key, index);
        }
        order
            .into_iter()
            .map(|key| &self.entries[latest[&key]])
            .collect()
    }

    /// 去重后的生效对象数（不同标识个数）。
    pub fn effective_count(&self) -> usize {
        self.entries
            .iter()
            .map(FdfReplayedObject::key)
            .collect::<HashSet<_>>()
            .len()
    }

    /// 是否检出任一增量更新标记（`xref`/`trailer`/`startxref`/`%%EOF`）——
    /// 佐证载荷确为增量更新片段而非任意对象集合。
    pub fn is_incremental(&self) -> bool {
        self.xref_sections > 0 || self.trailers > 0 || self.startxrefs > 0 || self.eof_markers > 0
    }
}

impl FdfDifferences {
    /// 把 `/Differences` 流的增量更新字节按 §7.5.6 语义**重放**成对象序列
    /// 的类型化只读视图。
    ///
    /// 解码经 [`FdfDifferences::decoded_bytes`]（PDF 侧过滤器链与 `/Crypt`
    /// 照常，12.7.7.3 的 FDF 侧 RC4 **不作用于此流**）：未确证为流或解码
    /// 失败时**不报错**，只在 [`FdfDifferencesReplay::issues`] 记诊断、返回
    /// 空重放。流内对象解析失败同样降级为诊断（含字节偏移）并停止，绝不
    /// 上抛（读端宽容）。
    pub fn replay(&self, document: &Document) -> FdfDifferencesReplay {
        let bytes = match self.decoded_bytes(document) {
            Ok(bytes) => bytes,
            Err(error) => {
                return FdfDifferencesReplay {
                    entries: Vec::new(),
                    xref_sections: 0,
                    trailers: 0,
                    startxrefs: 0,
                    eof_markers: 0,
                    scanned_bytes: 0,
                    issues: vec![error.to_string()],
                };
            }
        };
        replay_bytes(&bytes)
    }
}

/// 在解码后的增量字节上扫描间接对象序列。窗口、条目数上限与「游标不前进」
/// 三重防护，畸形只停止不报错。
pub(crate) fn replay_bytes(bytes: &[u8]) -> FdfDifferencesReplay {
    let mut issues = Vec::new();
    // 累计字节预算：增量片段可压缩，解码字节远大于文件字节；只重放前若干
    // 字节，超出记诊断（不 panic、不无限分配）。
    let window = if bytes.len() > MAX_FDF_DIFFERENCE_SCAN_BYTES {
        issues.push(format!(
            "FDF /Differences incremental bytes hold {} bytes above the \
             {MAX_FDF_DIFFERENCE_SCAN_BYTES} byte replay window; only the leading window is \
             replayed",
            bytes.len()
        ));
        &bytes[..MAX_FDF_DIFFERENCE_SCAN_BYTES]
    } else {
        bytes
    };
    let mut entries: Vec<FdfReplayedObject> = Vec::new();
    let mut xref_sections = 0usize;
    let mut trailers = 0usize;
    let mut startxrefs = 0usize;
    let mut lex = Lexer::at(window, 0);
    loop {
        lex.skip_ws();
        if lex.peek().is_none() {
            break;
        }
        let before = lex.pos;
        // 经典 xref 段：只计数并跳到其后的 `trailer`，**不解析条目、不据其
        // 偏移定位对象**（偏移相对底层文档，本流内不可解引用）。不调用
        // parse_xref：它对不可信的 count 做 Vec::with_capacity 预分配。
        if lex.starts_with(b"xref") {
            match find_marker(window, lex.pos, b"trailer") {
                Some(next) => {
                    xref_sections += 1;
                    lex.pos = next;
                }
                None => {
                    issues.push(format!(
                        "FDF /Differences xref section at byte offset {before} is not followed \
                         by a trailer; replay stops here"
                    ));
                    break;
                }
            }
            continue;
        }
        if lex.starts_with(b"trailer") {
            match parse_trailer(&mut lex) {
                Ok(_) => trailers += 1,
                Err(error) => {
                    issues.push(format!(
                        "FDF /Differences trailer at byte offset {before} is malformed: {error}; \
                         replay stops here"
                    ));
                    break;
                }
            }
            if lex.pos <= before {
                break;
            }
            continue;
        }
        if lex.starts_with(b"startxref") {
            // `%%EOF` 是注释，由下一轮 skip_ws 自然跳过，故这里只消费
            // `startxref <offset>`（一个整数，无预分配风险）。
            match parse_startxref(&mut lex) {
                Ok(_) => startxrefs += 1,
                Err(error) => {
                    issues.push(format!(
                        "FDF /Differences startxref at byte offset {before} is malformed: \
                         {error}; replay stops here"
                    ));
                    break;
                }
            }
            if lex.pos <= before {
                break;
            }
            continue;
        }
        match parse_indirect_object(&mut lex) {
            Ok(Object {
                num,
                generation,
                value,
            }) => entries.push(FdfReplayedObject {
                num,
                generation,
                byte_offset: before,
                value,
            }),
            Err(error) => {
                issues.push(format!(
                    "FDF /Differences incremental bytes are not a well-formed indirect object at \
                     byte offset {before}: {error}; replay stops here"
                ));
                break;
            }
        }
        // 前进保证：任何情况下都不允许游标不动的循环。
        if lex.pos <= before {
            break;
        }
        // 条目数上限（库策略，不可信输入）。
        if entries.len() >= MAX_FDF_DIFFERENCE_OBJECTS {
            issues.push(format!(
                "FDF /Differences holds more than {MAX_FDF_DIFFERENCE_OBJECTS} incremental \
                 objects; the tail is not replayed"
            ));
            break;
        }
    }
    FdfDifferencesReplay {
        entries,
        xref_sections,
        trailers,
        startxrefs,
        eof_markers: count_markers(window, b"%%EOF"),
        scanned_bytes: window.len(),
        issues,
    }
}

/// `bytes[from..]` 里第一个 `needle` 的起始下标（逐窗口比较，不做窗口预分配）。
fn find_marker(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| from + at)
}

/// `bytes` 里 `needle` 的出现次数（非重叠，逐窗口比较）。
fn count_markers(bytes: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || bytes.len() < needle.len() {
        return 0;
    }
    bytes
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}
