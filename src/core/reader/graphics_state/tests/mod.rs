//! Graphics state (ExtGState) reader model tests.
//!
//! 按主题目录化拆分（与 `reader/shading/tests/`、`reader/image/tests/` 同形）：
//! - `parsing`：Table 58 标量/数组条目的解析与保留语义；
//! - `resources`：`Document::ext_g_state(s)` 资源字典入口与非字典拒绝；
//! - `validation`：范围、类型与畸形条目的拒绝路径；
//! - `soft_mask`：Table 144 软遮罩字典的解析与拒绝；
//! - `table_conformance`：Table 58/89/144/146 与 10.6.2 核对结论的
//!   characterization 测试。
//!
//! 跨主题夹具（文档构造、字典构造与错误断言）留在本文件。
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

use super::{
    BlendMode, DashPattern, ExtGStateFont, GraphicsStateError, HybridColorSpace,
    HybridColorSpaceElement, HybridColorSpaceStyle, SoftMaskSubtype, parse_ext_g_state,
};

fn document_with_objects(objects: Vec<(i64, Value)>) -> Document {
    let document = Document::new();
    for (num, value) in objects {
        document.set_object(Object::new(num, 0, value));
    }
    document
}

fn state(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

fn assert_invalid(document: &Document, entries: Vec<(&str, Value)>, needle: &str) {
    let dict = state(entries);
    match parse_ext_g_state(document, &dict) {
        Err(GraphicsStateError::Invalid(message)) => assert!(
            message.contains(needle),
            "error {message:?} should mention {needle}"
        ),
        other => panic!("expected an invalid graphics state, got {other:?}"),
    }
}

fn xobject_stream(subtype: Option<&str>) -> Value {
    let mut dict = Dict::new().insert("Type", Value::Name("XObject".into()));
    if let Some(subtype) = subtype {
        dict = dict.insert("Subtype", Value::Name(subtype.into()));
    }
    Value::Stream(dict, StreamData::Owned(Vec::new()))
}

mod blend_mode;
mod hybrid_color_space;
mod parsing;
mod resources;
mod soft_mask;
mod table_conformance;
mod validation;
