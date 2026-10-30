//! Go-To-3D-View 动作读端测试（12.6.4.15 Table 216）。

use crate::core::reader::three_d::{ThreeDMatrixSource, ThreeDView};

use super::*;

const THREE_D_ANNOTATION: i64 = 40;

fn three_d_annotation() -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("Annot"))
            .insert("Subtype", name("3D"))
            .insert("Rect", arr(vec![Value::Integer(0), Value::Integer(0)])),
    )
}

fn go_to_3d_view(target: i64, view: Value) -> Value {
    Value::Dict(
        action_dict("GoTo3DView")
            .insert("TA", Value::Ref(target, 0))
            .insert("V", view),
    )
}

fn inline_view() -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("3DView"))
            .insert("XN", Value::String("Top".into()))
            .insert("IN", Value::String("top".into()))
            .insert("MS", name("M"))
            .insert("C2W", arr(vec![Value::Float(1.0); 12])),
    )
}

fn parsed(action: &Value) -> (i64, i64, GoTo3DViewSelection) {
    let doc = document_with_pages(1);
    insert(&doc, THREE_D_ANNOTATION, three_d_annotation());
    match doc.action(action).expect("parse GoTo3DView action") {
        Action::GoTo3DView(action) => (action.target.0, action.target.1, action.view),
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn parses_go_to_3d_view_action_selection_forms() {
    let forms = [
        (Value::Integer(1), GoTo3DViewSelection::Index(1)),
        (
            Value::HexString("6261636B".into()),
            GoTo3DViewSelection::Name(Value::HexString("6261636B".into())),
        ),
        (name("F"), GoTo3DViewSelection::First),
        (name("L"), GoTo3DViewSelection::Last),
        (name("N"), GoTo3DViewSelection::Next),
        (name("P"), GoTo3DViewSelection::Previous),
        (name("D"), GoTo3DViewSelection::Default),
        (name("X"), GoTo3DViewSelection::Other("X".to_owned())),
    ];
    for (value, expected) in forms {
        let (number, generation, view) = parsed(&go_to_3d_view(THREE_D_ANNOTATION, value));
        assert_eq!((number, generation), (THREE_D_ANNOTATION, 0));
        assert_eq!(view, expected);
    }

    // 内联视图字典按 Table 304 解析。
    let (_, _, view) = parsed(&go_to_3d_view(THREE_D_ANNOTATION, inline_view()));
    let GoTo3DViewSelection::Inline(view) = view else {
        panic!("expected inline view");
    };
    assert_eq!(
        view,
        Box::new(ThreeDView {
            external_name: Value::String("Top".into()),
            internal_name: Some(Value::String("top".into())),
            matrix_source: Some(ThreeDMatrixSource::M),
            camera_to_world: Some([1.0; 12]),
            u3d_path: None,
            center_of_orbit: None,
            projection: None,
            overlay: None,
            background: None,
            render_mode: None,
            lighting: None,
            cross_sections: None,
            nodes: None,
            restore_nodes: None,
        })
    );
}

#[test]
fn malformed_go_to_3d_view_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    insert(&doc, THREE_D_ANNOTATION, three_d_annotation());
    // `/TA` 目标不是字典、`/Subtype` 非 3D 的对象。
    insert(&doc, 41, Value::Integer(7));
    insert(
        &doc,
        42,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Annot"))
                .insert("Subtype", name("Movie")),
        ),
    );

    let cases = [
        // 缺 `/TA`。
        Value::Dict(action_dict("GoTo3DView").insert("V", Value::Integer(0))),
        // `/TA` 不是间接引用。
        Value::Dict(
            action_dict("GoTo3DView")
                .insert("TA", Value::Dict(Dict::new().insert("Subtype", name("3D"))))
                .insert("V", Value::Integer(0)),
        ),
        // `/TA` 引用不可解析。
        go_to_3d_view(99, Value::Integer(0)),
        // `/TA` 解析为非字典。
        go_to_3d_view(41, Value::Integer(0)),
        // `/TA` 目标是 Movie 注解。
        go_to_3d_view(42, Value::Integer(0)),
        // 缺 `/V`。
        Value::Dict(action_dict("GoTo3DView").insert("TA", Value::Ref(THREE_D_ANNOTATION, 0))),
        // `/V` 为布尔。
        go_to_3d_view(THREE_D_ANNOTATION, Value::Bool(true)),
        // `/V` 为负整数。
        go_to_3d_view(THREE_D_ANNOTATION, Value::Integer(-1)),
        // `/V` 为实数。
        go_to_3d_view(THREE_D_ANNOTATION, Value::Float(1.0)),
        // 内联视图缺 `/XN`。
        go_to_3d_view(
            THREE_D_ANNOTATION,
            Value::Dict(Dict::new().insert("Type", name("3DView"))),
        ),
    ];
    for case in cases {
        let error = doc.action(&case).expect_err("invalid GoTo3DView action");
        assert!(
            matches!(error, ActionError::Malformed(_)),
            "{case:?}: {error:?}"
        );
    }
}
