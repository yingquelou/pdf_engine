use crate::core::types::{Array, Dict, Value};

use super::super::text_string_value;

/// 结构属性对象（14.7.5.2 Table 327）。
///
/// `value` 直接作为属性对象写出：字典内联，流提升为间接对象后写引用。
#[derive(Debug, Clone, PartialEq)]
pub struct StructureAttribute {
    /// 属性对象（字典或流，`/O` 为必填非空名字）。
    pub value: Value,
    /// 元素 `/A` 数组中的修订号；`None` 表示缺省 0 且不写出。
    pub revision: Option<u32>,
}

impl StructureAttribute {
    /// 由属性对象字典/流构造（无修订号）。
    pub fn new(value: Value) -> Self {
        Self {
            value,
            revision: None,
        }
    }

    /// 设置数组 `/A` 中的修订号。
    pub fn with_revision(mut self, revision: u32) -> Self {
        self.revision = Some(revision);
        self
    }

    /// 用户属性属性对象（14.7.5.3 Table 328）。
    ///
    /// 生成 `/O /UserProperties` + `/P` 属性数组，顺序与传入一致；
    /// 要求 PDF 1.6，且文档 `/MarkInfo /UserProperties` 为 true。
    pub fn user_properties(properties: Vec<UserProperty>) -> Self {
        let mut array = Array::new();
        for property in properties {
            array = array.push(Value::Dict(property.into_dict()));
        }
        Self::new(Value::Dict(
            Dict::new()
                .insert("O", Value::Name("UserProperties".into()))
                .insert("P", Value::Array(array)),
        ))
    }
}

/// 用户属性（14.7.5.3 Table 329）。
#[derive(Debug, Clone, PartialEq)]
pub struct UserProperty {
    /// `/N`：属性名（文本串，必填非空）。
    pub name: String,
    /// `/V`：属性值（任意值，必填）。
    pub value: Value,
    /// `/F`：属性值的格式化表示（可选文本串）。
    pub formatted: Option<String>,
    /// `/H`：是否隐藏（缺省 false；true 时写出 `/H true`）。
    pub hidden: bool,
}

impl UserProperty {
    /// 新建用户属性：`/N` + `/V`。
    pub fn new(name: impl Into<String>, value: Value) -> Self {
        Self {
            name: name.into(),
            value,
            formatted: None,
            hidden: false,
        }
    }

    /// 设置 `/F` 格式化表示。
    pub fn with_formatted(mut self, formatted: impl Into<String>) -> Self {
        self.formatted = Some(formatted.into());
        self
    }

    /// 设置 `/H` 隐藏标志。
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// 用户属性字典（Table 329）。
    fn into_dict(self) -> Dict {
        let mut dict = Dict::new()
            .insert("N", text_string_value(&self.name))
            .insert("V", self.value);
        if let Some(formatted) = &self.formatted {
            dict = dict.insert("F", text_string_value(formatted));
        }
        if self.hidden {
            dict = dict.insert("H", Value::Bool(true));
        }
        dict
    }
}
