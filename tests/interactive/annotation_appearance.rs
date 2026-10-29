//! E2：`/AP /N` 状态子字典与 `/AS` 的写出 → 解析往返（12.5.5，Table 168）。

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
