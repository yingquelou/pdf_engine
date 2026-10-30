//! 动作字典写出的单元测试（12.6 与 12.7.5）。

use super::*;
use crate::core::reader::navigation::Destination;

#[test]
fn goto_action_writes_dictionary() {
    let action = Action::GoTo(Destination::Fit { page: 0 });
    let dict = action.to_dict(&[], &[]).unwrap();
    assert_eq!(dict.get("S"), Some(&Value::Name("GoTo".into())));
    assert!(dict.get("D").is_some());
}

#[test]
fn uri_action_writes_string() {
    let action = Action::Uri("https://example.com".into());
    let dict = action.to_dict(&[], &[]).unwrap();
    assert_eq!(dict.get("S"), Some(&Value::Name("URI".into())));
    assert!(matches!(dict.get("URI"), Some(Value::HexString(_))));
}

#[test]
fn launch_action_writes_new_window() {
    let action = Action::Launch {
        file: ActionFileSpec::new("viewer.exe"),
        new_window: Some(true),
    };
    let dict = action.to_dict(&[], &[]).unwrap();
    assert_eq!(dict.get("S"), Some(&Value::Name("Launch".into())));
    assert_eq!(dict.get("NewWindow"), Some(&Value::Bool(true)));
}

#[test]
fn javascript_action_writes_script() {
    let action = Action::JavaScript("app.alert(1);".into());
    let dict = action.to_dict(&[], &[]).unwrap();
    assert_eq!(dict.get("S"), Some(&Value::Name("JavaScript".into())));
    assert!(matches!(dict.get("JS"), Some(Value::HexString(_))));
}

#[test]
fn named_action_writes_name() {
    let action = Action::Named(ActionNamedAction::NextPage);
    let dict = action.to_dict(&[], &[]).unwrap();
    assert_eq!(dict.get("S"), Some(&Value::Name("Named".into())));
    assert_eq!(dict.get("N"), Some(&Value::Name("NextPage".into())));
}

#[test]
fn goto_remote_action_writes_file_and_destination() {
    let action = Action::GoToR {
        file: ActionFileSpec::new("other.pdf"),
        destination: ActionRemoteDestination::Page(5),
    };
    let dict = action.to_dict(&[], &[]).unwrap();
    assert_eq!(dict.get("S"), Some(&Value::Name("GoToR".into())));
    assert!(dict.get("F").is_some());
    assert_eq!(dict.get("D"), Some(&Value::Integer(5)));
}

#[test]
fn hide_action_writes_targets_and_flag() {
    let action = Action::Hide(
        HideAction::new(vec![
            HideTarget::Annotation(0),
            HideTarget::FieldName("group.note".into()),
        ])
        .with_hidden(false),
    );
    let dict = action.to_dict(&[20], &[]).unwrap();
    assert_eq!(dict.get("S"), Some(&Value::Name("Hide".into())));
    let targets = dict.get("T").and_then(Value::as_array).expect("/T");
    assert_eq!(targets.0[0], Value::Ref(20, 0));
    assert!(matches!(targets.0[1], Value::HexString(_)));
    assert_eq!(dict.get("H"), Some(&Value::Bool(false)));

    let single = Action::Hide(HideAction::new(vec![HideTarget::Annotation(0)]));
    let dict = single.to_dict(&[20], &[]).unwrap();
    assert_eq!(dict.get("T"), Some(&Value::Ref(20, 0)), "单目标写标量");
    assert!(dict.get("H").is_none(), "缺省 /H 不物化");
}

#[test]
fn hide_action_rejects_invalid_targets() {
    let empty = Action::Hide(HideAction::new(Vec::new()));
    assert!(matches!(
        empty.to_dict(&[], &[]),
        Err(WriteError::InvalidObject(_))
    ));

    let unknown = Action::Hide(HideAction::new(vec![HideTarget::Annotation(3)]));
    assert!(matches!(
        unknown.to_dict(&[20], &[]),
        Err(WriteError::InvalidObject(_))
    ));

    let duplicated = Action::Hide(HideAction::new(vec![
        HideTarget::FieldName("name".into()),
        HideTarget::FieldName("name".into()),
    ]));
    assert!(matches!(
        duplicated.to_dict(&[], &[]),
        Err(WriteError::InvalidObject(_))
    ));
}

#[test]
fn set_ocg_state_action_writes_flat_state_array() {
    let action = Action::SetOcgState(
        SetOcgStateAction::new(vec![
            OcgStateSequence::new(OcgStateOperation::Off, vec![0, 1]),
            OcgStateSequence::new(OcgStateOperation::Toggle, vec![1]),
        ])
        .with_preserve_radio_buttons(false),
    );
    let dict = action.to_dict(&[], &[31, 32]).unwrap();
    assert_eq!(dict.get("S"), Some(&Value::Name("SetOCGState".into())));
    let state = dict.get("State").and_then(Value::as_array).expect("/State");
    assert_eq!(
        state.0,
        vec![
            Value::Name("OFF".into()),
            Value::Ref(31, 0),
            Value::Ref(32, 0),
            Value::Name("Toggle".into()),
            Value::Ref(32, 0),
        ]
    );
    assert_eq!(dict.get("PreserveRB"), Some(&Value::Bool(false)));

    let minimal = Action::SetOcgState(SetOcgStateAction::new(vec![OcgStateSequence::new(
        OcgStateOperation::On,
        vec![0],
    )]));
    let dict = minimal.to_dict(&[], &[31]).unwrap();
    assert_eq!(
        dict.get("State"),
        Some(&Value::Array(Array(vec![
            Value::Name("ON".into()),
            Value::Ref(31, 0),
        ])))
    );
    assert!(dict.get("PreserveRB").is_none(), "缺省 /PreserveRB 不物化");
}

#[test]
fn set_ocg_state_action_rejects_invalid_groups() {
    let empty = Action::SetOcgState(SetOcgStateAction::new(Vec::new()));
    assert!(matches!(
        empty.to_dict(&[], &[]),
        Err(WriteError::InvalidObject(_))
    ));

    let no_groups = Action::SetOcgState(SetOcgStateAction::new(vec![OcgStateSequence::new(
        OcgStateOperation::On,
        Vec::new(),
    )]));
    assert!(matches!(
        no_groups.to_dict(&[], &[]),
        Err(WriteError::InvalidObject(_))
    ));

    let unknown = Action::SetOcgState(SetOcgStateAction::new(vec![OcgStateSequence::new(
        OcgStateOperation::On,
        vec![2],
    )]));
    assert!(matches!(
        unknown.to_dict(&[], &[31, 32]),
        Err(WriteError::InvalidObject(_))
    ));
}

#[test]
fn thread_action_writes_indices() {
    let action = Action::Thread(ThreadAction::new(2).with_bead(1));
    let dict = action.to_dict(&[], &[]).unwrap();
    assert_eq!(dict.get("S"), Some(&Value::Name("Thread".into())));
    assert_eq!(dict.get("D"), Some(&Value::Integer(2)));
    assert_eq!(dict.get("B"), Some(&Value::Integer(1)));

    let no_bead = Action::Thread(ThreadAction::new(0));
    let dict = no_bead.to_dict(&[], &[]).unwrap();
    assert_eq!(dict.get("D"), Some(&Value::Integer(0)));
    assert!(dict.get("B").is_none(), "缺省 /B 不物化");
}
