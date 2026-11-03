//! 标准结构 Table owner 属性读端测试（ISO 32000-1 14.8.5.5）。

use super::*;

#[test]
fn table_attributes_parse_spans_headers_scope_and_summary() {
    let doc = document_with_elements(vec![
        (
            5,
            element("Table").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("Table"))
                        .insert("Summary", Value::String("Quarterly results".into())),
                ),
            ),
        ),
        (
            6,
            element("TH").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("Table"))
                        .insert("RowSpan", Value::Integer(2))
                        .insert("ColSpan", Value::Integer(3))
                        .insert(
                            "Headers",
                            arr(vec![
                                Value::String("h1".into()),
                                Value::HexString("6832".into()),
                            ]),
                        )
                        .insert("Scope", name("Both")),
                ),
            ),
        ),
        (
            7,
            element("TD").insert("A", Value::Dict(Dict::new().insert("O", name("Table")))),
        ),
    ]);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let table = tree.standard_attributes(0).expect("table attributes");
    let attributes = table.table.expect("Table owner");
    assert_eq!(attributes.row_span, 1);
    assert_eq!(attributes.col_span, 1);
    assert!(attributes.headers.is_empty());
    assert_eq!(attributes.scope, None);
    assert_eq!(attributes.summary.as_deref(), Some("Quarterly results"));

    let cell = tree.standard_attributes(1).expect("table attributes");
    let attributes = cell.table.expect("Table owner");
    assert_eq!(attributes.row_span, 2);
    assert_eq!(attributes.col_span, 3);
    assert_eq!(attributes.headers, vec![b"h1".to_vec(), b"h2".to_vec()]);
    assert_eq!(attributes.scope, Some(TableScope::Both));
    assert_eq!(attributes.summary, None);

    let defaults = tree.standard_attributes(2).expect("table attributes");
    let attributes = defaults.table.expect("Table owner");
    assert_eq!(attributes.row_span, 1);
    assert_eq!(attributes.col_span, 1);
    assert!(attributes.headers.is_empty());
    assert_eq!(attributes.scope, None);
    assert_eq!(attributes.summary, None);
}
