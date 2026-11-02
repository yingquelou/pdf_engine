//! 子类型专有条目读取（12.5.6）。
//!
//! 按主题目录化拆分：`text_markup` 覆盖 Text/Caret/Popup/Ink 与跨类
//! 类型化错误；`line` 覆盖 Line 的 Table 175/176 条目；`free_text` 覆盖
//! FreeText 的 Table 174 条目；`stamp_quad` 覆盖 Rubber Stamp（Table 181）
//! 与文本标记 `/QuadPoints`（Table 179）。共享夹具 `quad_array` 留在此处，
//! 断言、夹具与注释自原单文件逐行搬运，测试叶名零改动。

mod free_text;
mod line;
mod stamp_quad;
mod text_markup;

use crate::core::types::{Array, Value};

/// 构造 `/QuadPoints` 数组（整数值即可）。
fn quad_array(values: &[i64]) -> Value {
    Value::Array(Array(values.iter().map(|v| Value::Integer(*v)).collect()))
}
