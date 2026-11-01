//! 词边界语义测试（ISO 32000-1 14.9.3、14.9.4、14.9.5）。

use crate::core::reader::accessibility::{
    SubstitutionDeclaration, WordBreak, WordBreakRule, word_break_between,
};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

const CATALOG: i64 = 1;
const ROOT: i64 = 2;
const PAGES: i64 = 3;
const PAGE: i64 = 4;
const FIRST_ELEMENT: i64 = 5;
const SECOND_ELEMENT: i64 = 6;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

fn declaration(
    alternate_text: bool,
    actual_text: bool,
    expansion: bool,
) -> SubstitutionDeclaration {
    SubstitutionDeclaration {
        alternate_text,
        actual_text,
        expansion,
    }
}

/// 单页文档：内容流；`elements` 给出时附结构树与父树（MCID 0/1）。
fn page_document(content: &[u8], elements: Option<[(i64, Dict); 2]>) -> Document {
    let mut doc = Document::new();
    let mut root = Dict::new().insert("Type", name("StructTreeRoot"));
    if let Some(elements) = elements.as_ref() {
        root = root
            .insert(
                "K",
                arr(vec![
                    Value::Ref(elements[0].0, 0),
                    Value::Ref(elements[1].0, 0),
                ]),
            )
            .insert(
                "ParentTree",
                Value::Dict(Dict::new().insert(
                    "Nums",
                    arr(vec![
                        Value::Integer(0),
                        arr(vec![
                            Value::Ref(elements[0].0, 0),
                            Value::Ref(elements[1].0, 0),
                        ]),
                    ]),
                )),
            );
    }
    insert(&doc, ROOT, Value::Dict(root));
    if let Some(elements) = elements {
        for (number, dict) in elements {
            insert(
                &doc,
                number,
                Value::Dict(
                    dict.insert("Type", name("StructElem"))
                        .insert("P", Value::Ref(ROOT, 0)),
                ),
            );
        }
    }
    insert(
        &doc,
        PAGES,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGE, 0)]))
                .insert("Count", Value::Integer(1)),
        ),
    );
    insert(
        &doc,
        PAGE,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(PAGES, 0))
                .insert("StructParents", Value::Integer(0))
                .insert("Resources", Value::Dict(Dict::new()))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
                ),
        ),
    );
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES, 0))
                .insert("StructTreeRoot", Value::Ref(ROOT, 0)),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

/// 一个带给定文本条目的结构元素字典。
fn element_with(entries: &[(&str, &str)]) -> Dict {
    let mut dict = Dict::new().insert("S", name("P"));
    for (key, value) in entries {
        dict = dict.insert(*key, Value::String((*value).to_owned()));
    }
    dict
}

#[test]
fn word_break_between_two_alt_declarations_reports_present_break() {
    // 14.9.3：两侧都带 /Alt → 视为其间存在词边界（词/短语替换）。
    let decision = word_break_between(
        &declaration(true, false, false),
        &declaration(true, false, false),
    );
    assert_eq!(decision.word_break, WordBreak::Present);
    assert_eq!(decision.rule, Some(WordBreakRule::AlternateTextPair));
    assert!(!decision.conflicting);
}

#[test]
fn word_break_between_one_sided_alt_is_not_declared() {
    for (before, after) in [
        (
            declaration(true, false, false),
            declaration(false, false, false),
        ),
        (
            declaration(false, false, false),
            declaration(true, false, false),
        ),
    ] {
        let decision = word_break_between(&before, &after);
        assert_eq!(
            decision.word_break,
            WordBreak::NotDeclared,
            "只有一个元素带 /Alt 时规范没有给出边界声明：{before:?} / {after:?}"
        );
        assert_eq!(decision.rule, None);
        assert!(!decision.conflicting);
    }
}

#[test]
fn word_break_between_two_actual_text_declarations_reports_absent_break() {
    // 14.9.4：连续的两侧都带 /ActualText → 视为其间不存在词边界（字符替换）。
    let decision = word_break_between(
        &declaration(false, true, false),
        &declaration(false, true, false),
    );
    assert_eq!(decision.word_break, WordBreak::Absent);
    assert_eq!(decision.rule, Some(WordBreakRule::ActualTextPair));
    assert!(!decision.conflicting);
}

#[test]
fn word_break_between_expansion_on_either_side_reports_present_break() {
    // 14.9.5：/E 与「周围任何文本」之间都有词边界，单边即成立。
    let left = word_break_between(
        &declaration(false, false, true),
        &declaration(false, false, false),
    );
    assert_eq!(left.word_break, WordBreak::Present);
    assert_eq!(left.rule, Some(WordBreakRule::ExpansionAdjacent));

    let right = word_break_between(
        &declaration(true, false, false),
        &declaration(false, false, true),
    );
    assert_eq!(
        right.word_break,
        WordBreak::Present,
        "/Alt 单边不成对，但邻侧的 /E 仍是词边界"
    );
    assert_eq!(right.rule, Some(WordBreakRule::ExpansionAdjacent));
    assert!(!right.conflicting);
}

#[test]
fn word_break_between_alt_pair_and_actual_pair_is_conflicting_but_present() {
    let decision = word_break_between(
        &declaration(true, true, false),
        &declaration(true, true, false),
    );
    assert_eq!(decision.word_break, WordBreak::Present);
    assert_eq!(decision.rule, Some(WordBreakRule::AlternateTextPair));
    assert!(
        decision.conflicting,
        "14.9.3 与 14.9.4 两条 shall 同时命中时必须如实标记冲突"
    );

    let expansion_conflict = word_break_between(
        &declaration(false, true, true),
        &declaration(false, true, false),
    );
    assert_eq!(expansion_conflict.word_break, WordBreak::Present);
    assert!(expansion_conflict.conflicting);
}

#[test]
fn word_break_between_empty_declarations_is_not_declared() {
    let empty = SubstitutionDeclaration::default();
    assert!(empty.is_empty());
    let decision = word_break_between(&empty, &empty);
    assert_eq!(decision.word_break, WordBreak::NotDeclared);
    assert!(!declaration(true, false, false).is_empty());
}

#[test]
fn page_marked_content_word_breaks_uses_span_property_list_pairs() {
    let content = b"/Span <</Alt (one)>> BDC BT 72 700 Td (a) Tj ET EMC \
                    /Span <</Alt (two)>> BDC BT 72 680 Td (b) Tj ET EMC \
                    /Span <</ActualText (c)>> BDC BT 72 660 Td (c) Tj ET EMC \
                    /Span <</ActualText (d)>> BDC BT 72 640 Td (d) Tj ET EMC";
    let doc = page_document(content, None);
    let boundaries = doc
        .page_marked_content_word_breaks(0)
        .expect("word breaks between spans");
    assert_eq!(boundaries.len(), 3, "{boundaries:?}");
    assert_eq!(boundaries[0].before, 0);
    assert_eq!(boundaries[0].after, 1);
    assert_eq!(boundaries[0].decision.word_break, WordBreak::Present);
    assert_eq!(
        boundaries[0].decision.rule,
        Some(WordBreakRule::AlternateTextPair)
    );
    assert_eq!(
        boundaries[1].decision.word_break,
        WordBreak::NotDeclared,
        "前侧只有 /Alt、后侧只有 /ActualText：两条 shall 都不成对"
    );
    assert_eq!(boundaries[2].decision.word_break, WordBreak::Absent);
    assert_eq!(
        boundaries[2].decision.rule,
        Some(WordBreakRule::ActualTextPair)
    );
}

#[test]
fn page_marked_content_word_breaks_falls_back_to_structure_element_entries() {
    // 属性表只带 /MCID：词边界的声明来源是结构元素字典的 /Alt（14.9.3 第一条）。
    let content = b"/P <</MCID 0>> BDC BT 72 700 Td (a) Tj ET EMC \
                    /P <</MCID 1>> BDC BT 72 680 Td (b) Tj ET EMC";
    let doc = page_document(
        content,
        Some([
            (FIRST_ELEMENT, element_with(&[("Alt", "first alt")])),
            (SECOND_ELEMENT, element_with(&[("Alt", "second alt")])),
        ]),
    );
    let boundaries = doc
        .page_marked_content_word_breaks(0)
        .expect("word breaks from structure elements");
    assert_eq!(boundaries.len(), 1, "{boundaries:?}");
    assert!(boundaries[0].before_declaration.alternate_text);
    assert_eq!(boundaries[0].decision.word_break, WordBreak::Present);
    assert_eq!(
        boundaries[0].decision.rule,
        Some(WordBreakRule::AlternateTextPair)
    );
}

#[test]
fn page_marked_content_word_breaks_reports_actual_text_pair_from_structure_elements() {
    let content = b"/P <</MCID 0>> BDC BT 72 700 Td (Dru) Tj ET EMC \
                    /P <</MCID 1>> BDC BT 72 680 Td (k-) Tj ET EMC";
    let doc = page_document(
        content,
        Some([
            (FIRST_ELEMENT, element_with(&[("ActualText", "c")])),
            (SECOND_ELEMENT, element_with(&[("ActualText", "k")])),
        ]),
    );
    let boundaries = doc
        .page_marked_content_word_breaks(0)
        .expect("word breaks for character replacement");
    assert_eq!(boundaries[0].decision.word_break, WordBreak::Absent);
}

#[test]
fn page_marked_content_word_breaks_expansion_on_one_side_splits_neighbour() {
    // 14.9.5 的 EXAMPLE：/E (Doctor) 与前后文本之间都是词边界。
    let content = b"/Span <</E (Doctor)>> BDC BT 72 700 Td (Dr.) Tj ET EMC \
                    /Span BMC BT 72 680 Td (Healwell) Tj ET EMC";
    let doc = page_document(content, None);
    let boundaries = doc
        .page_marked_content_word_breaks(0)
        .expect("expansion word break");
    assert_eq!(boundaries.len(), 1);
    assert_eq!(boundaries[0].decision.word_break, WordBreak::Present);
    assert_eq!(
        boundaries[0].decision.rule,
        Some(WordBreakRule::ExpansionAdjacent)
    );
}

#[test]
fn page_marked_content_word_breaks_has_no_boundary_for_single_section() {
    let content = b"/Span <</Alt (only)>> BDC BT 72 700 Td (a) Tj ET EMC";
    let doc = page_document(content, None);
    assert!(
        doc.page_marked_content_word_breaks(0)
            .expect("single section")
            .is_empty(),
        "首个区段之前没有可判定的相邻边界"
    );
}

#[test]
fn page_marked_content_word_breaks_propagates_malformed_property_list_text() {
    let content = b"/Span <</Alt 3>> BDC BT 72 700 Td (a) Tj ET EMC \
                    /Span <</Alt (b)>> BDC BT 72 680 Td (b) Tj ET EMC";
    let doc = page_document(content, None);
    let error = doc
        .page_marked_content_word_breaks(0)
        .expect_err("non-text /Alt shall be rejected");
    assert!(error.to_string().contains("text string"), "{error}");
}
