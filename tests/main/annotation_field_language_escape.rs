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

/// 第三方文档夹具：把对象体按 `n 0 obj … endobj` 顺序写出，补一张标准 xref
/// 与 trailer（形态同 `xref_streams.rs` 的裸字节夹具）。本库写端不产出注解
/// `/TM`（ISO 32000-1 无此条目）与字段 `/RV`（未建模条目），故这两条视图只能
/// 经真实解析器读第三方文档来验证。
fn third_party_pdf(objects: &[String]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for body in objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(body.as_bytes());
        pdf.extend_from_slice(b"\n");
    }
    let size = objects.len() + 1;
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
            .as_bytes(),
    );
    pdf
}

/// 带目录 `/Lang (en)`、一条 `Text` 注解与两个字段的第三方文档。
fn third_party_document(contents: &str, modified_text: &str, rich_value: &str) -> Document {
    let objects = vec![
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Lang (en) \
         /AcroForm << /Fields [5 0 R 6 0 R] /DA (/F1 12 Tf 0 g) >> >>\nendobj"
            .to_owned(),
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj".to_owned(),
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Annots [4 0 R] >>\nendobj"
            .to_owned(),
        format!(
            "4 0 obj\n<< /Type /Annot /Subtype /Text /Rect [72 700 92 720] \
             /Contents <{}> /TM <{}> >>\nendobj",
            hex_of(contents),
            hex_of(modified_text)
        ),
        format!(
            "5 0 obj\n<< /FT /Tx /T (rich) /RV <{}> >>\nendobj",
            hex_of(rich_value)
        ),
        // 第二个字段用名字形态的 `/RV`：不是 7.9.2.2 的文本串，恒判无转义。
        "6 0 obj\n<< /FT /Tx /T (named) /RV /RichTextName >>\nendobj".to_owned(),
    ];
    parse_buf(&third_party_pdf(&objects))
}

/// 写入 → 重新解析（第三方裸字节）：未建模的注解 `/TM` 与已建模的 `/Contents`
/// 在同一条注解上各自给出语言转义视图，目录 `/Lang (en)` 被串内转义覆盖
/// （14.9.2.1 raw L31940–L31941）；`/TM` 的原值仍留在 `Annotation::dict`，
/// 视图不改动它。
#[test]
fn third_party_annotation_modified_text_and_contents_escapes_read_through_language_views() {
    let contents = format!(
        "{}First note {}Zweiter Absatz.",
        escape("en"),
        escape("deCH")
    );
    let modified_text = format!("{}修正済みテキスト", escape("ja"));
    let doc = third_party_document(&contents, &modified_text, "<body>unused</body>");

    let prevailing = doc.document_language().expect("catalog /Lang (en)");
    assert_eq!(
        prevailing.as_ref().map(LanguageIdentifier::as_str),
        Some("en")
    );

    let annotation = &doc.page_annotations(0).expect("annotations")[0];
    assert_eq!(annotation.subtype, AnnotationSubtype::Text);

    // 视图是纯读端：`/TM` 未被建模，原值仍按字典形态在场。
    assert_eq!(
        annotation.dict.get("TM"),
        Some(&Value::HexString(hex_of(&modified_text))),
        "解析器不读 TM 键，原值留在 Annotation::dict"
    );
    assert!(annotation.has_modified_text_language_escape());
    let view = annotation
        .modified_text_semantic_text()
        .expect("terminated escape")
        .expect("/TM present");
    assert_eq!(view.escape_count(), 1);
    assert_eq!(
        annotation
            .modified_text_text()
            .expect("/TM view")
            .as_deref()
            .expect("/TM present"),
        "修正済みテキスト"
    );
    assert_eq!(
        annotation
            .modified_text_language(prevailing.as_ref())
            .expect("/TM language"),
        Some(tag("ja")),
        "串内转义 shall 覆盖 prevailing Lang"
    );

    // 同一条注解的 `/Contents` 逐段视图独立可读。
    let contents_view = annotation
        .contents_semantic_text()
        .expect("two escapes")
        .expect("/Contents present");
    assert_eq!(contents_view.escape_count(), 2);
    assert_eq!(contents_view.segments().len(), 2);
    assert_eq!(
        contents_view.segments()[1]
            .language()
            .map(LanguageIdentifier::as_str),
        Some("de-CH"),
        "ISO 3166 国家码拼为区域标签（7.9.2.2 c）"
    );
    assert_eq!(
        annotation
            .contents_language(prevailing.as_ref())
            .expect("/Contents language"),
        Some(tag("en")),
        "首个转义决定整串生效语言"
    );
}

/// 写入 → 重新解析（第三方裸字节）：字段 `/RV`（Table 222 raw L23767–L23771）
/// 的两种值形态各自按只读视图口径落地——文本串形态展开转义并覆盖 `/Lang`，
/// 名字形态恒判「无转义」、`Ok(None)` 且不报错（同注解 `/DS` 的先例）。
#[test]
fn third_party_field_rich_value_text_string_and_name_forms_read_through_language_views() {
    let rich_value = format!("{}<body>Valeur</body>", escape("fr"));
    let doc = third_party_document("plain note", "plain text", &rich_value);

    let prevailing = doc.document_language().expect("catalog /Lang (en)");
    let form = doc.acro_form().expect("form").expect("form present");

    let text_field = &form.fields[0];
    assert_eq!(
        text_field.dict.get("RV"),
        Some(&Value::HexString(hex_of(&rich_value))),
        "未建模条目原样留在 Field::dict"
    );
    assert!(text_field.has_rich_value_language_escape());
    assert_eq!(
        text_field
            .rich_value_semantic_text()
            .expect("terminated escape")
            .expect("/RV present")
            .escape_count(),
        1
    );
    assert_eq!(
        text_field
            .rich_value_text()
            .expect("/RV view")
            .as_deref()
            .expect("/RV present"),
        "<body>Valeur</body>"
    );
    assert_eq!(
        text_field
            .rich_value_language(prevailing.as_ref())
            .expect("/RV language"),
        Some(tag("fr")),
        "14.9.3 raw L32117–L32120 的 shall 在 /RV 上生效"
    );

    // 名字形态：不是文本串，恒判无转义、无视图、不报错，语言回落 prevailing。
    let named_field = &form.fields[1];
    assert_eq!(
        named_field.dict.get("RV"),
        Some(&Value::Name("RichTextName".into()))
    );
    assert!(!named_field.has_rich_value_language_escape());
    assert_eq!(
        named_field
            .rich_value_semantic_text()
            .expect("name form is not an error"),
        None,
        "名字形态没有语言视图，也不报错"
    );
    assert_eq!(
        named_field.rich_value_text().expect("/RV view").as_deref(),
        Some("RichTextName"),
        "名字形态的自然文本即名字本身"
    );
    assert_eq!(
        named_field
            .rich_value_language(prevailing.as_ref())
            .expect("/RV language"),
        prevailing,
        "名字永不覆盖 prevailing Lang"
    );
}
