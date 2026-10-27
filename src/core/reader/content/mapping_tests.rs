//! C5 读写模型桥接：`to_content_op` / `from_content_op` / `serialize_ops` 测试。

use super::{
    Op, OpMappingError, TextElement, from_content_op, parse_content_stream, serialize_ops,
    to_content_op,
};
use crate::core::content::{
    ContentStream, LineCap, LineJoin, MarkedContentProperties, Op as WriteOp,
};

/// 断言 `bytes` 解析出的模型经 serialize → parse 后保持不变。
fn assert_model_roundtrip(bytes: &[u8]) -> Vec<Op> {
    let ops = parse_content_stream(bytes).expect("initial parse");
    let serialized = serialize_ops(&ops);
    let back = parse_content_stream(&serialized).unwrap_or_else(|e| {
        panic!(
            "reparse failed: {e}; bytes = {}",
            String::from_utf8_lossy(&serialized)
        )
    });
    assert_eq!(
        ops,
        back,
        "model changed; re-serialized bytes = {}",
        String::from_utf8_lossy(&serialized)
    );
    ops
}

#[test]
fn horizontal_scaling_parses_tz_keyword() {
    let ops = parse_content_stream(b"BT 80 Tz (x) Tj ET").expect("parse Tz");
    assert!(
        ops.contains(&Op::SetHorizontalScaling(80.0)),
        "Tz must model horizontal scaling, got {ops:?}"
    );
}

#[test]
fn graphics_state_and_path_operators_roundtrip() {
    assert_model_roundtrip(
        b"q 1 0 0 1 10 20 cm 0.5 w 1 J 0 j 4 M [3 2] 1 d \
           /RelativeColorimetric ri 5 i /GS0 gs \
           1 2 m 3 4 l 5 6 7 8 9 10 c 11 12 13 14 v 15 16 17 18 y h \
           0 0 100 50 re W W* n S s f F f* B B* b b* Q",
    );
}

#[test]
fn text_operators_roundtrip_including_hex_binary_and_escapes() {
    let ops = assert_model_roundtrip(
        b"BT /F1 12 Tf 2 Tc 3 Tw 90 Tz 4 TL 1 Tr 5 Ts 6 7 Td \
           0.24 0 0 0.24 50 400 Tm T* \
           (Hello) Tj <00FF81> Tj \
           [(A) -20 <C328>] TJ \
           (Next) ' 1.5 2.5 (Spaced) \" ET",
    );
    assert!(ops.contains(&Op::ShowText(vec![0x00, 0xFF, 0x81])));
    assert!(ops.contains(&Op::ShowTextWithPositioning(vec![
        TextElement::String(b"A".to_vec()),
        TextElement::Adjustment(-20.0),
        TextElement::String(vec![0xC3, 0x28]),
    ])));
}

#[test]
fn td_d0_d1_bx_ex_roundtrip_through_raw_fallback() {
    assert_model_roundtrip(b"BX 1 2 TD 3 4 d0 1 2 100 200 3 4 d1 EX");
    let bytes = serialize_ops(&[Op::MoveTextPositionAndSetLeading(1.0, 2.0)]);
    assert_eq!(bytes, b"1.0 2.0 TD\n");
}

#[test]
fn out_of_range_linecap_linejoin_and_render_mode_roundtrip() {
    assert_model_roundtrip(b"7 J 9 j 300 Tr");
    assert_eq!(
        to_content_op(&Op::SetLineCap(7)),
        Err(OpMappingError::OperandOutOfRange {
            operator: "J".into(),
            value: 7,
        })
    );
    assert_eq!(
        to_content_op(&Op::SetTextRenderingMode(300)),
        Err(OpMappingError::OperandOutOfRange {
            operator: "Tr".into(),
            value: 300,
        })
    );
}

#[test]
fn to_content_op_errors_for_variants_without_writer_model() {
    let cases = [
        (Op::MoveTextPositionAndSetLeading(0.0, 1.0), "TD"),
        (Op::SetGlyphWidth(1.0, 2.0), "d0/d1"),
        (
            Op::SetGlyphWidthAndBBox(0.0, 0.0, 1.0, 1.0, 1.0, 2.0),
            "d0/d1",
        ),
        (Op::BeginCompatibility, "BX/EX"),
        (Op::EndCompatibility, "BX/EX"),
        (
            Op::Unmodeled {
                operator: "zz".into(),
                operands: vec![],
            },
            "zz",
        ),
        (Op::NextLineAndShowText(vec![0xFF, 0xFE]), "'"),
        (Op::SetSpacingAndShowText(1.0, 2.0, vec![0xC0]), "\""),
        (
            Op::ShowTextWithPositioning(vec![TextElement::String(vec![0xFF])]),
            "TJ",
        ),
    ];
    for (op, expected_operator) in cases {
        let error = to_content_op(&op).expect_err("must not map to writer Op");
        match &error {
            OpMappingError::NotRepresentableByWriter { operator, .. } => {
                assert_eq!(operator, expected_operator, "wrong label for {op:?}");
            }
            OpMappingError::OperandOutOfRange { .. } => {
                panic!("expected NotRepresentableByWriter for {op:?}, got {error}")
            }
        }
    }
}

#[test]
fn show_text_selects_literal_for_utf8_and_hex_for_binary() {
    assert_eq!(
        to_content_op(&Op::ShowText("Café \u{1F600}".as_bytes().to_vec())),
        Ok(WriteOp::ShowText("Café \u{1F600}".into()))
    );
    assert_eq!(
        to_content_op(&Op::ShowText(vec![0xFF, 0x00, 0x28])),
        Ok(WriteOp::ShowTextHex(vec![0xFF, 0x00, 0x28]))
    );
    // 二进制 Tj 走十六进制形式后仍可原样读回同一字节序列。
    assert_model_roundtrip(b"<FF280A> Tj");
}

#[test]
fn marked_content_operators_roundtrip_name_and_inline_dict() {
    let ops = assert_model_roundtrip(
        b"/Span BMC 5 6 m 3 4 l S \
           /P <</MCID 0 /Title (Sec 1)>> BDC 0.5 g EMC \
           /Artifact <</Type /Pagination>> DP EMC \
           EMC",
    );
    assert!(ops.contains(&Op::BeginMarkedContent("Span".into())));
    assert!(ops.contains(&Op::EndMarkedContent));
}

#[test]
fn color_pattern_xobject_shading_and_extgstate_roundtrip() {
    assert_model_roundtrip(
        b"/Cs1 CS /Cs2 cs 1 0 0 RG 0 1 0 rg 0 0 0 1 K 1 1 1 0 k \
           0.5 G 0.25 g 0.2 0.7 1.0 SCN 0 0.5 1 sc \
           /Pat1 scn 0.5 0.25 /Pat2 SCN \
           /GS1 gs /Im0 Do /Sh1 sh",
    );
}

#[test]
fn unmodeled_operators_preserve_array_name_and_string_operands() {
    let ops = assert_model_roundtrip(b"1 [2 3] (s) /N myop 42 otherop");
    assert!(ops.contains(&Op::Unmodeled {
        operator: "myop".into(),
        operands: vec![
            crate::core::types::Value::Float(1.0),
            crate::core::types::Value::Array(crate::core::types::Array(vec![
                crate::core::types::Value::Float(2.0),
                crate::core::types::Value::Float(3.0),
            ])),
            crate::core::types::Value::String("s".into()),
            crate::core::types::Value::Name("N".into()),
        ],
    }));
}

#[test]
fn from_content_op_maps_writer_stream_ops_and_remaps_losslessly() {
    let mut stream = ContentStream::new();
    stream
        .begin_text()
        .set_font("F1", 12.0)
        .show_text_at("F1", 10.0, 50.0, 60.0, "Hi")
        .end_text()
        .save_state()
        .concat_matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
        .set_line_cap(LineCap::Round)
        .set_line_join(LineJoin::Bevel)
        .set_dash(vec![4.0, 2.0], 1.0)
        .line(0.0, 0.0, 10.0, 10.0)
        .rect(1.0, 2.0, 3.0, 4.0)
        .fill_stroke()
        .set_fill_rgb(0.1, 0.2, 0.3)
        .set_ext_g_state("GS0")
        .draw_xobject("Im0")
        .paint_shading("Sh0")
        .set_stroke_pattern_with_components([0.5], "Pat1")
        .begin_marked_content_properties("P", MarkedContentProperties::Name("MC0".into()))
        .end_marked_content()
        .marked_content_point_properties("Artifact", MarkedContentProperties::Name("A0".into()));
    for writer_op in stream.ops().to_vec() {
        let reader_op = from_content_op(&writer_op);
        let remapped = to_content_op(&reader_op)
            .unwrap_or_else(|e| panic!("reader op {reader_op:?} must map back: {e}"));
        if matches!(writer_op, WriteOp::MoveNextLineSetLeading { .. }) {
            // 单操作数 TD 是唯一有损点：不要求映射回去相等。
            continue;
        }
        assert_eq!(writer_op, remapped, "lossy for {writer_op:?}");
    }
}

#[test]
fn writer_model_ops_map_to_reader_model_shapes() {
    assert_eq!(
        from_content_op(&WriteOp::SetTextScale(90.0)),
        Op::SetHorizontalScaling(90.0)
    );
    assert_eq!(
        from_content_op(&WriteOp::ShowTextHex(vec![0x00, 0xFF])),
        Op::ShowText(vec![0x00, 0xFF])
    );
    assert_eq!(
        from_content_op(&WriteOp::MoveNextLineSetLeading { leading: 14.0 }),
        Op::MoveTextPositionAndSetLeading(0.0, 14.0)
    );
    assert_eq!(from_content_op(&WriteOp::FillRule), Op::FillObsolete);
    assert_eq!(
        from_content_op(&WriteOp::ClosePathFillStrokeEvenOdd),
        Op::CloseFillAndStrokeEvenOdd
    );
    assert_eq!(
        from_content_op(&WriteOp::BeginMarkedContentProperties {
            tag: "OC".into(),
            properties: MarkedContentProperties::Dict(crate::core::types::Dict::new()),
        }),
        Op::BeginMarkedContentWithProperties(
            "OC".into(),
            crate::core::types::Value::Dict(crate::core::types::Dict::new(),)
        )
    );
}

#[test]
fn inline_image_variant_serializes_and_reparses() {
    // Parsed dictionaries come back in sorted key order, so the model uses
    // the same order to keep the roundtrip comparison meaningful.
    let op = Op::InlineImage(super::InlineImage {
        dict: vec![
            (
                "CS".into(),
                crate::core::types::Value::Name("DeviceGray".into()),
            ),
            ("W".into(), crate::core::types::Value::Float(2.0)),
        ],
        data: vec![0x01, 0x02],
    });
    let bytes = serialize_ops(std::slice::from_ref(&op));
    assert!(bytes.starts_with(b"BI /CS /DeviceGray /W 2.0 ID\n"));
    assert!(bytes.ends_with(b"\nEI\n"));
    assert_eq!(
        crate::core::reader::content::parse_content_stream(&bytes).unwrap(),
        vec![op]
    );
}
