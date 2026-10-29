//! 外观特性字典 `/MK`（12.5.2，Table 189）与图标适配字典 `/IF`（Table 247）读取。

use super::support::*;
use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Object, Value};

fn numbers(values: &[f64]) -> Value {
    Value::Array(Array(
        values
            .iter()
            .map(|value| Value::Float(*value))
            .collect::<Vec<_>>(),
    ))
}

#[test]
fn appearance_characteristics_reads_table_189_entries() {
    let mk = Dict::new()
        .insert("R", Value::Integer(270))
        .insert("BC", numbers(&[0.1, 0.2, 0.3]))
        .insert("BG", numbers(&[0.5]))
        .insert("CA", Value::String("OK".into()))
        .insert("RC", Value::HexString("686f766572".into())) // "hover"
        .insert("AC", Value::String("down".into()))
        .insert("I", Value::Ref(5, 0))
        .insert("RI", Value::Ref(6, 0))
        .insert("IX", Value::Ref(7, 0))
        .insert("TP", Value::Integer(3));
    let dict = base_annotation("Screen").insert("MK", Value::Dict(mk));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("screen annotation");
    let mk = annotation
        .appearance_characteristics
        .expect("appearance characteristics");
    assert_eq!(mk.rotation, Some(270));
    assert_eq!(mk.border_color, Some(vec![0.1, 0.2, 0.3]));
    assert_eq!(mk.background_color, Some(vec![0.5]));
    assert_eq!(mk.caption.as_deref(), Some("OK"));
    assert_eq!(mk.rollover_caption.as_deref(), Some("hover"));
    assert_eq!(mk.alternate_caption.as_deref(), Some("down"));
    assert_eq!(mk.icon, Some(Value::Ref(5, 0)));
    assert_eq!(mk.rollover_icon, Some(Value::Ref(6, 0)));
    assert_eq!(mk.alternate_icon, Some(Value::Ref(7, 0)));
    assert_eq!(mk.text_position, Some(3));
    assert!(mk.icon_fit.is_none());

    // 空 `/BC` 数组按 Table 189 表示「无色/透明」，合法保留。
    let transparent = base_annotation("Widget").insert(
        "MK",
        Value::Dict(Dict::new().insert("BC", Value::Array(Array(Vec::new())))),
    );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &transparent).expect("transparent");
    assert_eq!(
        annotation
            .appearance_characteristics
            .expect("mk")
            .border_color,
        Some(Vec::new())
    );

    // 缺省值由渲染端应用，读端保持 `None`。
    let plain = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Screen"),
    )
    .expect("plain screen");
    assert!(plain.appearance_characteristics.is_none());
}

#[test]
fn icon_fit_dictionary_types_names_and_leftover() {
    let icon_fit = Dict::new()
        .insert("SW", Value::Name("S".into()))
        .insert("S", Value::Name("A".into()))
        .insert("A", numbers(&[0.0, 1.0]))
        .insert("FB", Value::Bool(true));
    let dict = base_annotation("Widget").insert(
        "MK",
        Value::Dict(Dict::new().insert("IF", Value::Dict(icon_fit))),
    );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("icon fit");
    let icon_fit = annotation
        .appearance_characteristics
        .expect("mk")
        .icon_fit
        .expect("icon fit dictionary");
    assert_eq!(icon_fit.scale_when, Some(IconScaleWhen::Smaller));
    assert_eq!(icon_fit.scaling, Some(IconScaling::Anamorphic));
    assert_eq!(icon_fit.leftover, Some([0.0, 1.0]));
    assert_eq!(icon_fit.fit_border, Some(true));

    // 未知名保留原文；缺省条目保持 `None`。
    let dict = base_annotation("Widget").insert(
        "MK",
        Value::Dict(
            Dict::new()
                .insert(
                    "IF",
                    Value::Dict(Dict::new().insert("SW", Value::Name("Z".into()))),
                )
                .insert("R", Value::Integer(0)),
        ),
    );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("unknown name");
    let mk = annotation.appearance_characteristics.expect("mk");
    assert_eq!(mk.rotation, Some(0));
    assert_eq!(
        mk.icon_fit.expect("icon fit").scale_when,
        Some(IconScaleWhen::Other("Z".into()))
    );
    assert_eq!(IconScaleWhen::Other("Z".into()).name(), "Z");
    assert_eq!(IconScaleWhen::from_name("N"), IconScaleWhen::Never);
    assert_eq!(IconScaleWhen::Never.name(), "N");
    assert_eq!(IconScaling::from_name("P"), IconScaling::Proportional);
    assert_eq!(IconScaling::Proportional.name(), "P");
    assert_eq!(IconScaling::from_name("Q"), IconScaling::Other("Q".into()));
}

#[test]
fn indirect_appearance_characteristics_are_resolved() {
    let document = doc(vec![
        catalog_object(),
        Object::new(
            9,
            0,
            Value::Dict(
                Dict::new()
                    .insert("R", Value::Integer(-90))
                    .insert("TP", Value::Integer(6)),
            ),
        ),
    ]);
    let dict = base_annotation("Screen").insert("MK", Value::Ref(9, 0));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("indirect /MK resolves");
    let mk = annotation.appearance_characteristics.expect("mk");
    assert_eq!(mk.rotation, Some(-90), "Table 189 只要求 90 的倍数");
    assert_eq!(mk.text_position, Some(6));
    assert_eq!(mk.border_color, None);
}

#[test]
fn malformed_appearance_characteristics_are_typed_errors() {
    let document = doc(vec![catalog_object()]);

    // `/MK` 非字典。
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Screen").insert("MK", Value::Integer(1)),
    )
    .expect_err("/MK integer");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/MK")),
        "{error:?}"
    );

    // `/R` 不是 90 的倍数，且必须是整数。
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Screen").insert(
            "MK",
            Value::Dict(Dict::new().insert("R", Value::Integer(45))),
        ),
    )
    .expect_err("/R 45");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("multiple of 90")),
        "{error:?}"
    );
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Screen").insert(
            "MK",
            Value::Dict(Dict::new().insert("R", Value::Float(90.5))),
        ),
    )
    .expect_err("/R float");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/R")),
        "{error:?}"
    );

    // 颜色分量个数与取值范围。
    for (key, components) in [("BC", vec![0.5, 0.5]), ("BG", vec![2.0, 0.0, 0.0])] {
        let error = parse_annotation(
            &document,
            0,
            None,
            &base_annotation("Widget").insert(
                "MK",
                Value::Dict(Dict::new().insert(key, numbers(&components))),
            ),
        )
        .expect_err("bad color");
        assert!(
            matches!(&error, AnnotationError::Malformed(message) if message.contains(key)),
            "{error:?}"
        );
    }

    // `/I` 须为间接引用；`/TP` 须为 0–6 整数。
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Widget")
            .insert("MK", Value::Dict(Dict::new().insert("I", numbers(&[1.0])))),
    )
    .expect_err("direct /I");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/I")),
        "{error:?}"
    );
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Widget").insert(
            "MK",
            Value::Dict(Dict::new().insert("TP", Value::Integer(7))),
        ),
    )
    .expect_err("/TP 7");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/TP")),
        "{error:?}"
    );

    // `/IF` 须为字典，`/A` 须恰两元素，`/FB` 须布尔。
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Widget").insert(
            "MK",
            Value::Dict(Dict::new().insert("IF", Value::Integer(3))),
        ),
    )
    .expect_err("/IF integer");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/IF")),
        "{error:?}"
    );
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Widget").insert(
            "MK",
            Value::Dict(Dict::new().insert(
                "IF",
                Value::Dict(Dict::new().insert("A", numbers(&[0.5, 0.5, 0.5]))),
            )),
        ),
    )
    .expect_err("/A three numbers");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/A")),
        "{error:?}"
    );
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Widget").insert(
            "MK",
            Value::Dict(Dict::new().insert(
                "IF",
                Value::Dict(Dict::new().insert("FB", Value::Integer(1))),
            )),
        ),
    )
    .expect_err("/FB integer");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/FB")),
        "{error:?}"
    );

    // `/SW` 与 `/S` 非名字同样报错。
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Widget").insert(
            "MK",
            Value::Dict(Dict::new().insert(
                "IF",
                Value::Dict(Dict::new().insert("SW", Value::Integer(1))),
            )),
        ),
    )
    .expect_err("/SW integer");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/SW")),
        "{error:?}"
    );

    // `/CA` 是文本串，与通用 `/CA` 不透明度互不相干。
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Widget").insert(
            "MK",
            Value::Dict(Dict::new().insert("CA", Value::Integer(1))),
        ),
    )
    .expect_err("/MK /CA integer");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/CA")),
        "{error:?}"
    );
}
