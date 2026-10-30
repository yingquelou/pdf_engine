//! E3 Set-OCG-State 动作的写出 → 解析 → 回读往返（12.6.4.12，Table 213）。

use super::*;

/// 写入 → 重新解析：`/State` 的扁平名字/组引用序列经读端还原为操作序列，
/// 组引用指向 `/Type /OCG` 对象，`/PreserveRB` 显式值回读。
#[test]
fn builder_set_ocg_state_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, Font, OcgStateOperation, OcgStateSequence, OptionalContentGroup,
        PageSize, PdfAnnotation, PdfBuilder, SetOcgStateAction,
    };
    use pdf_engine::core::reader::action::{
        Action as ReadAction, OcgStateOperation as ReadOperation,
    };
    use pdf_engine::core::types::PdfTextString;

    fn hex_of(text: &str) -> String {
        let mut hex = String::new();
        for byte in PdfTextString::from(text).encode() {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02X}");
        }
        hex
    }

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let foreground = builder.optional_content_group(&OptionalContentGroup::new("Foreground"));
    let background = builder.optional_content_group(&OptionalContentGroup::new("Background"));
    builder.begin_optional_content(p0, foreground);
    builder.text(p0, &Font::helvetica(12.0), 72.0, 700.0, "Layer");
    builder.end_optional_content(p0);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(
            Action::SetOcgState(
                SetOcgStateAction::new(vec![
                    OcgStateSequence::new(OcgStateOperation::Off, vec![background]),
                    OcgStateSequence::new(OcgStateOperation::Toggle, vec![foreground, background]),
                ])
                .with_preserve_radio_buttons(false),
            ),
        ),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build set-ocg-state action");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 1);
    let state = match doc
        .action(annotations[0].action.as_ref().expect("/A"))
        .expect("parse set-ocg-state action")
    {
        ReadAction::SetOCGState(state) => state,
        other => panic!("unexpected action: {other:?}"),
    };
    assert_eq!(state.preserve_radio_buttons, Some(false));
    assert_eq!(state.state.len(), 2);
    assert_eq!(state.state[0].operation, ReadOperation::Off);
    assert_eq!(state.state[1].operation, ReadOperation::Toggle);
    assert_eq!(state.state[0].groups.len(), 1);
    assert_eq!(state.state[1].groups.len(), 2);
    assert_eq!(state.state[0].groups, state.state[1].groups[1..].to_vec());

    let names: Vec<Value> = state.state[1]
        .groups
        .iter()
        .map(|&(number, generation)| {
            assert_eq!(generation, 0);
            let object = doc
                .get_object(number)
                .expect("group lookup")
                .expect("group object");
            let dict = object.value.as_dict().expect("group dictionary");
            assert_eq!(dict.get("Type").and_then(Value::as_name), Some("OCG"));
            dict.get("Name").cloned().expect("/Name")
        })
        .collect();
    assert_eq!(
        names,
        vec![
            Value::HexString(hex_of("Foreground")),
            Value::HexString(hex_of("Background")),
        ]
    );
}
