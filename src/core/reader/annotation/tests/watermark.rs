//! Watermark 注解专有条目读取（12.5.6.22，Table 190/191）：`/FixedPrint`
//! 固定打印字典的 `/Matrix`、`/H`、`/V`。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Object, Value};

use super::support::*;

fn numbers(values: &[f64]) -> Value {
    Value::Array(Array(
        values
            .iter()
            .map(|value| Value::Float(*value))
            .collect::<Vec<_>>(),
    ))
}

#[test]
fn watermark_annotation_reads_fixed_print_entries() {
    let document = doc(vec![catalog_object()]);
    let dict = base_annotation("Watermark").insert(
        "FixedPrint",
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FixedPrint".into()))
                .insert("Matrix", numbers(&[1.0, 0.0, 0.0, 1.0, 72.0, -72.0]))
                .insert("H", Value::Integer(0))
                .insert("V", Value::Float(1.0)),
        ),
    );
    let annotation = parse_annotation(&document, 0, None, &dict).expect("watermark annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::Watermark);
    let fixed = annotation.fixed_print.expect("/FixedPrint present");
    assert_eq!(fixed.matrix, Some([1.0, 0.0, 0.0, 1.0, 72.0, -72.0]));
    assert_eq!(fixed.horizontal, Some(0.0));
    assert_eq!(fixed.vertical, Some(1.0));
}

#[test]
fn fixed_print_defaults_are_not_materialized() {
    let document = doc(vec![catalog_object()]);
    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Watermark").insert("FixedPrint", Value::Dict(Dict::new())),
    )
    .expect("empty fixed print dictionary");
    let fixed = annotation.fixed_print.expect("/FixedPrint present");
    assert_eq!(
        fixed,
        FixedPrint::default(),
        "Table 191 的缺省（单位矩阵、/H /V 为 0）不物化"
    );
    assert_eq!(fixed.matrix, None);
    assert_eq!(fixed.horizontal, None);
    assert_eq!(fixed.vertical, None);

    let annotation = parse_annotation(&document, 0, None, &base_annotation("Watermark"))
        .expect("watermark without /FixedPrint");
    assert_eq!(
        annotation.fixed_print, None,
        "Table 190 的 /FixedPrint 是可选条目"
    );
}

#[test]
fn indirect_fixed_print_dictionary_is_resolved() {
    let document = doc(vec![
        catalog_object(),
        Object::new(
            10,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("FixedPrint".into()))
                    .insert("Matrix", numbers(&[2.0, 0.0, 0.0, 2.0, 0.0, 0.0]))
                    .insert("H", Value::Float(0.25)),
            ),
        ),
    ]);
    let dict = base_annotation("Watermark").insert("FixedPrint", Value::Ref(10, 0));
    let annotation =
        parse_annotation(&document, 0, None, &dict).expect("规范示例把 /FixedPrint 写成间接引用");
    let fixed = annotation.fixed_print.expect("resolved dictionary");
    assert_eq!(fixed.matrix, Some([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]));
    assert_eq!(fixed.horizontal, Some(0.25));
    assert_eq!(fixed.vertical, None);
}

#[test]
fn malformed_fixed_print_entries_are_typed_errors() {
    let document = doc(vec![catalog_object()]);

    // `/FixedPrint` 须为字典（可经间接引用解引用）。
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Watermark").insert("FixedPrint", Value::Integer(10)),
    )
    .expect_err("non-dictionary /FixedPrint");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/FixedPrint")),
        "{error:?}"
    );

    // `/Type` 存在时须为 FixedPrint。
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Watermark").insert(
            "FixedPrint",
            Value::Dict(Dict::new().insert("Type", Value::Name("Print".into()))),
        ),
    )
    .expect_err("wrong /Type");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("FixedPrint")),
        "{error:?}"
    );

    // `/Matrix` 须恰六项数值。
    for matrix in [
        numbers(&[1.0, 0.0, 0.0, 1.0]),
        numbers(&[1.0, 0.0, 0.0, 1.0, 0.0, 72.0, 0.0]),
    ] {
        let error = parse_annotation(
            &document,
            0,
            None,
            &base_annotation("Watermark").insert(
                "FixedPrint",
                Value::Dict(Dict::new().insert("Matrix", matrix)),
            ),
        )
        .expect_err("bad /Matrix");
        assert!(
            matches!(&error, AnnotationError::Malformed(message) if message.contains("/Matrix")),
            "{error:?}"
        );
    }

    // `/H` `/V` 须为数值。
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Watermark").insert(
            "FixedPrint",
            Value::Dict(Dict::new().insert("H", Value::Name("half".into()))),
        ),
    )
    .expect_err("non-numeric /H");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/H")),
        "{error:?}"
    );
}
