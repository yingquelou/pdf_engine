//! `Document::ext_g_state` 与 `Document::ext_g_states` 入口：
//! 资源 `/ExtGState` 子字典、缺失资源与非字典值的拒绝。

use super::*;

#[test]
fn parses_ext_g_state_resource_dictionary() {
    let document = document_with_objects(vec![(
        11,
        Value::Dict(Dict::new().insert("ca", Value::Integer(1))),
    )]);
    let resources = Dict::new().insert(
        "ExtGState",
        Value::Dict(
            Dict::new()
                .insert(
                    "GS1",
                    Value::Dict(Dict::new().insert("LW", Value::Integer(1))),
                )
                .insert("GS2", Value::Ref(11, 0)),
        ),
    );

    let states = document
        .ext_g_states(&resources)
        .expect("resource dictionary should parse");

    assert_eq!(states.len(), 2);
    assert_eq!(states["GS1"].line_width, Some(1.0));
    assert_eq!(states["GS2"].fill_alpha, Some(1.0));
}

#[test]
fn returns_no_states_for_resources_without_ext_g_state() {
    let document = Document::new();
    let resources = Dict::new().insert("Font", Value::Dict(Dict::new()));

    let states = document
        .ext_g_states(&resources)
        .expect("absent /ExtGState should not fail");

    assert!(states.is_empty());
}

#[test]
fn rejects_non_dictionary_values() {
    let document = Document::new();

    let error = document
        .ext_g_state(&Value::Integer(1))
        .expect_err("must fail");
    assert!(
        matches!(&error, GraphicsStateError::Invalid(message) if message.contains("must be a dictionary")),
        "{error:?}"
    );

    let error = document
        .ext_g_state(&Value::Ref(42, 0))
        .expect_err("missing reference must fail");
    assert!(
        matches!(&error, GraphicsStateError::Invalid(message) if message.contains("unresolved object reference")),
        "{error:?}"
    );

    let resources = Dict::new().insert("ExtGState", Value::Integer(1));
    let error = document.ext_g_states(&resources).expect_err("must fail");
    assert!(
        matches!(&error, GraphicsStateError::Invalid(message) if message.contains("not a dictionary")),
        "{error:?}"
    );
}
