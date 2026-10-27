use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

use super::{Type3Font, Type3GlyphMetrics};

fn char_proc() -> Value {
    Value::Stream(Dict::new(), StreamData::Owned(b"0 0 d0".to_vec()))
}

/// 以指定字节构造无过滤器的字形程序流。
fn char_proc_with(bytes: &[u8]) -> Value {
    Value::Stream(Dict::new(), StreamData::Owned(bytes.to_vec()))
}

/// 以指定的 `/CharProcs` 条目构造 Type 3 字体字典（其余条目沿用默认）。
fn type3_dict_with_char_procs(encoding: Value, entries: &[(&str, Value)]) -> Dict {
    let mut procs = Dict::new();
    for (name, value) in entries {
        procs = procs.insert(*name, value.clone());
    }
    type3_dict(encoding).insert("CharProcs", Value::Dict(procs))
}

/// 解析只含字形 A 的字体，返回字符码 65 的 `glyph_metrics` 错误。
fn glyph_metrics_error(proc_bytes: &[u8]) -> super::FontError {
    let document = Document::new();
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[("A", char_proc_with(proc_bytes))],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    parsed.glyph_metrics(&document, 65).unwrap_err()
}

fn differences_encoding() -> Value {
    Value::Dict(
        Dict::new().insert(
            "Differences",
            Value::Array(
                Array::new()
                    .push(Value::Integer(65))
                    .push(Value::Name("A".into()))
                    .push(Value::Integer(66))
                    .push(Value::Name("B".into())),
            ),
        ),
    )
}

fn type3_dict(encoding: Value) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name("Type3".into()))
        .insert(
            "FontBBox",
            Value::Array(
                Array::new()
                    .push(Value::Integer(0))
                    .push(Value::Integer(0))
                    .push(Value::Integer(1000))
                    .push(Value::Integer(1000)),
            ),
        )
        .insert(
            "FontMatrix",
            Value::Array(
                Array::new()
                    .push(Value::Float(0.001))
                    .push(Value::Integer(0))
                    .push(Value::Integer(0))
                    .push(Value::Float(0.001))
                    .push(Value::Integer(0))
                    .push(Value::Integer(0)),
            ),
        )
        .insert(
            "CharProcs",
            Value::Dict(
                Dict::new()
                    .insert("B", char_proc())
                    .insert("A", char_proc()),
            ),
        )
        .insert("FirstChar", Value::Integer(65))
        .insert("LastChar", Value::Integer(66))
        .insert(
            "Widths",
            Value::Array(
                Array::new()
                    .push(Value::Integer(500))
                    .push(Value::Integer(600)),
            ),
        )
        .insert("Encoding", encoding)
}

fn type3_font(encoding: Value) -> Value {
    Value::Dict(type3_dict(encoding))
}

fn resources_with_font(font: Value) -> Dict {
    Dict::new().insert("Font", Value::Dict(Dict::new().insert("F1", font)))
}

fn assert_contains(error: super::FontError, needle: &str) {
    let message = error.to_string();
    assert!(
        message.contains(needle),
        "expected error containing {needle:?}, got {message:?}"
    );
}

#[test]
fn type3_font_parses_font_matrix_bbox_widths_and_char_procs() {
    let document = Document::new();
    let font = type3_font(differences_encoding());
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(parsed.font_matrix, [0.001, 0.0, 0.0, 0.001, 0.0, 0.0]);
    assert_eq!(parsed.font_bbox, [0.0, 0.0, 1000.0, 1000.0]);
    assert_eq!(parsed.first_char, 65);
    assert_eq!(parsed.widths, vec![500.0, 600.0]);
    assert_eq!(parsed.char_procs, vec!["A".to_string(), "B".to_string()]);
    assert_eq!(parsed.width_for(65), Some(500.0));
    assert_eq!(parsed.width_for(66), Some(600.0));
    assert_eq!(parsed.width_for(64), None);
    assert_eq!(parsed.width_for(67), None);

    // The same entries resolved through indirect objects.
    let document = Document::new();
    document.set_object(Object::new(
        10,
        0,
        Value::Array(
            Array::new()
                .push(Value::Float(0.002))
                .push(Value::Integer(0))
                .push(Value::Integer(0))
                .push(Value::Float(0.002))
                .push(Value::Integer(0))
                .push(Value::Integer(0)),
        ),
    ));
    document.set_object(Object::new(
        11,
        0,
        Value::Array(
            Array::new()
                .push(Value::Integer(0))
                .push(Value::Integer(0))
                .push(Value::Integer(2000))
                .push(Value::Integer(2000)),
        ),
    ));
    document.set_object(Object::new(
        12,
        0,
        Value::Dict(Dict::new().insert("A", char_proc())),
    ));
    document.set_object(Object::new(
        13,
        0,
        Value::Array(Array::new().push(Value::Integer(750))),
    ));
    let indirect = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type3".into()))
            .insert("FontBBox", Value::Ref(11, 0))
            .insert("FontMatrix", Value::Ref(10, 0))
            .insert("CharProcs", Value::Ref(12, 0))
            .insert("FirstChar", Value::Integer(65))
            .insert("LastChar", Value::Integer(65))
            .insert("Widths", Value::Ref(13, 0))
            .insert("Encoding", Value::Name("WinAnsiEncoding".into())),
    );
    let parsed = document.type3_font(&indirect).unwrap().unwrap();
    assert_eq!(parsed.font_matrix, [0.002, 0.0, 0.0, 0.002, 0.0, 0.0]);
    assert_eq!(parsed.font_bbox, [0.0, 0.0, 2000.0, 2000.0]);
    assert_eq!(parsed.widths, vec![750.0]);
    assert_eq!(parsed.char_procs, vec!["A".to_string()]);
    assert_eq!(parsed.width_for(65), Some(750.0));
}

#[test]
fn type3_font_requires_char_procs() {
    let document = Document::new();

    let mut dict = type3_dict(differences_encoding());
    dict.0.remove("CharProcs");
    assert_contains(
        document.type3_font(&Value::Dict(dict)).unwrap_err(),
        "/CharProcs",
    );

    let mut dict = type3_dict(differences_encoding());
    dict.0.remove("FontMatrix");
    assert_contains(
        document.type3_font(&Value::Dict(dict)).unwrap_err(),
        "/FontMatrix",
    );

    let mut dict = type3_dict(differences_encoding());
    dict.0.remove("FontBBox");
    assert_contains(
        document.type3_font(&Value::Dict(dict)).unwrap_err(),
        "/FontBBox",
    );

    let mut dict = type3_dict(differences_encoding());
    dict.0.remove("Encoding");
    assert_contains(
        document.type3_font(&Value::Dict(dict)).unwrap_err(),
        "/Encoding",
    );

    // Non-Type-3 dictionaries are not Type 3 fonts.
    let other = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert!(document.type3_font(&other).unwrap().is_none());
}

#[test]
fn extracts_type3_differences_text() {
    let document = Document::new();
    let resources = resources_with_font(type3_font(differences_encoding()));
    let content = b"BT /F1 12 Tf (AB) Tj ET";
    assert_eq!(document.extract_text(content, &resources), "AB");
}

#[test]
fn extracts_type3_predefined_encoding_text() {
    let document = Document::new();
    let resources = resources_with_font(type3_font(Value::Name("WinAnsiEncoding".into())));
    let content = b"BT /F1 12 Tf <E9> Tj ET";
    assert_eq!(document.extract_text(content, &resources), "é");
}

#[test]
fn type3_encoding_without_base_encoding_uses_differences() {
    let document = Document::new();
    let encoding = Value::Dict(
        Dict::new().insert(
            "Differences",
            Value::Array(
                Array::new()
                    .push(Value::Integer(65))
                    .push(Value::Name("eacute".into())),
            ),
        ),
    );
    let font = type3_font(encoding);
    let parsed: Type3Font = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(
        parsed.encoding.base_encoding(),
        super::super::PredefinedEncoding::Standard
    );
    assert_eq!(
        parsed.encoding.unicode_sequence_for(65).as_deref(),
        Some("é")
    );
    // Codes not overridden by /Differences fall back to the StandardEncoding
    // approximation documented on `Document::type3_font`.
    assert_eq!(
        parsed.encoding.unicode_sequence_for(66).as_deref(),
        Some("B")
    );
}

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

#[test]
fn glyph_metrics_rejects_missing_or_truncated_first_operator() {
    // 空流与仅含注释/空白的流没有首操作符。
    assert_contains(glyph_metrics_error(b""), "ends before its d0/d1");
    assert_contains(
        glyph_metrics_error(b"  % only a comment\n"),
        "ends before its d0/d1",
    );
    // 截断流：只有操作数，没有操作符。
    assert_contains(glyph_metrics_error(b"500 0"), "ends before its d0/d1");
}

#[test]
fn glyph_metrics_rejects_non_d0_d1_first_operator() {
    assert_contains(
        glyph_metrics_error(b"500 0 Tj"),
        "first operator of character procedure /A is \"Tj\"",
    );
    assert_contains(
        glyph_metrics_error(b"q 500 0 d0"),
        "first operator of character procedure /A is \"q\"",
    );
}

#[test]
fn glyph_metrics_rejects_bad_operand_count() {
    assert_contains(
        glyph_metrics_error(b"500 d0"),
        "takes 2 numeric operands, found 1",
    );
    assert_contains(
        glyph_metrics_error(b"1 2 3 d0"),
        "takes 2 numeric operands, found 3",
    );
    assert_contains(
        glyph_metrics_error(b"500 0 1 d1"),
        "takes 6 numeric operands, found 3",
    );
}

#[test]
fn glyph_metrics_rejects_illegal_operand_tokens() {
    // 字符串、名称、数组等非法操作数。
    assert_contains(glyph_metrics_error(b"(s) 0 d0"), "non-numeric operand");
    assert_contains(glyph_metrics_error(b"/N 0 d0"), "non-numeric operand");
    assert_contains(glyph_metrics_error(b"[ 0 d0"), "non-numeric operand");
    // 形如数值但无法解析的 token。
    assert_contains(
        glyph_metrics_error(b"1.2.3 0 d0"),
        "invalid numeric operand",
    );
    assert_contains(glyph_metrics_error(b"+ 0 d0"), "invalid numeric operand");
}

#[test]
fn glyph_metrics_errors_when_char_proc_entry_missing_or_not_stream() {
    let document = Document::new();

    // /CharProcs 缺少编码给出的字形名。
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[("B", char_proc())],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.glyph_metrics(&document, 65).unwrap_err(),
        "/CharProcs has no entry for glyph /A",
    );

    // 条目不是流。
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[("A", Value::Integer(1))],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.glyph_metrics(&document, 65).unwrap_err(),
        "is not a stream",
    );

    // 间接引用无法解析。
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[("A", Value::Ref(99, 0))],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.glyph_metrics(&document, 65).unwrap_err(),
        "unresolved object reference 99 0 R",
    );
}

#[test]
fn glyph_metrics_returns_none_when_encoding_has_no_glyph_name() {
    // WinAnsiEncoding 无 /Differences 时无法解析字形名，无从定位字形程序。
    let document = Document::new();
    let font = type3_font(Value::Name("WinAnsiEncoding".into()));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_eq!(parsed.glyph_metrics(&document, 65).unwrap(), None);
}

/// 以指定的 `/Resources` 值构造 Type 3 字体字典。
fn type3_dict_with_resources(resources: Value) -> Dict {
    type3_dict(differences_encoding()).insert("Resources", resources)
}

/// 字形程序可引用的示例资源字典。
fn sample_resources() -> Dict {
    Dict::new()
        .insert(
            "XObject",
            Value::Dict(Dict::new().insert("Im1", Value::Ref(30, 0))),
        )
        .insert(
            "ProcSet",
            Value::Array(Array::new().push(Value::Name("PDF".into()))),
        )
}

#[test]
fn type3_font_reads_direct_resources() {
    let document = Document::new();
    let resources = sample_resources();
    let font = Value::Dict(type3_dict_with_resources(Value::Dict(resources.clone())));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(parsed.resources(&document).unwrap(), Some(resources));
}

#[test]
fn type3_font_resolves_indirect_resources() {
    let document = Document::new();
    // /Resources 为间接引用，指向对象 40 中的字典。
    let resources = Dict::new().insert(
        "ColorSpace",
        Value::Dict(Dict::new().insert("CS0", Value::Name("DeviceRGB".into()))),
    );
    document.set_object(Object::new(40, 0, Value::Dict(resources.clone())));
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(40, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(parsed.resources(&document).unwrap(), Some(resources));
}

#[test]
fn type3_font_resources_missing_returns_none() {
    let document = Document::new();
    let font = type3_font(differences_encoding());
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(parsed.resources(&document).unwrap(), None);
}

#[test]
fn type3_font_resources_rejects_non_dict_and_unresolved() {
    let document = Document::new();

    // 直接值不是字典。
    let font = Value::Dict(type3_dict_with_resources(Value::Integer(1)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "/Resources in Type 3 font is not a dictionary",
    );

    // 间接引用无法解析。
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(99, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "unresolved object reference 99 0 R",
    );

    // 间接引用的目标不是字典。
    document.set_object(Object::new(41, 0, Value::Name("NotADict".into())));
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(41, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "/Resources in Type 3 font is not a dictionary",
    );
}

/// Table 112 可选条目：`/Name`、`/FontDescriptor` 与 `/ToUnicode`。
#[test]
fn type3_font_parses_name_descriptor_and_to_unicode() {
    let document = Document::new();
    let descriptor = Dict::new()
        .insert("Type", Value::Name("FontDescriptor".into()))
        .insert("FontName", Value::Name("SampleType3".into()))
        .insert("Flags", Value::Integer(4))
        .insert(
            "FontBBox",
            Value::Array(
                Array::new()
                    .push(Value::Integer(0))
                    .push(Value::Integer(-200))
                    .push(Value::Integer(1000))
                    .push(Value::Integer(900)),
            ),
        )
        .insert("ItalicAngle", Value::Integer(0))
        .insert("Ascent", Value::Integer(800))
        .insert("Descent", Value::Integer(-200))
        .insert("CapHeight", Value::Integer(700))
        .insert("StemV", Value::Integer(80));
    let font = Value::Dict(
        type3_dict(differences_encoding())
            .insert("Name", Value::Name("SampleType3".into()))
            .insert("FontDescriptor", Value::Dict(descriptor))
            .insert("ToUnicode", char_proc_with(b"begincmap endcmap")),
    );

    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_eq!(parsed.name.as_deref(), Some("SampleType3"));
    let descriptor = parsed.descriptor.expect("应解析 /FontDescriptor");
    assert_eq!(descriptor.font_name.as_deref(), Some("SampleType3"));
    assert_eq!(descriptor.font_bbox, Some([0.0, -200.0, 1000.0, 900.0]));
    assert_eq!(descriptor.ascent, Some(800.0));
    assert!(parsed.has_to_unicode);
}

/// 可选条目存在但类型非法时整份字体解析失败。
#[test]
fn type3_font_rejects_invalid_name_descriptor_and_to_unicode() {
    let document = Document::new();

    let font = Value::Dict(type3_dict(differences_encoding()).insert("Name", Value::Integer(1)));
    assert_contains(document.type3_font(&font).unwrap_err(), "/Name");

    let font =
        Value::Dict(type3_dict(differences_encoding()).insert("FontDescriptor", Value::Integer(1)));
    assert_contains(
        document.type3_font(&font).unwrap_err(),
        "/FontDescriptor is not a dictionary",
    );

    // /FontDescriptor 缺少必需 /Type。
    let font = Value::Dict(
        type3_dict(differences_encoding()).insert("FontDescriptor", Value::Dict(Dict::new())),
    );
    assert_contains(document.type3_font(&font).unwrap_err(), "/Type");

    let font = Value::Dict(
        type3_dict(differences_encoding()).insert("ToUnicode", Value::Name("X".into())),
    );
    assert_contains(
        document.type3_font(&font).unwrap_err(),
        "/ToUnicode in Type 3 font is not a stream",
    );
}

/// 未提供可选条目时字段为 `None` / `false`。
#[test]
fn type3_font_omits_optional_entries_when_absent() {
    let document = Document::new();
    let parsed = document
        .type3_font(&type3_font(differences_encoding()))
        .unwrap()
        .unwrap();

    assert_eq!(parsed.name, None);
    assert_eq!(parsed.descriptor, None);
    assert!(!parsed.has_to_unicode);
}

/// `/Resources` 间接引用链（多级）应解引用到最终字典。
#[test]
fn type3_resources_follow_multi_level_indirect_chain() {
    let document = Document::new();
    let resources = sample_resources();
    document.set_object(Object::new(10, 0, Value::Ref(11, 0)));
    document.set_object(Object::new(11, 0, Value::Ref(12, 0)));
    document.set_object(Object::new(12, 0, Value::Dict(resources.clone())));

    let font = Value::Dict(type3_dict_with_resources(Value::Ref(10, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_eq!(parsed.resources(&document).unwrap(), Some(resources));
}

/// 引用环与超出深度上限均返回类型化错误。
#[test]
fn type3_resources_reject_reference_cycle_and_depth_overflow() {
    // 环：10 → 10。
    let document = Document::new();
    document.set_object(Object::new(10, 0, Value::Ref(10, 0)));
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(10, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "reference cycle at 10 0 R",
    );

    // 17 层引用超过 16 层上限。
    let document = Document::new();
    for num in 100..117 {
        document.set_object(Object::new(num, 0, Value::Ref(num + 1, 0)));
    }
    document.set_object(Object::new(117, 0, Value::Dict(sample_resources())));
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(100, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "exceeds 16 indirect references",
    );
}

/// 类别字典可为间接引用链；资源名按 `#xx` 解码后匹配。
#[test]
fn type3_resource_resolves_category_chain_and_escaped_names() {
    let document = Document::new();
    document.set_object(Object::new(
        20,
        0,
        Value::Dict(
            Dict::new()
                .insert("Im#201", Value::Ref(40, 0))
                .insert("Plain", Value::Ref(41, 0)),
        ),
    ));
    let resources = Dict::new().insert("XObject", Value::Ref(20, 0));
    let font = Value::Dict(type3_dict_with_resources(Value::Dict(resources)));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(
        parsed.resource(&document, "XObject", "Im 1").unwrap(),
        Some(Value::Ref(40, 0))
    );
    assert_eq!(
        parsed.resource(&document, "XObject", "Plain").unwrap(),
        Some(Value::Ref(41, 0))
    );
    assert_eq!(
        parsed.resource(&document, "XObject", "Missing").unwrap(),
        None
    );
    // 类别缺失或类别不是字典时返回 None。
    assert_eq!(parsed.resource(&document, "Pattern", "Im 1").unwrap(), None);
    document.set_object(Object::new(21, 0, Value::Integer(1)));
    let font = Value::Dict(type3_dict_with_resources(Value::Dict(
        Dict::new().insert("XObject", Value::Ref(21, 0)),
    )));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_eq!(parsed.resource(&document, "XObject", "Im 1").unwrap(), None);
}
