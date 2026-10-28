//! 注解写端测试：`/Annots` 间接对象、通用条目与目标/弹引引用解析
//! （ISO 32000-1 12.5.1–12.5.5）。

use super::*;
use crate::core::content::ContentStream;
use crate::core::reader::navigation::Destination;
use crate::core::types::{PdfFeature, Value};

/// 取第 `page_num` 页 `/Annots[index]` 指向的注解字典。
fn annot_object(doc: &Document, page_num: i64, index: usize) -> Dict {
    let (num, dict) = annot_object_with_num(doc, page_num, index);
    let _ = num;
    dict
}

fn annot_object_with_num(doc: &Document, page_num: i64, index: usize) -> (i64, Dict) {
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
fn assert_text_entry(dict: &Dict, key: &str) {
    match dict.get(key) {
        Some(Value::HexString(_)) | Some(Value::String(_)) => {}
        other => panic!("expected text string at {key}, got {other:?}"),
    }
}

#[test]
fn text_annotation_is_written_as_indirect_object() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_contents("A note")
            .with_title("Reviewer")
            .with_flags(4),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    assert_eq!(dict.get("Type"), Some(&Value::Name("Annot".into())));
    assert_eq!(dict.get("Subtype"), Some(&Value::Name("Text".into())));
    assert_eq!(dict.get("F"), Some(&Value::Integer(4)));
    assert_text_entry(&dict, "Contents");
    assert_text_entry(&dict, "T");
    let rect = dict.get("Rect").and_then(|v| v.as_array()).expect("/Rect");
    assert_eq!(rect.0.len(), 4);
}

#[test]
fn link_annotation_writes_uri_action_and_named_destination() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 100.0, 20.0])
            .with_action(Action::Uri("https://qoder.com".into()))
            .with_destination(AnnotationDestination::Named("chapter-1".into())),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let action = dict
        .get("A")
        .and_then(|v| v.as_dict())
        .expect("inline /A dict");
    assert_eq!(action.get("S"), Some(&Value::Name("URI".into())));
    assert_text_entry(action, "URI");
    assert_text_entry(&dict, "Dest");
}

#[test]
fn link_goto_action_resolves_destination_page_reference() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    // 第二页 Page 字典对象号为 5（catalog=1、pages=2、页从 3 起两两成对）。
    b.add_annotation(
        0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 50.0, 50.0])
            .with_action(Action::GoTo(Destination::Fit { page: 1 })),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let action = dict.get("A").and_then(|v| v.as_dict()).expect("/A dict");
    let dest = action
        .get("D")
        .and_then(|v| v.as_array())
        .expect("/D array");
    assert_eq!(dest.0[0], Value::Ref(5, 0));
    assert_eq!(dest.0[1], Value::Name("Fit".into()));
}

#[test]
fn popup_handle_resolves_to_annotation_reference() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let popup = b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Popup, [96.0, 690.0, 240.0, 750.0]),
    );
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0]).with_popup(popup),
    );
    let doc = b.assemble_document().expect("assemble");
    // `/Annots` 按注册顺序引用；popup 先注册，其对象号即 text 的 `/Popup` 目标。
    let (popup_num, popup_dict) = annot_object_with_num(&doc, 3, 0);
    assert_eq!(
        popup_dict.get("Subtype"),
        Some(&Value::Name("Popup".into()))
    );
    let dict = annot_object(&doc, 3, 1);
    assert_eq!(dict.get("Popup"), Some(&Value::Ref(popup_num, 0)));
}

#[test]
fn markup_annotation_writes_quad_points_and_records_feature() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Highlight, [72.0, 700.0, 200.0, 712.0])
            .with_quad_points(vec![72.0, 712.0, 200.0, 712.0, 72.0, 700.0, 200.0, 700.0])
            .with_color(vec![1.0, 1.0, 0.0]),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let quad = dict
        .get("QuadPoints")
        .and_then(|v| v.as_array())
        .expect("/QuadPoints");
    assert_eq!(quad.0.len(), 8);
    assert!(dict.get("C").is_some());
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::MarkupAnnotations),
        "markup kinds must record the 1.3 feature"
    );
}

#[test]
fn border_style_entries_written_to_bs_dictionary() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [10.0, 10.0, 90.0, 90.0])
            .with_border_width(2.0)
            .with_border_style(AnnotationBorderStyle::Dashed)
            .with_border_dash(vec![3.0, 1.0]),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let bs = dict.get("BS").and_then(|v| v.as_dict()).expect("/BS dict");
    assert_eq!(bs.get("W"), Some(&Value::Float(2.0)));
    assert_eq!(bs.get("S"), Some(&Value::Name("D".into())));
    let dash = bs.get("D").and_then(|v| v.as_array()).expect("/BS /D");
    assert_eq!(dash.0.len(), 2);
}

#[test]
fn opacity_records_transparency_feature() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 20.0, 20.0]).with_opacity(0.5),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    assert_eq!(dict.get("CA"), Some(&Value::Float(0.5)));
    assert!(
        doc.features.borrow().contains(PdfFeature::Transparency),
        "/CA must record the transparency feature"
    );
}

#[test]
fn squiggly_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Squiggly, [0.0, 0.0, 50.0, 10.0]),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.3 must reject Squiggly");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn markup_requires_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Highlight, [0.0, 0.0, 50.0, 10.0]),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("1.2 must reject markup annotations");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn appearance_streams_written_as_normal_down_rollover_references() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let mut normal_content = ContentStream::new();
    normal_content
        .save_state()
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 20.0, 20.0)
        .fill()
        .restore_state();
    let normal = b.form_xobject(&FormXObject::new([0.0, 0.0, 20.0, 20.0], normal_content));
    let mut down_content = ContentStream::new();
    down_content.rect(0.0, 0.0, 20.0, 20.0).fill();
    let down = b.form_xobject(&FormXObject::new([0.0, 0.0, 20.0, 20.0], down_content));
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 20.0, 20.0])
            .with_appearance(normal)
            .with_appearance_down(down),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let ap = dict.get("AP").and_then(|v| v.as_dict()).expect("/AP dict");
    // 表单对象号紧跟页面槽位（catalog=1、pages=2、page=3、content=4）按句柄分配。
    assert_eq!(ap.get("N"), Some(&Value::Ref(5, 0)));
    assert_eq!(ap.get("D"), Some(&Value::Ref(6, 0)));
    assert!(ap.get("R").is_none());
    let form = doc
        .get_object(5)
        .expect("object")
        .expect("form exists")
        .value;
    let form_dict = match &form {
        Value::Stream(dict, _) => dict,
        other => panic!("expected /AP /N stream, got {other:?}"),
    };
    assert_eq!(form_dict.get("Subtype"), Some(&Value::Name("Form".into())));
    assert!(form_dict.get("BBox").is_some());
}

#[test]
fn appearance_requires_pdf_1_2() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p0 = b.page(PageSize::A4);
    let form = b.form_xobject(&FormXObject::new(
        [0.0, 0.0, 10.0, 10.0],
        ContentStream::new(),
    ));
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_appearance(form),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.1 must reject /AP");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn appearance_handle_out_of_range_is_rejected() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_appearance(3),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("unknown form handle");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(out.len(), 0);
}

#[test]
fn invalid_annotation_is_rejected_with_zero_bytes() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Highlight, [0.0, 0.0, 50.0, 10.0])
            .with_quad_points(vec![1.0, 2.0, 3.0]),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("quad points length must be 8*k");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(out.len(), 0);
}

#[test]
fn annotation_on_unknown_page_is_rejected() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.add_annotation(
        5,
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("page index out of range");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(out.len(), 0);
}

#[test]
fn destination_on_unknown_page_is_rejected() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.add_annotation(
        0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0]).with_destination(
            AnnotationDestination::Explicit(Destination::Fit { page: 9 }),
        ),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("destination page out of range");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(out.len(), 0);
}
