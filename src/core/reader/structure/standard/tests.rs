//! 标准结构属性读端测试（ISO 32000-1 14.8.5.2、14.8.5.5–14.8.5.7）。

use crate::core::reader::structure::{
    FormatSpecificOwner, ListNumbering, PrintFieldChecked, PrintFieldRole, StandardAttributeOwner,
    StructureError, TableScope,
};
use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

const CATALOG: i64 = 1;
const ROOT: i64 = 2;

pub(super) fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

pub(super) fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

pub(super) fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

pub(super) fn element(structure_type: &str) -> Dict {
    Dict::new()
        .insert("Type", name("StructElem"))
        .insert("S", name(structure_type))
        .insert("P", Value::Ref(ROOT, 0))
}

pub(super) fn struct_root(entries: Dict) -> Dict {
    entries.insert("Type", name("StructTreeRoot"))
}

/// Catalog 1 → StructTreeRoot 2，顶层元素按给定对象号引用。
pub(super) fn document_with_root(root: Dict) -> Document {
    let mut doc = Document::new();
    insert(&doc, ROOT, Value::Dict(root));
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("StructTreeRoot", Value::Ref(ROOT, 0)),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

/// 若干顶层元素：每个 (对象号, 结构元素字典)。
pub(super) fn document_with_elements(elements: Vec<(i64, Dict)>) -> Document {
    let kids = arr(elements
        .iter()
        .map(|(number, _)| Value::Ref(*number, 0))
        .collect());
    let doc = document_with_root(struct_root(Dict::new()).insert("K", kids));
    for (number, element) in elements {
        insert(&doc, number, Value::Dict(element));
    }
    doc
}

/// 单个顶层元素，`/A` 为给定属性对象字典。
pub(super) fn attribute_document(attribute: Dict) -> Document {
    document_with_elements(vec![(5, element("P").insert("A", Value::Dict(attribute)))])
}

pub(super) fn assert_malformed(doc: &Document, needle: &str) {
    match doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present")
        .standard_attributes(0)
    {
        Err(StructureError::Malformed(message)) => {
            assert!(
                message.contains(needle),
                "expected {needle:?}, got {message}"
            );
        }
        other => panic!("expected malformed error containing {needle:?}, got {other:?}"),
    }
}

#[test]
fn list_attributes_parse_numbering_and_default() {
    let doc = document_with_elements(vec![
        (
            5,
            element("L").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("List"))
                        .insert("ListNumbering", name("Decimal")),
                ),
            ),
        ),
        (
            6,
            element("L").insert("A", Value::Dict(Dict::new().insert("O", name("List")))),
        ),
    ]);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let first = tree.standard_attributes(0).expect("list attributes");
    let list = first.list.expect("List owner");
    assert_eq!(list.numbering, ListNumbering::Decimal);
    assert_eq!(
        list.dict.get("O").and_then(Value::as_name),
        Some("List"),
        "dict 保留原始条目"
    );
    assert!(first.print_field.is_none());
    assert!(first.table.is_none());
    assert!(first.other.is_empty());

    let second = tree.standard_attributes(1).expect("list attributes");
    assert_eq!(
        second.list.expect("List owner").numbering,
        ListNumbering::None,
        "/ListNumbering 缺省为 None"
    );

    let missing = tree.standard_attributes(99).expect("out of range");
    assert_eq!(missing, Default::default(), "越界下标返回全空属性");
}

#[test]
fn print_field_attributes_parse_role_checked_and_description() {
    let doc = document_with_elements(vec![
        (
            5,
            element("Form").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("PrintField"))
                        .insert("Role", name("rb"))
                        .insert("checked", name("on"))
                        .insert("Desc", Value::String("Radio button".into())),
                ),
            ),
        ),
        (
            6,
            element("Form").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("PrintField"))
                        .insert("Role", name("tv"))
                        .insert("checked", name("neutral")),
                ),
            ),
        ),
        (
            7,
            element("Form").insert(
                "A",
                Value::Dict(Dict::new().insert("O", name("PrintField"))),
            ),
        ),
        (
            8,
            element("Form").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("PrintField"))
                        .insert("Role", name("xx"))
                        .insert("checked", name("maybe"))
                        .insert("Desc", Value::HexString("48656C6C6F".into())),
                ),
            ),
        ),
    ]);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let radio = tree.standard_attributes(0).expect("print field");
    let field = radio.print_field.expect("PrintField owner");
    assert_eq!(field.role, Some(PrintFieldRole::RadioButton));
    assert_eq!(field.checked, PrintFieldChecked::On);
    assert_eq!(field.description.as_deref(), Some("Radio button"));

    let neutral = tree.standard_attributes(1).expect("print field");
    let field = neutral.print_field.expect("PrintField owner");
    assert_eq!(field.role, Some(PrintFieldRole::TextValue));
    assert_eq!(field.checked, PrintFieldChecked::Neutral);
    assert_eq!(field.description, None);

    let defaults = tree.standard_attributes(2).expect("print field");
    let field = defaults.print_field.expect("PrintField owner");
    assert_eq!(field.role, None);
    assert_eq!(field.checked, PrintFieldChecked::Off, "/checked 缺省为 off");
    assert_eq!(field.description, None);

    let unknown = tree.standard_attributes(3).expect("print field");
    let field = unknown.print_field.expect("PrintField owner");
    assert_eq!(field.role, Some(PrintFieldRole::Other("xx".into())));
    assert_eq!(field.checked, PrintFieldChecked::Other("maybe".into()));
    assert_eq!(field.description.as_deref(), Some("Hello"));
}

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

#[test]
fn standard_attribute_owners_are_classified() {
    for (name, expected) in [
        ("Layout", StandardAttributeOwner::Layout),
        ("List", StandardAttributeOwner::List),
        ("PrintField", StandardAttributeOwner::PrintField),
        ("Table", StandardAttributeOwner::Table),
        ("XML-1.00", StandardAttributeOwner::Other("XML-1.00".into())),
        (
            "HTML-3.20",
            StandardAttributeOwner::Other("HTML-3.20".into()),
        ),
        (
            "HTML-4.01",
            StandardAttributeOwner::Other("HTML-4.01".into()),
        ),
        ("OEB-1.00", StandardAttributeOwner::Other("OEB-1.00".into())),
        ("RTF-1.05", StandardAttributeOwner::Other("RTF-1.05".into())),
        ("CSS-1.00", StandardAttributeOwner::Other("CSS-1.00".into())),
        ("CSS-2.00", StandardAttributeOwner::Other("CSS-2.00".into())),
        ("Vendor", StandardAttributeOwner::Other("Vendor".into())),
    ] {
        let owner = StandardAttributeOwner::from_name(name);
        assert_eq!(owner, expected);
        assert_eq!(owner.as_str(), name);
    }
}

#[test]
fn malformed_standard_attributes_are_typed_errors() {
    let list = Dict::new()
        .insert("O", name("List"))
        .insert("ListNumbering", Value::Integer(1));
    let doc = attribute_document(list);
    assert_malformed(&doc, "structure element 0 /ListNumbering must be a name");

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("PrintField"))
            .insert("checked", Value::Integer(1)),
    );
    assert_malformed(&doc, "structure element 0 /checked must be a name");

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("PrintField"))
            .insert("Desc", Value::Integer(1)),
    );
    assert_malformed(&doc, "structure element 0 /Desc must be a text string");

    for span in [0, -1] {
        let doc = attribute_document(
            Dict::new()
                .insert("O", name("Table"))
                .insert("RowSpan", Value::Integer(span)),
        );
        assert_malformed(&doc, "structure element 0 /RowSpan must be an integer");
    }

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("Table"))
            .insert("Headers", Value::Integer(1)),
    );
    assert_malformed(&doc, "structure element 0 /Headers must be an array");

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("Table"))
            .insert("Headers", arr(vec![Value::String(String::new())])),
    );
    assert_malformed(
        &doc,
        "structure element 0 /Headers entry 0 must not be empty",
    );

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("Table"))
            .insert("Headers", arr(vec![Value::Integer(1)])),
    );
    assert_malformed(
        &doc,
        "structure element 0 /Headers entry 0 must be a byte string",
    );
}

#[test]
fn standard_attributes_merge_class_and_element_objects() {
    let class_map = Dict::new().insert(
        "Body",
        Value::Dict(
            Dict::new()
                .insert("O", name("List"))
                .insert("ListNumbering", name("Disc")),
        ),
    );
    let layout = Dict::new()
        .insert("O", name("Layout"))
        .insert("SpaceBefore", Value::Integer(12));
    let element = element("L").insert("C", name("Body")).insert(
        "A",
        arr(vec![
            Value::Dict(
                Dict::new()
                    .insert("O", name("List"))
                    .insert("ListNumbering", name("Circle")),
            ),
            Value::Dict(layout),
        ]),
    );
    let doc = document_with_root(
        struct_root(Dict::new())
            .insert("ClassMap", Value::Dict(class_map))
            .insert("K", arr(vec![Value::Ref(5, 0)])),
    );
    insert(&doc, 5, Value::Dict(element));
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let resolved = tree.resolved_attributes(0);
    assert_eq!(resolved.len(), 2, "同一 owner 合并，Layout 保留");
    assert_eq!(resolved[0].owner, "List");

    let standard = tree.standard_attributes(0).expect("standard attributes");
    let list = standard.list.expect("List owner");
    assert_eq!(
        list.numbering,
        ListNumbering::Circle,
        "元素 /A 覆盖 ClassMap 的 /ListNumbering"
    );
    let layout = standard.layout.expect("Layout owner");
    assert_eq!(layout.space_before, Some(12.0), "Layout 对象进入类型化字段");
    assert_eq!(
        layout.dict.get("O").and_then(Value::as_name),
        Some("Layout")
    );
    assert!(standard.other.is_empty(), "Layout 不再归入 other");
}

#[test]
fn format_specific_owners_cover_table341() {
    for (name, expected) in [
        ("XML-1.00", FormatSpecificOwner::Xml1_00),
        ("HTML-3.20", FormatSpecificOwner::Html3_20),
        ("HTML-4.01", FormatSpecificOwner::Html4_01),
        ("OEB-1.00", FormatSpecificOwner::Oeb1_00),
        ("RTF-1.05", FormatSpecificOwner::Rtf1_05),
        ("CSS-1.00", FormatSpecificOwner::Css1_00),
        ("CSS-2.00", FormatSpecificOwner::Css2_00),
    ] {
        assert_eq!(FormatSpecificOwner::from_name(name), Some(expected));
        assert_eq!(expected.as_str(), name);
    }
    assert_eq!(FormatSpecificOwner::from_name("Vendor"), None);
}

#[test]
fn format_specific_attributes_are_exposed_with_raw_entries() {
    let stylesheet = Dict::new()
        .insert("O", name("CSS-2.00"))
        .insert("text-align", Value::String("left".into()))
        .insert("margin", Value::Integer(4));
    let user_properties = Dict::new().insert("O", name("UserProperties")).insert(
        "P",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("N", Value::String("role".into()))
                .insert("V", Value::String("caption".into())),
        )]),
    );
    let doc = document_with_root(
        struct_root(Dict::new())
            .insert(
                "ClassMap",
                Value::Dict(Dict::new().insert("CssClass", Value::Dict(stylesheet))),
            )
            .insert("K", arr(vec![Value::Ref(5, 0), Value::Ref(6, 0)])),
    );
    insert(
        &doc,
        5,
        Value::Dict(element("Figure").insert(
            "A",
            arr(vec![
                Value::Dict(
                    Dict::new()
                        .insert("O", name("XML-1.00"))
                        .insert("role", Value::String("caption".into()))
                        .insert("level", Value::Integer(2)),
                ),
                Value::Dict(user_properties),
                Value::Dict(
                    Dict::new()
                        .insert("O", name("Layout"))
                        .insert("SpaceBefore", Value::Integer(12)),
                ),
            ]),
        )),
    );
    insert(
        &doc,
        6,
        Value::Dict(element("Figure").insert("C", name("CssClass")).insert(
            "A",
            arr(vec![Value::Dict(
                Dict::new()
                    .insert("O", name("CSS-2.00"))
                    .insert("margin", Value::Integer(8)),
            )]),
        )),
    );
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let own = tree.standard_attributes(0).expect("standard attributes");
    assert!(
        own.layout
            .as_ref()
            .is_some_and(|layout| layout.space_before == Some(12.0)),
        "Layout 仍进入类型化字段"
    );
    assert_eq!(
        own.format_specific.len(),
        1,
        "XML-1.00 进入 format_specific"
    );
    let xml = &own.format_specific[0];
    assert_eq!(xml.owner, FormatSpecificOwner::Xml1_00);
    assert_eq!(xml.revision, 0);
    let dict = xml.dict().expect("raw dictionary");
    assert_eq!(
        dict.get("role"),
        Some(&Value::String("caption".into())),
        "自定义条目原样保留"
    );
    assert_eq!(dict.get("level"), Some(&Value::Integer(2)));
    assert_eq!(own.other.len(), 1, "UserProperties 仍归入 other");
    assert_eq!(own.other[0].owner, "UserProperties");

    let classed = tree.standard_attributes(1).expect("standard attributes");
    assert!(classed.layout.is_none(), "元素自身没有 Layout 属性对象");
    assert_eq!(
        classed.format_specific.len(),
        1,
        "类与 /A 的同一 CSS-2.00 合并为单项"
    );
    let css = &classed.format_specific[0];
    assert_eq!(css.owner, FormatSpecificOwner::Css2_00);
    let dict = css.dict().expect("raw dictionary");
    assert_eq!(
        dict.get("margin"),
        Some(&Value::Integer(8)),
        "元素 /A 的键优先于类属性"
    );
    assert_eq!(
        dict.get("text-align"),
        Some(&Value::String("left".into())),
        "类属性其余键保留"
    );
    assert!(classed.other.is_empty());
}

#[test]
fn format_specific_attributes_are_not_inherited() {
    let doc = document_with_root(struct_root(Dict::new()).insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(
        &doc,
        5,
        Value::Dict(
            element("Div")
                .insert(
                    "A",
                    arr(vec![Value::Dict(
                        Dict::new()
                            .insert("O", name("XML-1.00"))
                            .insert("role", Value::String("caption".into())),
                    )]),
                )
                .insert("K", arr(vec![Value::Ref(6, 0)])),
        ),
    );
    insert(
        &doc,
        6,
        Value::Dict(element("P").insert("P", Value::Ref(5, 0))),
    );
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    assert!(
        tree.standard_attributes(0)
            .expect("parent attributes")
            .format_specific
            .len()
            == 1,
        "父元素自身报告 XML-1.00"
    );
    assert!(
        tree.effective_standard_attributes(1)
            .expect("child effective attributes")
            .format_specific
            .is_empty(),
        "格式专用属性不参与 14.8.5.3 继承"
    );
    assert!(
        tree.effective_attributes(1)
            .iter()
            .all(|attribute| attribute.owner != "XML-1.00"),
        "子元素 effective_attributes 不含 XML-1.00 所有者"
    );
}
