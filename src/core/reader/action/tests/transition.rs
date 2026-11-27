//! Transition 动作测试（12.6.4.14）。

use crate::core::types::{Dict, Value};

use super::*;

#[test]
fn parses_transition_action() {
    use crate::core::reader::transition::{
        Transition, TransitionDimension, TransitionDirection, TransitionMotion, TransitionStyle,
    };

    let doc = document_with_pages(1);

    let split = Value::Dict(
        action_dict("Trans").insert(
            "Trans",
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Trans"))
                    .insert("S", name("Split"))
                    .insert("D", Value::Float(2.5))
                    .insert("Dm", name("V"))
                    .insert("M", name("O")),
            ),
        ),
    );
    assert_eq!(
        doc.action(&split).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::Split,
            duration: Some(2.5),
            dimension: Some(TransitionDimension::Vertical),
            motion: Some(TransitionMotion::Outward),
            direction: None,
            scale: None,
            border: None,
        })
    );

    let fly = Value::Dict(
        action_dict("Trans").insert(
            "Trans",
            Value::Dict(
                Dict::new()
                    .insert("S", name("Fly"))
                    .insert("D", Value::Integer(3))
                    .insert("Di", name("None"))
                    .insert("SS", Value::Float(0.25))
                    .insert("B", Value::Bool(true)),
            ),
        ),
    );
    assert_eq!(
        doc.action(&fly).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::Fly,
            duration: Some(3.0),
            dimension: None,
            motion: None,
            direction: Some(TransitionDirection::None),
            scale: Some(0.25),
            border: Some(true),
        })
    );

    let wipe = Value::Dict(
        action_dict("Trans").insert(
            "Trans",
            Value::Dict(
                Dict::new()
                    .insert("S", name("Wipe"))
                    .insert("Di", Value::Integer(90)),
            ),
        ),
    );
    assert_eq!(
        doc.action(&wipe).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::Wipe,
            duration: None,
            dimension: None,
            motion: None,
            direction: Some(TransitionDirection::Degrees(90)),
            scale: None,
            border: None,
        })
    );

    let defaulted = Value::Dict(action_dict("Trans").insert(
        "Trans",
        Value::Dict(Dict::new().insert("D", Value::Integer(0))),
    ));
    assert_eq!(
        doc.action(&defaulted).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::R,
            duration: Some(0.0),
            dimension: None,
            motion: None,
            direction: None,
            scale: None,
            border: None,
        })
    );

    let unknown = Value::Dict(action_dict("Trans").insert(
        "Trans",
        Value::Dict(Dict::new().insert("S", name("Spiral"))),
    ));
    assert_eq!(
        doc.action(&unknown).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::Other("Spiral".into()),
            duration: None,
            dimension: None,
            motion: None,
            direction: None,
            scale: None,
            border: None,
        })
    );
}

#[test]
fn malformed_transition_actions_are_typed_errors() {
    let doc = document_with_pages(1);

    let cases = [
        Value::Dict(action_dict("Trans")),
        Value::Dict(action_dict("Trans").insert("Trans", Value::Integer(1))),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("Type", name("Foo"))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("S", Value::Integer(1))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("D", Value::String("2".into()))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("D", Value::Float(-1.0))),
        )),
        Value::Dict(
            action_dict("Trans").insert("Trans", Value::Dict(Dict::new().insert("Dm", name("X")))),
        ),
        Value::Dict(
            action_dict("Trans").insert("Trans", Value::Dict(Dict::new().insert("M", name("X")))),
        ),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("Di", Value::Integer(45))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("Di", name("Horizontal"))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("SS", Value::Bool(true))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("SS", Value::Float(0.0))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("B", Value::Integer(1))),
        )),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}

#[test]
fn transition_applicability_issues_follow_table_162() {
    use crate::core::reader::transition::{
        TransitionApplicabilityIssue as Issue, TransitionDimension, TransitionDirection,
        TransitionMotion, TransitionStyle,
    };

    // Split 的合法组合无诊断。
    let valid = Transition {
        style: TransitionStyle::Split,
        duration: None,
        dimension: Some(TransitionDimension::Horizontal),
        motion: Some(TransitionMotion::Inward),
        direction: None,
        scale: None,
        border: None,
    };
    assert!(valid.applicability_issues().is_empty());

    // R + Dm/M/Di(None)/SS/B：五条适用性规则全部命中。
    let invalid = Transition {
        style: TransitionStyle::R,
        duration: None,
        dimension: Some(TransitionDimension::Vertical),
        motion: Some(TransitionMotion::Outward),
        direction: Some(TransitionDirection::None),
        scale: Some(0.5),
        border: Some(false),
    };
    assert_eq!(
        invalid.applicability_issues(),
        vec![
            Issue::Dimension,
            Issue::Motion,
            Issue::Direction,
            Issue::DirectionNone,
            Issue::Scale,
            Issue::Border,
        ]
    );

    // /Di 90 仅 Wipe、315 仅 Glitter。
    let wipe_315 = Transition {
        style: TransitionStyle::Wipe,
        duration: None,
        dimension: None,
        motion: None,
        direction: Some(TransitionDirection::Degrees(315)),
        scale: None,
        border: None,
    };
    assert_eq!(wipe_315.applicability_issues(), vec![Issue::Direction315]);
    let glitter_90 = Transition {
        style: TransitionStyle::Glitter,
        duration: None,
        dimension: None,
        motion: None,
        direction: Some(TransitionDirection::Degrees(90)),
        scale: None,
        border: None,
    };
    assert_eq!(
        glitter_90.applicability_issues(),
        vec![Issue::Direction90Or180]
    );

    // 未知样式不判不适用，避免误报。
    let custom = Transition {
        style: TransitionStyle::Other("Vendor".into()),
        duration: None,
        dimension: Some(TransitionDimension::Horizontal),
        motion: Some(TransitionMotion::Inward),
        direction: Some(TransitionDirection::Degrees(45)),
        scale: None,
        border: None,
    };
    assert!(custom.applicability_issues().is_empty());
}
