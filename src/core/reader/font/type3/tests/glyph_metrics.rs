use super::*;

use super::super::Type3GlyphMetrics;

#[test]
fn glyph_metrics_reads_d0_declared_width() {
    let document = Document::new();
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[
            ("A", char_proc_with(b"500 0 d0")),
            ("B", char_proc_with(b"600.5 -1 d0")),
        ],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(
        parsed.glyph_metrics(&document, 65).unwrap(),
        Some(Type3GlyphMetrics {
            width: [500.0, 0.0],
            bbox: None,
        })
    );
    assert_eq!(
        parsed.glyph_metrics(&document, 66).unwrap(),
        Some(Type3GlyphMetrics {
            width: [600.5, -1.0],
            bbox: None,
        })
    );
}

#[test]
fn glyph_metrics_reads_d1_declared_width_and_bbox() {
    let document = Document::new();
    // d1 之后的图形操作符不解释；前导空白与注释应被跳过。
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[(
            "A",
            char_proc_with(b"% glyph A\n  600 0 -10 -20 610 700 d1 0 0 600 700 re f"),
        )],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(
        parsed.glyph_metrics(&document, 65).unwrap(),
        Some(Type3GlyphMetrics {
            width: [600.0, 0.0],
            bbox: Some([-10.0, -20.0, 610.0, 700.0]),
        })
    );
}

#[test]
fn glyph_metrics_coexists_with_widths_entry() {
    // /Widths 原始宽度与 d0/d1 声明宽度是两个独立数据源，均可查询，
    // 不做一致性校验；width_for 语义不变。
    let document = Document::new();
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[
            ("A", char_proc_with(b"550 0 d0")),
            ("B", char_proc_with(b"650 0 0 0 650 700 d1")),
        ],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(parsed.width_for(65), Some(500.0));
    assert_eq!(parsed.width_for(66), Some(600.0));
    assert_eq!(
        parsed.glyph_metrics(&document, 65).unwrap().unwrap().width,
        [550.0, 0.0]
    );
    assert_eq!(
        parsed.glyph_metrics(&document, 66).unwrap().unwrap().width,
        [650.0, 0.0]
    );
}

#[test]
fn glyph_metrics_decodes_filtered_and_indirect_char_procs() {
    let document = Document::new();
    // ASCIIHexDecode("3530302030206430>") == "500 0 d0"。
    let filtered = Value::Stream(
        Dict::new().insert("Filter", Value::Name("ASCIIHexDecode".into())),
        StreamData::Owned(b"3530302030206430>".to_vec()),
    );
    // 条目为间接引用，指向对象 20 中的流。
    document.set_object(Object::new(20, 0, char_proc_with(b"600 0 0 0 600 700 d1")));
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[("A", filtered), ("B", Value::Ref(20, 0))],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(
        parsed.glyph_metrics(&document, 65).unwrap(),
        Some(Type3GlyphMetrics {
            width: [500.0, 0.0],
            bbox: None,
        })
    );
    assert_eq!(
        parsed.glyph_metrics(&document, 66).unwrap(),
        Some(Type3GlyphMetrics {
            width: [600.0, 0.0],
            bbox: Some([0.0, 0.0, 600.0, 700.0]),
        })
    );

    // 解码失败（FlateDecode 垃圾数据）应返回类型化错误而非 panic。
    let bad = Value::Stream(
        Dict::new().insert("Filter", Value::Name("FlateDecode".into())),
        StreamData::Owned(b"not-a-zlib-stream".to_vec()),
    );
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[("A", bad)],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.glyph_metrics(&document, 65).unwrap_err(),
        "cannot decode character procedure",
    );
}
