//! Tests for the `/DW2` and `/W2` vertical metric tables (Table 116).

use crate::core::types::{Array, Document, Value};

use super::super::{DefaultVerticalMetrics, VerticalMetrics};
use super::cid_font;

#[test]
fn parses_vertical_metrics_from_dw2_and_w2() {
    let document = Document::new();
    let font = cid_font(vec![
        (
            "W2",
            Value::Array(
                Array::new()
                    .push(Value::Integer(1))
                    .push(Value::Array(
                        Array::new()
                            .push(Value::Integer(-500))
                            .push(Value::Integer(250))
                            .push(Value::Integer(700))
                            .push(Value::Integer(-600))
                            .push(Value::Integer(300))
                            .push(Value::Integer(800)),
                    ))
                    .push(Value::Integer(5))
                    .push(Value::Integer(6))
                    .push(Value::Integer(-700))
                    .push(Value::Integer(350))
                    .push(Value::Integer(900)),
            ),
        ),
        (
            "DW2",
            Value::Array(
                Array::new()
                    .push(Value::Integer(900))
                    .push(Value::Integer(-800)),
            ),
        ),
        (
            "W",
            Value::Array(
                Array::new()
                    .push(Value::Integer(2))
                    .push(Value::Array(Array::new().push(Value::Integer(600)))),
            ),
        ),
    ]);

    let font = document.cid_font_type2(&font).unwrap().unwrap();
    assert_eq!(
        font.default_vertical_metrics(),
        DefaultVerticalMetrics {
            v_y: 900.0,
            w1_y: -800.0
        }
    );
    assert_eq!(
        font.vertical_metrics_for_cid(1),
        VerticalMetrics {
            w1_y: -500.0,
            v1x: 250.0,
            v1y: 700.0
        }
    );
    assert_eq!(
        font.vertical_metrics_for_cid(2),
        VerticalMetrics {
            w1_y: -600.0,
            v1x: 300.0,
            v1y: 800.0
        }
    );
    // CID 3 has no /W2 entry: /DW2 supplies w1_y and v_y, while v_x
    // defaults to half the horizontal width (/DW = 1000 here).
    assert_eq!(
        font.vertical_metrics_for_cid(3),
        VerticalMetrics {
            w1_y: -800.0,
            v1x: 500.0,
            v1y: 900.0
        }
    );
    assert_eq!(
        font.vertical_metrics_for_cid(5),
        VerticalMetrics {
            w1_y: -700.0,
            v1x: 350.0,
            v1y: 900.0
        }
    );
    assert_eq!(
        font.vertical_metrics_for_cid(6),
        VerticalMetrics {
            w1_y: -700.0,
            v1x: 350.0,
            v1y: 900.0
        }
    );
}

#[test]
fn defaults_vertical_metrics_and_uses_half_width_for_vx() {
    let document = Document::new();
    let font = cid_font(vec![(
        "W",
        Value::Array(
            Array::new()
                .push(Value::Integer(7))
                .push(Value::Array(Array::new().push(Value::Integer(620)))),
        ),
    )]);

    let font = document.cid_font_type2(&font).unwrap().unwrap();
    assert_eq!(
        font.default_vertical_metrics(),
        DefaultVerticalMetrics::default()
    );
    assert_eq!(
        font.vertical_metrics_for_cid(7),
        VerticalMetrics {
            w1_y: -1000.0,
            v1x: 310.0,
            v1y: 880.0
        }
    );
    assert_eq!(
        font.vertical_metrics_for_cid(9),
        VerticalMetrics {
            w1_y: -1000.0,
            v1x: 500.0,
            v1y: 880.0
        }
    );
}

#[test]
fn overlapping_w2_entries_use_the_last_value() {
    let document = Document::new();
    let font = cid_font(vec![(
        "W2",
        Value::Array(
            Array::new()
                .push(Value::Integer(4))
                .push(Value::Array(
                    Array::new()
                        .push(Value::Integer(-100))
                        .push(Value::Integer(200))
                        .push(Value::Integer(300)),
                ))
                .push(Value::Integer(4))
                .push(Value::Integer(4))
                .push(Value::Integer(-400))
                .push(Value::Integer(500))
                .push(Value::Integer(600)),
        ),
    )]);

    let font = document.cid_font_type2(&font).unwrap().unwrap();
    assert_eq!(
        font.vertical_metrics_for_cid(4),
        VerticalMetrics {
            w1_y: -400.0,
            v1x: 500.0,
            v1y: 600.0
        },
        "后出现的 /W2 条目应覆盖先前的 CID 4"
    );
}

#[test]
fn rejects_invalid_vertical_metrics() {
    let document = Document::new();
    let not_array = cid_font(vec![("DW2", Value::Integer(880))]);
    assert!(document.cid_font_type2(&not_array).is_err());

    let short_dw2 = cid_font(vec![(
        "DW2",
        Value::Array(Array::new().push(Value::Integer(880))),
    )]);
    assert!(document.cid_font_type2(&short_dw2).is_err());

    let non_numeric_dw2 = cid_font(vec![(
        "DW2",
        Value::Array(
            Array::new()
                .push(Value::Name("Tall".into()))
                .push(Value::Integer(-1000)),
        ),
    )]);
    assert!(document.cid_font_type2(&non_numeric_dw2).is_err());

    let not_array_w2 = cid_font(vec![("W2", Value::Integer(1))]);
    assert!(document.cid_font_type2(&not_array_w2).is_err());

    let empty_triples = cid_font(vec![(
        "W2",
        Value::Array(
            Array::new()
                .push(Value::Integer(1))
                .push(Value::Array(Array::new())),
        ),
    )]);
    assert!(document.cid_font_type2(&empty_triples).is_err());

    let short_triples = cid_font(vec![(
        "W2",
        Value::Array(
            Array::new().push(Value::Integer(1)).push(Value::Array(
                Array::new()
                    .push(Value::Integer(-500))
                    .push(Value::Integer(250)),
            )),
        ),
    )]);
    assert!(document.cid_font_type2(&short_triples).is_err());

    let reversed_range = cid_font(vec![(
        "W2",
        Value::Array(
            Array::new()
                .push(Value::Integer(5))
                .push(Value::Integer(4))
                .push(Value::Integer(-500))
                .push(Value::Integer(250))
                .push(Value::Integer(700)),
        ),
    )]);
    assert!(document.cid_font_type2(&reversed_range).is_err());

    let truncated_range = cid_font(vec![(
        "W2",
        Value::Array(
            Array::new()
                .push(Value::Integer(5))
                .push(Value::Integer(6))
                .push(Value::Integer(-500)),
        ),
    )]);
    assert!(document.cid_font_type2(&truncated_range).is_err());

    let truncated_entry = cid_font(vec![(
        "W2",
        Value::Array(Array::new().push(Value::Integer(5))),
    )]);
    assert!(document.cid_font_type2(&truncated_entry).is_err());
}
