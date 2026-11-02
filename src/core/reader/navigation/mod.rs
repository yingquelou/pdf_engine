//! 文档级导航读取：名称树、显式/命名目标、大纲、页面标签、命名页面、
//! 命名模板页、文章线、便携集合、备用演示与扩展字典。
//!
//! 覆盖 ISO 32000-1 7.7.4（名称字典）、12.3（Destinations、Outline、
//! Collections）、12.4.2（Page Labels）、12.4.3（Articles）、12.7.6
//! （命名页面与模板页）与 13.5（Alternate Presentations）。动作（12.6）
//! 由 E3 建模，大纲条目的 `/A` 在此原样保留。

use std::collections::BTreeMap;
use std::fmt;

use crate::core::types::{Dict, Document, Value};

mod alternate_presentation;
mod articles;
mod collection;
pub mod destination;
mod extensions;
mod name_tree;
pub(crate) use name_tree::{name_tree_entries, number_tree_entries};
mod outline;
#[cfg(test)]
mod outline_tests;
mod page_labels;
mod page_tree;
mod templates;
#[cfg(test)]
mod tests;
mod viewer_preferences;
#[cfg(test)]
mod viewer_preferences_tests;

pub use alternate_presentation::Slideshow;
pub(crate) use articles::MAX_ARTICLE_BEADS;
pub use articles::{ArticleBead, ArticleThread};
pub use collection::{
    Collection, CollectionField, CollectionFieldType, CollectionSort, CollectionView,
};
pub use destination::Destination;
pub use extensions::{DeveloperExtension, ExtensionsDictionary};
pub use outline::{Outline, OutlineDestination, OutlineItem};
pub use page_labels::{PageLabelRange, PageLabelStyle, PageLabels};
pub(crate) use page_tree::{PageTreeIndex, page_tree_index};
pub use templates::TemplatePage;
pub use viewer_preferences::{
    Duplex, NonFullScreenPageMode, PageBoundaryName, PrintScaling, ReadingDirection,
    ViewerPreferences,
};

/// 导航结构读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum NavigationError {
    /// 文档目录缺失或无法解析为字典。
    CatalogMissing,
    /// 结构条目缺失、类型错误或引用不可解析。
    Malformed(String),
    /// 目标页面不在页面树中或页码越界。
    PageNotFound,
    /// 超出遍历深度或节点数量上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for NavigationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogMissing => write!(f, "document catalog is missing or malformed"),
            Self::Malformed(message) => write!(f, "{message}"),
            Self::PageNotFound => write!(f, "page not found"),
            Self::LimitExceeded(what) => write!(f, "navigation limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for NavigationError {}

type Result<T> = std::result::Result<T, NavigationError>;

/// 名称树/数值树的最大遍历深度。
pub(crate) const MAX_TREE_DEPTH: usize = 64;
/// 名称树/数值树的最大节点数。
pub(crate) const MAX_TREE_NODES: usize = 4096;
/// 大纲最大条目数。
pub(crate) const MAX_OUTLINE_ITEMS: usize = 10_000;
/// 大纲最大嵌套深度。
pub(crate) const MAX_OUTLINE_DEPTH: usize = 64;

pub(crate) fn malformed(message: impl Into<String>) -> NavigationError {
    NavigationError::Malformed(message.into())
}

/// 解引用间接引用；直接对象原样克隆。
///
/// 与 [`Document::get_object`] 一致按对象号取最新修订：增量更新会把
/// 变更对象升代写出，而历史引用仍写旧代号，严格按引用中的代号过滤
/// 会把这类合法引用判为悬空（与主流阅读器行为亦不一致）。
pub(crate) fn resolve(document: &Document, value: &Value) -> Result<Value> {
    match value {
        Value::Ref(num, generation) => document
            .get_object(*num)
            .map_err(|error| {
                malformed(format!("failed to read object {num} {generation}: {error}"))
            })?
            .map(|object| object.value.clone())
            .ok_or_else(|| malformed(format!("object {num} {generation} not found"))),
        other => Ok(other.clone()),
    }
}

/// 文档目录字典（`trailer /Root`）。
pub(crate) fn catalog(document: &Document) -> Result<Dict> {
    let root = document
        .trailer
        .dict
        .get("Root")
        .ok_or(NavigationError::CatalogMissing)?;
    let root = resolve(document, root)?;
    root.as_dict()
        .cloned()
        .ok_or(NavigationError::CatalogMissing)
}

impl Document {
    /// 页面树中的页面总数。
    pub fn page_count(&self) -> Result<usize> {
        Ok(page_tree_index(self)?.len())
    }

    /// 读取目录 `/Names` 下指定类别的名称树，按键排序返回。
    ///
    /// 类别缺失时返回空映射；`/Names` 或节点结构损坏返回类型化错误。
    /// 键保持 PDF 字符串的原始字节（每字节一个 `char`）。
    pub fn name_tree(&self, category: &str) -> Result<BTreeMap<String, Value>> {
        let catalog = catalog(self)?;
        let Some(names) = catalog.get("Names") else {
            return Ok(BTreeMap::new());
        };
        let names = resolve(self, names)?;
        let names = names
            .as_dict()
            .ok_or_else(|| malformed("/Names is not a dictionary"))?;
        let Some(node) = names.get(category) else {
            return Ok(BTreeMap::new());
        };
        let node = resolve(self, node)?;
        let node = node
            .as_dict()
            .ok_or_else(|| malformed(format!("/Names /{category} is not a dictionary")))?;
        name_tree::name_tree_entries(self, node)
    }

    /// 按名称解析命名目标（12.3.2.3）。
    ///
    /// 先查 `/Names /Dests` 名称树（PDF 1.2+），再查目录旧式 `/Dests`
    /// 字典（PDF 1.1）；名称不存在返回 `None`。两条通道的条目值都可为
    /// Table 151 数组或含 `/D` 数组的字典（见
    /// `destination::parse_named_destination`）。
    ///
    /// 通道**共存是规范允许的**，不是重名错误：12.3.2.3 NOTE 3
    /// （PDF32000_2008.pdf `pdftotext -raw` L20532–L20535）原文
    /// 「The use of strings as destination names is a PDF 1.2 feature. If
    /// compatibility with earlier versions of PDF is required, only name
    /// objects may be used to refer to named destinations. A document that
    /// supports PDF 1.2 can contain both types. However, if backward
    /// compatibility is not a consideration, applications should use the
    /// string form of representation in the Dests name tree.」其中
    /// 「can contain both types」是许可而非禁令，写端据此**不**做跨通道重名
    /// 校验。规范未规定同名的解析优先级（全篇 `Dests` 只出现在 L3582、L3941、
    /// L20521–L20535、L35909，均无 shall/should 取舍），故「名称树优先」属
    /// **库策略**：与 NOTE 3 末尾对写者的 should（推荐文本串形态）一致，
    /// 也让 1.2+ 文件的新通道覆盖 1.1 遗留条目。
    pub fn named_destination(&self, name: &str) -> Result<Option<Destination>> {
        if let Some(value) = self.name_tree("Dests")?.get(name) {
            let pages = page_tree_index(self)?;
            return destination::parse_named_destination(self, value, &pages).map(Some);
        }
        let catalog = catalog(self)?;
        let Some(dests) = catalog.get("Dests") else {
            return Ok(None);
        };
        let dests = resolve(self, dests)?;
        let dests = dests
            .as_dict()
            .ok_or_else(|| malformed("catalog /Dests is not a dictionary"))?;
        let mut entry = dests.get(name);
        if entry.is_none() {
            entry = dests
                .0
                .iter()
                .find(|(key, _)| {
                    crate::core::reader::page_content::decode_name_escapes(key) == name
                })
                .map(|(_, value)| value);
        }
        let Some(entry) = entry else {
            return Ok(None);
        };
        let pages = page_tree_index(self)?;
        destination::parse_named_destination(self, entry, &pages).map(Some)
    }

    /// 按名称解析命名页面（12.7.6 `/Names /Pages` 名称树），返回 0 基页面索引。
    pub fn named_page(&self, name: &str) -> Result<Option<usize>> {
        let Some(value) = self.name_tree("Pages")?.get(name).cloned() else {
            return Ok(None);
        };
        let Some((num, generation)) = value.as_ref() else {
            return Err(malformed(format!(
                "named page /{name} is not an indirect page reference"
            )));
        };
        page_tree_index(self)?
            .index_of(num, generation)
            .map(Some)
            .ok_or(NavigationError::PageNotFound)
    }

    /// 读取文档大纲（12.3.4）；目录无 `/Outlines` 时返回 `None`。
    pub fn outline(&self) -> Result<Option<Outline>> {
        outline::read_outline(self)
    }

    /// 读取页面标签（12.4.2）；目录无 `/PageLabels` 时返回 `None`。
    pub fn page_labels(&self) -> Result<Option<PageLabels>> {
        page_labels::read_page_labels(self)
    }

    /// 指定 0 基页面的标签文本；无页面标签或标签为空时返回 `None`。
    pub fn page_label(&self, page_index: usize) -> Result<Option<String>> {
        let page_count = page_tree_index(self)?.len();
        if page_index >= page_count {
            return Err(NavigationError::PageNotFound);
        }
        match page_labels::read_page_labels(self)? {
            Some(labels) => Ok(labels
                .label_for(page_index)
                .filter(|label| !label.is_empty())),
            None => Ok(None),
        }
    }
}
