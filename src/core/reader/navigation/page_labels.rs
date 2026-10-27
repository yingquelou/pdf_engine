//! 页面标签（ISO 32000-1 12.4.2）：数值树与标签格式化。

use crate::core::reader::content::text_value;
use crate::core::types::Document;

use super::name_tree::number_tree_entries;
use super::page_tree::page_tree_index;
use super::{NavigationError, catalog, malformed, resolve};

/// 页面标签编号样式（Table 159 `/S`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLabelStyle {
    /// 十进制阿拉伯数字。
    Decimal,
    /// 大写罗马数字。
    UppercaseRoman,
    /// 小写罗马数字。
    LowercaseRoman,
    /// 大写字母（A–Z、AA–ZZ…）。
    UppercaseLetters,
    /// 小写字母。
    LowercaseLetters,
}

/// 一个页面标签范围；`start` 为起始页面的 0 基索引。
#[derive(Debug, Clone, PartialEq)]
pub struct PageLabelRange {
    pub start: usize,
    pub style: Option<PageLabelStyle>,
    pub prefix: String,
    pub first_number: u64,
}

/// 文档页面标签；`ranges` 按键升序。
#[derive(Debug, Clone, PartialEq)]
pub struct PageLabels {
    ranges: Vec<PageLabelRange>,
    page_count: usize,
}

impl PageLabels {
    /// 指定 0 基页面的标签文本；超出页面范围返回 `None`。
    pub fn label_for(&self, page_index: usize) -> Option<String> {
        if page_index >= self.page_count {
            return None;
        }
        let range = self
            .ranges
            .iter()
            .rev()
            .find(|range| range.start <= page_index)?;
        let Some(style) = range.style else {
            return Some(range.prefix.clone());
        };
        let number = range.first_number + (page_index - range.start) as u64;
        Some(format!("{}{}", range.prefix, format_number(style, number)))
    }

    pub fn page_count(&self) -> usize {
        self.page_count
    }

    pub fn ranges(&self) -> &[PageLabelRange] {
        &self.ranges
    }
}

/// 读取目录 `/PageLabels`；不存在时返回 `None`。
pub(crate) fn read_page_labels(document: &Document) -> Result<Option<PageLabels>, NavigationError> {
    let catalog = catalog(document)?;
    let Some(labels) = catalog.get("PageLabels") else {
        return Ok(None);
    };
    let labels = resolve(document, labels)?;
    let labels = labels
        .as_dict()
        .ok_or_else(|| malformed("/PageLabels is not a dictionary"))?;
    let entries = number_tree_entries(document, labels)?;
    let page_count = page_tree_index(document)?.len();

    let mut ranges = Vec::with_capacity(entries.len());
    for (start, value) in entries {
        if start < 0 {
            return Err(malformed(
                "page label range starts at a negative page index",
            ));
        }
        let start = start as usize;
        let value = resolve(document, &value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| malformed("page label range is not a dictionary"))?;
        let style = match dict.get("S") {
            Some(value) => {
                let value = resolve(document, value)?;
                let name = value
                    .as_name()
                    .ok_or_else(|| malformed("page label /S is not a name"))?;
                Some(parse_style(name)?)
            }
            None => None,
        };
        let prefix = match dict.get("P") {
            Some(value) => {
                text_value(value).ok_or_else(|| malformed("page label /P is not a string"))?
            }
            None => String::new(),
        };
        let first_number = match dict.get("St") {
            Some(value) => {
                let value = resolve(document, value)?;
                let number = value
                    .as_integer()
                    .ok_or_else(|| malformed("page label /St is not an integer"))?;
                if number < 1 {
                    return Err(malformed("page label /St must be at least 1"));
                }
                number as u64
            }
            None => 1,
        };
        ranges.push(PageLabelRange {
            start,
            style,
            prefix,
            first_number,
        });
    }
    Ok(Some(PageLabels { ranges, page_count }))
}

fn parse_style(name: &str) -> Result<PageLabelStyle, NavigationError> {
    match name {
        "D" => Ok(PageLabelStyle::Decimal),
        "R" => Ok(PageLabelStyle::UppercaseRoman),
        "r" => Ok(PageLabelStyle::LowercaseRoman),
        "A" => Ok(PageLabelStyle::UppercaseLetters),
        "a" => Ok(PageLabelStyle::LowercaseLetters),
        other => Err(malformed(format!("unknown page label style /{other}"))),
    }
}

fn format_number(style: PageLabelStyle, number: u64) -> String {
    match style {
        PageLabelStyle::Decimal => number.to_string(),
        PageLabelStyle::UppercaseRoman => roman(number),
        PageLabelStyle::LowercaseRoman => roman(number).to_lowercase(),
        PageLabelStyle::UppercaseLetters => letters(number, true),
        PageLabelStyle::LowercaseLetters => letters(number, false),
    }
}

fn roman(mut number: u64) -> String {
    const NUMERALS: &[(u64, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, symbol) in NUMERALS {
        while number >= *value {
            out.push_str(symbol);
            number -= value;
        }
    }
    out
}

fn letters(mut number: u64, uppercase: bool) -> String {
    let base = if uppercase { b'A' } else { b'a' };
    let mut out = Vec::new();
    while number > 0 {
        let digit = ((number - 1) % 26) as u8;
        out.push(base + digit);
        number = (number - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).expect("letters are ASCII")
}
