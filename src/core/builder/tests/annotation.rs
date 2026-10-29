//! 注解写端测试：`/Annots` 间接对象、通用条目与目标/弹引引用解析
//! （ISO 32000-1 12.5.1–12.5.5）。

use super::support::{annot_object, annot_object_with_num, assert_text_entry};
use super::*;
use crate::core::content::ContentStream;
use crate::core::reader::navigation::Destination;
use crate::core::types::{PdfFeature, Value};

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
fn link_annotation_writes_uri_action() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 100.0, 20.0])
            .with_action(Action::Uri("https://qoder.com".into())),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let action = dict
        .get("A")
        .and_then(|v| v.as_dict())
        .expect("inline /A dict");
    assert_eq!(action.get("S"), Some(&Value::Name("URI".into())));
    assert_text_entry(action, "URI");
    assert!(dict.get("Dest").is_none(), "/A excludes /Dest");
}

#[test]
fn link_annotation_writes_named_destination() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 100.0, 20.0])
            .with_destination(AnnotationDestination::Named("chapter-1".into())),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    assert_text_entry(&dict, "Dest");
    assert!(dict.get("A").is_none(), "/Dest excludes /A");
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

#[test]
fn text_annotation_writes_icon_open_and_markup_state() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_icon_name("Paragraph")
            .with_open(true)
            .with_state("Accepted")
            .with_state_model("Review"),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    assert_eq!(dict.get("Name"), Some(&Value::Name("Paragraph".into())));
    assert_eq!(dict.get("Open"), Some(&Value::Bool(true)));
    assert_text_entry(&dict, "State");
    assert_text_entry(&dict, "StateModel");
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::AnnotationMarkupState),
        "/State must record the 1.3 markup state feature"
    );
}

#[test]
fn caret_annotation_writes_symbol_name() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Caret, [10.0, 10.0, 20.0, 20.0]).with_symbol("Cross"),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    assert_eq!(dict.get("Symbol"), Some(&Value::Name("Cross".into())));
    assert!(dict.get("State").is_none(), "no markup state on Caret");
}

#[test]
fn ink_annotation_writes_ink_list_paths() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Ink, [0.0, 0.0, 50.0, 50.0])
            .with_ink_path(vec![1.0, 2.0, 3.0, 4.0])
            .with_ink_path(vec![5.0, 6.0]),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let paths = dict
        .get("InkList")
        .and_then(|v| v.as_array())
        .expect("/InkList");
    assert_eq!(paths.0.len(), 2);
    let first = paths.0[0].as_array().expect("first path").clone();
    assert_eq!(
        first.0,
        vec![
            Value::Float(1.0),
            Value::Float(2.0),
            Value::Float(3.0),
            Value::Float(4.0)
        ]
    );
    let second = paths.0[1].as_array().expect("second path").clone();
    assert_eq!(second.0.len(), 2);
}

#[test]
fn subtype_entries_are_rejected_on_other_subtypes() {
    let cases = vec![
        PdfAnnotation::new(AnnotationKind::Highlight, [0.0, 0.0, 10.0, 10.0]).with_state("Marked"),
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0]).with_icon_name("Note"),
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0]).with_open(true),
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_symbol("Cross"),
        PdfAnnotation::new(AnnotationKind::Caret, [0.0, 0.0, 10.0, 10.0])
            .with_ink_path(vec![1.0, 2.0]),
    ];
    for annot in cases {
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        b.add_annotation(p0, annot);
        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("subtype entry on wrong subtype");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("specific to"), "{message}");
        assert_eq!(out.len(), 0);
    }
}

#[test]
fn invalid_subtype_entry_values_are_rejected() {
    let cases = vec![
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_icon_name(""),
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_icon_name("No/Icon"),
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_state(""),
        PdfAnnotation::new(AnnotationKind::Caret, [0.0, 0.0, 10.0, 10.0]).with_symbol("New Line"),
        PdfAnnotation::new(AnnotationKind::Ink, [0.0, 0.0, 10.0, 10.0]).with_ink_path(vec![1.0]),
        PdfAnnotation::new(AnnotationKind::Ink, [0.0, 0.0, 10.0, 10.0])
            .with_ink_path(vec![1.0, f64::NAN]),
    ];
    for annot in cases {
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        b.add_annotation(p0, annot);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid subtype entry value");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
        assert_eq!(out.len(), 0);
    }
}

#[test]
fn markup_state_requires_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_state("Marked"),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("1.2 must reject /State and /StateModel");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message) if message.contains("/State")),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}
