//! AcroForm 写端测试的跨主题共享夹具：目录/字段字典取用、文本串条目断言与
//! 字段级构建失败断言。仅服务单一主题的夹具（`page_annots`、`form_build_error`、
//! `form_with_fields_error`、`calculate_field`）随主题留在对应子文件。

use super::super::*;
use crate::core::writer::WriteError;

pub(super) fn assert_text_entry(dict: &Dict, key: &str) {
    match dict.get(key) {
        Some(Value::HexString(_)) | Some(Value::String(_)) => {}
        other => panic!("expected text string at {key}, got {other:?}"),
    }
}

pub(super) fn object_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("object lookup")
        .expect("object exists")
        .value
        .as_dict()
        .expect("object is a dictionary")
        .clone()
}

pub(super) fn catalog(doc: &Document) -> Dict {
    object_dict(doc, 1)
}

/// 构造单页文档并注册一个字段树；返回组装后的文档。
pub(super) fn build_error(field: FormField, page: usize) -> WriteError {
    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, field);
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("invalid form must be rejected");
    assert!(out.is_empty(), "failed build must not write bytes");
    error
}

pub(super) fn acro_dict(doc: &Document) -> Dict {
    catalog(doc)
        .get("AcroForm")
        .and_then(|value| value.as_dict())
        .expect("/AcroForm")
        .clone()
}
