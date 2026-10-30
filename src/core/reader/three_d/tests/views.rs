//! 3D 视图字典（Table 304）低层测试：矩阵/路径约束、上限与条目保留。

use crate::core::types::{Array, Dict, Value};

use super::super::ThreeDError;
use super::super::view::{ThreeDMatrixSource, parse_view};
use super::{document, view};

fn texts(values: &[&str]) -> Value {
    Value::Array(Array(
        values
            .iter()
            .map(|value| Value::String((*value).into()))
            .collect(),
    ))
}

#[test]
fn view_matrix_and_path_requirements() {
    let matrix: Vec<Value> = (0..12).map(Value::Integer).collect();
    let parsed = parse_view(
        &document(),
        &view("M")
            .insert("MS", Value::Name("M".into()))
            .insert("C2W", Value::Array(Array(matrix.clone()))),
        "view",
    )
    .expect("matrix view");
    assert_eq!(parsed.matrix_source, Some(ThreeDMatrixSource::M));
    assert_eq!(
        parsed.camera_to_world,
        Some([0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0])
    );
    assert_eq!(parsed.u3d_path, None);

    for (dict, fragment) in [
        (view("M").insert("MS", Value::Name("M".into())), "/C2W"),
        (
            view("M")
                .insert("MS", Value::Name("M".into()))
                .insert("C2W", Value::Array(Array(matrix[..11].to_vec()))),
            "12",
        ),
    ] {
        let error = parse_view(&document(), &dict, "view").expect_err("invalid matrix");
        assert!(error.to_string().contains(fragment), "{error}");
    }

    let single = parse_view(
        &document(),
        &view("P")
            .insert("MS", Value::Name("U3D".into()))
            .insert("U3DPath", Value::String("Root".into())),
        "view",
    )
    .expect("single path");
    assert_eq!(single.u3d_path, Some(vec![Value::String("Root".into())]));

    let array = parse_view(
        &document(),
        &view("P")
            .insert("MS", Value::Name("U3D".into()))
            .insert("U3DPath", texts(&["Root", "Node"])),
        "view",
    )
    .expect("path array");
    assert_eq!(
        array.u3d_path,
        Some(vec![
            Value::String("Root".into()),
            Value::String("Node".into())
        ])
    );

    for (dict, fragment) in [
        (
            view("P").insert("MS", Value::Name("U3D".into())),
            "/U3DPath",
        ),
        (
            view("P")
                .insert("MS", Value::Name("U3D".into()))
                .insert("U3DPath", Value::Array(Array(Vec::new()))),
            "at least one",
        ),
        (
            view("P")
                .insert("MS", Value::Name("U3D".into()))
                .insert("U3DPath", Value::String(String::new())),
            "empty",
        ),
    ] {
        let error = parse_view(&document(), &dict, "view").expect_err("invalid path");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}

#[test]
fn view_path_limit_is_enforced() {
    let path: Vec<Value> = (0..=64)
        .map(|index| Value::String(format!("Node {index}")))
        .collect();
    let error = parse_view(
        &document(),
        &view("P")
            .insert("MS", Value::Name("U3D".into()))
            .insert("U3DPath", Value::Array(Array(path))),
        "view",
    )
    .expect_err("too long path");
    assert_eq!(
        error,
        ThreeDError::LimitExceeded("3D view /U3DPath"),
        "{error}"
    );
}

#[test]
fn view_ignores_irrelevant_matrix_entries_and_preserves_metadata() {
    let parsed = parse_view(
        &document(),
        &view("P")
            .insert("IN", Value::String("inner".into()))
            .insert("C2W", Value::Integer(7))
            .insert("U3DPath", Value::Integer(7))
            .insert("CO", Value::Float(3.5))
            .insert("O", Value::Ref(40, 0))
            .insert("RM", Value::Name("Wireframe".into()))
            .insert("LS", Value::Name("Artwork".into()))
            .insert("SA", Value::Array(Array(Vec::new())))
            .insert("NA", Value::Array(Array(Vec::new())))
            .insert("NR", Value::Bool(true)),
        "view",
    )
    .expect("tolerant view");
    assert_eq!(parsed.matrix_source, None);
    assert_eq!(parsed.camera_to_world, None);
    assert_eq!(parsed.u3d_path, None);
    assert_eq!(parsed.center_of_orbit, Some(3.5));
    assert_eq!(parsed.overlay, Some(Value::Ref(40, 0)));
    assert_eq!(parsed.render_mode, Some(Value::Name("Wireframe".into())));
    assert_eq!(parsed.lighting, Some(Value::Name("Artwork".into())));
    assert_eq!(parsed.cross_sections, Some(Value::Array(Array(Vec::new()))));
    assert_eq!(parsed.nodes, Some(Value::Array(Array(Vec::new()))));
    assert_eq!(parsed.restore_nodes, Some(true));
    assert_eq!(parsed.internal_name, Some(Value::String("inner".into())));

    let error = parse_view(
        &document(),
        &view("P").insert("CO", Value::Integer(-1)),
        "view",
    )
    .expect_err("negative orbit");
    assert!(error.to_string().contains("/CO"), "{error}");
}

#[test]
fn view_requires_name_and_type_is_checked() {
    for (dict, fragment) in [
        (
            Dict::new().insert("Type", Value::Name("3DView".into())),
            "/XN",
        ),
        (
            Dict::new()
                .insert("Type", Value::Name("Wrong".into()))
                .insert("XN", Value::String("View".into())),
            "/Type",
        ),
        (
            Dict::new()
                .insert("Type", Value::Name("3DView".into()))
                .insert("XN", Value::Integer(1)),
            "/XN",
        ),
    ] {
        let error = parse_view(&document(), &dict, "view").expect_err("malformed view");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}
