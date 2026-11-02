//! 图形状态参数（ExtGState）资源与内容操作符测试。
//!
//! 按主题目录化拆分（与 `tests/cid/`、`tests/structure/`、`tests/shading/` 的
//! 目录化形态一致）：
//! - `resources`：全标量参数字典写出、混合模式兜底数组、`/Font` 条目、同名
//!   状态资源复用、跨页对象共享与未使用不写出；
//! - `validation`：非法状态参数构建期拒绝且零字节输出、PDF 1.3/1.4 透明度
//!   门禁与透明度仅经 `gs` 应用（内容流禁止内联）的契约；
//! - `soft_mask`：`/SMask` 字典（`S` `/BC` `/G` `/TR` `/SMaskInData` `/Matte`）
//!   写出、透明组表单引用与登记、参数校验与 1.4/1.6 版本门禁；
//! - `black_generation`：`/BG` `/BG2` `/UCR` `/UCR2` 函数共享句柄单次写出、
//!   单输出契约与未知句柄拒绝；
//! - `transfer`：`/TR2` 传递函数扩展字典逐字写出与字典形态校验；
//! - `halftone`：`/HT` 半色调字典（Type 1/Type 2 色料）写出、函数句柄共享
//!   解析内联与参数校验。
//!
//! 原 `graphics_state.rs` 中被多个主题引用的夹具集中保留在本文件（可见性
//! 放宽到 `pub(super)`），仅服务单一主题的夹具（`mask_form`、
//! `four_input_function`）随主题留在对应子文件。

mod black_generation;
mod halftone;
mod resources;
mod soft_mask;
mod transfer;
mod validation;

use super::*;

pub(super) fn page_dict(doc: &Document, index: i64) -> Dict {
    let page_num = 3 + index * 2;
    let object = doc
        .get_object(page_num)
        .expect("page lookup")
        .expect("page object");
    object
        .value
        .as_dict()
        .expect("page object should be a dictionary")
        .clone()
}

pub(super) fn content_stream(doc: &Document, index: i64) -> String {
    let content_num = 4 + index * 2;
    let object = doc
        .get_object(content_num)
        .expect("content lookup")
        .expect("content object");
    let bytes = doc.stream_bytes(&object.value).expect("content bytes");
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(super) fn resource_entries(doc: &Document, index: i64, kind: &str) -> Dict {
    page_dict(doc, index)
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get(kind))
        .and_then(Value::as_dict)
        .cloned()
        .unwrap_or_else(|| panic!("page should have a /{kind} resource dictionary"))
}

pub(super) fn ext_g_state(doc: &Document, index: i64, name: &str) -> (i64, Dict) {
    let resources = resource_entries(doc, index, "ExtGState");
    let num = resources
        .get(name)
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .unwrap_or_else(|| panic!("missing /{name} graphics state"));
    let object = doc
        .get_object(num)
        .expect("graphics state lookup")
        .expect("graphics state object");
    let dict = object
        .value
        .as_dict()
        .expect("graphics state should be a dictionary")
        .clone();
    (num, dict)
}

pub(super) fn as_number(value: &Value) -> f64 {
    match value {
        Value::Integer(number) => *number as f64,
        Value::Float(number) => *number,
        other => panic!("expected a number, got {other:?}"),
    }
}

pub(super) fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        _ => None,
    }
}

pub(super) fn one_output_function() -> Function {
    Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0))
}

pub(super) fn assert_build_error(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("构建应失败");
    let message = error.to_string();
    assert!(
        message.contains(expected),
        "error {message:?} should mention {expected:?}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}
