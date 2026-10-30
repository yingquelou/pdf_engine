//! Embedded Go-To 动作读端测试（12.6.4.4 Table 201/202）。

use super::*;

#[test]
fn parses_embedded_go_to_action_targets() {
    let doc = document_with_pages(1);

    // 父 -> 子（名称树）目标链，/F 与 /NewWindow 显式声明。
    insert(
        &doc,
        40,
        Value::Dict(
            Dict::new().insert("R", name("P")).insert(
                "T",
                Value::Dict(
                    Dict::new()
                        .insert("R", name("C"))
                        .insert("N", Value::String("Embedded document".into())),
                ),
            ),
        ),
    );
    let chained = Value::Dict(
        action_dict("GoToE")
            .insert("F", Value::String("root.pdf".into()))
            .insert("D", Value::String("Chapter 1".into()))
            .insert("NewWindow", Value::Bool(false))
            .insert("T", Value::Ref(40, 0)),
    );
    assert_eq!(
        doc.action(&chained).unwrap(),
        Action::GoToE(GoToEAction {
            file: Some(FileSpec {
                file: "root.pdf".into(),
            }),
            destination: RemoteDestination::Named("Chapter 1".into()),
            new_window: Some(false),
            target: Some(GoToETarget {
                relation: GoToETargetRelation::Parent,
                name: None,
                page: None,
                annotation: None,
                target: Some(Box::new(GoToETarget {
                    relation: GoToETargetRelation::Child,
                    name: Some("Embedded document".into()),
                    page: None,
                    annotation: None,
                    target: None,
                })),
            }),
        })
    );

    // 附件注解子目标：页号 + 注解名；缺省 /F 与 /NewWindow。
    let attachment = Value::Dict(
        action_dict("GoToE").insert("D", Value::Integer(3)).insert(
            "T",
            Value::Dict(
                Dict::new()
                    .insert("R", name("C"))
                    .insert("P", Value::Integer(2))
                    .insert("A", Value::String("note".into())),
            ),
        ),
    );
    assert_eq!(
        doc.action(&attachment).unwrap(),
        Action::GoToE(GoToEAction {
            file: None,
            destination: RemoteDestination::Page(3),
            new_window: None,
            target: Some(GoToETarget {
                relation: GoToETargetRelation::Child,
                name: None,
                page: Some(GoToETargetPage::Page(2)),
                annotation: Some(GoToETargetAnnotation::Name("note".into())),
                target: None,
            }),
        })
    );

    // 命名目标页 + 注解索引；目标字典经间接引用嵌套。
    insert(
        &doc,
        41,
        Value::Dict(
            Dict::new()
                .insert("R", name("C"))
                .insert("P", Value::String("Chapter start".into()))
                .insert("A", Value::Integer(0)),
        ),
    );
    let named = Value::Dict(
        action_dict("GoToE")
            .insert("D", arr(vec![Value::Integer(0), name("Fit")]))
            .insert("T", Value::Ref(41, 0)),
    );
    assert_eq!(
        doc.action(&named).unwrap(),
        Action::GoToE(GoToEAction {
            file: None,
            destination: RemoteDestination::Page(0),
            new_window: None,
            target: Some(GoToETarget {
                relation: GoToETargetRelation::Child,
                name: None,
                page: Some(GoToETargetPage::Named("Chapter start".into())),
                annotation: Some(GoToETargetAnnotation::Index(0)),
                target: None,
            }),
        })
    );

    // 未知关系名保留。
    let unknown = Value::Dict(
        action_dict("GoToE")
            .insert("D", Value::Integer(0))
            .insert("T", Value::Dict(Dict::new().insert("R", name("X")))),
    );
    match doc.action(&unknown).unwrap() {
        Action::GoToE(action) => {
            let target = action.target.expect("target");
            assert_eq!(target.relation, GoToETargetRelation::Other("X".into()));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn malformed_embedded_go_to_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    // 自引用的目标字典。
    insert(
        &doc,
        50,
        Value::Dict(
            Dict::new()
                .insert("R", name("P"))
                .insert("T", Value::Ref(50, 0)),
        ),
    );
    let target = |dict: Dict| Value::Dict(dict);
    let cases = vec![
        // 缺 /D。
        Value::Dict(action_dict("GoToE").insert("T", target(Dict::new().insert("R", name("P"))))),
        // 既无 /F 也无 /T。
        Value::Dict(action_dict("GoToE").insert("D", Value::String("x".into()))),
        // /T 不是字典。
        Value::Dict(
            action_dict("GoToE")
                .insert("D", Value::Integer(0))
                .insert("T", Value::Integer(1)),
        ),
        // 缺 /R。
        Value::Dict(action_dict("GoToE").insert("D", Value::Integer(0)).insert(
            "T",
            target(Dict::new().insert("N", Value::String("x".into()))),
        )),
        // /R 非名称。
        Value::Dict(
            action_dict("GoToE")
                .insert("D", Value::Integer(0))
                .insert("T", target(Dict::new().insert("R", Value::Integer(1)))),
        ),
        // 子目标无 /N 也无 /P + /A。
        Value::Dict(
            action_dict("GoToE")
                .insert("D", Value::Integer(0))
                .insert("T", target(Dict::new().insert("R", name("C")))),
        ),
        // 子目标同时声明 /N 与 /P + /A。
        Value::Dict(
            action_dict("GoToE").insert("D", Value::Integer(0)).insert(
                "T",
                target(
                    Dict::new()
                        .insert("R", name("C"))
                        .insert("N", Value::String("x".into()))
                        .insert("P", Value::Integer(0))
                        .insert("A", Value::Integer(0)),
                ),
            ),
        ),
        // 子目标只有 /P 缺 /A。
        Value::Dict(
            action_dict("GoToE").insert("D", Value::Integer(0)).insert(
                "T",
                target(
                    Dict::new()
                        .insert("R", name("C"))
                        .insert("P", Value::Integer(0)),
                ),
            ),
        ),
        // 父目标带 /N。
        Value::Dict(
            action_dict("GoToE").insert("D", Value::Integer(0)).insert(
                "T",
                target(
                    Dict::new()
                        .insert("R", name("P"))
                        .insert("N", Value::String("x".into())),
                ),
            ),
        ),
        // 负页号。
        Value::Dict(
            action_dict("GoToE").insert("D", Value::Integer(0)).insert(
                "T",
                target(
                    Dict::new()
                        .insert("R", name("C"))
                        .insert("P", Value::Integer(-1))
                        .insert("A", Value::Integer(0)),
                ),
            ),
        ),
        // /P 类型错误。
        Value::Dict(
            action_dict("GoToE").insert("D", Value::Integer(0)).insert(
                "T",
                target(
                    Dict::new()
                        .insert("R", name("C"))
                        .insert("P", Value::Bool(true))
                        .insert("A", Value::Integer(0)),
                ),
            ),
        ),
        // 负注解索引。
        Value::Dict(
            action_dict("GoToE").insert("D", Value::Integer(0)).insert(
                "T",
                target(
                    Dict::new()
                        .insert("R", name("C"))
                        .insert("P", Value::Integer(0))
                        .insert("A", Value::Integer(-2)),
                ),
            ),
        ),
        // /A 类型错误。
        Value::Dict(
            action_dict("GoToE").insert("D", Value::Integer(0)).insert(
                "T",
                target(
                    Dict::new()
                        .insert("R", name("C"))
                        .insert("P", Value::Integer(0))
                        .insert("A", Value::Bool(false)),
                ),
            ),
        ),
        // /NewWindow 非布尔。
        Value::Dict(
            action_dict("GoToE")
                .insert("D", Value::Integer(0))
                .insert("F", Value::String("a.pdf".into()))
                .insert("NewWindow", Value::Integer(1)),
        ),
        // 自引用目标字典。
        Value::Dict(
            action_dict("GoToE")
                .insert("D", Value::Integer(0))
                .insert("T", Value::Ref(50, 0)),
        ),
    ];
    for case in cases {
        let error = doc.action(&case).expect_err("malformed GoToE action");
        assert!(
            matches!(error, ActionError::Malformed(_)),
            "{case:?}: {error}"
        );
    }
}
