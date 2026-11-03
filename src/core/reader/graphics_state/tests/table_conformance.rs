//! 规范核对（characterization）：把本次 Table 审计的结论钉在测试里。
//!
//! - `Table 144`（11.6.5.2）软遮罩字典只有 `Type/S/G/BC/TR`；`/Matte`
//!   （`Table 146`，11.6.5.3，PDF 1.4）与 `/SMaskInData`（`Table 89`，8.9.5，
//!   PDF 1.5）是**邻近子句**的条目，读端宽容地继续类型化它们，其余未知键
//!   原样落 `other`；
//! - `/FL` 边界沿用 `Table 57` 的 `i` 操作数「0..=100、0 表示输出设备默认
//!   平坦度」，而 10.6.2 只说 "positive number"、初值 1.0（`Table 53`）；
//! - 枚举码越界的消息须点出键名与期望区间；
//! - `STANDARD_ENTRIES` 与 `Table 58`（8.4.5）逐键对齐，`/DS` 一类非
//!   ExtGState 键不会被误吞。

use super::*;

/// Table 144 的五项之外，`/Matte` 与 `/SMaskInData` 仍被类型化（不进 `other`），
/// 真正的未知键才进 `other`。
#[test]
fn soft_mask_keeps_table_144_entries_and_tolerizes_the_image_clause_extras() {
    let document = document_with_objects(vec![(7, xobject_stream(Some("Form")))]);
    let mask = document
        .soft_mask(&Value::Dict(state(vec![
            ("Type", Value::Name("Mask".into())),
            ("S", Value::Name("Alpha".into())),
            ("G", Value::Ref(7, 0)),
            (
                "BC",
                Value::Array(Array(vec![Value::Integer(0), Value::Integer(1)])),
            ),
            ("TR", Value::Name("Identity".into())),
            ("SMaskInData", Value::Integer(1)),
            ("Matte", Value::Array(Array(vec![Value::Float(0.5)]))),
            ("Whatever", Value::Integer(42)),
        ])))
        .expect("Table 144 plus the tolerated extras should parse");

    assert_eq!(mask.subtype, SoftMaskSubtype::Alpha);
    assert_eq!(mask.group, Value::Ref(7, 0));
    assert_eq!(mask.backdrop, Some(vec![0.0, 1.0]));
    assert_eq!(mask.transfer, Some(Value::Name("Identity".into())));
    assert_eq!(mask.smask_in_data, Some(1));
    assert_eq!(mask.matte, Some(vec![0.5]));
    // Only the genuinely unknown entry is preserved verbatim; `/Type` is
    // consumed too (Table 144 allows it, and when present it shall be /Mask).
    assert_eq!(
        mask.other.get("Whatever"),
        Some(&Value::Integer(42)),
        "Table 144 entries and the two tolerated image-clause keys must not leak into other"
    );
    assert_eq!(mask.other.len(), 1, "only the unknown entry may be kept");
}

/// `/Matte` 与 `/SMaskInData` 都来自「可选」条目，缺失即 `None`，不报错。
#[test]
fn soft_mask_tolerated_extras_stay_optional() {
    let document = document_with_objects(vec![(7, xobject_stream(Some("Form")))]);
    let mask = document
        .soft_mask(&Value::Dict(state(vec![
            ("S", Value::Name("Luminosity".into())),
            ("G", Value::Ref(7, 0)),
        ])))
        .expect("a bare Table 144 dictionary is complete on its own");

    assert_eq!(mask.backdrop, None, "/BC is optional in Table 144");
    assert_eq!(mask.transfer, None, "/TR is optional in Table 144");
    assert_eq!(mask.smask_in_data, None);
    assert_eq!(mask.matte, None);
    assert!(mask.other.is_empty());
}

/// `/FL` 的 0 与 100 都被接受（`Table 57` 的 `i` 行），越界消息含键名与区间。
#[test]
fn flatness_accepts_the_operator_range_including_zero() {
    let document = Document::new();
    for (value, expected) in [
        (Value::Integer(0), 0.0),
        (Value::Float(1.0), 1.0),
        (Value::Integer(100), 100.0),
    ] {
        let state_dict = state(vec![("FL", value)]);
        let parsed = parse_ext_g_state(&document, &state_dict)
            .expect("flatness within 0..=100 must be accepted");
        assert_eq!(parsed.flatness, Some(expected));
        assert!(parsed.other.is_empty(), "/FL is a Table 58 entry");
    }

    assert_invalid(
        &document,
        vec![("FL", Value::Integer(101))],
        "/FL must be within 0..=100",
    );
    assert_invalid(
        &document,
        vec![("FL", Value::Float(-0.5))],
        "/FL must be within 0..=100",
    );
}

/// 枚举码越界：消息点名键与期望区间（含负数与 255 以上的整数）。
#[test]
fn out_of_range_codes_report_the_key_and_the_expected_range() {
    let document = Document::new();
    for (key, value, needle) in [
        (
            "LC",
            Value::Integer(3),
            "/LC must be an integer within 0..=2, got 3",
        ),
        (
            "LJ",
            Value::Integer(-1),
            "/LJ must be an integer within 0..=2, got -1",
        ),
        (
            "OPM",
            Value::Integer(2),
            "/OPM must be an integer within 0..=1, got 2",
        ),
        (
            "LC",
            Value::Integer(300),
            "/LC must be an integer within 0..=2, got 300",
        ),
    ] {
        assert_invalid(&document, vec![(key, value)], needle);
    }

    // A value that is not a number at all keeps reporting the type problem.
    assert_invalid(
        &document,
        vec![("LC", Value::Name("Round".into()))],
        "/LC must be an integer",
    );
}

/// `Table 58`（8.4.5）的 26 个参数键全部被类型化，未知键（如 `/DS`）落 `other`。
#[test]
fn every_table_58_entry_is_typed_and_nothing_else_is_claimed() {
    let document = Document::new();
    let dict = state(vec![
        ("LW", Value::Integer(1)),
        ("LC", Value::Integer(1)),
        ("LJ", Value::Integer(1)),
        ("ML", Value::Integer(2)),
        (
            "D",
            Value::Array(Array(vec![
                Value::Array(Array(vec![Value::Integer(3)])),
                Value::Integer(0),
            ])),
        ),
        ("RI", Value::Name("RelativeColorimetric".into())),
        ("OP", Value::Bool(true)),
        ("op", Value::Bool(false)),
        ("OPM", Value::Integer(1)),
        (
            "Font",
            Value::Array(Array(vec![Value::Dict(Dict::new()), Value::Integer(12)])),
        ),
        ("BG", Value::Name("Default".into())),
        ("BG2", Value::Name("Default".into())),
        ("UCR", Value::Name("Default".into())),
        ("UCR2", Value::Name("Default".into())),
        ("TR", Value::Name("Identity".into())),
        ("TR2", Value::Name("Default".into())),
        ("HT", Value::Dict(Dict::new())),
        ("FL", Value::Integer(1)),
        ("SM", Value::Float(0.5)),
        ("SA", Value::Bool(true)),
        ("BM", Value::Name("Normal".into())),
        ("SMask", Value::Name("None".into())),
        ("CA", Value::Integer(1)),
        ("ca", Value::Integer(1)),
        ("AIS", Value::Bool(true)),
        ("TK", Value::Bool(false)),
    ]);

    let parsed = parse_ext_g_state(&document, &dict).expect("a full Table 58 dictionary");
    assert!(
        parsed.other.is_empty(),
        "Table 58 lists exactly these 26 keys, got leftovers {:?}",
        parsed.other.keys().collect::<Vec<_>>()
    );
    assert_eq!(parsed.line_width, Some(1.0));
    assert_eq!(
        parsed.rendering_intent.as_deref(),
        Some("RelativeColorimetric")
    );
    assert_eq!(parsed.blend_mode, Some(BlendMode::Name("Normal".into())));
    assert_eq!(parsed.soft_mask, Some(Value::Name("None".into())));
    assert_eq!(parsed.halftone, Some(Value::Dict(Dict::new())));

    // `/DS` is a free-text annotation and variable-text field entry, not a
    // graphics state key: it must survive as an unknown entry, untouched.
    let with_unknown = state(vec![("DS", Value::Integer(1)), ("LW", Value::Integer(1))]);
    let parsed = parse_ext_g_state(&document, &with_unknown).expect("unknown keys are kept");
    assert_eq!(parsed.other.get("DS"), Some(&Value::Integer(1)));
    assert_eq!(parsed.line_width, Some(1.0));
}
