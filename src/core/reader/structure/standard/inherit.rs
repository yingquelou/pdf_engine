//! 可继承标准属性的继承链解析（ISO 32000-1 14.8.5.3）。
//!
//! 14.8.5.3 规定一个属性的取值优先级：元素自身 `/A` 中的属性对象优先于其
//! `/C` 类（`/ClassMap`）属性，后者又优先于最近的声明该可继承条目的
//! 祖先的属性。前两级已由 [`StructureTree::resolved_attributes`] 合并；
//! 本模块补上祖先一级：
//!
//! - [`StructureTree::effective_attributes`] 沿 `parent` 链由近及远，把祖先
//!   属性字典中**可继承**的条目复制进当前结果里尚未出现的键位；
//! - [`StructureTree::effective_standard_attributes`] 把结果交给标准属性
//!   解析器（与 [`StructureTree::standard_attributes`] 共用同一实现）。
//!
//! 可继承条目集合（Table 342 与 14.8.5.5–14.8.5.7 正文核对）：
//!
//! - `Layout`：[`LAYOUT_INHERITABLE`]（WritingMode、BorderColor、
//!   BorderThickness、Color、StartIndent、EndIndent、TextIndent、TextAlign、
//!   BlockAlign、InlineAlign、TBorderStyle、TPadding、LineHeight、
//!   TextDecorationColor、TextDecorationThickness、RubyAlign、RubyPosition、
//!   GlyphOrientationVertical）；
//! - `List`：[`LIST_INHERITABLE`]（ListNumbering）；
//! - `PrintField`、`Table`：没有可继承条目（Role、checked、Desc、RowSpan、
//!   ColSpan、Headers、Scope、Summary 均不继承）。
//!
//! 写端不参与：写端只写显式值，继承是读端绑定。
//!
//! 已知缺口（14.8.5.3 未完全覆盖）：
//!
//! - 格式专用所有者（`XML-1.00`、`HTML-3.20`、`OEB-1.00`、`RTF-1.05`、
//!   `CSS2` 等）与未知所有者不参与继承，其专用条目集合未建模；
//! - 流形态的属性对象（Table 327 允许）没有可读的键集合：既不被继承，
//!   也不接收继承键（整体跳过）；元素自身为流形态时同样整体优先；
//! - 规范缺省值（14.8.5.3 第一级）不物化：类型化解析器保持 `None`，调用方
//!   需要时自行套用；
//! - 祖先链超过 [`MAX_INHERIT_DEPTH`] 层或成环时静默停止（解析器已按
//!   [`MAX_STRUCTURE_DEPTH`] 限制链长并拒绝成环，此处仅为防御）。

use std::collections::BTreeSet;

use crate::core::types::{Dict, Value};

use super::super::MAX_STRUCTURE_DEPTH;
use super::{
    StandardAttributes, StructureAttribute, StructureError, StructureTree, parse_attributes,
};

/// 继承链最大深度：与结构树展开深度上限一致。
const MAX_INHERIT_DEPTH: usize = MAX_STRUCTURE_DEPTH;

/// `Layout` 可继承条目（Table 342；14.8.5.4 正文）。
const LAYOUT_INHERITABLE: &[&str] = &[
    "WritingMode",
    "BorderColor",
    "BorderThickness",
    "Color",
    "StartIndent",
    "EndIndent",
    "TextIndent",
    "TextAlign",
    "BlockAlign",
    "InlineAlign",
    "TBorderStyle",
    "TPadding",
    "LineHeight",
    "TextDecorationColor",
    "TextDecorationThickness",
    "RubyAlign",
    "RubyPosition",
    "GlyphOrientationVertical",
];

/// `List` 可继承条目（Table 347）。
const LIST_INHERITABLE: &[&str] = &["ListNumbering"];

impl StructureTree {
    /// 结构元素实际生效的属性对象（14.8.5.3）。
    ///
    /// 起点是 [`StructureTree::resolved_attributes`]（元素自身 `/A` 覆盖
    /// `/ClassMap` 类属性），再沿 `parent` 链由近及远补入祖先的**可继承**
    /// 条目：同一所有者下当前结果尚不存在的键才复制，因此自身 `/A` >
    /// 类属性 > 最近祖先的优先级成立。非可继承条目与已存在的值不受影响。
    ///
    /// 元素自身没有某所有者的属性对象时，祖先的可继承条目会新建一个只含
    /// 这些键的属性对象（`revision` 取来源祖先的属性对象），并按最近祖先
    /// 优先的顺序追加在自身所有者之后。越界下标返回空 `Vec`。
    ///
    /// 流形态的属性对象没有可读键集合，整体跳过（见模块文档缺口）。
    pub fn effective_attributes(&self, index: usize) -> Vec<StructureAttribute> {
        let mut effective = self.resolved_attributes(index);
        if self.element(index).is_none() {
            return effective;
        }
        let mut visited = BTreeSet::new();
        visited.insert(index);
        let mut current = index;
        for _ in 0..MAX_INHERIT_DEPTH {
            let Some(parent) = self.element(current).and_then(|element| element.parent) else {
                break;
            };
            // 防御性终止：arena 由解析器按前序构建，正常不会成环。
            if !visited.insert(parent) {
                break;
            }
            for attribute in self.resolved_attributes(parent) {
                inherit_attribute(&mut effective, attribute);
            }
            current = parent;
        }
        effective
    }

    /// 结构元素实际生效的标准属性（14.8.5.3 继承 +
    /// 14.8.5.2、14.8.5.4–14.8.5.7 类型化解析）。
    ///
    /// 对 [`StructureTree::effective_attributes`] 的结果做与
    /// [`StructureTree::standard_attributes`] 相同的解析（错误消息也一致）。
    /// 越界下标返回全空 [`StandardAttributes`]。
    pub fn effective_standard_attributes(
        &self,
        index: usize,
    ) -> Result<StandardAttributes, StructureError> {
        parse_attributes(&self.effective_attributes(index), index)
    }
}

/// 把一个祖先的属性对象并入累积结果：只复制给定所有者下可继承且尚未
/// 出现的键。
fn inherit_attribute(effective: &mut Vec<StructureAttribute>, source: StructureAttribute) {
    let Some(keys) = inheritable_keys(&source.owner) else {
        return;
    };
    // 流形态没有可读键集合；元素自身的流形态同样不接收继承键。
    let Value::Dict(source_dict) = &source.value else {
        return;
    };
    if let Some(existing) = effective
        .iter_mut()
        .find(|attribute| attribute.owner == source.owner)
    {
        let Value::Dict(target) = &mut existing.value else {
            return;
        };
        for key in keys {
            if target.get(key).is_none()
                && let Some(value) = source_dict.get(key)
            {
                target.0.insert((*key).to_owned(), value.clone());
            }
        }
        return;
    }
    let mut inherited = Dict::new();
    for key in keys {
        if let Some(value) = source_dict.get(key) {
            inherited = inherited.insert((*key).to_owned(), value.clone());
        }
    }
    if inherited.0.is_empty() {
        return;
    }
    effective.push(StructureAttribute {
        owner: source.owner,
        value: Value::Dict(inherited),
        revision: source.revision,
        user_properties: None,
    });
}

/// 所有者对应的可继承条目集合；没有可继承条目时返回 `None`。
fn inheritable_keys(owner: &str) -> Option<&'static [&'static str]> {
    match owner {
        "Layout" => Some(LAYOUT_INHERITABLE),
        "List" => Some(LIST_INHERITABLE),
        _ => None,
    }
}
