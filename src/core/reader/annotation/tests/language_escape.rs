//! 注解 `/T` 标题、`/Subj` 主题、`/NM` 注解名与 FreeText `/DS` 默认样式串的
//! 语言转义读端测试（ISO 32000-1 7.9.2.2、12.5.2 Table 164、
//! 12.5.6.2 Table 170、12.5.6.3 Table 174、14.9.2.1 与 14.9.3）。
//!
//! Table 170（raw L21666–L21668）把 `/T` 定为「显示在注解弹出窗口标题栏的
//! 文本标签」，L21706–L21707 把 `/Subj` 定为「简述注解主题」的文本串；
//! Table 164（raw L21283–L21284）把 `/NM` 定为「注解名，在本页所有注解中唯一
//! 标识它的文本串」（PDF 1.4）；Table 174（raw L21909）把 FreeText 的 `/DS`
//! 定为「默认样式串，见 12.7.3.4 Rich Text Strings」（PDF 1.5）。
//! 7.9.2.2（raw L4242–L4248）允许任意 Unicode 文本串内带
//! U+001B 定界的语言转义；14.9.2.1（raw L31940–L31941）与 14.9.3 NOTE 3
//! （raw L32117–L32120）要求「串内转义覆盖 prevailing Lang」。注解字典按
//! Table 164/Table 170 没有 `/Lang` 条目，故 prevailing 即目录 `/Lang`
//! （14.9.2.3 raw L31955–L31956「catalogue Lang shall 为全文默认语言」）。
//! 这里钉住 `Annotation` 的口径：`title` / `subject` / `name` 保留解码原串、
//! `*_text()` 给去转义文本、`*_language(prevailing)` 给生效语言、
//! `*_semantic_text()` 给逐段视图。
//!
//! `/DS` 另有一条形态口径：解析器按库策略用 `parse_name_or_text` 同时接受
//! **名字**与**文本串**（Table 174 只声明 text string），且 `Annotation::default_style`
//! 存的是解码后的 `String`；7.9.2.2 的转义只定义在文本串里，故名字形态一律判为
//! 「无转义」并返回 `Ok(None)` 视图而非报错。
//!
//! 7.9.2.2 a) 把转义写成「the byte sequence 0 followed by 27」，即 UTF-16BE
//! 的 `00 1B`，故夹具用 `PdfTextString` 编出的 UTF-16BE 十六进制串；
//! PDFDocEncoding 侧另有一条测试钉住「字面串里的 0x1B 解为 U+02D9、因而
//! 不构成转义」。

use crate::core::reader::accessibility::{
    LANGUAGE_ESCAPE, LanguageError, LanguageIdentifier, SemanticText,
};
use crate::core::reader::annotation::*;
use crate::core::types::{PdfTextString, Value};

use super::support::*;

/// 7.9.2.2 a)–d) 的一个完整语言转义序列（U+001B + 码 + U+001B）。
fn escape(code: &str) -> String {
    format!("{LANGUAGE_ESCAPE}{code}{LANGUAGE_ESCAPE}")
}

/// 只有开界符、没有闭界符的残缺转义。
fn unterminated_escape(code: &str) -> String {
    format!("{LANGUAGE_ESCAPE}{code}")
}

/// 按 `PdfTextString` 编码：含 U+001B 等非 PDFDocEncoding 字符时整体退到
/// UTF-16BE + BOM，再写成十六进制文本串。
fn unicode_text(text: &str) -> Value {
    let hex: String = PdfTextString::from(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    Value::HexString(hex)
}

fn identifier(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

/// 语言结论写成可读的标签串。
fn language_tag(language: Option<&LanguageIdentifier>) -> Option<&str> {
    language.map(LanguageIdentifier::as_str)
}

/// 单条 `Text` 注解，`/T` 为给定的文本串值。
fn annotation_with_title(title: Value) -> Annotation {
    let document = doc(vec![catalog_object()]);
    parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Text").insert("T", title),
    )
    .expect("annotation parses")
}

/// 单条不含 `/T` 的 `Text` 注解。
fn annotation_without_title() -> Annotation {
    let document = doc(vec![catalog_object()]);
    parse_annotation(&document, 0, None, &base_annotation("Text")).expect("annotation parses")
}

#[test]
fn annotation_title_with_language_escape_returns_plain_text_and_declared_language() {
    let title = format!("{}付箋", escape("ja"));
    let annotation = annotation_with_title(unicode_text(&title));

    // `title` 保留解码原串（不去转义），与书签 `/Title` 同一口径。
    assert_eq!(
        annotation.title,
        Some(unicode_text(&title)),
        "/T keeps the undecoded escaped string"
    );
    assert!(annotation.has_title_language_escape());
    assert_eq!(
        annotation
            .title_semantic_text()
            .unwrap()
            .expect("escaped /T")
            .escape_count(),
        1
    );
    assert_eq!(annotation.title_text().unwrap().as_deref(), Some("付箋"));
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .title_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("ja"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941）"
    );
}

#[test]
fn annotation_title_without_escape_keeps_prevailing_lang_entry() {
    let annotation = annotation_with_title(Value::String("Reviewer".into()));
    assert!(!annotation.has_title_language_escape());
    assert_eq!(
        annotation.title_text().unwrap().as_deref(),
        Some("Reviewer"),
        "无转义时自然文本即原串"
    );
    let text = annotation
        .title_semantic_text()
        .unwrap()
        .expect("plain title still yields a view");
    assert_eq!(text.escape_count(), 0);
    assert!(!text.has_language_escape());
    assert_eq!(text.segments().len(), 1);
    assert_eq!(segments_language_tag(&text, 0), None);

    let prevailing = identifier("en-US");
    assert_eq!(
        language_tag(
            annotation
                .title_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en-US"),
        "注解自身没有 /Lang，prevailing 由调用方传入目录 /Lang"
    );
    assert_eq!(
        annotation.title_language(None).unwrap(),
        None,
        "文档未声明 /Lang 时不臆造语言"
    );
}

#[test]
fn annotation_without_title_has_no_language_view() {
    let annotation = annotation_without_title();
    assert!(!annotation.has_title_language_escape());
    assert_eq!(annotation.title, None);
    assert_eq!(
        annotation.title_semantic_text().unwrap(),
        None,
        "缺 /T 返回 Ok(None) 而非空视图"
    );
    assert_eq!(annotation.title_text().unwrap(), None);
    assert_eq!(annotation.title_language(None).unwrap(), None);
}

#[test]
fn annotation_title_segments_split_at_each_escape() {
    let title = format!("{}Note {}Bemerkung", escape("en"), escape("fr"));
    let annotation = annotation_with_title(unicode_text(&title));

    let text = annotation
        .title_semantic_text()
        .unwrap()
        .expect("both escapes terminate");
    assert_eq!(text.escape_count(), 2);
    assert!(text.has_language_escape());
    let segments = text.segments();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].text(), "Note ");
    assert_eq!(segments[1].text(), "Bemerkung");
    assert_eq!(text.plain_text(), "Note Bemerkung");
    // 整串语言取首个转义（14.9.3 NOTE 3 的 shall）；逐段结论以 `segments` 为准。
    assert_eq!(
        language_tag(text.effective_language(Some(&identifier("fr"))).as_ref()),
        Some("en")
    );
}

#[test]
fn annotation_title_segment_language_follows_escape_and_prevailing_before_it() {
    // 转义之前的段沿用 prevailing（language 为 None），其后的段取转义语言。
    let title = format!("Vorwort {}Ende", escape("de"));
    let annotation = annotation_with_title(unicode_text(&title));
    let text = annotation
        .title_semantic_text()
        .unwrap()
        .expect("escape terminates");
    assert_eq!(text.segments().len(), 2);
    assert_eq!(segments_language_tag(&text, 0), None);
    assert_eq!(text.segments()[0].text(), "Vorwort ");
    assert_eq!(segments_language_tag(&text, 1), Some("de"));
    assert_eq!(text.segments()[1].text(), "Ende");
    // 逐段语言不受整串结论影响。
    assert_eq!(
        language_tag(
            annotation
                .title_language(Some(&identifier("en")))
                .unwrap()
                .as_ref()
        ),
        Some("de")
    );
}

#[test]
fn annotation_title_with_iso_3166_country_code_escape_resolves_regional_tag() {
    // 7.9.2.2 c)（raw L4246）的可选 2 字节 ISO 3166 码拼为 `de-AT`。
    let title = format!("{}Einleitung", escape("deAT"));
    let annotation = annotation_with_title(unicode_text(&title));
    assert_eq!(
        annotation.title_text().unwrap().as_deref(),
        Some("Einleitung")
    );
    assert_eq!(
        language_tag(annotation.title_language(None).unwrap().as_ref()),
        Some("de-AT")
    );
}

#[test]
fn annotation_title_unterminated_escape_is_typed_malformed_error() {
    // 残缺转义不毁掉注解本身的读取，但索取语言视图时必须报错（库策略：不
    // 静默把残缺转义当普通文本），错误沿用既有 LanguageError::Malformed。
    let title = format!("broken {}", unterminated_escape("en"));
    let annotation = annotation_with_title(unicode_text(&title));
    assert!(
        annotation.has_title_language_escape(),
        "残缺转义仍含 U+001B，存在性判断不校验合法性"
    );
    assert_eq!(
        annotation.title,
        Some(unicode_text(&title)),
        "原串仍按解析结果保留"
    );

    for result in [
        annotation.title_semantic_text().map(|_| ()),
        annotation.title_text().map(|_| ()),
        annotation.title_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("annotation /T"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }
}

#[test]
fn annotation_title_non_ascii_letter_escape_code_is_typed_malformed_error() {
    let title = format!("{}x", escape("e1"));
    let annotation = annotation_with_title(unicode_text(&title));
    let error = annotation
        .title_semantic_text()
        .expect_err("digits are no ISO 639 code");
    assert!(matches!(error, LanguageError::Malformed(_)));
    assert!(error.to_string().contains("ASCII letters"), "{error}");

    // 码长非 2/4 同样报错，且带上条目上下文。
    let title = format!("{}x", escape("eng"));
    let error = annotation_with_title(unicode_text(&title))
        .title_text()
        .expect_err("three-byte codes are not a valid escape");
    assert!(error.to_string().contains("annotation /T"), "{error}");
    assert!(error.to_string().contains("2-byte ISO 639"), "{error}");
}

#[test]
fn annotation_title_literal_pdfdocencoding_escape_byte_is_not_a_language_escape() {
    // 字面串里的 0x1B 按 PDFDocEncoding（Annex D）解为 U+02D9，不是 U+001B，
    // 故不构成转义：prevailing Lang 不被覆盖、也无转义可剥离。
    let raw = format!("{}Chapter", escape("ja"));
    let annotation = annotation_with_title(Value::String(raw));
    assert!(
        !annotation.has_title_language_escape(),
        "PDFDocEncoding 字面串解不出 U+001B，故无转义"
    );
    assert_eq!(
        annotation.title_text().unwrap().as_deref(),
        Some("\u{02D9}ja\u{02D9}Chapter"),
        "原样返回解码后的文本"
    );
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .title_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en"),
        "无转义时 prevailing 保持生效"
    );
}

/// 第 `index` 段的转义语言标签（越界即 panic，便于测试失败定位）。
fn segments_language_tag(text: &SemanticText, index: usize) -> Option<&str> {
    language_tag(text.segments()[index].language())
}

/// 单条 `Text` 注解，`/Subj` 为给定的文本串值。
fn annotation_with_subject(subject: Value) -> Annotation {
    let document = doc(vec![catalog_object()]);
    parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Text").insert("Subj", subject),
    )
    .expect("annotation parses")
}

#[test]
fn annotation_subject_with_language_escape_returns_plain_text_and_declared_language() {
    // Table 170 L21706–L21707 的 `/Subj` 是文本串，故与 `/T` 共用 7.9.2.2
    // 的转义口径：原串保留、语言视图按需解析。
    let subject = format!("{}Rechnung", escape("de"));
    let annotation = annotation_with_subject(unicode_text(&subject));

    assert_eq!(
        annotation.subject,
        Some(unicode_text(&subject)),
        "/Subj 保留未去转义的原串"
    );
    assert!(annotation.has_subject_language_escape());
    assert_eq!(
        annotation
            .subject_semantic_text()
            .unwrap()
            .expect("escaped /Subj")
            .escape_count(),
        1
    );
    assert_eq!(
        annotation.subject_text().unwrap().as_deref(),
        Some("Rechnung")
    );
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .subject_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("de"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941）"
    );
}

#[test]
fn annotation_subject_without_escape_keeps_prevailing_lang_entry() {
    let annotation = annotation_with_subject(Value::String("Invoice".into()));
    assert!(!annotation.has_subject_language_escape());
    assert_eq!(
        annotation.subject_text().unwrap().as_deref(),
        Some("Invoice"),
        "无转义时自然文本即原串"
    );
    let prevailing = identifier("en-US");
    assert_eq!(
        language_tag(
            annotation
                .subject_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en-US")
    );
    assert_eq!(annotation.subject_language(None).unwrap(), None);
}

#[test]
fn annotation_without_subject_has_no_language_view() {
    let annotation = annotation_without_title();
    assert_eq!(annotation.subject, None);
    assert!(!annotation.has_subject_language_escape());
    assert_eq!(annotation.subject_semantic_text().unwrap(), None);
    assert_eq!(annotation.subject_text().unwrap(), None);
    assert_eq!(annotation.subject_language(None).unwrap(), None);
}

#[test]
fn annotation_subject_unterminated_escape_is_typed_malformed_error() {
    // 残缺转义不影响注解读取与 `/T` 视图，只在索取 `/Subj` 语言视图时报错，
    // 且上下文是 `/Subj`（与 `/T` 的错误串可区分）。
    let subject = format!("broken {}", unterminated_escape("en"));
    let annotation = annotation_with_subject(unicode_text(&subject));
    assert!(annotation.has_subject_language_escape());
    assert_eq!(
        annotation.subject,
        Some(unicode_text(&subject)),
        "原串仍按解析结果保留"
    );
    assert_eq!(annotation.title_text().unwrap(), None, "无 /T 不受影响");

    for result in [
        annotation.subject_semantic_text().map(|_| ()),
        annotation.subject_text().map(|_| ()),
        annotation.subject_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("annotation /Subj"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }
}

/// 单条 `Text` 注解，`/NM` 为给定的文本串值。
fn annotation_with_name(name: Value) -> Annotation {
    let document = doc(vec![catalog_object()]);
    parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Text").insert("NM", name),
    )
    .expect("annotation parses")
}

/// 单条不含 `/NM` 的 `Text` 注解。
fn annotation_without_name() -> Annotation {
    let document = doc(vec![catalog_object()]);
    parse_annotation(&document, 0, None, &base_annotation("Text")).expect("annotation parses")
}

#[test]
fn annotation_name_with_language_escape_returns_plain_text_and_declared_language() {
    // Table 164 L21283–L21284 的 `/NM` 是文本串，与 `/T`、`/Subj` 共用
    // 7.9.2.2 的转义口径：原串保留、语言视图按需解析。
    let name = format!("{}付箋-01", escape("ja"));
    let annotation = annotation_with_name(unicode_text(&name));

    assert_eq!(
        annotation.name,
        Some(unicode_text(&name)),
        "/NM 保留未去转义的原串"
    );
    assert!(annotation.has_name_language_escape());
    assert_eq!(
        annotation
            .name_semantic_text()
            .unwrap()
            .expect("escaped /NM")
            .escape_count(),
        1
    );
    assert_eq!(annotation.name_text().unwrap().as_deref(), Some("付箋-01"));
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .name_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("ja"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941）"
    );
}

#[test]
fn annotation_name_without_escape_keeps_original_string_and_prevailing_lang() {
    let annotation = annotation_with_name(Value::String("annot-42".into()));
    assert!(!annotation.has_name_language_escape());
    assert_eq!(
        annotation.name_text().unwrap().as_deref(),
        Some("annot-42"),
        "无转义时自然文本即原串"
    );
    let text = annotation
        .name_semantic_text()
        .unwrap()
        .expect("plain /NM still yields a view");
    assert_eq!(text.escape_count(), 0);
    assert!(!text.has_language_escape());
    assert_eq!(text.segments().len(), 1);
    assert_eq!(segments_language_tag(&text, 0), None);
    assert_eq!(
        text.plain_text(),
        "annot-42",
        "去转义文本与 /NM 原串逐字相同"
    );

    let prevailing = identifier("en-US");
    assert_eq!(
        language_tag(
            annotation
                .name_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en-US"),
        "注解自身没有 /Lang，prevailing 由调用方传入目录 /Lang"
    );
    assert_eq!(annotation.name_language(None).unwrap(), None);
}

#[test]
fn annotation_without_name_has_no_language_view() {
    let annotation = annotation_without_name();
    assert_eq!(annotation.name, None);
    assert!(!annotation.has_name_language_escape());
    assert_eq!(
        annotation.name_semantic_text().unwrap(),
        None,
        "缺 /NM 返回 Ok(None) 而非空视图"
    );
    assert_eq!(annotation.name_text().unwrap(), None);
    assert_eq!(annotation.name_language(None).unwrap(), None);
}

#[test]
fn annotation_name_segments_split_at_each_escape() {
    let name = format!("{}note {}NOTiz", escape("en"), escape("de"));
    let annotation = annotation_with_name(unicode_text(&name));
    let text = annotation
        .name_semantic_text()
        .unwrap()
        .expect("both escapes terminate");
    assert_eq!(text.escape_count(), 2);
    assert!(text.has_language_escape());
    let segments = text.segments();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].text(), "note ");
    // 串首即转义，故第一段已带 `en`；只有转义之前的文本才落到 `None`。
    assert_eq!(segments_language_tag(&text, 0), Some("en"));
    assert_eq!(segments[1].text(), "NOTiz");
    assert_eq!(segments_language_tag(&text, 1), Some("de"));
    assert_eq!(text.plain_text(), "note NOTiz");
    assert_eq!(
        language_tag(text.effective_language(Some(&identifier("fr"))).as_ref(),),
        Some("en"),
        "整串语言取首个转义"
    );
}

#[test]
fn annotation_name_with_iso_3166_country_code_escape_resolves_regional_tag() {
    // 7.9.2.2 c)（raw L4246）的可选 2 字节 ISO 3166 码拼为 `de-AT`。
    let name = format!("{}Anmerkung", escape("deAT"));
    let annotation = annotation_with_name(unicode_text(&name));
    assert_eq!(
        annotation.name_text().unwrap().as_deref(),
        Some("Anmerkung")
    );
    assert_eq!(
        language_tag(annotation.name_language(None).unwrap().as_ref()),
        Some("de-AT")
    );
}

#[test]
fn annotation_name_unterminated_escape_is_typed_malformed_error() {
    // 残缺转义不毁掉注解本身的读取，只在索取 `/NM` 语言视图时报错，
    // 且上下文是 `/NM`（与 `/T`、`/Subj` 的错误串可区分）。
    let name = format!("broken {}", unterminated_escape("en"));
    let annotation = annotation_with_name(unicode_text(&name));
    assert!(
        annotation.has_name_language_escape(),
        "残缺转义仍含 U+001B，存在性判断不校验合法性"
    );
    assert_eq!(
        annotation.name,
        Some(unicode_text(&name)),
        "原串仍按解析结果保留"
    );
    assert_eq!(annotation.title_text().unwrap(), None, "无 /T 不受影响");

    for result in [
        annotation.name_semantic_text().map(|_| ()),
        annotation.name_text().map(|_| ()),
        annotation.name_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("annotation /NM"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }
}

#[test]
fn annotation_name_non_ascii_letter_escape_code_is_typed_malformed_error() {
    // 非法转义序列一律返回类型化错误而非 panic（库策略：报错不静默当文本）。
    let name = format!("{}x", escape("e1"));
    let error = annotation_with_name(unicode_text(&name))
        .name_semantic_text()
        .expect_err("digits are no ISO 639 code");
    assert!(matches!(error, LanguageError::Malformed(_)));
    assert!(error.to_string().contains("annotation /NM"), "{error}");
    assert!(error.to_string().contains("ASCII letters"), "{error}");

    let name = format!("{}x", escape("eng"));
    let error = annotation_with_name(unicode_text(&name))
        .name_text()
        .expect_err("three-byte codes are not a valid escape");
    assert!(error.to_string().contains("2-byte ISO 639"), "{error}");
}

#[test]
fn annotation_name_literal_pdfdocencoding_escape_byte_is_not_a_language_escape() {
    // 字面串里的 0x1B 按附录 D.3 的 PDFDocEncoding 表（raw L34054）解为
    // U+02D9，不是 U+001B，故不构成转义：prevailing Lang 不被覆盖。
    let raw = format!("{}draft", escape("ja"));
    let annotation = annotation_with_name(Value::String(raw));
    assert!(
        !annotation.has_name_language_escape(),
        "PDFDocEncoding 字面串解不出 U+001B，故无转义"
    );
    assert_eq!(
        annotation.name_text().unwrap().as_deref(),
        Some("\u{02D9}ja\u{02D9}draft"),
        "原样返回解码后的文本"
    );
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .name_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en"),
        "无转义时 prevailing 保持生效"
    );
}

/// 单条 `FreeText` 注解，`/DS` 为给定的名字或文本串值。
fn free_text_with_default_style(style: Value) -> Annotation {
    let document = doc(vec![catalog_object()]);
    parse_annotation(
        &document,
        0,
        None,
        &base_annotation("FreeText")
            .insert("DA", Value::String("/F1 12 Tf 0 g".into()))
            .insert("DS", style),
    )
    .expect("free text annotation parses")
}

#[test]
fn free_text_default_style_text_string_with_escape_returns_plain_text_and_declared_language() {
    // Table 174 L21909 的 `/DS` 是文本串：原串（已解码、未去转义）保留在
    // `default_style`，语言视图按需解析。
    let style = format!("{}font: ArialMT 12pt;", escape("ja"));
    let annotation = free_text_with_default_style(unicode_text(&style));

    assert_eq!(
        annotation.default_style.as_deref(),
        Some(style.as_str()),
        "/DS 保留未去转义的解码原串"
    );
    assert!(annotation.has_default_style_language_escape());
    assert_eq!(
        annotation
            .default_style_semantic_text()
            .unwrap()
            .expect("escaped text-string /DS")
            .escape_count(),
        1
    );
    assert_eq!(
        annotation
            .default_style_text()
            .unwrap()
            .as_deref()
            .expect("/DS 视图"),
        "font: ArialMT 12pt;"
    );
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .default_style_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("ja"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941）"
    );
    // 其余条目不受 `/DS` 残缺与否影响。
    assert_eq!(
        annotation.default_appearance.as_deref(),
        Some("/F1 12 Tf 0 g")
    );
}

#[test]
fn free_text_default_style_without_escape_keeps_original_string_and_prevailing_lang() {
    let annotation = free_text_with_default_style(Value::String("font: 18pt Arial;".into()));
    assert!(!annotation.has_default_style_language_escape());
    let text = annotation
        .default_style_semantic_text()
        .unwrap()
        .expect("plain /DS still yields a view");
    assert_eq!(text.escape_count(), 0);
    assert_eq!(text.segments().len(), 1);
    assert_eq!(segments_language_tag(&text, 0), None);
    assert_eq!(
        annotation
            .default_style_text()
            .unwrap()
            .as_deref()
            .expect("/DS 视图"),
        "font: 18pt Arial;",
        "无转义时自然文本即原串"
    );
    assert_eq!(
        annotation
            .default_style_language(Some(&identifier("en-US")))
            .unwrap(),
        Some(identifier("en-US"))
    );
    assert_eq!(annotation.default_style_language(None).unwrap(), None);
}

#[test]
fn free_text_without_default_style_has_no_language_view() {
    let document = doc(vec![catalog_object()]);
    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("FreeText").insert("DA", Value::String("/F1 12 Tf 0 g".into())),
    )
    .expect("free text without /DS parses");
    assert_eq!(annotation.default_style, None);
    assert!(!annotation.has_default_style_language_escape());
    assert_eq!(
        annotation.default_style_semantic_text().unwrap(),
        None,
        "缺 /DS 返回 Ok(None)"
    );
    assert_eq!(annotation.default_style_text().unwrap(), None);
    assert_eq!(
        annotation
            .default_style_language(Some(&identifier("en")))
            .unwrap(),
        None,
        "没有 /DS 就无所谓语言"
    );
}

#[test]
fn free_text_default_style_as_name_has_no_language_escape_and_no_error() {
    // 解析器按库策略用 parse_name_or_text 宽容接受名字形态的 `/DS`；
    // 7.9.2.2 只把转义定义在文本串里，故名字形态一律判为无转义、视图为
    // Ok(None)，而不是报错。
    let annotation = free_text_with_default_style(Value::Name("ArialMT".into()));
    assert_eq!(annotation.default_style.as_deref(), Some("ArialMT"));
    assert!(
        !annotation.has_default_style_language_escape(),
        "名字形态不携带转义"
    );
    assert_eq!(
        annotation.default_style_semantic_text().unwrap(),
        None,
        "名字形态没有语言视图，也不报错"
    );
    assert_eq!(
        annotation.default_style_text().unwrap().as_deref(),
        Some("ArialMT"),
        "名字形态的自然文本即名字本身"
    );
    assert_eq!(
        annotation
            .default_style_language(Some(&identifier("en")))
            .unwrap(),
        Some(identifier("en")),
        "名字永不覆盖 prevailing Lang"
    );
    assert_eq!(annotation.default_style_language(None).unwrap(), None);

    // 判别走的是形态而非字符：名字里真有 U+001B 也不按转义处理、不报错。
    let escaped_name = format!("Arial{}", escape("ja"));
    let annotation = free_text_with_default_style(Value::Name(escaped_name.clone()));
    assert!(!annotation.has_default_style_language_escape());
    assert_eq!(annotation.default_style_semantic_text().unwrap(), None);
    assert_eq!(
        annotation.default_style_text().unwrap().as_deref(),
        Some(escaped_name.as_str()),
        "原样返回名字，不剥离任何字符"
    );
    assert_eq!(
        annotation
            .default_style_language(Some(&identifier("de")))
            .unwrap(),
        Some(identifier("de")),
        "名字形态下 prevailing 保持生效"
    );
}

#[test]
fn free_text_default_style_unterminated_escape_is_typed_malformed_error() {
    // 文本串形态的残缺转义不影响注解读取，只在索取 `/DS` 语言视图时返回
    // 类型化 Malformed（不 panic），错误串带 `annotation /DS` 上下文。
    let style = format!("broken {}", unterminated_escape("en"));
    let annotation = free_text_with_default_style(unicode_text(&style));
    assert!(annotation.has_default_style_language_escape());
    assert_eq!(
        annotation.default_style.as_deref(),
        Some(style.as_str()),
        "原串仍按解析结果保留"
    );
    assert_eq!(
        annotation.name_semantic_text().unwrap(),
        None,
        "无 /NM 不受影响"
    );

    for result in [
        annotation.default_style_semantic_text().map(|_| ()),
        annotation.default_style_text().map(|_| ()),
        annotation.default_style_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("annotation /DS"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }

    // 码长非法同样只报错、不 panic。
    let error = free_text_with_default_style(unicode_text(&format!("{}x", escape("eng"))))
        .default_style_text()
        .expect_err("three-byte codes are not a valid escape");
    assert!(error.to_string().contains("annotation /DS"), "{error}");
    assert!(error.to_string().contains("2-byte ISO 639"), "{error}");
}
