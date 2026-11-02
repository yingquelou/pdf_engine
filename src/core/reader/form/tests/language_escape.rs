//! 表单字段 `/TU` 替代名的语言转义读端测试（ISO 32000-1 7.9.2.2、
//! 12.7.3.2、14.9.2.1 与 14.9.3）。
//!
//! Table 220（raw L23645–L23650）把 `/TU` 定为「替代字段名」，并写明它在
//! 提取内容供无障碍使用时有用（见 14.9.3）；14.9.3（raw L32111–L32114）
//! 要求替代名「shall be used in place of the actual field name」，NOTE 2
//! （raw L32114）指出它「useful for vocalization purposes」，NOTE 3
//! （raw L32117–L32120）要求「Unicode escape sequences specifying language
//! shall override the prevailing Lang entry」。字段字典按 Table 220 没有
//! `/Lang` 条目，故 prevailing 即目录 `/Lang`（14.9.2.3 raw
//! L31955–L31956）。
//!
//! 12.7.3.2（raw L23696–L23697）只要求各表示的 `/FT`、`/V`、`/DV` 一致，
//! **没有**规定 `/TU` 一致，也未把它标为可继承条目，因此这里钉住：节点
//! `Field` 与归并视图 `LogicalField` 各自解析自己暴露的那一条替代名。

use crate::core::reader::accessibility::{LANGUAGE_ESCAPE, LanguageError, LanguageIdentifier};

use super::*;

/// 7.9.2.2 a)–d) 的一个完整语言序列（U+001B + 码 + U+001B）。
fn escape(code: &str) -> String {
    format!("{LANGUAGE_ESCAPE}{code}{LANGUAGE_ESCAPE}")
}

/// 只有开界符、没有闭界符的残缺转义。
fn unterminated_escape(code: &str) -> String {
    format!("{LANGUAGE_ESCAPE}{code}")
}

fn identifier(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

/// 语言结论写成可读的标签串。
fn language_tag(language: Option<&LanguageIdentifier>) -> Option<&str> {
    language.map(LanguageIdentifier::as_str)
}

/// 单字段的 `/AcroForm`，`/TU` 取给定文本串。
fn field_of(alternate_name: Value) -> Field {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("name"))
                .insert("FT", name("Tx"))
                .insert("TU", alternate_name),
        )]),
    )));
    let mut form = doc.acro_form().expect("parse").expect("form present");
    form.fields.remove(0)
}

#[test]
fn field_alternate_name_with_language_escape_returns_plain_text_and_declared_language() {
    let alternate = field_of(text(&format!("{}Nom", escape("fr"))));

    // `alternate_name` 保留解码原串（不去转义）。
    assert_eq!(
        alternate.alternate_name.as_deref(),
        Some("\u{1b}fr\u{1b}Nom"),
        "/TU keeps the undecoded escaped string"
    );
    assert!(alternate.has_alternate_name_language_escape());
    assert_eq!(
        alternate
            .alternate_name_text()
            .unwrap()
            .as_deref()
            .expect("escaped /TU"),
        "Nom"
    );
    assert_eq!(
        language_tag(
            alternate
                .alternate_name_language(Some(&identifier("en")))
                .unwrap()
                .as_ref()
        ),
        Some("fr"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.3 NOTE 3）"
    );
}

#[test]
fn field_alternate_name_without_escape_keeps_prevailing_lang_entry() {
    let alternate = field_of(Value::String("Full name".into()));
    assert!(!alternate.has_alternate_name_language_escape());

    let text_view = alternate
        .alternate_name_semantic_text()
        .unwrap()
        .expect("plain /TU still yields a view");
    assert_eq!(text_view.escape_count(), 0);
    assert!(!text_view.has_language_escape());
    assert_eq!(text_view.plain_text(), "Full name");
    assert_eq!(text_view.segments().len(), 1);
    assert_eq!(
        language_tag(text_view.segments()[0].language()),
        None,
        "无转义的段不携带语言，由 prevailing 决定"
    );

    let prevailing = identifier("en-GB");
    assert_eq!(
        language_tag(
            alternate
                .alternate_name_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en-GB"),
        "字段自身没有 /Lang，prevailing 由调用方传入目录 /Lang"
    );
    assert_eq!(
        alternate.alternate_name_language(None).unwrap(),
        None,
        "文档未声明 /Lang 时不臆造语言"
    );
}

#[test]
fn field_without_alternate_name_has_no_language_view() {
    // 缺 `/TU`：不物化空视图。
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("T", text("plain")).insert("FT", name("Tx")),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("form present");
    let field = &form.fields[0];
    assert!(!field.has_alternate_name_language_escape());
    assert_eq!(field.alternate_name_semantic_text().unwrap(), None);
    assert_eq!(field.alternate_name_text().unwrap(), None);
    assert_eq!(
        field
            .alternate_name_language(Some(&identifier("en")))
            .unwrap(),
        None,
        "无替代名时不返回 prevailing"
    );

    // 祖先声明的 `/TU` 不下传（Table 220 未标 inheritable），子节点仍是 None。
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("person"))
                .insert("TU", text(&format!("{}Person", escape("de"))))
                .insert(
                    "Kids",
                    arr(vec![Value::Dict(
                        Dict::new().insert("T", text("name")).insert("FT", name("Tx")),
                    )]),
                ),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("form present");
    assert!(form.fields[0].has_alternate_name_language_escape());
    assert_eq!(form.fields[0].kids[0].alternate_name, None);
    assert!(!form.fields[0].kids[0].has_alternate_name_language_escape());
    assert_eq!(form.fields[0].kids[0].alternate_name_text().unwrap(), None);
}

#[test]
fn field_alternate_name_unterminated_escape_is_typed_malformed_error() {
    // 残缺转义不毁掉表单读取，但索取语言视图时必须报错（库策略：不静默把
    // 残缺转义当普通文本），错误沿用既有 LanguageError::Malformed 变体。
    let field = field_of(text(&format!("broken {}", unterminated_escape("en"))));
    assert!(
        field.has_alternate_name_language_escape(),
        "残缺转义仍含 U+001B，存在性判断不校验合法性"
    );
    assert_eq!(
        field.alternate_name.as_deref(),
        Some("broken \u{1b}en"),
        "原串仍按解析结果保留"
    );

    for result in [
        field.alternate_name_semantic_text().map(|_| ()),
        field.alternate_name_text().map(|_| ()),
        field.alternate_name_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("form field /TU"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }
}

#[test]
fn field_alternate_name_segments_split_at_each_escape() {
    let field = field_of(text(&format!(
        "{}Label {}Beschriftung",
        escape("en"),
        escape("deAT")
    )));
    let view = field
        .alternate_name_semantic_text()
        .unwrap()
        .expect("both escapes terminate");
    assert_eq!(view.escape_count(), 2);
    assert_eq!(view.plain_text(), "Label Beschriftung");
    let segments = view.segments();
    assert_eq!(segments.len(), 2);
    assert_eq!(language_tag(segments[0].language()), Some("en"));
    assert_eq!(segments[0].text(), "Label ");
    assert_eq!(
        language_tag(segments[1].language()),
        Some("de-AT"),
        "ISO 3166 国家码拼为区域标签（7.9.2.2 c）"
    );
    assert_eq!(segments[1].text(), "Beschriftung");
    // 整串语言取首个转义；逐段结论以 `segments` 为准。
    assert_eq!(
        language_tag(view.effective_language(Some(&identifier("fr"))).as_ref()),
        Some("en")
    );
}

/// 12.7.3.2 只要求各表示的 `/FT` `/V` `/DV` 一致，替代名按前序首个表示
/// 暴露；`LogicalField` 的语言视图即该条 `/TU` 的视图。
#[test]
fn logical_field_alternate_name_escape_view_matches_first_representation() {
    let named_widget = |label: &str| {
        Value::Dict(
            widget(arr(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(20),
                Value::Integer(20),
            ]))
            .insert("T", text(label))
            .insert("TU", text(&format!("{}Group label", escape("es")))),
        )
    };
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![named_widget("first"), named_widget("second")]),
    )));
    let form = doc.acro_form().expect("parse").expect("form present");
    let groups = form.logical_fields();
    assert_eq!(groups.len(), 2, "distinct partial names stay distinct");

    let group = &groups[0];
    assert_eq!(group.fully_qualified_name, "first");
    assert!(group.has_alternate_name_language_escape());
    assert_eq!(
        group.alternate_name_text().unwrap().as_deref(),
        Some("Group label")
    );
    assert_eq!(
        language_tag(
            group
                .alternate_name_language(Some(&identifier("en")))
                .unwrap()
                .as_ref()
        ),
        Some("es")
    );
    // 与节点视图逐字一致：同一个原串、同一个解析结果。
    let node = group.representations[0];
    assert_eq!(
        node.alternate_name_semantic_text().unwrap(),
        group.alternate_name_semantic_text().unwrap()
    );
}

/// 归并视图没有 `/TU` 时同样返回 `Ok(None)`，不误报残缺。
#[test]
fn logical_field_without_alternate_name_has_no_language_view() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("T", text("plain")).insert("FT", name("Tx")),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("form present");
    let group = &form.logical_fields()[0];
    assert_eq!(group.alternate_name, None);
    assert!(!group.has_alternate_name_language_escape());
    assert_eq!(group.alternate_name_semantic_text().unwrap(), None);
    assert_eq!(group.alternate_name_text().unwrap(), None);
    assert_eq!(
        group
            .alternate_name_language(Some(&identifier("en")))
            .unwrap(),
        None
    );
}
