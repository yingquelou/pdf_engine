//! 3D 视图字典（Table 304）低层测试：矩阵/路径约束、上限与条目保留。

use crate::core::types::{Array, Dict, Value};

use super::super::ThreeDError;
use super::super::lighting::{ThreeDLightingScheme, ThreeDLightingSchemeType};
use super::super::render_mode::{ThreeDRenderMode, ThreeDRenderModeType};
use super::super::view::{ThreeDMatrix, ThreeDMatrixSource, parse_view};
use super::{document, numbers, view};

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
            .insert(
                "RM",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("3DRenderMode".into()))
                        .insert("Subtype", Value::Name("Wireframe".into())),
                ),
            )
            .insert(
                "LS",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("3DLightingScheme".into()))
                        .insert("Subtype", Value::Name("Artwork".into())),
                ),
            )
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
    assert_eq!(
        parsed.render_mode,
        Some(ThreeDRenderMode {
            subtype: ThreeDRenderModeType::Wireframe,
            auxiliary_color: None,
            face_color: None,
            opacity: None,
            crease_value: None,
        })
    );
    assert_eq!(
        parsed.lighting,
        Some(ThreeDLightingScheme {
            subtype: ThreeDLightingSchemeType::Artwork,
        })
    );
    assert_eq!(parsed.cross_sections, Some(Vec::new()));
    assert_eq!(parsed.nodes, Some(Vec::new()));
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

/// 13.6.5 的 12 数矩阵：绕 z 轴 +90°，相机位于世界 (10, -3, 2)。
fn camera_to_world() -> [f64; 12] {
    [
        0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 10.0, -3.0, 2.0,
    ]
}

#[test]
fn three_d_matrix_maps_points_per_13_6_5() {
    // 平移矩阵：x' = x + 5、y' = y - 2、z' = z + 1.5。
    let translation = ThreeDMatrix::new([
        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 5.0, -2.0, 1.5,
    ])
    .expect("translation matrix");
    assert_eq!(translation.transform_point([0.0, 0.0, 0.0]), [5.0, -2.0, 1.5]);
    assert_eq!(translation.transform_point([1.0, 2.0, 3.0]), [6.0, 0.0, 4.5]);

    // 13.6.5 的行向量约定：x' = a·x + d·y + g·z + tx，故 +90° 绕 z 轴把
    // 线性部分的 (1, 0, 0) 送到 (0, 1, 0)、(0, 1, 0) 送到 (-1, 0, 0)，
    // 再叠加相机平移 (10, -3, 2)。
    let rotation = ThreeDMatrix::new(camera_to_world()).expect("rotation matrix");
    assert_eq!(rotation.transform_point([1.0, 0.0, 0.0]), [10.0, -2.0, 2.0]);
    assert_eq!(rotation.transform_point([0.0, 1.0, 0.0]), [9.0, -3.0, 2.0]);
    assert_eq!(rotation.transform_point([0.0, 0.0, 2.0]), [10.0, -3.0, 4.0]);

    // 恒等变换逐分量保持。
    assert_eq!(
        ThreeDMatrix::IDENTITY.transform_point([3.0, -4.0, 5.5]),
        [3.0, -4.0, 5.5]
    );
    assert_eq!(ThreeDMatrix::IDENTITY.entries(), [
        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0
    ]);
}

#[test]
fn three_d_matrix_inverse_maps_world_back_to_camera() {
    let world_to_camera = ThreeDMatrix::new(camera_to_world())
        .expect("camera matrix")
        .inverse()
        .expect("invertible matrix");
    // 线性部分转置求逆、平移部分取 -t·A⁻¹。
    assert_eq!(world_to_camera.entries(), [
        0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3.0, 10.0, -2.0
    ]);
    // 世界坐标中的相机位置 (10, -3, 2) 就是相机坐标原点。
    assert_eq!(
        world_to_camera.transform_point([10.0, -3.0, 2.0]),
        [0.0, 0.0, 0.0]
    );
    // 世界→相机→世界 往返保持原值。
    let world = [1.5, -2.0, 0.25];
    let camera = world_to_camera.transform_point(world);
    assert_eq!(camera, [1.0, 8.5, -1.75]);
    assert_eq!(
        ThreeDMatrix::new(camera_to_world())
            .expect("camera matrix")
            .transform_point(camera),
        world
    );
}

#[test]
fn three_d_matrix_composes_artwork_and_camera_transformations() {
    // 13.6.5 步骤 c：[xa ya za 1] × (aw × (cw)⁻¹)。
    let artwork_to_world = ThreeDMatrix::new([
        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0,
    ])
    .expect("artwork matrix");
    let world_to_camera = ThreeDMatrix::new(camera_to_world())
        .expect("camera matrix")
        .inverse()
        .expect("invertible matrix");
    let artwork_to_camera = artwork_to_world
        .compose(&world_to_camera)
        .expect("composed matrix");
    let point = [2.0, -1.0, 0.5];
    assert_eq!(artwork_to_camera.transform_point(point), [2.0, 7.0, -1.5]);
    assert_eq!(
        artwork_to_camera.transform_point(point),
        world_to_camera.transform_point(artwork_to_world.transform_point(point))
    );
}

#[test]
fn three_d_matrix_rejects_non_finite_entries_and_degenerate_matrices() {
    let error = ThreeDMatrix::new([f64::NAN; 12]).expect_err("NaN entries");
    assert!(error.to_string().contains("finite"), "{error}");

    let mut infinite = ThreeDMatrix::IDENTITY.entries();
    infinite[4] = f64::INFINITY;
    let error = ThreeDMatrix::new(infinite).expect_err("infinite entries");
    assert!(error.to_string().contains("finite"), "{error}");

    // 线性部分全 0：行列式为 0，求逆报错但映射仍可计算（只剩平移）。
    let degenerate = ThreeDMatrix::new([
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0,
    ])
    .expect("degenerate matrix is still a valid value");
    assert_eq!(degenerate.transform_point([1.0, 1.0, 1.0]), [1.0, 2.0, 3.0]);
    let error = degenerate.inverse().expect_err("singular matrix");
    assert!(error.to_string().contains("singular"), "{error}");

    // 线性部分行线性相关（第二行 = 2 × 第一行）同样是退化矩阵。
    let dependent = ThreeDMatrix::new([
        1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
    ])
    .expect("dependent matrix is still a valid value");
    let error = dependent.inverse().expect_err("dependent rows");
    assert!(error.to_string().contains("singular"), "{error}");
}

#[test]
fn view_exposes_camera_to_world_matrix() {
    let parsed = parse_view(
        &document(),
        &view("M")
            .insert("MS", Value::Name("M".into()))
            .insert("C2W", numbers(&camera_to_world())),
        "view",
    )
    .expect("matrix view");
    let matrix = parsed
        .camera_to_world_matrix()
        .expect("/MS /M exposes /C2W matrix");
    assert_eq!(matrix.transform_point([0.0, 0.0, 0.0]), [10.0, -3.0, 2.0]);

    let mut nan = vec![Value::Float(0.0); 12];
    nan[3] = Value::Float(f64::NAN);
    let error = parse_view(
        &document(),
        &view("M")
            .insert("MS", Value::Name("M".into()))
            .insert("C2W", Value::Array(Array(nan))),
        "view",
    )
    .expect_err("non-finite matrix entry");
    assert!(error.to_string().contains("finite"), "{error}");
    assert!(parsed.camera_to_world_matrix().is_some());
}
