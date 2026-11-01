//! 反序显示串（ISO 32000-1 14.8.2.3.3「Reverse-Order Show Strings」）。
//!
//! 规范原文（要点）：
//!
//! > The marked-content tag ReversedChars informs the conforming reader that
//! > show strings within a marked-content sequence contain characters in the
//! > reverse of page content order. If the sequence encompasses multiple show
//! > strings, only the individual characters within each string shall be
//! > reversed; the strings themselves shall be in natural reading order.
//! > The show strings may have a SPACE (U+0020) character at the beginning or
//! > end to indicate a word break (see 14.8.2.5) but shall not contain
//! > interior SPACEs.
//!
//! 识别依据是**标签本身**：规范的 EXAMPLE 写作 `/ReversedChars BMC … EMC`，
//! 不带属性表；`BDC` 形态（本仓库 [`PdfBuilder::begin_reversed_chars`] 写出
//! `/ReversedChars << /ReversedChars true >> BDC`）同样只按标签识别，属性表
//! 条目不参与判定。区段内的每个显示串（`Tj`、`TJ` 数组的字符串元素、`'`、
//! `"`）逐串反转回自然顺序；串与串之间的自然顺序保持不变。
//!
//! 显示串按**字节**反转——规范场景下反转的是字符码（单字节简单字体编码），
//! 因此读端以逐字节形态给出还原文本与诊断，不经过字体 `/ToUnicode` 映射；
//! 完整文本提取路径（[`Document::extract_page_text`] 等）中的还原由提取器
//! 在解码后按字符完成。反转串内部的 U+0020 空格违反 shall 约束，以
//! [`ReversedCharsDiagnostic`] 诊断返回，不 panic、也不丢弃文本。

use std::fmt;

use crate::core::reader::content::{MarkedContentError, MarkedSection, Op, TextElement};
use crate::core::types::Document;

/// 14.8.2.3.3 规定的标记内容标签；只有精确匹配才算反序显示串序列。
pub const REVERSED_CHARS_TAG: &str = "ReversedChars";

/// 一个 `ReversedChars` 显示串上的不合规诊断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReversedCharsDiagnostic {
    /// 显示串为空串，没有任何可反转的字符。
    Empty {
        /// 该串在区段显示串序列中的位置（0 基）。
        string: usize,
    },
    /// 显示串首尾之外出现 U+0020 空格（14.8.2.3.3：shall not contain
    /// interior SPACEs）。
    InteriorSpace {
        /// 该串在区段显示串序列中的位置（0 基）。
        string: usize,
    },
    /// 显示串含可打印 ASCII（U+0020–U+007E）之外的字节；逐字节反转对
    /// 多字节编码（UTF-8/UTF-16 形态的字面串）不再保证字符级正确。
    NonPrintable {
        /// 该串在区段显示串序列中的位置（0 基）。
        string: usize,
    },
}

impl fmt::Display for ReversedCharsDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { string } => write!(f, "reversed show string {string} is empty"),
            Self::InteriorSpace { string } => write!(
                f,
                "reversed show string {string} contains an interior U+0020 space (14.8.2.3.3 allows spaces only at the ends)"
            ),
            Self::NonPrintable { string } => write!(
                f,
                "reversed show string {string} holds bytes outside printable ASCII; byte-wise reversal is not character safe"
            ),
        }
    }
}

impl std::error::Error for ReversedCharsDiagnostic {}

/// 一个 `ReversedChars` 标记内容区段的还原结果。
#[derive(Debug, Clone, PartialEq)]
pub struct ReversedCharsSequence {
    /// [`Document::page_marked_content`] 返回序列中的区段下标。
    pub section: usize,
    /// 区段内各显示串按显示顺序还原成的自然顺序文本。
    pub strings: Vec<String>,
    /// 不合规显示串的诊断（按串位置升序，每串至多一条）。
    pub diagnostics: Vec<ReversedCharsDiagnostic>,
}

impl ReversedCharsSequence {
    /// 把所有还原串按自然顺序拼接（串本身已是自然顺序，直接连接）。
    pub fn text(&self) -> String {
        self.strings.concat()
    }
}

impl MarkedSection {
    /// 本区段是否为 `ReversedChars` 标记内容序列（`MP`/`DP` 点不算，
    /// 点序列不包围任何显示串）。
    pub fn is_reversed_chars(&self) -> bool {
        self.tag == REVERSED_CHARS_TAG && !self.point
    }
}

impl Document {
    /// 还原一页上所有 `ReversedChars` 区段的显示串（14.8.2.3.3）。
    ///
    /// 对每个区段收集其 [`MarkedSection::start`] 与 [`MarkedSection::end`]
    /// 算子区间内的显示串（`Tj`、`TJ` 数组元素、`'`、`"`；含嵌套区段内
    /// 显示的串），逐串按字节反转移回自然顺序，串与串保持显示顺序。
    /// 空串、内部空格与非可打印 ASCII 字节分别记 [`ReversedCharsDiagnostic`]
    /// 诊断，文本照常还原。页面内容流或标记内容配对失败返回
    /// [`MarkedContentError`]；表单 XObject 内容不被递归（沿用
    /// [`Document::page_marked_content`] 的行为）。
    pub fn page_reversed_chars(
        &self,
        page_index: usize,
    ) -> Result<Vec<ReversedCharsSequence>, MarkedContentError> {
        let sections = self.page_marked_content(page_index)?;
        let ops = self
            .page_content_ops(page_index)
            .map_err(|error| MarkedContentError::ContentStream(format!("{error:?}")))?;
        let mut sequences = Vec::new();
        for (index, section) in sections.iter().enumerate() {
            if !section.is_reversed_chars() {
                continue;
            }
            let mut sequence = ReversedCharsSequence {
                section: index,
                strings: Vec::new(),
                diagnostics: Vec::new(),
            };
            let start = section.start.min(ops.len());
            let end = section.end.clamp(start, ops.len());
            for op in &ops[start..end] {
                for bytes in show_string_bytes(op) {
                    let position = sequence.strings.len();
                    sequence.strings.push(restore_order(bytes));
                    if let Some(diagnostic) = diagnostic_for(bytes, position) {
                        sequence.diagnostics.push(diagnostic);
                    }
                }
            }
            sequences.push(sequence);
        }
        Ok(sequences)
    }
}

/// 一条算子里的所有显示串字节（`TJ` 的数组元素逐个计）。
fn show_string_bytes(op: &Op) -> Vec<&[u8]> {
    match op {
        Op::ShowText(bytes)
        | Op::NextLineAndShowText(bytes)
        | Op::SetSpacingAndShowText(_, _, bytes) => vec![bytes.as_slice()],
        Op::ShowTextWithPositioning(elements) => elements
            .iter()
            .filter_map(|element| match element {
                TextElement::String(bytes) => Some(bytes.as_slice()),
                TextElement::Adjustment(_) => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// 把显示串按字节反转移回自然顺序；非 UTF-8 字节走 lossy 还原。
fn restore_order(bytes: &[u8]) -> String {
    let reversed: Vec<u8> = bytes.iter().rev().copied().collect();
    match String::from_utf8(reversed.clone()) {
        Ok(text) => text,
        Err(_) => String::from_utf8_lossy(&reversed).into_owned(),
    }
}

/// 单个显示串的合规判定；每串至多一条诊断（空串 > 非可打印 > 内部空格）。
fn diagnostic_for(bytes: &[u8], position: usize) -> Option<ReversedCharsDiagnostic> {
    if bytes.is_empty() {
        return Some(ReversedCharsDiagnostic::Empty { string: position });
    }
    if !bytes.iter().all(|byte| (0x20..=0x7E).contains(byte)) {
        return Some(ReversedCharsDiagnostic::NonPrintable { string: position });
    }
    // 首尾允许 U+0020（分词空格），中间任何位置都不允许。
    if bytes.len() > 2 && bytes[1..bytes.len() - 1].contains(&b' ') {
        return Some(ReversedCharsDiagnostic::InteriorSpace { string: position });
    }
    None
}
