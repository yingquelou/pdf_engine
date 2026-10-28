//! builder 单元测试共享夹具：注解字典取用与文本条目断言。

use crate::core::types::{Dict, Document, Value};

/// 取第 `page_num` 页 `/Annots[index]` 指向的注解字典。
pub(super) fn annot_object(doc: &Document, page_num: i64, index: usize) -> Dict {
    annot_object_with_num(doc, page_num, index).1
}

/// 同上，一并返回注解的间接对象号。
pub(super) fn annot_object_with_num(doc: &Document, page_num: i64, index: usize) -> (i64, Dict) {
    let page = doc
        .get_object(page_num)
        .expect("page object")
        .expect("page exists")
        .value;
    let page_dict = page.as_dict().expect("page dict");
    let annots = page_dict
        .get("Annots")
        .and_then(|v| v.as_array())
        .expect("page /Annots array");
    let annot_ref = annots.0[index]
        .as_ref()
        .expect("/Annots entry should be an indirect reference");
    let num = annot_ref.0;
    let object = doc
        .get_object(num)
        .expect("annotation object")
        .expect("exist");
    (
        num,
        object.value.as_dict().expect("annotation dict").clone(),
    )
}

/// 文本条目一律按 `text_string_value` 写成十六进制串。
pub(super) fn assert_text_entry(dict: &Dict, key: &str) {
    match dict.get(key) {
        Some(Value::HexString(_)) | Some(Value::String(_)) => {}
        other => panic!("expected text string at {key}, got {other:?}"),
    }
}
