use super::*;

/// 单字段的 `/AcroForm`，`/RV` 取给定字典值。
///
/// Table 222（raw L23767–L23771）把 `/RV` 定为「rich text string」
/// （text string or text stream，PDF 1.5）；本库不建模它，值留在
/// `Field::dict`，视图访问器只读回查形态。
fn field_with_rich_value(rich_value: Value) -> Field {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("name"))
                .insert("FT", name("Tx"))
                .insert("RV", rich_value),
        )]),
    )));
    let mut form = doc.acro_form().expect("parse").expect("form present");
    form.fields.remove(0)
}

/// 父节点声明 `/RV`、子节点未声明的两层字段树，返回 `(父, 子)`。
fn group_and_leaf_with_rich_value_on_group(rich_value: Value) -> (Field, Field) {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("group"))
                .insert("RV", rich_value)
                .insert(
                    "Kids",
                    arr(vec![Value::Dict(
                        Dict::new().insert("T", text("leaf")).insert("FT", name("Tx")),
                    )]),
                ),
        )]),
    )));
    let mut form = doc.acro_form().expect("parse").expect("form present");
    let mut group = form.fields.remove(0);
    let leaf = group.kids.remove(0);
    (group, leaf)
}

#[test]
fn field_rich_value_with_language_escape_returns_plain_text_and_declared_language() {
    let rich = format!("{}<body>Valeur</body>", escape("fr"));
    let field = field_with_rich_value(text(&rich));

    assert_eq!(
        field.dict.get("RV"),
        Some(&text(&rich)),
        "只读加法：解析器不建模 /RV，原值仍留在字典"
    );
    assert!(field.has_rich_value_language_escape());
    assert_eq!(
        field
            .rich_value_semantic_text()
            .unwrap()
            .expect("escaped /RV")
            .escape_count(),
        1
    );
    assert_eq!(
        field
            .rich_value_text()
            .unwrap()
            .as_deref()
            .expect("/RV 视图"),
        "<body>Valeur</body>"
    );

    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            field
                .rich_value_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("fr"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.3 raw L32117–L32120）"
    );
    // 无关条目不受影响：本字段没有 /TU。
    assert_eq!(field.alternate_name_semantic_text().unwrap(), None);
}

#[test]
fn field_rich_value_with_two_language_escapes_exposes_per_segment_languages() {
    let rich = format!("{}<p>One</p>{}<p>Eins</p>", escape("en"), escape("deAT"));
    let field = field_with_rich_value(text(&rich));

    let view = field
        .rich_value_semantic_text()
        .unwrap()
        .expect("two escapes");
    assert_eq!(view.escape_count(), 2);
    assert_eq!(view.segments().len(), 2);
    assert_eq!(
        view.segments()[0].language().map(|l| l.as_str()),
        Some("en")
    );
    assert_eq!(view.segments()[0].text(), "<p>One</p>");
    assert_eq!(
        view.segments()[1].language().map(|l| l.as_str()),
        Some("de-AT"),
        "ISO 3166 国家码拼为区域标签（7.9.2.2 c）"
    );
    assert_eq!(
        field.rich_value_language(Some(&identifier("ja"))).unwrap(),
        Some(identifier("en")),
        "首个转义决定整串生效语言"
    );
}

#[test]
fn field_rich_value_without_language_escape_keeps_original_string_and_prevailing_lang() {
    let field = field_with_rich_value(text("<body>Plain</body>"));
    assert!(!field.has_rich_value_language_escape());
    let view = field
        .rich_value_semantic_text()
        .unwrap()
        .expect("plain /RV still yields a view");
    assert_eq!(view.escape_count(), 0);
    assert_eq!(view.segments().len(), 1);
    assert_eq!(view.segments()[0].language(), None);
    assert_eq!(
        field
            .rich_value_text()
            .unwrap()
            .as_deref()
            .expect("/RV 视图"),
        "<body>Plain</body>",
        "无转义时自然文本即原串"
    );
    assert_eq!(
        field
            .rich_value_language(Some(&identifier("en-US")))
            .unwrap(),
        Some(identifier("en-US")),
        "无转义时原样返回 prevailing"
    );
    assert_eq!(field.rich_value_language(None).unwrap(), None);
}

#[test]
fn field_without_rich_value_has_no_language_view() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("T", text("plain")).insert("FT", name("Tx")),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("form present");
    let field = &form.fields[0];
    assert_eq!(field.dict.get("RV"), None);
    assert!(!field.has_rich_value_language_escape());
    assert_eq!(
        field.rich_value_semantic_text().unwrap(),
        None,
        "缺 /RV 返回 Ok(None)"
    );
    assert_eq!(field.rich_value_text().unwrap(), None);
    assert_eq!(
        field.rich_value_language(Some(&identifier("en"))).unwrap(),
        None,
        "没有 /RV 就无所谓语言"
    );
}

/// 名字形态的 `/RV`（不是 7.9.2.2 定义转义的 Unicode 文本串）恒判「无转义」，
/// 视图 `Ok(None)` 且不报错、自然文本即名字本身、语言回落 prevailing——
/// 与注解 FreeText `/DS` 的名字形态口径同规则。
#[test]
fn field_rich_value_as_name_has_no_language_escape_and_returns_no_view() {
    let escaped_name = format!("body{}", escape("ja"));
    for value in [name("body"), Value::Name(escaped_name.clone())] {
        let field = field_with_rich_value(value.clone());
        assert!(
            !field.has_rich_value_language_escape(),
            "名字形态不携带转义"
        );
        assert_eq!(
            field.rich_value_semantic_text().unwrap(),
            None,
            "名字形态没有语言视图，也不报错"
        );
        match &value {
            Value::Name(text_value) => assert_eq!(
                field.rich_value_text().unwrap().as_deref(),
                Some(text_value.as_str()),
                "原样返回名字，不剥离任何字符"
            ),
            other => panic!("expected a name value, got {other:?}"),
        }
        assert_eq!(
            field.rich_value_language(Some(&identifier("de"))).unwrap(),
            Some(identifier("de")),
            "名字形态下 prevailing 保持生效"
        );
        assert_eq!(field.rich_value_language(None).unwrap(), None);
    }
}

/// 文本流形态的 `/RV`（Table 222 允许 text string **or** text stream）：
/// 本切片不解引用、不取流字节，故与名字形态一样判「无转义」、`Ok(None)`、
/// 不报错，语言回落 prevailing。
#[test]
fn field_rich_value_as_text_stream_has_no_language_escape_and_returns_no_view() {
    let stream = Value::Stream(
        Dict::new().insert("Type", name("OCMD")),
        crate::core::types::StreamData::Owned(
            PdfTextString::from(format!("{}<body>Streamed</body>", escape("ja"))).encode(),
        ),
    );
    let field = field_with_rich_value(stream.clone());
    assert_eq!(field.dict.get("RV"), Some(&stream));
    assert!(!field.has_rich_value_language_escape());
    assert_eq!(
        field.rich_value_semantic_text().unwrap(),
        None,
        "流形态需解引用取字节，本切片不扩，故无视图且不报错"
    );
    assert_eq!(field.rich_value_text().unwrap(), None);
    assert_eq!(
        field.rich_value_language(Some(&identifier("en"))).unwrap(),
        Some(identifier("en")),
        "读不到转义时回落 prevailing"
    );

    // 指向文本流的间接引用同形态处置。
    let field = field_with_rich_value(Value::Ref(7, 0));
    assert!(!field.has_rich_value_language_escape());
    assert_eq!(field.rich_value_semantic_text().unwrap(), None);
    assert_eq!(
        field.rich_value_language(Some(&identifier("fr"))).unwrap(),
        Some(identifier("fr"))
    );
}

#[test]
fn field_rich_value_unterminated_escape_is_typed_malformed_error() {
    let broken = format!("broken {}", unterminated_escape("en"));
    let field = field_with_rich_value(text(&broken));
    assert!(field.has_rich_value_language_escape());
    assert_eq!(
        field.dict.get("RV"),
        Some(&text(&broken)),
        "原值按解析结果留在字典，读取表单不失败"
    );

    for result in [
        field.rich_value_semantic_text().map(|_| ()),
        field.rich_value_text().map(|_| ()),
        field.rich_value_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("form field /RV"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }

    // 码字节非 ASCII 字母：同样只在索取视图时报错。
    let error = field_with_rich_value(text(&format!("x{}x", escape("e1"))))
        .rich_value_semantic_text()
        .expect_err("digits are not ISO 639 codes");
    assert!(error.to_string().contains("form field /RV"), "{error}");
    assert!(error.to_string().contains("ASCII letters"), "{error}");
}

/// 既非文本串、也非名字/流形态的 `/RV`（Table 222 不给这些形态）报
/// 带 `form field /RV` 上下文的类型化 Malformed。
#[test]
fn field_rich_value_with_number_value_is_typed_malformed_error() {
    let field = field_with_rich_value(Value::Integer(7));
    assert!(!field.has_rich_value_language_escape());
    let error = field
        .rich_value_semantic_text()
        .expect_err("an integer is not a rich text string");
    assert!(matches!(error, LanguageError::Malformed(_)), "{error:?}");
    assert!(error.to_string().contains("form field /RV"), "{error}");
    assert!(
        error
            .to_string()
            .contains("shall be a text string or text stream, found integer"),
        "{error}"
    );
    assert!(field.rich_value_language(None).is_err());
}

/// Table 222 只把 `/DA`、`/Q` 标为 inheritable，`/RV` 没有该标注：父节点
/// 声明的富文本值不落到子表示上（与 `/TU` 同一口径）。
#[test]
fn field_rich_value_is_not_inherited_by_child_field() {
    let rich = format!("{}<body>Groupe</body>", escape("fr"));
    let (group, leaf) = group_and_leaf_with_rich_value_on_group(text(&rich));

    assert!(group.has_rich_value_language_escape());
    assert_eq!(
        group
            .rich_value_text()
            .unwrap()
            .as_deref()
            .expect("group /RV"),
        "<body>Groupe</body>"
    );
    assert_eq!(leaf.dict.get("RV"), None, "解析器不做 /RV 继承");
    assert!(!leaf.has_rich_value_language_escape());
    assert_eq!(leaf.rich_value_semantic_text().unwrap(), None);
    assert_eq!(leaf.rich_value_text().unwrap(), None);
    assert_eq!(
        leaf.rich_value_language(Some(&identifier("en"))).unwrap(),
        None,
        "子节点没有 /RV 就无所谓语言"
    );
}
