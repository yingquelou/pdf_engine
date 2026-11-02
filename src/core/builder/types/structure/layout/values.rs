//! Layout 条目的取值序列化辅助（ISO 32000-1 14.8.5.4 Table 342–346）：
//! 名字、RGB 颜色、四数字数组、边框单值/four-entry 形态与列数组到 `Value`
//! 的生成；未设置的条目由 `dict` 模块直接跳过，规范缺省值不物化。

use crate::core::types::{Array, Value};

use super::forms::{
    BorderColor, BorderStyles, BorderThickness, NumberOrAuto, NumberOrFour, NumberOrNumbers,
};

pub(super) fn name_value(name: &str) -> Value {
    Value::Name(name.to_owned())
}

pub(super) fn color_value(color: [f64; 3]) -> Value {
    let mut array = Array::new();
    for component in color {
        array = array.push(Value::Float(component));
    }
    Value::Array(array)
}

pub(super) fn four_numbers(numbers: [f64; 4]) -> Value {
    let mut array = Array::new();
    for number in numbers {
        array = array.push(Value::Float(number));
    }
    Value::Array(array)
}

pub(super) fn border_color_value(color: &BorderColor) -> Value {
    match color {
        BorderColor::Uniform(color) => color_value(*color),
        BorderColor::Edges(edges) => {
            let mut array = Array::new();
            for edge in edges {
                array = array.push(edge.map_or(Value::Null, color_value));
            }
            Value::Array(array)
        }
    }
}

pub(super) fn border_styles_value(style: &BorderStyles) -> Value {
    match style {
        BorderStyles::Uniform(style) => name_value(style.name()),
        BorderStyles::Edges(edges) => {
            let mut array = Array::new();
            for edge in edges {
                array = array.push(
                    edge.as_ref()
                        .map_or(Value::Null, |style| name_value(style.name())),
                );
            }
            Value::Array(array)
        }
    }
}

pub(super) fn border_thickness_value(thickness: &BorderThickness) -> Value {
    match thickness {
        BorderThickness::Uniform(thickness) => Value::Float(*thickness),
        BorderThickness::Edges(edges) => {
            let mut array = Array::new();
            for edge in edges {
                array = array.push(edge.map_or(Value::Null, Value::Float));
            }
            Value::Array(array)
        }
    }
}

pub(super) fn number_or_four_value(padding: &NumberOrFour) -> Value {
    match padding {
        NumberOrFour::Uniform(number) => Value::Float(*number),
        NumberOrFour::Edges(edges) => four_numbers(*edges),
    }
}

pub(super) fn number_or_auto_value(size: &NumberOrAuto) -> Value {
    match size {
        NumberOrAuto::Number(number) => Value::Float(*number),
        other => name_value(other.name()),
    }
}

pub(super) fn number_or_numbers_value(value: &NumberOrNumbers) -> Value {
    match value {
        NumberOrNumbers::Number(number) => Value::Float(*number),
        NumberOrNumbers::Array(numbers) => {
            let mut array = Array::new();
            for number in numbers {
                array = array.push(Value::Float(*number));
            }
            Value::Array(array)
        }
    }
}
