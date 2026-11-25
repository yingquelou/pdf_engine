//! 7.7.3.3 页面 `/Contents`：单流（默认）与流数组（追加内容流）的写→读往返。

use super::*;
use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};
use pdf_engine::core::content::{ContentStream, MarkedContentProperties};
use pdf_engine::core::reader::content::Op as ReadOp;

#[test]
fn default_page_writes_a_single_contents_stream() {
    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let page = super::first_page_dict(&doc);
    assert!(
        matches!(page.get("Contents"), Some(Value::Ref(_, 0))),
        "无追加内容流时 /Contents 仍是单引用: {page:?}"
    );
}

#[test]
fn appended_content_streams_write_a_contents_array_in_order() {
    let mut first = ContentStream::new();
    first
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("extra-1")
        .end_text();
    let mut second = ContentStream::new();
    second
        .begin_text()
        .set_font("F1", 12.0)
        .show_text("extra-2")
        .end_text();

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "main");
    builder.append_content_stream(page, first);
    builder.append_content_stream(page, second);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let page_dict = super::first_page_dict(&doc);
    let array = page_dict
        .get("Contents")
        .and_then(Value::as_array)
        .expect("/Contents 应为流数组");
    assert_eq!(array.len(), 3, "主内容流 + 2 追加流");
    for item in array.iter() {
        assert!(matches!(item, Value::Ref(_, 0)), "{item:?}");
    }

    let shown: Vec<String> = doc
        .page_content_ops(0)
        .expect("page content ops")
        .iter()
        .filter_map(|op| match op {
            ReadOp::ShowText(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(shown, ["main", "extra-1", "extra-2"], "按序连接");
}

#[test]
fn appended_content_stream_unknown_page_is_ignored() {
    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    builder.append_content_stream(7, ContentStream::new());
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let page = super::first_page_dict(&doc);
    assert!(matches!(page.get("Contents"), Some(Value::Ref(_, 0))));
}

#[test]
fn property_handle_in_appended_stream_is_registered_on_the_page() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let handle = builder
        .add_marked_content_properties(Dict::new().insert("ActualText", Value::String("extra".into())));
    let mut extra = ContentStream::new();
    extra
        .begin_marked_content_properties("Span", MarkedContentProperties::pending_handle(handle))
        .end_marked_content();
    builder.append_content_stream(page, extra);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let text = String::from_utf8_lossy(&buf).into_owned();
    assert!(text.contains("/Span /MC1 BDC"), "{text}");

    let doc = parse_buf(&buf);
    let resource = doc
        .page_resource(page, "Properties", "MC1")
        .expect("resource lookup")
        .expect("MC1 on page");
    let Value::Ref(object, 0) = resource else {
        panic!("MC1 must be a generation-0 ref: {resource:?}");
    };
    assert_eq!(
        doc.get_object(object).expect("lookup").expect("object").value,
        Value::Dict(Dict::new().insert("ActualText", Value::String("extra".into())))
    );
}
