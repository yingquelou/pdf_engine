//! measure 字典读取（12.9，Table 261–263）：直角坐标条目、
//! 数字格式数组字段与间接引用解析。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Object, Value};

use super::support::*;

fn format_dict(unit: &str, conversion: f64) -> Value {
    Value::Dict(
        Dict::new()
            .insert("U", Value::String(unit.into()))
            .insert("C", Value::Float(conversion)),
    )
}

fn formats(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

#[test]
fn line_annotation_reads_measure_dictionary() {
    let x_format = Dict::new()
        .insert("U", Value::String("mi".into()))
        .insert("C", Value::Float(63360.0))
        .insert("F", Value::Name("F".into()))
        .insert("D", Value::Integer(16))
        .insert("FD", Value::Bool(true))
        .insert("RT", Value::String(",".into()))
        .insert("RD", Value::String(".".into()))
        .insert("PS", Value::String("".into()))
        .insert("SS", Value::String(" ".into()))
        .insert("O", Value::Name("P".into()));
    let measure = Dict::new()
        .insert("Type", Value::Name("Measure".into()))
        .insert("Subtype", Value::Name("RL".into()))
        .insert("R", Value::String("1 in = 0.1 mi".into()))
        .insert("X", formats(vec![Value::Dict(x_format)]))
        .insert("Y", formats(vec![format_dict("km", 2.54e-5)]))
        .insert("D", formats(vec![format_dict("mi", 63360.0)]))
        .insert("A", formats(vec![format_dict("acres", 4.047e-6)]))
        .insert("T", formats(vec![format_dict("deg", 1.0)]))
        .insert("S", formats(vec![format_dict("slope", 1.0)]))
        .insert(
            "O",
            Value::Array(Array(vec![Value::Integer(10), Value::Float(20.5)])),
        )
        .insert("CYX", Value::Float(1.5));
    let annotation = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Line").insert("Measure", Value::Dict(measure)),
    )
    .expect("line annotation with /Measure");
    let measure = annotation.measure.expect("/Measure");
    assert_eq!(measure.subtype.as_deref(), Some("RL"));
    assert_eq!(measure.ratio.as_deref(), Some("1 in = 0.1 mi"));

    let x = measure.x.as_ref().expect("/X");
    assert_eq!(x.len(), 1);
    assert_eq!(x[0].unit.as_deref(), Some("mi"));
    assert_eq!(x[0].conversion, Some(63360.0));
    assert_eq!(x[0].fraction, Some(NumberFraction::Fraction));
    assert_eq!(x[0].precision, Some(16));
    assert_eq!(x[0].force_denominator, Some(true));
    assert_eq!(x[0].thousands_separator.as_deref(), Some(","));
    assert_eq!(x[0].decimal_separator.as_deref(), Some("."));
    assert_eq!(x[0].prefix_spacing.as_deref(), Some(""));
    assert_eq!(x[0].suffix_spacing.as_deref(), Some(" "));
    assert_eq!(x[0].label_order, Some(NumberLabelOrder::Prefix));
    assert!(x[0].dict.get("C").is_some());

    assert_eq!(measure.y.as_ref().map(Vec::len), Some(1));
    assert_eq!(measure.distance.as_ref().map(Vec::len), Some(1));
    assert_eq!(measure.area.as_ref().map(Vec::len), Some(1));
    assert_eq!(measure.angle.as_ref().map(Vec::len), Some(1));
    assert_eq!(measure.slope.as_ref().map(Vec::len), Some(1));
    assert_eq!(measure.origin, Some([10.0, 20.5]));
    assert_eq!(measure.conversion_yx, Some(1.5));
    assert!(measure.dict.get("R").is_some());

    // 未声明 `/Measure` 的注解保留 `None`。
    let plain = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Line"),
    )
    .expect("line without /Measure");
    assert!(plain.measure.is_none());
}

#[test]
fn unknown_measure_names_are_preserved() {
    let measure = Dict::new()
        .insert("Subtype", Value::Name("Curved".into()))
        .insert("R", Value::String("1:1".into()))
        .insert(
            "X",
            formats(vec![Value::Dict(
                Dict::new()
                    .insert("U", Value::String("u".into()))
                    .insert("C", Value::Integer(1))
                    .insert("F", Value::Name("X".into()))
                    .insert("O", Value::Name("M".into())),
            )]),
        );
    let annotation = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("PolyLine").insert("Measure", Value::Dict(measure)),
    )
    .expect("nonstandard measure names");
    let measure = annotation.measure.expect("/Measure");
    assert_eq!(measure.subtype.as_deref(), Some("Curved"));
    let format = &measure.x.as_ref().expect("/X")[0];
    assert_eq!(format.fraction, Some(NumberFraction::Other("X".into())));
    assert_eq!(
        format.label_order,
        Some(NumberLabelOrder::Other("M".into()))
    );
}

#[test]
fn malformed_measure_entries_are_rejected() {
    let with_measure =
        |measure: Dict| base_annotation("Line").insert("Measure", Value::Dict(measure));
    let cases = vec![
        base_annotation("Line").insert("Measure", Value::Integer(3)),
        with_measure(Dict::new().insert("X", Value::Integer(1))),
        // 数组元素不是字典
        with_measure(Dict::new().insert("X", formats(vec![Value::Integer(1)]))),
        // `/O` 原点不是两个数值
        with_measure(Dict::new().insert("O", formats(vec![Value::Integer(1)]))),
        with_measure(Dict::new().insert("O", Value::Integer(1))),
        // `/CYX` 不是数值
        with_measure(Dict::new().insert("CYX", Value::Name("x".into()))),
        // 数字格式取值类型错误
        with_measure(Dict::new().insert(
            "X",
            formats(vec![Value::Dict(
                Dict::new().insert("U", Value::Integer(1)),
            )]),
        )),
        with_measure(Dict::new().insert(
            "X",
            formats(vec![Value::Dict(
                Dict::new().insert("C", Value::Name("x".into())),
            )]),
        )),
        with_measure(Dict::new().insert(
            "X",
            formats(vec![Value::Dict(
                Dict::new().insert("F", Value::Integer(1)),
            )]),
        )),
        with_measure(Dict::new().insert(
            "X",
            formats(vec![Value::Dict(
                Dict::new().insert("D", Value::Float(1.5)),
            )]),
        )),
        with_measure(Dict::new().insert(
            "X",
            formats(vec![Value::Dict(
                Dict::new().insert("FD", Value::Integer(1)),
            )]),
        )),
        with_measure(Dict::new().insert("Subtype", Value::Integer(1))),
    ];
    for dict in cases {
        assert!(
            matches!(
                parse_annotation(&doc(vec![]), 0, None, &dict),
                Err(AnnotationError::Malformed(_))
            ),
            "case must be malformed: {dict:?}"
        );
    }
}

#[test]
fn measure_resolves_indirect_dictionary_and_number_formats() {
    let annotation = Object::new(
        4,
        0,
        Value::Dict(base_annotation("Polygon").insert("Measure", Value::Ref(5, 0))),
    );
    let measure = Object::new(
        5,
        0,
        Value::Dict(
            Dict::new()
                .insert("R", Value::String("1 cm = 1 m".into()))
                .insert("X", formats(vec![Value::Ref(6, 0)]))
                .insert("D", formats(vec![Value::Ref(6, 0)]))
                .insert("A", formats(vec![Value::Ref(6, 0)])),
        ),
    );
    let format = Object::new(
        6,
        0,
        Value::Dict(
            Dict::new()
                .insert("U", Value::String("m".into()))
                .insert("C", Value::Integer(100)),
        ),
    );
    let document = doc_with_annots(vec![Value::Ref(4, 0)], vec![annotation, measure, format]);
    let annotations = document.page_annotations(0).expect("annotations");
    let measure = annotations[0].measure.as_ref().expect("/Measure");
    assert_eq!(measure.ratio.as_deref(), Some("1 cm = 1 m"));
    let x = measure.x.as_ref().expect("/X");
    assert_eq!(x[0].unit.as_deref(), Some("m"));
    assert_eq!(x[0].conversion, Some(100.0));
    assert_eq!(measure.distance.as_ref().map(Vec::len), Some(1));
    assert_eq!(measure.area.as_ref().map(Vec::len), Some(1));
}
