//! E2：`/AP /N` 状态子字典与 `/AS` 的写出 → 解析往返（12.5.5，Table 168），
//! 以及注解级单一流 `/AP /N` 的外观 Form XObject 往返。

use super::*;

/// 写入多状态常态外观 → 由页面 `/Annots` 重新解析：`/N` 为状态名到
/// Form XObject 的字典，`/AS` 选中状态名，各引用可解析为 `/Subtype /Form`。
#[test]
fn builder_appearance_states_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, FormXObject, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::types::Value;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let on = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ));
    let off = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [10.0, 10.0, 30.0, 30.0])
            .with_appearance_state("On", on)
            .with_appearance_state("Off", off)
            .with_selected_appearance_state("Off"),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 1);
    let annotation = &annotations[0];
    assert_eq!(annotation.appearance_state.as_deref(), Some("Off"));

    let appearance = annotation.appearance.as_ref().expect("/AP dict");
    let states = appearance
        .normal
        .as_ref()
        .and_then(|value| value.as_dict())
        .expect("/AP /N state dictionary");
    assert_eq!(states.0.len(), 2);
    for state in ["On", "Off"] {
        let (form_num, _) = states
            .get(state)
            .and_then(|value| value.as_ref())
            .unwrap_or_else(|| panic!("missing state {state}"));
        let form = doc.get_object(form_num).expect("object").expect("exists");
        match &form.value {
            Value::Stream(dict, _) => {
                assert_eq!(dict.get("Subtype"), Some(&Value::Name("Form".into())));
            }
            other => panic!("state {state} must reference a Form XObject, got {other:?}"),
        }
    }
}

/// 写入 → 重新解析：注解 `/AP` 外观流应回读为 Form XObject 流引用，
/// `/N` 状态、流字典与内容绘制算子完整保留（12.5.5）。
#[test]
fn builder_annotation_appearance_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, FormXObject, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::types::Value;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let mut content = ContentStream::new();
    content
        .save_state()
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 20.0, 20.0)
        .fill()
        .restore_state();
    let normal = builder.form_xobject(&FormXObject::new([0.0, 0.0, 20.0, 20.0], content));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [10.0, 10.0, 30.0, 30.0])
            .with_appearance(normal),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(annots.len(), 1);
    let appearance = annots[0].appearance.as_ref().expect("/AP dict");
    let normal_value = appearance.normal.as_ref().expect("/AP /N");
    let (form_num, _) = normal_value.as_ref().expect("/N should be a reference");
    let form = doc
        .get_object(form_num)
        .expect("form object")
        .expect("exist");
    let form_dict = match &form.value {
        Value::Stream(dict, _) => dict.clone(),
        other => panic!("expected appearance Form XObject, got {other:?}"),
    };
    assert_eq!(form_dict.get("Type"), Some(&Value::Name("XObject".into())));
    assert_eq!(form_dict.get("Subtype"), Some(&Value::Name("Form".into())));
    assert!(form_dict.get("BBox").is_some());
    let content = doc
        .decode_stream_bytes(&form.value)
        .expect("appearance content decodes");
    let text = String::from_utf8_lossy(&content);
    assert!(text.contains(" re"), "appearance content: {text}");
    assert!(
        text.lines().any(|line| line.trim() == "f"),
        "appearance content: {text}"
    );
}
