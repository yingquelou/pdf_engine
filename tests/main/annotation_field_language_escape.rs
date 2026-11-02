//! 注解 `/T`（12.5.6.1 Table 170）与表单字段 `/TU`（12.7.3 Table 220、
//! 14.9.3）的 7.9.2.2 Unicode 语言转义写出 → 重新解析往返。
//!
//! 规范依据（`pdftotext -raw docs/PDF32000_2008.pdf` 的行号）：转义语法
//! a)–d) 在 L4242–L4248；14.9.2.1 L31940–L31941 把「串内转义覆盖 prevailing
//! Lang」推广到一切 Unicode 文本串；14.9.3 L32111–L32120 另对字段替代名
//! `/TU` 给出 shall（替代名用于界面标识、串内转义覆盖 prevailing Lang）；
//! 14.9.2.3 L31955–L31956 说明 prevailing 即目录 `/Lang`（注解与字段字典
//! 都没有 `/Lang` 条目）。12.7.3.2 L23696–L23697 只要求各表示的 `/FT` `/V`
//! `/DV` 一致，故 `/TU` 按节点各自暴露。
//!
//! U+001B 在 PDFDocEncoding 无表示，故 `PdfTextString` 整体退到 UTF-16BE +
//! BOM（7.9.2.2 a) 的「the byte sequence 0 followed by 27」）。

use super::*;

use pdf_engine::core::builder::{
    AcroForm, AnnotationKind, FormField, LanguageIdentifier, PageSize, PdfAnnotation, PdfBuilder,
};
use pdf_engine::core::reader::accessibility::SemanticText;
use pdf_engine::core::reader::annotation::AnnotationSubtype;

/// 7.9.2.2 a)–d) 的完整语言转义序列（U+001B + 码 + U+001B）。
fn escape(code: &str) -> String {
    format!("\u{001B}{code}\u{001B}")
}

/// 只有开界符、没有闭界符的残缺转义。
fn unterminated_escape(code: &str) -> String {
    format!("\u{001B}{code}")
}

/// 与 `PdfTextString` 编码一致的十六进制文本串（大写十六进制字符）。
fn hex_of(text: &str) -> String {
    pdf_engine::core::types::PdfTextString::from(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

/// 已校验的语言标识符。
fn tag(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

/// 写出带 7.9.2.2 语言转义的注解 `/T` → 重新解析：`Annotation::title` 保留
/// 带转义的原串、`title_text()` 给去转义的标题栏文本（Table 170 raw
/// L21666–L21668）、`title_language()` 按 14.9.2.1（raw L31940–L31941）覆盖
/// 目录 `/Lang`；同页无转义的兄弟注解仍沿用 `/Lang`。
#[test]
fn builder_annotation_title_language_escape_roundtrips_through_parser() {
    let escaped_title = format!("{}付箋", escape("ja"));
    let plain_title = "Reviewer".to_owned();

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.set_document_language(tag("en"));
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_title(escaped_title.clone())
            .with_contents("Memo"),
    );
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 660.0, 92.0, 680.0])
            .with_title(plain_title.clone()),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let prevailing = doc.document_language().expect("/Lang readable");
    assert_eq!(
        prevailing.as_ref().map(LanguageIdentifier::as_str),
        Some("en")
    );

    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let escaped = annotations
        .iter()
        .find(|annotation| annotation.has_title_language_escape())
        .expect("escaped /T survives the round-trip");
    assert_eq!(escaped.subtype, AnnotationSubtype::Text);
    assert_eq!(
        escaped.title,
        Some(Value::HexString(hex_of(&escaped_title))),
        "/T 保留不去转义的原串"
    );
    assert_eq!(
        escaped
            .title_text()
            .expect("valid escape")
            .as_deref()
            .expect("escaped /T"),
        "付箋"
    );
    assert_eq!(
        escaped
            .title_language(prevailing.as_ref())
            .expect("language readable")
            .expect("escape declares a language")
            .as_str(),
        "ja",
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1）"
    );

    let plain = annotations
        .iter()
        .find(|annotation| !annotation.has_title_language_escape())
        .expect("plain /T");
    assert_eq!(
        plain
            .title_text()
            .expect("no escape")
            .as_deref()
            .expect("plain /T"),
        "Reviewer"
    );
    assert_eq!(
        plain.title_language(prevailing.as_ref()).expect("ok"),
        prevailing,
        "无转义时沿用 prevailing Lang"
    );

    // 写出的 `/T` 确实是十六进制文本串：字典级路径 `SemanticText::from_value`
    // 与注解访问器给出同一个结论。
    let title_value = (3..=64)
        .filter_map(|num| doc.get_object(num).ok().flatten())
        .filter_map(|object| object.value.as_dict().cloned())
        .filter(|dict| dict.get("Subtype").and_then(Value::as_name) == Some("Text"))
        .find_map(|dict| match dict.get("T") {
            Some(value @ Value::HexString(_)) => Some(value.clone()),
            _ => None,
        })
        .expect("written annotation carries a hex /T");
    let text = SemanticText::from_value(&title_value).expect("written title parses");
    assert_eq!(text.plain_text(), "付箋");
    assert_eq!(
        text.escape_language().map(LanguageIdentifier::as_str),
        Some("ja")
    );
}

/// 写入 → 重新解析：一条注解 `/T` 里两次换语言（含 ISO 3166 国家码），逐段
/// 语言与 `escape_count` 在写出/重解析后保持一致；同一文档里的字段 `/TU`
/// 用四字节码（ISO 639 + ISO 3166）走同一套解析。
#[test]
fn builder_annotation_title_and_field_alternate_name_multilingual_escapes_roundtrip() {
    let annotation_title = format!("{}Overview {}Bemerkung", escape("en"), escape("deAT"));
    let alternate_name = format!("{}Nom", escape("frCA"));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.set_document_language(tag("en"));
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_title(annotation_title.clone()),
    );
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::text("name", [72.0, 660.0, 200.0, 680.0])
            .with_alternate_name(alternate_name.clone()),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let prevailing = doc.document_language().expect("/Lang readable");

    let annotations = doc.page_annotations(0).expect("annotations");
    let annotation = &annotations[0];
    assert_eq!(
        annotation.title,
        Some(Value::HexString(hex_of(&annotation_title)))
    );
    let text = annotation
        .title_semantic_text()
        .expect("two escapes")
        .expect("escaped /T");
    assert_eq!(text.escape_count(), 2);
    assert_eq!(text.plain_text(), "Overview Bemerkung");
    let segments = text.segments();
    assert_eq!(segments.len(), 2);
    assert_eq!(
        segments[0].language().map(LanguageIdentifier::as_str),
        Some("en")
    );
    assert_eq!(segments[0].text(), "Overview ");
    assert_eq!(
        segments[1].language().map(LanguageIdentifier::as_str),
        Some("de-AT"),
        "ISO 3166 国家码拼为区域标签（7.9.2.2 c）"
    );
    assert_eq!(
        annotation
            .title_language(prevailing.as_ref())
            .expect("ok")
            .expect("first escape wins")
            .as_str(),
        "en"
    );

    let parsed = doc.acro_form().expect("form").expect("form present");
    let field = &parsed.fields[0];
    assert_eq!(
        field.alternate_name.as_deref(),
        Some(alternate_name.as_str())
    );
    assert_eq!(
        field
            .alternate_name_text()
            .expect("valid escape")
            .as_deref()
            .expect("escaped /TU"),
        "Nom"
    );
    assert_eq!(
        field
            .alternate_name_language(prevailing.as_ref())
            .expect("ok")
            .expect("escape declares a language")
            .as_str(),
        "fr-CA",
        "14.9.3 NOTE 3（raw L32117–L32120）的 shall 在 `/TU` 上生效"
    );
    // 归并视图暴露同一节点的那一条 `/TU`。
    let group = parsed
        .logical_field("name")
        .expect("logical field for the text field");
    assert_eq!(
        group
            .alternate_name_language(prevailing.as_ref())
            .expect("ok")
            .expect("escaped /TU")
            .as_str(),
        "fr-CA"
    );
}

/// 写入 → 重新解析：注解 `/T` 里未闭合的残缺转义不破坏注解读取——原串仍
/// 往返、子类型与 `/Contents` 仍可读；只有索取语言视图时才返回既有
/// `LanguageError::Malformed`（库策略：报错而非静默把残缺转义当文本）。
#[test]
fn builder_annotation_unterminated_title_escape_roundtrip_reports_malformed_on_request() {
    let broken_title = format!("{}Note", unterminated_escape("en"));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_title(broken_title.clone())
            .with_contents("Still readable"),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let annotations = doc.page_annotations(0).expect("annotations stay readable");
    let annotation = &annotations[0];
    assert_eq!(annotation.subtype, AnnotationSubtype::Text);
    assert_eq!(
        annotation.title,
        Some(Value::HexString(hex_of(&broken_title))),
        "残缺转义不改变原串的往返"
    );
    assert!(annotation.has_title_language_escape());

    for result in [
        annotation.title_semantic_text().map(|_| ()),
        annotation.title_text().map(|_| ()),
        annotation.title_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(error.to_string().contains("annotation /T"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }

    // 同一份原串经 `SemanticText::parse` 也判为残缺，读端口径一致。
    let error = SemanticText::parse(&broken_title).expect_err("same policy");
    assert!(
        error.to_string().contains("is not terminated by U+001B"),
        "{error}"
    );
}

/// 写入 → 重新解析：字段替代名的转义在文档没有 `/Lang` 时也只由转义决定，
/// 无转义的替代名返回「无语言」（不臆造 prevailing）；`/TU` 挂在无名 Widget
/// 表示上时（12.7.3.2）按节点暴露，归并视图不回落到兄弟表示。
#[test]
fn builder_field_alternate_name_escape_roundtrip_without_document_language() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::radio_group("agree")
            .with_child(
                FormField::widget([72.0, 660.0, 92.0, 680.0])
                    .with_alternate_name(format!("{}Zustimmung", escape("de"))),
            )
            .with_child(FormField::widget([100.0, 660.0, 120.0, 680.0])),
    );
    form.add_field(
        page,
        FormField::text("plain", [72.0, 620.0, 200.0, 640.0]).with_alternate_name("Plain name"),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    assert_eq!(
        doc.document_language().expect("/Lang read"),
        None,
        "本夹具不声明目录 /Lang"
    );

    let parsed = doc.acro_form().expect("form").expect("form present");
    let group = &parsed.fields[0];
    assert_eq!(group.alternate_name, None, "父节点未声明 /TU");
    let widget = &group.kids[0];
    assert!(widget.has_alternate_name_language_escape());
    assert_eq!(
        widget
            .alternate_name_text()
            .expect("valid escape")
            .as_deref()
            .expect("escaped /TU"),
        "Zustimmung"
    );
    assert_eq!(
        widget
            .alternate_name_language(None)
            .expect("ok")
            .expect("escape declares a language")
            .as_str(),
        "de"
    );
    assert_eq!(group.kids[1].alternate_name_text().expect("ok"), None);

    // 归并视图只暴露前序首个表示自身的 `/TU`（12.7.3.2 未要求各表示一致）。
    let logical = parsed
        .logical_field("agree")
        .expect("logical field for the radio group");
    assert_eq!(logical.alternate_name, None);
    assert_eq!(
        logical
            .alternate_name_semantic_text()
            .expect("no /TU on the first representation"),
        None,
        "首个表示没有 /TU 时不回落兄弟表示"
    );
    assert_eq!(
        logical.representations[1]
            .alternate_name_language(None)
            .expect("ok")
            .expect("widget representation carries its own /TU")
            .as_str(),
        "de"
    );

    let plain = &parsed.fields[1];
    assert_eq!(
        plain.alternate_name_text().expect("no escape").as_deref(),
        Some("Plain name")
    );
    assert_eq!(
        plain.alternate_name_language(None).expect("ok"),
        None,
        "无转义且文档无 /Lang 时不臆造语言"
    );
    let explicit = tag("und");
    assert_eq!(
        plain
            .alternate_name_language(Some(&explicit))
            .expect("ok")
            .expect("explicit prevailing is kept")
            .as_str(),
        "und",
        "无转义时原样返回传入的 prevailing"
    );
}
