//! 跨主题共享夹具：对象号常量、字典/数组构造器、语言解析、标记内容区段
//! 装配、结构树根/元素构造与 `assert_malformed` 期望值辅助。
//! 仅服务单一主题的夹具（如 `document_with_root`、`structured_page_document`）
//! 随主题留在对应子文件。

use crate::core::reader::accessibility::{LanguageError, LanguageIdentifier};
use crate::core::reader::content::MarkedSection;
use crate::core::types::{Array, Dict, Document, Object, Value};

pub(super) const CATALOG: i64 = 1;
pub(super) const ROOT: i64 = 2;

pub(super) fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

pub(super) fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

pub(super) fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

pub(super) fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

pub(super) fn language(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

pub(super) fn marked_section(
    tag: &str,
    properties: Option<Dict>,
    parent: Option<usize>,
) -> MarkedSection {
    MarkedSection {
        tag: tag.to_owned(),
        properties,
        visibility: None,
        start: 0,
        end: 0,
        point: false,
        parent,
    }
}

/// 多语言文本数组：按 `(identifier, text)` 顺序展开。
pub(super) fn multi_language(pairs: &[(&str, &str)]) -> Value {
    let mut values = Vec::with_capacity(pairs.len() * 2);
    for (identifier, text_value) in pairs {
        values.push(text(identifier));
        values.push(text(text_value));
    }
    arr(values)
}

pub(super) fn element(structure_type: &str, parent: i64) -> Dict {
    Dict::new()
        .insert("Type", name("StructElem"))
        .insert("S", name(structure_type))
        .insert("P", Value::Ref(parent, 0))
}

pub(super) fn struct_root(entries: Dict) -> Dict {
    entries.insert("Type", name("StructTreeRoot"))
}

pub(super) fn assert_malformed<T: std::fmt::Debug>(result: Result<T, LanguageError>, needle: &str) {
    match result {
        Err(LanguageError::Malformed(message)) => {
            assert!(
                message.contains(needle),
                "expected {needle:?}, got {message}"
            );
        }
        other => panic!("expected malformed error containing {needle:?}, got {other:?}"),
    }
}
