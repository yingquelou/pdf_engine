//! 表单字段 `/TM` 映射名的 Unicode 语言转义读端测试（ISO 32000-1
//! 7.9.2.2、12.7.3.2、12.7.5.2 与 14.9.2.1）。
//!
//! 依据的规范原文（`docs/PDF32000_2008.pdf` 的 `pdftotext -raw` 行号，均已核）：
//!
//! - Table 220「Entries common to all field dictionaries」（表头 raw L23611）的
//!   `/TM` 行在 raw L23651–L23652：「TM text string (Optional; PDF 1.3) The
//!   mapping name that shall be used when exporting interactive form field data
//!   from the document」——PDF 1.3、类型列只有 text string、未标注 inheritable
//!   （同表的 `/Ff`、`/V`、`/DV` 都显式写了 inheritable，对照可见）。
//! - 14.9.3「Alternate Descriptions」的替代描述清单在 raw L32097–L32113，字段侧
//!   **只**点名「This alternate name, if provided, shall be specified using the
//!   TU entry of the field dictionary」（raw L32113）；`/TM` 不在清单里，故
//!   NOTE 3（raw L32117–L32120）那条「Within alternate descriptions, Unicode
//!   escape sequences specifying language shall override the prevailing Lang
//!   entry」不直接施于 `/TM`，本条的转义依据只有 14.9.2.1 raw L31940–L31941
//!   对**一切** Unicode 文本串的通用 shall。
//! - 7.9.2.2 的散文与语法 a)–d) 在 raw L4218–L4219 与 L4242–L4248。
//! - 字段字典没有 `/Lang` 条目（已核：Table 220 全文无 Lang 行），故 prevailing
//!   取目录 `/Lang`（14.9.2.3 raw L31955–L31956），由调用方传入。
//! - 导出按**原值**消费映射名：12.7.5.2 Table 236 的 SubmitCoordinates 项
//!   （raw L24611–L24616）写明 name 取「the field's mapping name (TM in the
//!   field dictionary) if present」，并有「If the value of the TM entry is a
//!   single ASCII SPACE (20h) character, both the name and the ASCII PERIOD
//!   (2Eh) following it shall be suppressed」的逐字节特例——因此这里同时钉住
//!   视图只读、`Field::dict` 的原值不变。
//! - 名字形态（`/TM` 写成 name 对象）在 ISO 32000-1 里**没有**依据：Table 220
//!   的类型列是 text string。本库按注解 `/DS`（Table 174，raw L21909）的宽容
//!   口径处理：7.9.2.2 的转义只存在于 Unicode 文本串，故名字一律判「无转义」、
//!   三条入口都是 `Ok(None)` 且永不报错，属**库策略**（待核：本地只持有
//!   32000-1 副本，无法核对 ISO 32000-2 是否放宽）。
//! - 12.7.3.2「Field Names」（raw L23696–L23697）只要求各表示的 `/FT`、`/V`、
//!   `/DV` 一致，**没有**规定 `/TM` 在各表示间一致，故归并视图按前序首个表示
//!   暴露那一条 `TM`，各表示自身的仍经 `representations` 逐节点访问。

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

/// 单字段且字典里带一个形态任意的 `TM` 条目。
fn field_with_mapping_name(entry: Value) -> Field {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("name"))
                .insert("FT", name("Tx"))
                .insert("TM", entry),
        )]),
    )));
    let mut form = doc.acro_form().expect("parse").expect("form present");
    form.fields.remove(0)
}

/// 三条入口（`*_semantic_text` 压成去转义文本、`*_language` 压成标签串）的
/// 形态结论，用于逐条断言同一个「都是 `Ok(None)`」的口径。
fn mapping_name_view_shape(
    field: &Field,
    prevailing: Option<&LanguageIdentifier>,
) -> Vec<Option<String>> {
    vec![
        field
            .mapping_name_semantic_text()
            .unwrap()
            .map(|view| view.plain_text()),
        field.mapping_name_text().unwrap(),
        field
            .mapping_name_language(prevailing)
            .unwrap()
            .map(|language| language.to_string()),
    ]
}

/// 文本串形态的 `/TM` 带 7.9.2.2 转义：去转义文本 + 转义语言覆盖 prevailing
/// （14.9.2.1 raw L31940–L31941；14.9.3 未把 `/TM` 列为替代描述，见模块文档）。
#[test]
fn field_mapping_name_with_language_escape_returns_plain_text_and_declared_language() {
    let entry = text(&format!("{}Customer name", escape("fr")));
    let field = field_with_mapping_name(entry.clone());

    // 只读视图：解析路径未动，原值仍在字段字典里，`Field` 也没有 /TM 字段。
    assert_eq!(field.dict.get("TM"), Some(&entry));
    assert!(field.has_mapping_name_language_escape());
    let view = field
        .mapping_name_semantic_text()
        .expect("escaped /TM")
        .expect("/TM 视图");
    assert_eq!(view.escape_count(), 1);
    assert_eq!(view.plain_text(), "Customer name");
    assert_eq!(
        language_tag(view.segments()[0].language()),
        Some("fr"),
        "串内转义覆盖传入的 prevailing Lang"
    );
    assert_eq!(
        field.mapping_name_text().unwrap().as_deref(),
        Some("Customer name")
    );
    assert_eq!(
        language_tag(
            field
                .mapping_name_language(Some(&identifier("en")))
                .unwrap()
                .as_ref()
        ),
        Some("fr"),
        "prevailing 被转义覆盖（14.9.2.1 raw L31940–L31941）"
    );

    // 同一原串换成多语言两段时逐段暴露各自语言，整串语言仍取首个转义。
    let field = field_with_mapping_name(text(&format!(
        "{}Customer {}Kunde",
        escape("en"),
        escape("deCH")
    )));
    let view = field
        .mapping_name_semantic_text()
        .unwrap()
        .expect("two escapes");
    assert_eq!(view.escape_count(), 2);
    assert_eq!(view.plain_text(), "Customer Kunde");
    let segments = view.segments();
    assert_eq!(language_tag(segments[0].language()), Some("en"));
    assert_eq!(
        language_tag(segments[1].language()),
        Some("de-CH"),
        "ISO 3166 国家码拼为区域标签（7.9.2.2 c）"
    );
    assert_eq!(
        language_tag(
            field
                .mapping_name_language(Some(&identifier("it")))
                .unwrap()
                .as_ref()
        ),
        Some("en")
    );
}

/// 无转义的文本串 `/TM` 按 prevailing 定语言：字段字典没有 `/Lang` 条目
/// （Table 220 全文无 Lang 行），prevailing 由调用方传入目录 `/Lang`。
#[test]
fn field_mapping_name_without_escape_uses_prevailing_lang_entry() {
    let field = field_with_mapping_name(Value::String("fullName".into()));
    assert!(!field.has_mapping_name_language_escape());

    let view = field
        .mapping_name_semantic_text()
        .unwrap()
        .expect("plain /TM still yields a view");
    assert_eq!(view.escape_count(), 0);
    assert!(!view.has_language_escape());
    assert_eq!(view.plain_text(), "fullName");
    assert_eq!(language_tag(view.segments()[0].language()), None);

    let prevailing = identifier("en-US");
    assert_eq!(
        language_tag(
            field
                .mapping_name_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en-US"),
        "字段无 /Lang，prevailing 由调用方传入目录 /Lang（14.9.2.3 raw L31955–L31956）"
    );
    assert_eq!(
        field.mapping_name_language(None).unwrap(),
        None,
        "文档未声明 /Lang 时不臆造语言"
    );
    // 无关条目不受影响：本节点既没有 /TU 也没被改写。
    assert_eq!(field.alternate_name, None);
    assert_eq!(
        field.dict.get("TM"),
        Some(&Value::String("fullName".into()))
    );
}

/// 名字形态的 `/TM`：转义只存在于 Unicode 文本串，故一律「无转义」、三条入口
/// 都 `Ok(None)` 且**永不报错**（Table 220 类型列只写 text string，名字写法属
/// 库策略宽容，见模块文档）。
#[test]
fn field_mapping_name_as_name_object_is_never_an_error_and_has_no_escape() {
    // 即使解码出的名字里真有 U+001B，也不按转义处理（与注解 `/DS` 同口径）。
    let entry = name(&format!("{}Zip", escape("de")));
    let field = field_with_mapping_name(entry.clone());

    assert_eq!(field.dict.get("TM"), Some(&entry));
    assert!(
        !field.has_mapping_name_language_escape(),
        "名字形态恒为「无转义」"
    );
    // 三条入口都是 `Ok(None)`：helper 内的 `unwrap` 即「永不报错」的断言。
    assert_eq!(
        mapping_name_view_shape(&field, Some(&identifier("en"))),
        vec![None, None, None]
    );

    // 普通名字同样 None，不是错误。
    let field = field_with_mapping_name(name("CustomerName"));
    assert!(!field.has_mapping_name_language_escape());
    assert_eq!(
        mapping_name_view_shape(&field, Some(&identifier("fr"))),
        vec![None, None, None]
    );
}

/// 缺 `TM` 键：不物化空视图。`/TM` 未标注 inheritable（Table 220），
/// 故祖先声明的那一条不下传给子节点。
#[test]
fn field_without_mapping_name_has_no_language_view() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("T", text("plain")).insert("FT", name("Tx")),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("form present");
    let field = &form.fields[0];
    assert_eq!(field.dict.get("TM"), None);
    assert!(!field.has_mapping_name_language_escape());
    assert_eq!(
        mapping_name_view_shape(field, Some(&identifier("en"))),
        vec![None, None, None]
    );

    // 父节点的 `/TM` 不下传，子节点仍是没有视图。
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("person"))
                .insert("TM", text(&format!("{}Person", escape("de"))))
                .insert(
                    "Kids",
                    arr(vec![Value::Dict(
                        Dict::new().insert("T", text("name")).insert("FT", name("Tx")),
                    )]),
                ),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("form present");
    assert!(form.fields[0].has_mapping_name_language_escape());
    assert!(!form.fields[0].kids[0].has_mapping_name_language_escape());
    assert_eq!(
        mapping_name_view_shape(&form.fields[0].kids[0], Some(&identifier("de"))),
        vec![None, None, None]
    );
}

/// 文本串形态里的残缺转义：不毁掉表单读取，但索取语言视图时返回类型化
/// Malformed（库策略：不静默把残缺转义当普通文本），错误串带 `form field /TM`。
#[test]
fn field_mapping_name_unterminated_escape_is_typed_malformed_error() {
    let field = field_with_mapping_name(text(&format!("broken {}", unterminated_escape("en"))));
    assert!(
        field.has_mapping_name_language_escape(),
        "残缺转义仍含 U+001B，存在性判断不校验合法性"
    );
    assert_eq!(
        field.dict.get("TM"),
        Some(&text(&format!("broken {}", unterminated_escape("en")))),
        "原值仍按解析结果留在字段字典里"
    );

    for result in [
        field.mapping_name_semantic_text().map(|_| ()),
        field.mapping_name_text().map(|_| ()),
        field.mapping_name_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("form field /TM"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }
}

/// 既非文本串又非名字的 `/TM`（数字）按 14.9.3 raw L32115–L32116 同源的
/// 形态门禁返回 Malformed：表单读取仍成功，视图才报错。
#[test]
fn field_mapping_name_of_unsupported_value_shape_is_typed_malformed_error() {
    let field = field_with_mapping_name(Value::Integer(7));
    assert!(!field.has_mapping_name_language_escape());
    let error = field
        .mapping_name_semantic_text()
        .expect_err("an integer is neither a text string nor a name");
    assert!(
        matches!(error, LanguageError::Malformed(_)),
        "got {error:?}"
    );
    assert!(error.to_string().contains("form field /TM"), "{error}");
    assert!(error.to_string().contains("integer"), "{error}");
}

/// 归并视图的映射名按前序首个表示暴露（12.7.3.2 只要求各表示的 `/FT` `/V`
/// `/DV` 一致，未规定 `/TM` 一致），语言视图即那一条 `TM` 的视图。
#[test]
fn logical_field_mapping_name_escape_view_matches_first_representation() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            widget(arr(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(20),
                Value::Integer(20),
            ]))
            .insert("T", text("zip"))
            .insert("TM", text(&format!("{}Postal code", escape("es")))),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("form present");
    let group = &form.logical_fields()[0];
    assert_eq!(group.fully_qualified_name, "zip");
    assert!(group.has_mapping_name_language_escape());
    assert_eq!(
        group.mapping_name_text().unwrap().as_deref(),
        Some("Postal code")
    );
    assert_eq!(
        language_tag(
            group
                .mapping_name_language(Some(&identifier("en")))
                .unwrap()
                .as_ref()
        ),
        Some("es")
    );
    // 与节点视图逐字一致：同一个原值、同一个解析结果。
    let node = group.representations[0];
    assert_eq!(
        node.mapping_name_semantic_text().unwrap(),
        group.mapping_name_semantic_text().unwrap()
    );
}

/// 归并视图没有 `/TM`，以及首个表示写的是名字形态时，都是 `Ok(None)` 不报错。
#[test]
fn logical_field_without_textual_mapping_name_has_no_language_view() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![
            Value::Dict(
                Dict::new()
                    .insert("T", text("plain"))
                    .insert("FT", name("Tx"))
                    .insert("TM", name("PlainName")),
            ),
            Value::Dict(
                Dict::new()
                    .insert("T", text("bare"))
                    .insert("FT", name("Tx")),
            ),
        ]),
    )));
    let form = doc.acro_form().expect("parse").expect("form present");
    let groups = form.logical_fields();
    assert_eq!(groups.len(), 2);
    for group in groups {
        assert!(!group.has_mapping_name_language_escape());
        assert_eq!(group.mapping_name_semantic_text().unwrap(), None);
        assert_eq!(group.mapping_name_text().unwrap(), None);
        assert_eq!(
            group
                .mapping_name_language(Some(&identifier("en")))
                .unwrap(),
            None
        );
    }
}
