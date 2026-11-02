//! 文档大纲遍历（ISO 32000-1 12.3.3「Document Outline」）。
//!
//! 取证更正：大纲是 **12.3.3**（raw L20536），本节没有 12.3.3.1/12.3.3.2
//! 这样的编号子条款，条目字典直接由 Table 152（outline dictionary）与
//! Table 153（outline item dictionary）给出；**12.3.4 是「Thumbnail
//! Images」**（raw L20657），与本模块无关。
//!
//! # 书签标题的语言转义（Table 153 `/Title` + 14.9.3 + 7.9.2.2）
//!
//! 规范原文（`pdftotext -raw docs/PDF32000_2008.pdf` 抽取，下列均为该 raw
//! 文本的行号；多列表格必须用 `-raw`，`-layout` 会列错位）：
//!
//! - Table 153「Entries in an outline item dictionary」的表头在 raw L20577，
//!   其 `/Title` 行在 raw L20578：
//!   > Title  text string  (Required) The text that shall be displayed on the
//!   > screen for this item.
//!
//!   即标题**必填**、类型是文本串，其语义是「显示在屏幕上的文本」。
//! - 7.9.2.2「Text String Type」把语言转义定义在**任意** Unicode 文本串里
//!   （raw L4218–L4219：「An escape sequence may appear anywhere in a Unicode
//!   text string to indicate the language in which subsequent text shall be
//!   written.」），语法在 raw L4242–L4248：U+001B + 2 字节 ISO 639 码 +
//!   （可选）2 字节 ISO 3166 码 + U+001B。
//! - 14.9.2.1（raw L31940–L31941）把「覆盖 prevailing Lang」推广到所有
//!   Unicode 文本串：
//!   > Text strings encoded in Unicode may include an escape sequence or
//!   > language tag indicating the language of the text and **overriding** the
//!   > prevailing Lang entry (see 7.9.2.2, "Text String Type").
//! - 14.9.3 NOTE 3（raw L32117–L32120，该子句标题在 raw L32092）的 shall：
//!   > Within alternate descriptions, Unicode escape sequences specifying
//!   > language **shall override** the prevailing Lang entry.
//!
//! # 本模块形态
//!
//! [`OutlineItem::title`] 保留按 PDF 文本串解码的**原串**（U+001B 转义不
//! 删）：转义本身就是「这段文字该用什么语言朗读」的信息，去转义后无从恢复，
//! 而 [`OutlineItem`] 的字段形状是既有公共 API，加字段属破坏性改动。自然
//! 文本与生效语言由标题访问器给出——
//! [`title_text`](OutlineItem::title_text) 对应 Table 153 的「shall be
//! displayed on the screen」，
//! [`title_language`](OutlineItem::title_language) 落实 14.9.3/14.9.2.1 的
//! shall（有转义即覆盖 prevailing Lang），
//! [`title_semantic_text`](OutlineItem::title_semantic_text) 给出逐段视图。
//! prevailing Lang 即目录 `/Lang`（`Document::document_language`），由调用方
//! 传入，与 [`Annotation::alternate_description`](crate::core::reader::annotation::Annotation::alternate_description)
//! 的口径一致。
//!
//! 转义解析复用 [`SemanticText`]（7.9.2.2 的类型化实现），本模块不重复
//! 实现语法；残缺转义在访问器上返回 [`NavigationError::Malformed`]，即
//! 「读取整棵树」不因单个残缺转义失败，但**要求语言结论时必须显式处理错误**
//! （库策略：报错而非静默把残缺转义当文本，与 `language_escape` 一致）。
//!
//! ## 缺口
//!
//! - 标题里的转义只解析、不改写：不做 BiDi、不按语言排序。
//! - 写端 [`crate::core::builder::OutlineEntry::title`] 原样透传转义字符，
//!   不校验其合法性（校验在读端访问器）。

use std::collections::HashSet;

use crate::core::reader::accessibility::{LANGUAGE_ESCAPE, LanguageIdentifier, SemanticText};
use crate::core::reader::content::text_value;
use crate::core::types::{Document, Value};

use super::destination::parse_outline_destination;
use super::page_tree::PageTreeIndex;
use super::{
    MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS, NavigationError, catalog, malformed, page_tree_index,
    resolve,
};

/// 文档大纲的顶层条目。
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub items: Vec<OutlineItem>,
}

/// 大纲条目；`children` 按文档顺序展开。
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    /// 已按 PDF 文本字符串解码的标题，**原样保留** 7.9.2.2 的 Unicode 语言
    /// 转义字符（U+001B 定界）：转义声明的语言是显示文本里没有的信息，
    /// 去掉便无法恢复，且 `title` 的字段形状是既有公共 API。
    ///
    /// Table 153（raw L20578）说的「the text that shall be displayed on the
    /// screen」是[`title_text`](Self::title_text)（去转义的自然文本）；
    /// 「串内转义覆盖 prevailing Lang」（14.9.2.1 raw L31940–L31941、
    /// 14.9.3 raw L32117–L32120 的 shall）由
    /// [`title_language`](Self::title_language) 给出。
    pub title: String,
    /// 目标（显式或命名）；条目可能只有动作而无目标。
    pub destination: Option<OutlineDestination>,
    /// `/A` 动作字典原样保留，语义由动作模型（E3）解析。
    pub action: Option<Value>,
    /// `/C` 标题颜色，各分量 0–1。
    pub color: Option<[f64; 3]>,
    /// `/F` 标志位：1 斜体、2 粗体。
    pub flags: u32,
    /// `/Count`：正数表示展开的可见子项数，负数表示折叠。
    pub count: i64,
    pub children: Vec<OutlineItem>,
}

/// 大纲目标：显式目的地或命名目的地。
#[derive(Debug, Clone, PartialEq)]
pub enum OutlineDestination {
    Explicit(super::Destination),
    Named(String),
}

impl OutlineItem {
    /// 标题里是否出现 7.9.2.2（raw L4242–L4248）的 Unicode 语言转义序列。
    ///
    /// 只做 U+001B 存在性判断，不校验转义是否合法；合法性由
    /// [`title_semantic_text`](Self::title_semantic_text) 负责。
    pub fn has_title_language_escape(&self) -> bool {
        self.title.contains(LANGUAGE_ESCAPE)
    }

    /// 标题按 7.9.2.2 解析出的 [`SemanticText`] 视图：逐段的转义语言与
    /// 去转义文本。
    ///
    /// 未闭合、长度非 2/4 或码字节非 ASCII 字母的残缺转义返回
    /// [`NavigationError::Malformed`]（库策略：报错而非静默当作文本，与
    /// `accessibility::language_escape` 一致）；`Document::outline` 本身
    /// 不因残缺转义失败，只有索取语言结论时才报错。
    pub fn title_semantic_text(&self) -> Result<SemanticText, NavigationError> {
        SemanticText::parse(&self.title)
            .map_err(|error| malformed(format!("outline /Title: {error}")))
    }

    /// 去掉全部语言转义后的自然文本，即 Table 153（raw L20578）所说
    /// 「the text that shall be displayed on the screen」的显示文本。
    ///
    /// 标题不含转义时与 [`title`](Self::title) 相同。
    pub fn title_text(&self) -> Result<String, NavigationError> {
        Ok(self.title_semantic_text()?.plain_text())
    }

    /// 标题生效的语言（14.9.3 raw L32117–L32120 与 14.9.2.1 raw
    /// L31940–L31941 的 shall：串内 Unicode 语言转义**覆盖** prevailing
    /// Lang entry）。
    ///
    /// 有转义时返回**首个转义**声明的语言并忽略传入的 `prevailing`；没有
    /// 转义时原样返回 `prevailing`（通常是目录 `/Lang`，即
    /// `Document::document_language` 的结果；`None` 表示文档没声明）。
    /// 标题内多个转义的逐段语言见
    /// [`title_semantic_text`](Self::title_semantic_text)。
    pub fn title_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, NavigationError> {
        Ok(self.title_semantic_text()?.effective_language(prevailing))
    }
}

struct OutlineReader<'a> {
    document: &'a Document,
    pages: &'a PageTreeIndex,
    visited: HashSet<(i64, i64)>,
    items: usize,
}

/// 读取目录 `/Outlines`；不存在时返回 `None`。
pub(crate) fn read_outline(document: &Document) -> Result<Option<Outline>, NavigationError> {
    let catalog = catalog(document)?;
    let Some(root) = catalog.get("Outlines") else {
        return Ok(None);
    };
    let root = resolve(document, root)?;
    let root = root
        .as_dict()
        .ok_or_else(|| malformed("/Outlines is not a dictionary"))?;
    let Some(first) = root.get("First") else {
        return Ok(Some(Outline { items: Vec::new() }));
    };
    let pages = page_tree_index(document)?;
    let mut reader = OutlineReader {
        document,
        pages: &pages,
        visited: HashSet::new(),
        items: 0,
    };
    let items = reader.read_siblings(first, 0)?;
    Ok(Some(Outline { items }))
}

impl OutlineReader<'_> {
    fn read_siblings(
        &mut self,
        first: &Value,
        depth: usize,
    ) -> Result<Vec<OutlineItem>, NavigationError> {
        if depth > MAX_OUTLINE_DEPTH {
            return Err(NavigationError::LimitExceeded("outline depth"));
        }
        let mut items = Vec::new();
        let mut current = Some(first.clone());
        while let Some(value) = current {
            if self.items >= MAX_OUTLINE_ITEMS {
                return Err(NavigationError::LimitExceeded("outline item count"));
            }
            if let Some(key) = value.as_ref()
                && !self.visited.insert(key)
            {
                return Err(malformed("outline item cycle"));
            }
            let node = resolve(self.document, &value)?;
            let dict = node
                .as_dict()
                .ok_or_else(|| malformed("outline item is not a dictionary"))?;
            self.items += 1;

            // Table 153（raw L20578）：/Title 是必填文本串。此处取解码后的
            // 原串（含 7.9.2.2 的语言转义字符），自然文本与生效语言经
            // OutlineItem 的标题访问器按 14.9.3 的 shall 给出。
            let title = dict
                .get("Title")
                .and_then(text_value)
                .ok_or_else(|| malformed("outline item is missing /Title"))?;
            let destination = match dict.get("Dest") {
                Some(value) => Some(parse_outline_destination(self.document, value, self.pages)?),
                None => None,
            };
            let action = dict.get("A").cloned();
            let color = parse_color(self.document, dict.get("C"))?;
            let flags = parse_flags(self.document, dict.get("F"))?;
            let count = parse_count(self.document, dict.get("Count"))?;
            let children = match dict.get("First") {
                Some(first) => self.read_siblings(first, depth + 1)?,
                None => Vec::new(),
            };
            items.push(OutlineItem {
                title,
                destination,
                action,
                color,
                flags,
                count,
                children,
            });
            current = dict.get("Next").cloned();
        }
        Ok(items)
    }
}

fn parse_color(
    document: &Document,
    value: Option<&Value>,
) -> Result<Option<[f64; 3]>, NavigationError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = resolve(document, value)?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("outline /C is not an array"))?;
    if array.len() != 3 {
        return Err(malformed("outline /C must contain 3 components"));
    }
    let mut color = [0.0; 3];
    for (slot, value) in color.iter_mut().zip(&array.0) {
        let value = resolve(document, value)?;
        let component = match value {
            Value::Integer(number) => number as f64,
            Value::Float(number) => number,
            other => {
                return Err(malformed(format!(
                    "outline /C component is not a number: {other:?}"
                )));
            }
        };
        if !(0.0..=1.0).contains(&component) {
            return Err(malformed("outline /C component is outside 0–1"));
        }
        *slot = component;
    }
    Ok(Some(color))
}

fn parse_flags(document: &Document, value: Option<&Value>) -> Result<u32, NavigationError> {
    let Some(value) = value else {
        return Ok(0);
    };
    let value = resolve(document, value)?;
    let flags = value
        .as_integer()
        .ok_or_else(|| malformed("outline /F is not an integer"))?;
    if !(0..=3).contains(&flags) {
        return Err(malformed("outline /F is outside 0–3"));
    }
    Ok(flags as u32)
}

fn parse_count(document: &Document, value: Option<&Value>) -> Result<i64, NavigationError> {
    let Some(value) = value else {
        return Ok(0);
    };
    let value = resolve(document, value)?;
    value
        .as_integer()
        .ok_or_else(|| malformed("outline /Count is not an integer"))
}
