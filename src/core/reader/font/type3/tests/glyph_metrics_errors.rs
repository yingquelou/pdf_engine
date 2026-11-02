use super::*;

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
