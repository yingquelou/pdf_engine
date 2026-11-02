//! 大纲（书签）写端（ISO 32000-1 12.3.4）：条目构建与 `/Outlines` 树写出。
//!
//! 大纲条目在 [`PdfBuilder::build`] 的导航后处理阶段从现有最大对象号之后
//! 分配；`/Dest` 与 `/A` 二选一，两者均设时优先写 `/A`（PDF 1.2+）。

use crate::core::reader::navigation::OutlineDestination;
use crate::core::types::{Array, Dict, Document, Object, Value};
use crate::core::writer::WriteError;

use super::super::types::Action;
use super::super::types::ActionRefs;
use super::super::types::text_string_value;
use super::PdfBuilder;
use super::names::destination_value;

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

impl PdfBuilder {
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

    /// 注册一个动作字典（12.6），返回句柄供大纲条目或注解引用。
    ///
    /// 动作字典作为间接对象写出；大纲条目的 `/A` 条目要求 PDF 1.2+。
    pub fn action(&mut self, action: Action) -> usize {
        let handle = self.navigation.actions.len();
        self.navigation.actions.push(action);
        handle
    }
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

/// 大纲写出期间共享的只读上下文；含 Hide、Set-OCG-State 与 Rendition
/// 动作所需的注解/可选内容组/表单/嵌入文件对象号映射。
pub(super) struct OutlineContext<'a> {
    pub(super) page_refs: &'a [(i64, i64)],
    pub(super) actions: &'a [Action],
    pub(super) action_refs: ActionRefs<'a>,
}

impl OutlineContext<'_> {
    fn action_refs(&self) -> ActionRefs<'_> {
        self.action_refs
    }
}

pub(super) fn write_outline(
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
                    let action_dict = actions[action_handle].to_dict(&context.action_refs())?;
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

pub(super) fn outline_has_actions(entries: &[OutlineEntry]) -> bool {
    entries
        .iter()
        .any(|entry| entry.action.is_some() || outline_has_actions(&entry.children))
}
