//! 文档级导航写端：页面标签、命名目标、命名页面与大纲
//! （ISO 32000-1 12.3、12.4.2、12.7.6）。
//!
//! 这些结构都挂在目录（catalog）上。构建器在 [`PdfBuilder::build`] 完成
//! 对象组装后统一写入：目录字典被覆盖写回，大纲条目从现有最大对象号之后
//! 分配，因此不影响既有对象布局。
//!
//! 版本门控（构建期返回 [`WriteError::UnsupportedVersion`]，零字节输出）：
//! 大纲自 PDF 1.1 起、命名目标自 1.2 起、页面标签与命名页面自 1.3 起。
//! 输入非法（页码越界、名称重复、标签起点乱序、颜色越界等）返回
//! [`WriteError::InvalidObject`]，同样零字节输出。

use crate::core::reader::navigation::{
    Destination, OutlineDestination, PageLabelRange, PageLabelStyle,
};
use crate::core::types::{Array, Dict, Document, Object, PdfTextString, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::PdfBuilder;
use super::types::Action;
use super::types::text_string_value;

mod articles;
mod validate;

pub use articles::{ArticleBead, ArticleThread};

/// 写端大纲条目（ISO 32000-1 12.3.4）。
///
/// `/Dest` 与 `/A` 二选一；两者均设时优先写 `/A`（PDF 1.2+）。命名目标
/// 以文本字符串写出，名称树键同样按 PDF 文本字符串编码（ASCII 名称即
/// 原样字节）。
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry {
    /// 条目标题；非 ASCII 字符按 UTF-16BE 文本字符串写出。
    pub title: String,
    /// 目标；`None` 时条目只有标题或仅有动作。
    pub destination: Option<OutlineDestination>,
    /// 动作句柄（由 [`PdfBuilder::action`] 注册）；与 `/Dest` 二选一，两者
    /// 均设时优先写 `/A`（要求 PDF 1.2+）。
    pub action: Option<usize>,
    /// 标题颜色，各分量 0–1；`None` 时使用查看器默认色。
    pub color: Option<[f64; 3]>,
    /// 标志位：bit 1 斜体、bit 2 粗体，其余位必须为 0。
    pub flags: u32,
    /// 有子项时是否展开（决定 `/Count` 正负）。
    pub expanded: bool,
    /// 子条目，按文档顺序。
    pub children: Vec<OutlineEntry>,
}

/// 构建器持有的文档级导航配置；空配置时不改写目录。
#[derive(Debug, Default)]
pub(crate) struct NavigationState {
    page_labels: Vec<PageLabelRange>,
    named_destinations: Vec<(String, Destination)>,
    named_pages: Vec<(String, usize)>,
    outline: Vec<OutlineEntry>,
    actions: Vec<Action>,
    article_threads: Vec<ArticleThread>,
}

impl NavigationState {
    /// 大纲条目实际引用到的动作（含子项，按文档顺序）。
    pub(super) fn outline_actions(&self) -> Vec<&Action> {
        fn collect<'a>(
            entries: &'a [OutlineEntry],
            actions: &'a [Action],
            used: &mut Vec<&'a Action>,
        ) {
            for entry in entries {
                if let Some(handle) = entry.action
                    && let Some(action) = actions.get(handle)
                {
                    used.push(action);
                }
                collect(&entry.children, actions, used);
            }
        }
        let mut used = Vec::new();
        collect(&self.outline, &self.actions, &mut used);
        used
    }
}

impl PdfBuilder {
    /// 设置页面标签范围（12.4.2），按起始页升序给出。
    ///
    /// 每个范围的 `start` 为 0 基页索引且必须严格递增、小于页数；
    /// `first_number` 须 ≥1。页面标签要求目标版本 ≥1.3，否则
    /// [`build`](PdfBuilder::build) 返回 [`WriteError::UnsupportedVersion`]。
    /// 非法输入返回 [`WriteError::InvalidObject`]。
    pub fn page_labels(&mut self, ranges: &[PageLabelRange]) -> &mut Self {
        self.navigation.page_labels = ranges.to_vec();
        self
    }

    /// 注册一个命名目标（12.3.2.3），写入目录 `/Names /Dests` 名称树。
    ///
    /// 名称重复或目标页码越界时构建返回 [`WriteError::InvalidObject`]；
    /// 目标版本低于 1.2 返回 [`WriteError::UnsupportedVersion`]。
    pub fn named_destination(
        &mut self,
        name: impl Into<String>,
        destination: Destination,
    ) -> &mut Self {
        self.navigation
            .named_destinations
            .push((name.into(), destination));
        self
    }

    /// 注册一个命名页面（12.7.6），写入目录 `/Names /Pages` 名称树。
    ///
    /// 名称重复或页码越界时构建返回 [`WriteError::InvalidObject`]；
    /// 目标版本低于 1.3 返回 [`WriteError::UnsupportedVersion`]。
    pub fn named_page(&mut self, name: impl Into<String>, page: usize) -> &mut Self {
        self.navigation.named_pages.push((name.into(), page));
        self
    }

    /// 设置文档大纲（12.3.4），整体替换已有配置。
    ///
    /// 条目深度与数量不得超过读端上限（64 层、10,000 条）；颜色分量须在
    /// 0–1、`flags` 只能置 bit 1/2。非法输入返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.1 返回
    /// [`WriteError::UnsupportedVersion`]。
    pub fn outline(&mut self, entries: &[OutlineEntry]) -> &mut Self {
        self.navigation.outline = entries.to_vec();
        self
    }

    /// 追加一条文章线（12.4.3），按注册顺序写入目录 `/Threads`。
    ///
    /// 线程与 bead 作为间接对象写出，bead 链首尾闭合，所在页面追加
    /// `/B` 数组。bead 至少一个、页索引须在范围内、矩形须为有限数，
    /// 否则构建返回 [`WriteError::InvalidObject`]；目标版本低于 1.1
    /// 返回 [`WriteError::UnsupportedVersion`]。
    pub fn add_article_thread(&mut self, thread: ArticleThread) -> &mut Self {
        self.navigation.article_threads.push(thread);
        self
    }

    /// 注册一个动作字典（12.6），返回句柄供大纲条目或注解引用。
    ///
    /// 动作字典作为间接对象写出；大纲条目的 `/A` 条目要求 PDF 1.2+。
    pub fn action(&mut self, action: Action) -> usize {
        let handle = self.navigation.actions.len();
        self.navigation.actions.push(action);
        handle
    }
}

/// 把导航配置写入已组装的 `Document` 目录；；无配置时不产生任何对象。
pub(super) fn apply(
    doc: &mut Document,
    version: PdfVersion,
    page_count: usize,
    navigation: &NavigationState,
    annotation_nums: &[i64],
    ocg_nums: &[i64],
) -> Result<(), WriteError> {
    let has_actions = outline_has_actions(&navigation.outline);
    let required = [
        (
            !navigation.page_labels.is_empty(),
            PdfVersion::V1_3,
            "page labels",
        ),
        (
            !navigation.named_destinations.is_empty(),
            PdfVersion::V1_2,
            "named destinations",
        ),
        (
            !navigation.named_pages.is_empty(),
            PdfVersion::V1_3,
            "named pages",
        ),
        (!navigation.outline.is_empty(), PdfVersion::V1_1, "outlines"),
        (has_actions, PdfVersion::V1_2, "outline actions (/A)"),
        (
            !navigation.article_threads.is_empty(),
            PdfVersion::V1_1,
            "article threads",
        ),
    ];
    for (present, minimum, feature) in required {
        if present && version < minimum {
            return Err(WriteError::UnsupportedVersion(format!(
                "{feature} require PDF {}, got {version}",
                minimum.as_str()
            )));
        }
    }
    if required.iter().all(|(present, ..)| !present) {
        return Ok(());
    }

    validate::page_labels(&navigation.page_labels, page_count)?;
    validate::named_destinations(&navigation.named_destinations, page_count)?;
    validate::named_pages(&navigation.named_pages, page_count)?;
    validate::outline(&navigation.outline, page_count)?;
    articles::validate(&navigation.article_threads, page_count)?;

    let (catalog_num, catalog_generation, mut catalog) = catalog_dict(doc)?;
    let page_refs = page_refs(doc, &catalog)?;
    if page_refs.len() != page_count {
        return Err(invalid(format!(
            "page tree has {} pages but the builder tracks {page_count}",
            page_refs.len()
        )));
    }

    if !navigation.page_labels.is_empty() {
        catalog = catalog.insert(
            "PageLabels",
            Value::Dict(page_labels_dict(&navigation.page_labels)),
        );
    }

    if !navigation.named_destinations.is_empty() || !navigation.named_pages.is_empty() {
        // 与 assemble 阶段已写入的 `/Names` 类别（如 /EmbeddedFiles）合并，
        // 不覆盖既有字典。
        let mut names = catalog
            .get("Names")
            .and_then(Value::as_dict)
            .cloned()
            .unwrap_or_else(Dict::new);
        if !navigation.named_destinations.is_empty() {
            let entries = navigation
                .named_destinations
                .iter()
                .map(|(name, destination)| {
                    Ok((name.clone(), destination_value(destination, &page_refs)?))
                })
                .collect::<Result<Vec<_>, WriteError>>()?;
            names = names.insert("Dests", name_tree(entries));
        }
        if !navigation.named_pages.is_empty() {
            let entries = navigation
                .named_pages
                .iter()
                .map(|(name, page)| {
                    let (num, generation) = page_refs[*page];
                    (name.clone(), Value::Ref(num, generation))
                })
                .collect();
            names = names.insert("Pages", name_tree(entries));
        }
        catalog = catalog.insert("Names", Value::Dict(names));
    }

    if !navigation.outline.is_empty() {
        let mut next_num = doc
            .objects_for_write()
            .iter()
            .map(|object| object.num)
            .max()
            .unwrap_or(0)
            + 1;
        let context = OutlineContext {
            page_refs: &page_refs,
            actions: &navigation.actions,
            annotation_nums,
            ocg_nums,
        };
        let root = write_outline(doc, &navigation.outline, &context, &mut next_num)?;
        catalog = catalog.insert("Outlines", Value::Ref(root, 0));
    }

    if !navigation.article_threads.is_empty() {
        let threads =
            articles::write_threads(doc, version, &page_refs, &navigation.article_threads)?;
        catalog = catalog.insert("Threads", Value::Array(threads));
    }

    doc.set_object(Object::new(
        catalog_num,
        catalog_generation,
        Value::Dict(catalog),
    ));
    Ok(())
}

fn invalid(message: impl Into<String>) -> WriteError {
    WriteError::InvalidObject(message.into())
}

fn style_name(style: PageLabelStyle) -> &'static str {
    match style {
        PageLabelStyle::Decimal => "D",
        PageLabelStyle::UppercaseRoman => "R",
        PageLabelStyle::LowercaseRoman => "r",
        PageLabelStyle::UppercaseLetters => "A",
        PageLabelStyle::LowercaseLetters => "a",
    }
}

fn page_labels_dict(labels: &[PageLabelRange]) -> Dict {
    let mut nums = Array::new();
    for range in labels {
        let mut dict = Dict::new();
        if let Some(style) = range.style {
            dict = dict.insert("S", Value::Name(style_name(style).into()));
        }
        if !range.prefix.is_empty() {
            dict = dict.insert("P", text_string_value(&range.prefix));
        }
        if range.first_number != 1 {
            dict = dict.insert("St", Value::Integer(range.first_number as i64));
        }
        nums = nums
            .push(Value::Integer(range.start as i64))
            .push(Value::Dict(dict));
    }
    Dict::new().insert("Nums", Value::Array(nums))
}

/// 名称树叶子节点：键按 PDF 文本字符串编码后升序排列。
pub(super) fn name_tree(entries: Vec<(String, Value)>) -> Value {
    let mut entries = entries;
    entries.sort_by_cached_key(|(name, _)| PdfTextString::from(name.as_str()).encode());
    let mut names = Array::new();
    for (name, value) in entries {
        names = names.push(text_string_value(&name)).push(value);
    }
    Value::Dict(Dict::new().insert("Names", Value::Array(names)))
}

pub(super) fn destination_value(
    destination: &Destination,
    page_refs: &[(i64, i64)],
) -> Result<Value, WriteError> {
    let (num, generation) = page_refs[destination.page()];
    let mut array = Array::new().push(Value::Ref(num, generation));
    array = match destination {
        Destination::Xyz {
            left, top, zoom, ..
        } => array
            .push(Value::Name("XYZ".into()))
            .push(optional_number(*left))
            .push(optional_number(*top))
            .push(optional_number(*zoom)),
        Destination::Fit { .. } => array.push(Value::Name("Fit".into())),
        Destination::FitH { top, .. } => array
            .push(Value::Name("FitH".into()))
            .push(optional_number(*top)),
        Destination::FitV { left, .. } => array
            .push(Value::Name("FitV".into()))
            .push(optional_number(*left)),
        Destination::FitR { rect, .. } => array
            .push(Value::Name("FitR".into()))
            .push(Value::Float(rect[0]))
            .push(Value::Float(rect[1]))
            .push(Value::Float(rect[2]))
            .push(Value::Float(rect[3])),
        Destination::FitB { .. } => array.push(Value::Name("FitB".into())),
        Destination::FitBH { top, .. } => array
            .push(Value::Name("FitBH".into()))
            .push(optional_number(*top)),
        Destination::FitBV { left, .. } => array
            .push(Value::Name("FitBV".into()))
            .push(optional_number(*left)),
    };
    Ok(Value::Array(array))
}

fn optional_number(value: Option<f64>) -> Value {
    value.map_or(Value::Null, Value::Float)
}

fn outline_destination_value(
    destination: &OutlineDestination,
    page_refs: &[(i64, i64)],
) -> Result<Value, WriteError> {
    match destination {
        OutlineDestination::Explicit(destination) => destination_value(destination, page_refs),
        OutlineDestination::Named(name) => Ok(text_string_value(name)),
    }
}

/// 大纲写出期间共享的只读上下文；含 Hide 与 Set-OCG-State 动作所需的
/// 注解/可选内容组对象号映射。
struct OutlineContext<'a> {
    page_refs: &'a [(i64, i64)],
    actions: &'a [Action],
    annotation_nums: &'a [i64],
    ocg_nums: &'a [i64],
}

fn write_outline(
    doc: &mut Document,
    entries: &[OutlineEntry],
    context: &OutlineContext<'_>,
    next_num: &mut i64,
) -> Result<i64, WriteError> {
    let root_num = *next_num;
    *next_num += 1;
    let mut action_refs = Vec::new();
    let children =
        write_outline_items(doc, entries, root_num, context, &mut action_refs, next_num)?;
    let mut dict = Dict::new().insert("Type", Value::Name("Outlines".into()));
    if let Some((first, last)) = children {
        let visible: u64 = entries
            .iter()
            .map(|item| {
                1 + if item.expanded {
                    visible_descendants(item)
                } else {
                    0
                }
            })
            .sum();
        dict = dict
            .insert("First", Value::Ref(first, 0))
            .insert("Last", Value::Ref(last, 0))
            .insert("Count", Value::Integer(visible as i64));
    }
    doc.set_object(Object::new(root_num, 0, Value::Dict(dict)));
    Ok(root_num)
}

fn write_outline_items(
    doc: &mut Document,
    items: &[OutlineEntry],
    parent_num: i64,
    context: &OutlineContext<'_>,
    action_refs: &mut Vec<i64>,
    next_num: &mut i64,
) -> Result<Option<(i64, i64)>, WriteError> {
    let page_refs = context.page_refs;
    let actions = context.actions;
    let nums: Vec<i64> = items
        .iter()
        .map(|_| {
            let num = *next_num;
            *next_num += 1;
            num
        })
        .collect();
    for (index, item) in items.iter().enumerate() {
        let num = nums[index];
        let children =
            write_outline_items(doc, &item.children, num, context, action_refs, next_num)?;
        let mut dict = Dict::new()
            .insert("Title", text_string_value(&item.title))
            .insert("Parent", Value::Ref(parent_num, 0));
        if let Some(action_handle) = item.action {
            if action_handle < actions.len() {
                if action_refs.len() <= action_handle {
                    action_refs.resize(action_handle + 1, 0);
                }
                if action_refs[action_handle] == 0 {
                    let action_num = *next_num;
                    *next_num += 1;
                    let action_dict = actions[action_handle]
                        .to_dict(context.annotation_nums, context.ocg_nums)?;
                    doc.set_object(Object::new(action_num, 0, Value::Dict(action_dict)));
                    action_refs[action_handle] = action_num;
                }
                dict = dict.insert("A", Value::Ref(action_refs[action_handle], 0));
            }
        } else if let Some(destination) = &item.destination {
            dict = dict.insert("Dest", outline_destination_value(destination, page_refs)?);
        }
        if let Some(color) = item.color {
            let mut array = Array::new();
            for component in color {
                array = array.push(Value::Float(component));
            }
            dict = dict.insert("C", Value::Array(array));
        }
        if item.flags != 0 {
            dict = dict.insert("F", Value::Integer(i64::from(item.flags)));
        }
        if let Some((first, last)) = children {
            dict = dict
                .insert("First", Value::Ref(first, 0))
                .insert("Last", Value::Ref(last, 0))
                .insert("Count", Value::Integer(outline_count(item)));
        }
        if index > 0 {
            dict = dict.insert("Prev", Value::Ref(nums[index - 1], 0));
        }
        if index + 1 < nums.len() {
            dict = dict.insert("Next", Value::Ref(nums[index + 1], 0));
        }
        doc.set_object(Object::new(num, 0, Value::Dict(dict)));
    }
    Ok(nums
        .first()
        .zip(nums.last())
        .map(|(first, last)| (*first, *last)))
}

fn outline_count(item: &OutlineEntry) -> i64 {
    if item.expanded {
        visible_descendants(item) as i64
    } else {
        -(total_descendants(item) as i64)
    }
}

fn visible_descendants(item: &OutlineEntry) -> u64 {
    item.children
        .iter()
        .map(|child| {
            1 + if child.expanded {
                visible_descendants(child)
            } else {
                0
            }
        })
        .sum()
}

fn total_descendants(item: &OutlineEntry) -> u64 {
    item.children
        .iter()
        .map(|child| 1 + total_descendants(child))
        .sum()
}

/// 目录对象号、代数与字典副本。
fn catalog_dict(doc: &Document) -> Result<(i64, i64, Dict), WriteError> {
    let (num, generation) = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .ok_or_else(|| invalid("document trailer /Root is not an indirect reference"))?;
    let objects = doc.objects_for_write();
    let object = objects
        .iter()
        .find(|object| object.num == num && object.generation == generation)
        .ok_or_else(|| invalid(format!("catalog object {num} {generation} is missing")))?;
    let dict = object
        .value
        .as_dict()
        .cloned()
        .ok_or_else(|| invalid("catalog object is not a dictionary"))?;
    Ok((num, generation, dict))
}

/// 页面树根 `/Kids` 中的间接页引用（构建器只产生扁平树）。
fn page_refs(doc: &Document, catalog: &Dict) -> Result<Vec<(i64, i64)>, WriteError> {
    let (pages_num, pages_generation) = catalog
        .get("Pages")
        .and_then(Value::as_ref)
        .ok_or_else(|| invalid("catalog /Pages is not an indirect reference"))?;
    let objects = doc.objects_for_write();
    let pages = objects
        .iter()
        .find(|object| object.num == pages_num && object.generation == pages_generation)
        .ok_or_else(|| invalid("page tree root object is missing"))?;
    let pages = pages
        .value
        .as_dict()
        .ok_or_else(|| invalid("page tree root is not a dictionary"))?;
    let kids = pages
        .get("Kids")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("page tree /Kids is not an array"))?;
    kids.iter()
        .map(|kid| {
            kid.as_ref()
                .ok_or_else(|| invalid("page tree kid is not an indirect reference"))
        })
        .collect()
}

fn outline_has_actions(entries: &[OutlineEntry]) -> bool {
    entries
        .iter()
        .any(|entry| entry.action.is_some() || outline_has_actions(&entry.children))
}
