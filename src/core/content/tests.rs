//! content 模块测试。

use super::*;
use crate::core::types::{Dict, Value};

#[test]
fn begin_end_text_roundtrip() {
    let mut s = ContentStream::new();
    s.begin_text().end_text();
    let bytes = s.to_bytes();
    assert_eq!(bytes, b"BT\nET\n");
}

#[test]
fn show_text_at_format() {
    let mut s = ContentStream::new();
    s.show_text_at("F1", 12.0, 72.0, 700.0, "Hello");
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("BT\n/F1 12.0 Tf\n72.0 700.0 Td\n(Hello) Tj\nET\n"));
}

#[test]
fn string_literal_escapes_parens_and_backslash() {
    let mut s = ContentStream::new();
    s.show_text(r"a(b)c\d");
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains(r"(a\(b\)c\\d) Tj"));
}

#[test]
fn show_text_hex_writes_uppercase_hexadecimal_string() {
    let mut s = ContentStream::new();
    s.show_text_hex([0x00u8, 0x41, 0xab, 0x0f]);
    let bytes = s.to_bytes();
    assert_eq!(bytes, b"<0041AB0F> Tj\n");
}

#[test]
fn save_concat_restore_chain() {
    let mut s = ContentStream::new();
    s.save_state()
        .concat_matrix([50.0, 0.0, 0.0, 50.0, 100.0, 100.0])
        .draw_xobject("Im1")
        .restore_state();
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text, "q\n50.0 0.0 0.0 50.0 100.0 100.0 cm\n/Im1 Do\nQ\n");
}

#[test]
fn line_operator() {
    let mut s = ContentStream::new();
    s.line(0.0, 0.0, 100.0, 100.0);
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text, "0.0 0.0 m\n100.0 100.0 l\nS\n");
}

#[test]
fn rect_operator() {
    let mut s = ContentStream::new();
    s.rect(10.0, 20.0, 100.0, 50.0).stroke();
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text, "10.0 20.0 100.0 50.0 re\nS\n");
}

#[test]
fn set_dash_pattern() {
    let mut s = ContentStream::new();
    s.set_dash(vec![3.0, 2.0], 0.0);
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text, "[3.0 2.0] 0.0 d\n");
}

#[test]
fn show_text_array_mixed() {
    let mut s = ContentStream::new();
    s.show_text_array(vec![
        TextArrayItem::String("A".into()),
        TextArrayItem::MoveBy(-25.0),
        TextArrayItem::String("B".into()),
    ]);
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text, "[(A) -25.0 (B) ] TJ\n");
}

#[test]
fn show_text_next_line_writes_apostrophe_operator() {
    let mut s = ContentStream::new();
    s.show_text_next_line("Hello");
    let bytes = s.to_bytes();
    assert_eq!(bytes, b"(Hello) '\n");
}

#[test]
fn set_spacing_show_text_next_line_writes_quote_operator() {
    let mut s = ContentStream::new();
    s.set_spacing_show_text_next_line(0.5, 1.0, "World");
    let bytes = s.to_bytes();
    assert_eq!(bytes, b"0.5 1.0 (World) \"\n");
}

#[test]
fn set_text_rise_writes_ts_operator() {
    let mut s = ContentStream::new();
    s.push(Op::SetTextRise(5.0));
    let bytes = s.to_bytes();
    assert_eq!(bytes, b"5.0 Ts\n");
}

#[test]
fn ext_g_state_serializes_gs_without_inline_transparency_operators() {
    let mut s = ContentStream::new();
    s.set_ext_g_state("GS1");
    let bytes = s.to_bytes();
    assert_eq!(bytes, b"/GS1 gs\n");
    let text = String::from_utf8_lossy(&bytes);
    for inline in [" ca\n", " CA\n", " BM\n"] {
        assert!(
            !text.contains(inline),
            "content stream must not emit inline {inline:?}: {text}"
        );
    }
}

#[test]
fn text_state_operators_individual() {
    let mut s = ContentStream::new();
    s.push(Op::SetCharSpacing(1.5));
    s.push(Op::SetWordSpacing(0.5));
    s.push(Op::SetTextScale(90.0));
    s.push(Op::SetTextLeading(20.0));
    s.push(Op::SetTextRender(2));
    s.push(Op::SetTextRise(3.5));
    s.push(Op::NextLine);
    s.push(Op::MoveNextLineSetLeading { leading: 24.0 });
    s.push(Op::SetTextMatrix([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]));
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("1.5 Tc\n"));
    assert!(text.contains("0.5 Tw\n"));
    assert!(text.contains("90.0 Tz\n"));
    assert!(text.contains("20.0 TL\n"));
    assert!(text.contains("2 Tr\n"));
    assert!(text.contains("3.5 Ts\n"));
    assert!(text.contains("T*\n"));
    assert!(text.contains("24.0 TD\n"));
    assert!(text.contains("1.0 0.0 0.0 1.0 10.0 20.0 Tm\n"));
}

#[test]
fn path_construction_all_kinds() {
    let mut s = ContentStream::new();
    s.push(Op::MoveTo { x: 0.0, y: 0.0 });
    s.push(Op::LineTo { x: 10.0, y: 10.0 });
    s.push(Op::CurveTo {
        x1: 1.0,
        y1: 2.0,
        x2: 3.0,
        y2: 4.0,
        x3: 5.0,
        y3: 6.0,
    });
    s.push(Op::CurveToInitial {
        x2: 1.0,
        y2: 2.0,
        x3: 3.0,
        y3: 4.0,
    });
    s.push(Op::CurveToFinal {
        x1: 1.0,
        y1: 2.0,
        x3: 3.0,
        y3: 4.0,
    });
    s.push(Op::ClosePath);
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("0.0 0.0 m\n"));
    assert!(text.contains("10.0 10.0 l\n"));
    assert!(text.contains("1.0 2.0 3.0 4.0 5.0 6.0 c\n"));
    assert!(text.contains("1.0 2.0 3.0 4.0 v\n"));
    assert!(text.contains("1.0 2.0 3.0 4.0 y\n"));
    assert!(text.contains("h\n"));
}

#[test]
fn path_painting_variants() {
    let mut s = ContentStream::new();
    s.push(Op::Stroke);
    s.push(Op::ClosePathStroke);
    s.push(Op::Fill);
    s.push(Op::FillRule);
    s.push(Op::FillEvenOdd);
    s.push(Op::FillStroke);
    s.push(Op::FillStrokeEvenOdd);
    s.push(Op::ClosePathFillStroke);
    s.push(Op::ClosePathFillStrokeEvenOdd);
    s.push(Op::EndPath);
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("S\n"));
    assert!(text.contains("s\n"));
    assert!(text.contains("f\n"));
    assert!(text.contains("F\n"));
    assert!(text.contains("f*\n"));
    assert!(text.contains("B\n"));
    assert!(text.contains("B*\n"));
    assert!(text.contains("b\n"));
    assert!(text.contains("b*\n"));
    assert!(text.contains("n\n"));
}

#[test]
fn clipping_variants() {
    let mut s = ContentStream::new();
    s.push(Op::Clip);
    s.push(Op::ClipEvenOdd);
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("W\n"));
    assert!(text.contains("W*\n"));
}

#[test]
fn color_variants() {
    let mut s = ContentStream::new();
    s.push(Op::SetStrokeGray { gray: 0.25 });
    s.push(Op::SetFillGray { gray: 0.75 });
    s.push(Op::SetStrokeRGB {
        r: 1.0,
        g: 0.0,
        b: 0.0,
    });
    s.push(Op::SetFillRGB {
        r: 0.0,
        g: 0.0,
        b: 1.0,
    });
    s.push(Op::SetStrokeCMYK {
        c: 1.0,
        m: 0.0,
        y: 0.0,
        k: 0.0,
    });
    s.push(Op::SetFillCMYK {
        c: 0.0,
        m: 1.0,
        y: 0.0,
        k: 0.0,
    });
    s.push(Op::SetStrokeColorSpace("DeviceGray".into()));
    s.push(Op::SetFillColorSpace("DeviceGray".into()));
    s.push(Op::SetStrokeColor(vec![0.5]));
    s.push(Op::SetFillColor(vec![0.5]));
    s.push(Op::SetStrokeColorN {
        values: vec![0.1, 0.2],
    });
    s.push(Op::SetFillColorN {
        values: vec![0.1, 0.2],
    });
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("0.25 G\n"));
    assert!(text.contains("0.75 g\n"));
    assert!(text.contains("1.0 0.0 0.0 RG\n"));
    assert!(text.contains("0.0 0.0 1.0 rg\n"));
    assert!(text.contains("1.0 0.0 0.0 0.0 K\n"));
    assert!(text.contains("0.0 1.0 0.0 0.0 k\n"));
    assert!(text.contains("/DeviceGray CS\n"));
    assert!(text.contains("/DeviceGray cs\n"));
    assert!(text.contains("0.5 SC\n"));
    assert!(text.contains("0.5 sc\n"));
    assert!(text.contains("0.1 0.2 SCN\n"));
    assert!(text.contains("0.1 0.2 scn\n"));
}

#[test]
fn cmyk_stream_methods_emit_uppercase_and_lowercase_operators() {
    let mut s = ContentStream::new();
    s.set_stroke_cmyk(0.1, 0.2, 0.3, 0.4)
        .set_fill_cmyk(0.5, 0.6, 0.7, 0.8);
    let text = String::from_utf8_lossy(&s.to_bytes()).into_owned();
    assert!(text.contains("0.1 0.2 0.3 0.4 K\n"));
    assert!(text.contains("0.5 0.6 0.7 0.8 k\n"));
}

#[test]
fn gray_stream_methods_emit_uppercase_and_lowercase_operators() {
    let mut s = ContentStream::new();
    s.set_stroke_gray(0.2).set_fill_gray(0.6);
    let text = String::from_utf8_lossy(&s.to_bytes()).into_owned();
    assert_eq!(text, "0.2 G\n0.6 g\n");
}

#[test]
fn shading_and_pattern_operators_serialize() {
    let mut s = ContentStream::new();
    s.set_fill_color_space("Pattern")
        .paint_shading("Sh1")
        .set_fill_pattern("P1")
        .set_stroke_pattern("P2");
    let text = String::from_utf8_lossy(&s.to_bytes()).into_owned();
    assert_eq!(text, "/Pattern cs\n/Sh1 sh\n/P1 scn\n/P2 SCN\n");
}

#[test]
fn uncolored_pattern_components_serialize_before_name() {
    let mut s = ContentStream::new();
    s.set_fill_color_space("Pattern")
        .set_fill_pattern_with_components(vec![0.2, 0.4, 0.6], "P1")
        .set_stroke_pattern_with_components(vec![0.5], "P2");
    let text = String::from_utf8_lossy(&s.to_bytes()).into_owned();
    assert_eq!(text, "/Pattern cs\n0.2 0.4 0.6 /P1 scn\n0.5 /P2 SCN\n");
}

#[test]
fn graphics_state_individual() {
    let mut s = ContentStream::new();
    s.push(Op::SetLineWidth(3.5));
    s.push(Op::SetLineCap(LineCap::Round));
    s.push(Op::SetLineJoin(LineJoin::Bevel));
    s.push(Op::SetMiterLimit(5.0));
    s.push(Op::SetRenderingIntent("Default".into()));
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("3.5 w\n"));
    assert!(text.contains("1 J\n"));
    assert!(text.contains("2 j\n"));
    assert!(text.contains("5.0 M\n"));
    assert!(text.contains("/Default ri\n"));
}

#[test]
fn ext_g_state_and_flatness_operators() {
    let mut s = ContentStream::new();
    s.set_ext_g_state("GS1");
    s.set_flatness(0.5);
    let bytes = s.to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/GS1 gs\n"));
    assert!(text.contains("0.5 i\n"));
}

#[test]
fn marked_content_operators_without_properties() {
    let mut s = ContentStream::new();
    s.begin_marked_content("P")
        .marked_content_point("Artifact")
        .end_marked_content();
    assert_eq!(s.to_bytes(), b"/P BMC\n/Artifact MP\nEMC\n");
}

#[test]
fn marked_content_properties_name_and_inline_dict() {
    let mut s = ContentStream::new();
    s.begin_marked_content_properties("OC", MarkedContentProperties::Name("Layer1".into()));
    let props = Dict::new()
        .insert("Lang", Value::String("en".into()))
        .insert("MCID", Value::Integer(3));
    s.begin_marked_content_properties("Span", MarkedContentProperties::Dict(props));
    s.marked_content_point_properties("Artifact", MarkedContentProperties::Name("Props1".into()));
    s.end_marked_content().end_marked_content();
    assert_eq!(
        s.to_bytes(),
        b"/OC /Layer1 BDC\n/Span << /Lang (en) /MCID 3 >> BDC\n/Artifact /Props1 DP\nEMC\nEMC\n"
    );
}

#[test]
fn optional_content_enum_serializes_through_marked_content_operators() {
    let mut s = ContentStream::new();
    s.optional_content(OptionalContent::Begin("Layer1".into()))
        .optional_content(OptionalContent::PropertyDict("P".into(), "Props1".into()))
        .optional_content(OptionalContent::End);
    assert_eq!(s.to_bytes(), b"/OC /Layer1 BDC\n/P /Props1 DP\nEMC\n");
}
