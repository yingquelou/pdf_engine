//! 反序显示串（ISO 32000-1 14.8.2.3.3「Reverse-Order Show Strings」）的写端。
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
//! [`PdfBuilder::begin_reversed_chars`] 接收**自然顺序**的文本串，逐串反转
//! 成显示串后写出 `/ReversedChars << /ReversedChars true >> BDC` + `BT` +
//! `Td` + 反转 `Tj`，[`PdfBuilder::end_reversed_chars`] 补 `ET` + `EMC`。
//! 反转由本模块完成，调用方无从拼错显示形态；识别依据是标签本身（规范
//! EXAMPLE 用不带属性表的 `BMC`，属性表条目只是本写端的自证标记）。
//!
//! 构建期校验落在 `assemble_preflight` 里、任何对象号分配与写出之前，
//! 失败保持零字节契约（对**所有** `ReversedChars` 区段生效，包括经通用
//! `begin_marked_content_properties` 手工注入的写法）：
//!
//! - 区段内没有显示串、或某个显示串为空串：拒绝；
//! - 显示串含可打印 ASCII（U+0020–U+007E）之外的字节、或以十六进制串
//!   （`<…> Tj`）形态显示（UTF-8/多字节码无法逐字符安全反转）：拒绝；
//! - 显示串首尾之外出现 U+0020（规范 shall not contain interior SPACEs）：
//!   拒绝；
//! - 区段以 `MP`/`DP` 点形态出现（点不包围任何显示串）：拒绝；
//! - 区段未在该页内容流内以 `EMC` 闭合：拒绝；
//! - 目标版本低于 PDF 1.4（Tagged PDF 的结构扩展）：
//!   [`WriteError::UnsupportedVersion`]。

use crate::core::builder::types::Font;
use crate::core::builder::{PageBuild, PdfBuilder};
use crate::core::content::{MarkedContentProperties, Op, TextArrayItem};
use crate::core::types::{Dict, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

/// 14.8.2.3.3 规定的标记内容标签；只有精确匹配才算反序显示串序列。
const TAG: &str = "ReversedChars";

/// 一个 `ReversedChars` 打开层：是否已出现显示串。
struct OpenLevel {
    reversed: bool,
    shown: bool,
}

/// 扫描全部页面内容流的 `ReversedChars` 区段并做写端门禁。
///
/// 返回文档是否含反序显示串，调用方据此把 [`PdfFeature::ReversedCharacters`]
/// 写入特征画像供版本推导。全部违规都发生在写出任何字节之前；版本不足
/// 返回 [`WriteError::UnsupportedVersion`]。
pub(in crate::core::builder) fn validate(
    pages: &[PageBuild],
    version: PdfVersion,
) -> Result<bool, WriteError> {
    let mut found = false;
    for (page, page_build) in pages.iter().enumerate() {
        let mut open: Vec<OpenLevel> = Vec::new();
        for op in page_build.content.ops() {
            match op {
                Op::BeginMarkedContent { tag } | Op::BeginMarkedContentProperties { tag, .. } => {
                    open.push(OpenLevel {
                        reversed: tag == TAG,
                        shown: false,
                    });
                }
                Op::MarkedContentPoint { tag } | Op::MarkedContentPointProperties { tag, .. }
                    if tag == TAG =>
                {
                    return Err(invalid(format!(
                        "page {page} ReversedChars appears as a marked-content point; 14.8.2.3.3 requires a sequence enclosing show strings"
                    )));
                }
                Op::EndMarkedContent => {
                    let Some(level) = open.pop() else { continue };
                    if level.reversed && !level.shown {
                        return Err(invalid(format!(
                            "page {page} ReversedChars sequence contains no show string to reverse"
                        )));
                    }
                }
                _ => {}
            }
            if !open.iter().any(|level| level.reversed) {
                continue;
            }
            found = true;
            match op {
                Op::ShowText(text) => {
                    check_show_string(page, text)?;
                    mark_shown(&mut open);
                }
                Op::ShowTextNextLine(text) | Op::SetSpacingShowTextNextLine { text, .. } => {
                    check_show_string(page, text)?;
                    mark_shown(&mut open);
                }
                Op::ShowTextArray(items) => {
                    for item in items {
                        let TextArrayItem::String(text) = item else {
                            continue;
                        };
                        check_show_string(page, text)?;
                    }
                    if items
                        .iter()
                        .any(|item| matches!(item, TextArrayItem::String(_)))
                    {
                        mark_shown(&mut open);
                    }
                }
                Op::ShowTextHex(_) => {
                    return Err(invalid(format!(
                        "page {page} ReversedChars shows a hexadecimal string; only printable ASCII show strings can be reversed character by character"
                    )));
                }
                _ => {}
            }
        }
        if let Some(level) = open.last()
            && level.reversed
        {
            return Err(invalid(format!(
                "page {page} ReversedChars sequence is not closed by EMC before the end of the content stream"
            )));
        }
    }
    if found && !version.supports(PdfFeature::ReversedCharacters) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::ReversedCharacters,
            PdfFeature::ReversedCharacters.min_version().as_str(),
            version.as_str()
        )));
    }
    Ok(found)
}

/// 把最近的 `ReversedChars` 打开层标记为已有显示串。
fn mark_shown(open: &mut [OpenLevel]) {
    if let Some(level) = open.iter_mut().rev().find(|level| level.reversed) {
        level.shown = true;
    }
}

/// 单个显示串的合规判定：非空、纯可打印 ASCII、U+0020 只允许在首尾。
fn check_show_string(page: usize, text: &str) -> Result<(), WriteError> {
    if text.is_empty() {
        return Err(invalid(format!(
            "page {page} ReversedChars show string is empty"
        )));
    }
    if !text
        .chars()
        .all(|character| matches!(character, '\u{20}'..='\u{7E}'))
    {
        return Err(invalid(format!(
            "page {page} ReversedChars show string {text:?} holds characters outside printable ASCII (U+0020-U+007E)"
        )));
    }
    let bytes = text.as_bytes();
    if bytes.len() > 2 && bytes[1..bytes.len() - 1].contains(&b' ') {
        return Err(invalid(format!(
            "page {page} ReversedChars show string {text:?} contains an interior U+0020 space; word-break spaces are allowed only at the beginning or end (14.8.2.3.3)"
        )));
    }
    Ok(())
}

impl PdfBuilder {
    /// 在指定页开始一段反序显示串标记内容（ISO 32000-1 14.8.2.3.3，
    /// Tagged PDF 自 PDF 1.4 引入）：
    /// `/ReversedChars << /ReversedChars true >> BDC` + `BT` + `Td`，
    /// 并按给定**自然顺序**文本逐串反转写出 `Tj`。
    ///
    /// 反转由本方法完成，调用方只提供自然顺序文本；显示串必须以
    /// [`PdfBuilder::end_reversed_chars`] 收尾（补 `ET` + `EMC`）。
    /// 构建期在任何对象号分配之前校验显示串形态（非空、纯可打印 ASCII、
    /// 内部无 U+0020 空格），违规返回 [`WriteError::InvalidObject`]、
    /// 目标版本低于 PDF 1.4 返回 [`WriteError::UnsupportedVersion`]，
    /// 两者均零字节输出。未知页索引被忽略。
    pub fn begin_reversed_chars(
        &mut self,
        page: usize,
        font: &Font,
        x: f64,
        y: f64,
        strings: &[String],
    ) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            // 字体去重与 PdfBuilder::text 同规则：同 BaseFont + 字号 + 编码，
            // 再额外区分 Multiple Master 标记（[`Font::same_dedup_key`]）。
            let fidx = match p.fonts.iter().position(|f| f.same_dedup_key(font)) {
                Some(index) => index,
                None => {
                    p.fonts.push(font.clone());
                    p.fonts.len() - 1
                }
            };
            let font_name = format!("F{}", fidx + 1);
            p.content.begin_marked_content_properties(
                TAG,
                MarkedContentProperties::Dict(Dict::new().insert(TAG, Value::Bool(true))),
            );
            p.content.begin_text();
            p.content.set_font(font_name, font.size);
            p.content.move_next_line(x, y);
            for text in strings {
                let reversed: String = text.chars().rev().collect();
                if reversed
                    .chars()
                    .all(|character| matches!(character, '\u{20}'..='\u{7E}'))
                {
                    p.content.show_text(reversed);
                } else {
                    // 非可打印 ASCII：以十六进制串写出，交给构建期门禁拒绝，
                    // 保证零字节契约而不是在这里悄悄丢弃。
                    p.content.show_text_hex(reversed.as_bytes().to_vec());
                }
            }
        }
        self
    }

    /// 结束当前反序显示串段：补 `ET` + `EMC`。未知页索引被忽略。
    pub fn end_reversed_chars(&mut self, page: usize) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.end_text();
            p.content.end_marked_content();
        }
        self
    }
}

fn invalid(message: impl Into<String>) -> WriteError {
    WriteError::InvalidObject(format!("reversed show strings: {}", message.into()))
}
