use std::collections::BTreeMap;

use crate::core::types::Value;

use super::element::StructureElement;

/// 结构树（14.7.2 Table 322）：顶层结构元素 + 根映射。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructureTree {
    /// `/RoleMap`：结构类型 → 标准类型的近似映射；空时不写出。
    pub role_map: BTreeMap<String, String>,
    /// `/ClassMap`：类名 → 属性对象或属性对象数组（14.7.5.2）；空时不
    /// 写出。数组形式把多个属性对象组合成一个属性类。
    pub class_map: BTreeMap<String, Value>,
    /// `/K`：顶层结构元素，按文档顺序。
    pub elements: Vec<StructureElement>,
}

impl StructureTree {
    /// 空结构树：只写出空的 `/StructTreeRoot`。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一条 `/RoleMap` 映射。
    pub fn with_role(
        mut self,
        structure_type: impl Into<String>,
        standard_type: impl Into<String>,
    ) -> Self {
        self.role_map
            .insert(structure_type.into(), standard_type.into());
        self
    }

    /// 追加一条 `/ClassMap` 属性类。
    ///
    /// `attributes` 为属性对象字典/流，或 `Value::Array` 形式的属性对象
    /// 数组（14.7.5.2）。每个属性对象须带合法非空 `/O`。
    pub fn with_class(mut self, name: impl Into<String>, attributes: Value) -> Self {
        self.class_map.insert(name.into(), attributes);
        self
    }

    /// 追加一个顶层结构元素。
    pub fn with_element(mut self, element: StructureElement) -> Self {
        self.elements.push(element);
        self
    }
}

/// 目录 `/MarkInfo` 标记信息（14.7.1 Table 321，PDF 1.4）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StructureMarkInfo {
    /// `/Marked`：文档是否符合标记内容约定（缺省 false，写出时显式声明）。
    pub marked: bool,
    /// `/UserProperties`：是否含用户属性（PDF 1.6，true 时写出）。
    pub user_properties: bool,
    /// `/Suspects`：是否含可疑标记内容（PDF 1.6，true 时写出）。
    pub suspects: bool,
}

impl StructureMarkInfo {
    /// 全部标志 false 的标记信息。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 `/Marked`。
    pub fn with_marked(mut self, marked: bool) -> Self {
        self.marked = marked;
        self
    }

    /// 设置 `/UserProperties`（要求 PDF 1.6）。
    pub fn with_user_properties(mut self, user_properties: bool) -> Self {
        self.user_properties = user_properties;
        self
    }

    /// 设置 `/Suspects`（要求 PDF 1.6）。
    pub fn with_suspects(mut self, suspects: bool) -> Self {
        self.suspects = suspects;
        self
    }
}
