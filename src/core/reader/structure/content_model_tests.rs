//! 内容模型诊断的单元测试（ISO 32000-1 14.8.4）。

use super::tests::{arr, document_with_root, element, insert, name, struct_root};
use super::{ChildModel, ContentModelRule, ContentModelViolation, StructureTree};
use crate::core::types::{Dict, Document, Value};

const TREE_ROOT: i64 = 2;
const AUX: i64 = 10;

fn document(root: Dict) -> Document {
    document_with_root(struct_root(root))
}

fn tree_of(document: &Document) -> StructureTree {
    document
        .structure_tree()
        .expect("structure tree parses")
        .expect("structure tree is present")
}

fn node(structure_type: &str, parent: i64, kids: Vec<Value>) -> Value {
    let dict = element(structure_type, parent);
    if kids.is_empty() {
        Value::Dict(dict)
    } else {
        Value::Dict(dict.insert("K", arr(kids)))
    }
}

fn put(document: &Document, number: i64, structure_type: &str, parent: i64, kids: Vec<Value>) {
    insert(document, number, node(structure_type, parent, kids));
}

fn violation(
    element: Option<usize>,
    kid: Option<usize>,
    rule: ContentModelRule,
) -> ContentModelViolation {
    ContentModelViolation { element, kid, rule }
}

fn objr(object: i64) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("OBJR"))
            .insert("Obj", Value::Ref(object, 0)),
    )
}

fn page() -> Value {
    Value::Dict(Dict::new().insert("Type", name("Page")))
}

#[test]
fn content_model_accepts_well_formed_tree() {
    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(
        &doc,
        5,
        "Document",
        TREE_ROOT,
        vec![Value::Ref(6, 0), Value::Ref(7, 0), Value::Ref(8, 0)],
    );
    put(&doc, 6, "L", 5, vec![Value::Ref(9, 0), Value::Ref(10, 0)]);
    put(&doc, 9, "Caption", 6, vec![]);
    put(
        &doc,
        10,
        "LI",
        6,
        vec![Value::Ref(11, 0), Value::Ref(12, 0)],
    );
    put(&doc, 11, "Lbl", 10, vec![]);
    put(&doc, 12, "LBody", 10, vec![]);
    put(
        &doc,
        7,
        "Table",
        5,
        vec![Value::Ref(13, 0), Value::Ref(14, 0), Value::Ref(15, 0)],
    );
    put(&doc, 13, "THead", 7, vec![Value::Ref(16, 0)]);
    put(&doc, 16, "TR", 13, vec![Value::Ref(17, 0)]);
    put(&doc, 17, "TH", 16, vec![]);
    put(&doc, 14, "TBody", 7, vec![Value::Ref(18, 0)]);
    put(&doc, 18, "TR", 14, vec![Value::Ref(19, 0)]);
    put(&doc, 19, "TD", 18, vec![]);
    put(&doc, 15, "TFoot", 7, vec![Value::Ref(20, 0)]);
    put(&doc, 20, "TR", 15, vec![Value::Ref(21, 0)]);
    put(&doc, 21, "TD", 20, vec![]);
    put(&doc, 8, "TOC", 5, vec![Value::Ref(22, 0)]);
    put(
        &doc,
        22,
        "TOCI",
        8,
        vec![Value::Ref(23, 0), Value::Ref(24, 0), Value::Ref(25, 0)],
    );
    put(&doc, 23, "Lbl", 22, vec![]);
    put(&doc, 24, "P", 22, vec![]);
    put(&doc, 25, "TOC", 22, vec![Value::Ref(26, 0)]);
    put(&doc, 26, "TOCI", 25, vec![Value::Ref(27, 0)]);
    put(&doc, 27, "P", 26, vec![]);
    let tree = tree_of(&doc);
    assert_eq!(tree.content_model_violations(), vec![]);
}

#[test]
fn content_model_reports_top_level_count() {
    let tree = tree_of(&document(Dict::new()));
    assert_eq!(
        tree.content_model_violations(),
        vec![violation(None, None, ContentModelRule::TopLevelCount(0))]
    );

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0), Value::Ref(6, 0)])));
    put(&doc, 5, "Document", TREE_ROOT, vec![]);
    put(&doc, 6, "Document", TREE_ROOT, vec![]);
    let tree = tree_of(&doc);
    let violations = tree.content_model_violations();
    assert_eq!(
        violations,
        vec![violation(None, None, ContentModelRule::TopLevelCount(2))]
    );
    assert!(violations[0].rule.is_mandatory());
}

#[test]
fn content_model_reports_content_items_in_grouping_elements() {
    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(&doc, AUX, page());
    put(
        &doc,
        5,
        "Document",
        TREE_ROOT,
        vec![
            Value::Integer(0),
            Value::Dict(
                Dict::new()
                    .insert("Type", name("MCR"))
                    .insert("Pg", Value::Ref(AUX, 0))
                    .insert("MCID", Value::Integer(1)),
            ),
            Value::Ref(6, 0),
        ],
    );
    put(&doc, 6, "Span", 5, vec![Value::Integer(0)]);
    let tree = tree_of(&doc);
    let violations = tree.content_model_violations();
    assert_eq!(
        violations,
        vec![
            violation(
                Some(0),
                Some(0),
                ContentModelRule::ContentItemInGroupingElement
            ),
            violation(
                Some(0),
                Some(1),
                ContentModelRule::ContentItemInGroupingElement
            ),
        ]
    );
    assert!(violations.iter().all(|item| item.rule.is_mandatory()));
}

#[test]
fn content_model_reports_list_children() {
    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(&doc, 5, "Document", TREE_ROOT, vec![Value::Ref(6, 0)]);
    put(
        &doc,
        6,
        "L",
        5,
        vec![Value::Ref(7, 0), Value::Ref(8, 0), Value::Ref(9, 0)],
    );
    put(&doc, 7, "LI", 6, vec![]);
    put(&doc, 8, "P", 6, vec![]);
    put(&doc, 9, "Caption", 6, vec![]);
    let tree = tree_of(&doc);
    assert_eq!(
        tree.content_model_violations(),
        vec![
            violation(
                Some(1),
                Some(1),
                ContentModelRule::UnexpectedChild(ChildModel::ListChild)
            ),
            violation(Some(1), Some(2), ContentModelRule::CaptionPlacement),
        ]
    );

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(&doc, 5, "Document", TREE_ROOT, vec![Value::Ref(6, 0)]);
    put(&doc, 6, "L", 5, vec![Value::Ref(7, 0)]);
    put(&doc, 7, "Caption", 6, vec![]);
    let tree = tree_of(&doc);
    assert_eq!(
        tree.content_model_violations(),
        vec![violation(
            Some(1),
            None,
            ContentModelRule::MissingChild(ChildModel::ListItem)
        )]
    );
}

#[test]
fn content_model_reports_table_children() {
    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(&doc, 5, "Document", TREE_ROOT, vec![Value::Ref(6, 0)]);
    put(
        &doc,
        6,
        "Table",
        5,
        vec![
            Value::Ref(7, 0),
            Value::Ref(8, 0),
            Value::Ref(9, 0),
            Value::Ref(10, 0),
        ],
    );
    put(&doc, 7, "TR", 6, vec![Value::Ref(11, 0)]);
    put(&doc, 11, "TD", 7, vec![]);
    put(&doc, 8, "P", 6, vec![]);
    put(&doc, 9, "Caption", 6, vec![]);
    put(&doc, 10, "Caption", 6, vec![]);
    let tree = tree_of(&doc);
    assert_eq!(
        tree.content_model_violations(),
        vec![
            violation(
                Some(1),
                Some(1),
                ContentModelRule::UnexpectedChild(ChildModel::TableChild)
            ),
            violation(Some(1), Some(2), ContentModelRule::CaptionPlacement),
        ]
    );

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(&doc, 5, "Document", TREE_ROOT, vec![Value::Ref(6, 0)]);
    put(
        &doc,
        6,
        "Table",
        5,
        vec![Value::Ref(7, 0), Value::Ref(8, 0)],
    );
    put(&doc, 7, "THead", 6, vec![]);
    put(&doc, 8, "TBody", 6, vec![Value::Ref(9, 0)]);
    put(&doc, 9, "P", 8, vec![]);
    let tree = tree_of(&doc);
    assert_eq!(
        tree.content_model_violations(),
        vec![
            violation(
                Some(2),
                None,
                ContentModelRule::MissingChild(ChildModel::TableRow)
            ),
            violation(
                Some(3),
                Some(0),
                ContentModelRule::UnexpectedChild(ChildModel::TableRow)
            ),
            violation(
                Some(3),
                None,
                ContentModelRule::MissingChild(ChildModel::TableRow)
            ),
        ]
    );

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(&doc, 5, "Document", TREE_ROOT, vec![Value::Ref(6, 0)]);
    put(&doc, 6, "Table", 5, vec![Value::Ref(7, 0)]);
    put(&doc, 7, "TR", 6, vec![Value::Ref(8, 0)]);
    put(&doc, 8, "P", 7, vec![]);
    let tree = tree_of(&doc);
    assert_eq!(
        tree.content_model_violations(),
        vec![
            violation(
                Some(2),
                Some(0),
                ContentModelRule::UnexpectedChild(ChildModel::TableCell)
            ),
            violation(
                Some(2),
                None,
                ContentModelRule::MissingChild(ChildModel::TableCell)
            ),
        ]
    );
}

#[test]
fn content_model_reports_form_object_reference_rule() {
    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(&doc, 5, "Form", TREE_ROOT, vec![]);
    let tree = tree_of(&doc);
    let violations = tree.content_model_violations();
    assert_eq!(
        violations,
        vec![violation(
            Some(0),
            None,
            ContentModelRule::FormObjectReference
        )]
    );
    assert!(violations[0].rule.is_mandatory());

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(
        &doc,
        5,
        Value::Dict(
            element("Form", TREE_ROOT).insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("PrintField"))
                        .insert("Role", name("RB")),
                ),
            ),
        ),
    );
    assert_eq!(tree_of(&doc).content_model_violations(), vec![]);

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(&doc, AUX, Value::Dict(Dict::new()));
    insert(
        &doc,
        5,
        Value::Dict(element("Form", TREE_ROOT).insert("K", objr(AUX))),
    );
    assert_eq!(tree_of(&doc).content_model_violations(), vec![]);

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(&doc, AUX, Value::Dict(Dict::new()));
    insert(
        &doc,
        5,
        Value::Dict(element("Form", TREE_ROOT).insert("K", arr(vec![objr(AUX), objr(AUX)]))),
    );
    assert_eq!(
        tree_of(&doc).content_model_violations(),
        vec![violation(
            Some(0),
            None,
            ContentModelRule::FormObjectReference
        )]
    );

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(&doc, AUX, page());
    insert(
        &doc,
        5,
        Value::Dict(
            element("Form", TREE_ROOT)
                .insert("Pg", Value::Ref(AUX, 0))
                .insert(
                    "K",
                    Value::Dict(
                        Dict::new()
                            .insert("Type", name("MCR"))
                            .insert("MCID", Value::Integer(0)),
                    ),
                ),
        ),
    );
    assert_eq!(
        tree_of(&doc).content_model_violations(),
        vec![violation(
            Some(0),
            None,
            ContentModelRule::FormObjectReference
        )]
    );
}

fn ruby_tree(parts: &[&str]) -> StructureTree {
    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    let kids: Vec<Value> = parts
        .iter()
        .enumerate()
        .map(|(offset, _)| Value::Ref(6 + offset as i64, 0))
        .collect();
    put(&doc, 5, "Ruby", TREE_ROOT, kids);
    for (offset, part) in parts.iter().enumerate() {
        put(&doc, 6 + offset as i64, part, 5, vec![]);
    }
    tree_of(&doc)
}

#[test]
fn content_model_reports_ruby_sequence() {
    assert_eq!(ruby_tree(&["RB", "RT"]).content_model_violations(), vec![]);
    assert_eq!(
        ruby_tree(&["RB", "RP", "RT", "RP"]).content_model_violations(),
        vec![]
    );
    let violations = ruby_tree(&["RB"]).content_model_violations();
    assert_eq!(
        violations,
        vec![violation(Some(0), None, ContentModelRule::RubySequence)]
    );
    assert!(violations[0].rule.is_mandatory());
    assert_eq!(
        ruby_tree(&["RB", "RT", "RP"]).content_model_violations(),
        vec![violation(Some(0), None, ContentModelRule::RubySequence)]
    );
}

#[test]
fn content_model_reports_link_and_annot_rules() {
    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(&doc, AUX, Value::Dict(Dict::new()));
    put(
        &doc,
        5,
        "Link",
        TREE_ROOT,
        vec![Value::Ref(6, 0), objr(AUX)],
    );
    put(&doc, 6, "Span", 5, vec![]);
    assert_eq!(tree_of(&doc).content_model_violations(), vec![]);

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(&doc, AUX, Value::Dict(Dict::new()));
    put(
        &doc,
        5,
        "Link",
        TREE_ROOT,
        vec![Value::Ref(6, 0), objr(AUX)],
    );
    put(&doc, 6, "Link", 5, vec![]);
    assert_eq!(
        tree_of(&doc).content_model_violations(),
        vec![
            violation(
                Some(0),
                Some(0),
                ContentModelRule::UnexpectedChild(ChildModel::ContentOrInline)
            ),
            violation(
                Some(1),
                None,
                ContentModelRule::MissingChild(ChildModel::ObjectReference)
            ),
        ]
    );

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(&doc, 5, "Link", TREE_ROOT, vec![Value::Ref(6, 0)]);
    put(&doc, 6, "Span", 5, vec![]);
    assert_eq!(
        tree_of(&doc).content_model_violations(),
        vec![violation(
            Some(0),
            None,
            ContentModelRule::MissingChild(ChildModel::ObjectReference)
        )]
    );

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(&doc, AUX, Value::Dict(Dict::new()));
    put(
        &doc,
        5,
        "Annot",
        TREE_ROOT,
        vec![objr(AUX), Value::Ref(6, 0)],
    );
    put(&doc, 6, "Annot", 5, vec![]);
    assert_eq!(
        tree_of(&doc).content_model_violations(),
        vec![
            violation(
                Some(0),
                Some(1),
                ContentModelRule::UnexpectedChild(ChildModel::ContentOrInline)
            ),
            violation(
                Some(1),
                None,
                ContentModelRule::MissingChild(ChildModel::ObjectReference)
            ),
        ]
    );

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(&doc, 5, "Annot", TREE_ROOT, vec![Value::Ref(6, 0)]);
    put(&doc, 6, "Span", 5, vec![]);
    assert_eq!(
        tree_of(&doc).content_model_violations(),
        vec![violation(
            Some(0),
            None,
            ContentModelRule::MissingChild(ChildModel::ObjectReference)
        )]
    );
}

#[test]
fn content_model_resolves_role_mapped_types() {
    let doc = document(
        Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])).insert(
            "RoleMap",
            Value::Dict(
                Dict::new()
                    .insert("MyTOC", name("TOC"))
                    .insert("MyEntry", name("TOCI"))
                    .insert("MyLbl", name("Lbl")),
            ),
        ),
    );
    put(
        &doc,
        5,
        "MyTOC",
        TREE_ROOT,
        vec![Value::Ref(6, 0), Value::Ref(7, 0)],
    );
    put(&doc, 6, "MyEntry", 5, vec![Value::Ref(8, 0)]);
    put(&doc, 8, "MyLbl", 6, vec![]);
    put(&doc, 7, "P", 5, vec![]);
    let tree = tree_of(&doc);
    assert_eq!(
        tree.content_model_violations(),
        vec![violation(
            Some(0),
            Some(1),
            ContentModelRule::UnexpectedChild(ChildModel::TocEntry)
        )]
    );

    let doc = document(Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])));
    put(&doc, 5, "Custom", TREE_ROOT, vec![Value::Integer(0)]);
    assert_eq!(tree_of(&doc).content_model_violations(), vec![]);
}
