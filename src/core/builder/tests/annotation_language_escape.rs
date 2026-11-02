//! 注解 `/NM` 与 FreeText `/DS` 的写端 → 重新解析 → 语言转义视图往返
//! （ISO 32000-1 12.5.2 Table 164、12.5.6.3 Table 174、7.9.2.2 与 14.9.2.1）。
//!
//! 写端把 `/NM`、`/DS` 一律按文本串（`text_string_value`）写出，含 U+001B 的
//! 串整体退到 UTF-16BE + BOM，正是 7.9.2.2 a)「the byte sequence 0 followed
//! by 27」的落点；读端因此能在新解析的文档上复原转义。名字形态的 `/DS`
//! 只在读第三方文档时出现（见 `reader/annotation/tests/language_escape.rs`），
//! 本库写端不产出，故此处只钉文本串形态。
//!
//! 目录 `/Lang` 由 `set_document_language` 写出，作为 14.9.2.1 的 prevailing
//! Lang entry：断言串内转义**覆盖**它。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::support::annot_object;
use super::*;
use crate::core::builder::types::text_string_value;
use crate::core::reader::accessibility::LANGUAGE_ESCAPE;
use crate::core::types::{Document, PdfVersion};

/// 7.9.2.2 a)–d) 的一个完整语言转义序列（U+001B + 码 + U+001B）。
fn escape(code: &str) -> String {
    format!("{LANGUAGE_ESCAPE}{code}{LANGUAGE_ESCAPE}")
}

fn language(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

/// 写出并重新解析（经 target 目录临时文件，避开沙箱对 %TEMP% 的拦截）。
fn build_and_parse(b: &mut PdfBuilder) -> (Vec<u8>, Document) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-annotation-language-escape-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    (out, doc)
}

/// 单页 + 单条注解 + 目录 `/Lang (en)` 的文档，写出后重新解析。
fn annotated(annot: PdfAnnotation, version: PdfVersion) -> Document {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.set_document_language(language("en"));
    b.add_annotation(p0, annot);
    build_and_parse(&mut b).1
}

#[test]
fn builder_annotation_name_escape_roundtrips_through_reparsed_document() {
    // `/NM`（Table 164）写出 → 重新解析 → 原串保留、去转义文本与生效语言
    // 由访问器给出，且转义覆盖目录 `/Lang`（14.9.2.1 raw L31940–L31941）。
    let name = format!("{}付箋-01", escape("ja"));
    let annot = PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
        .with_contents("A note")
        .with_name(name.clone());
    let doc = annotated(annot, PdfVersion::V1_6);

    let written = annot_object(&doc, 3, 0);
    assert_eq!(
        written.get("NM"),
        Some(&text_string_value(&name)),
        "/NM 按 UTF-16BE 文本串写出，转义字节原样在场"
    );

    let annotation = &doc.page_annotations(0).expect("annotations")[0];
    assert_eq!(annotation.name, Some(text_string_value(&name)));
    assert!(annotation.has_name_language_escape());
    assert_eq!(
        annotation
            .name_semantic_text()
            .expect("terminated escape")
            .expect("/NM present")
            .escape_count(),
        1
    );
    assert_eq!(
        annotation.name_text().expect("view").as_deref(),
        Some("付箋-01")
    );
    let prevailing = doc.document_language().expect("catalog /Lang (en)");
    assert_eq!(prevailing, Some(language("en")), "/Lang 随文档往返");
    assert_eq!(
        annotation
            .name_language(prevailing.as_ref())
            .expect("language view"),
        Some(language("ja")),
        "串内转义 shall 覆盖 prevailing Lang"
    );
    // 无关条目不受影响：本条注解没有 /Subj。
    assert_eq!(
        annotation.subject_semantic_text().expect("/Subj absent"),
        None
    );
}

#[test]
fn builder_free_text_default_style_escape_roundtrips_through_reparsed_document() {
    // FreeText `/DS`（Table 174）走同一条访问器形态：写端只产出文本串形态，
    // 故往返后仍带转义、可按 7.9.2.2 展开。
    let style = format!("{}font: ArialMT 12pt;", escape("de"));
    let annot = PdfAnnotation::new(AnnotationKind::FreeText, [72.0, 700.0, 300.0, 640.0])
        .with_default_appearance("/F1 12 Tf 0 g")
        .with_default_style(style.clone());
    let doc = annotated(annot, PdfVersion::V1_6);

    let written = annot_object(&doc, 3, 0);
    assert_eq!(
        written.get("DS"),
        Some(&text_string_value(&style)),
        "/DS 按文本串写出（本库写端不产出名字形态）"
    );

    let annotation = &doc.page_annotations(0).expect("annotations")[0];
    assert_eq!(
        annotation.default_style.as_deref(),
        Some(style.as_str()),
        "`default_style` 保留未去转义的解码原串"
    );
    assert!(annotation.has_default_style_language_escape());
    assert_eq!(
        annotation
            .default_style_semantic_text()
            .expect("terminated escape")
            .expect("/DS present")
            .escape_count(),
        1
    );
    assert_eq!(
        annotation
            .default_style_text()
            .expect("view")
            .as_deref()
            .expect("/DS present"),
        "font: ArialMT 12pt;"
    );
    let prevailing = doc.document_language().expect("catalog /Lang (en)");
    assert_eq!(
        annotation
            .default_style_language(prevailing.as_ref())
            .expect("language view"),
        Some(language("de")),
        "串内转义 shall 覆盖 prevailing Lang"
    );
    assert_eq!(
        annotation.default_appearance.as_deref(),
        Some("/F1 12 Tf 0 g"),
        "/DA 与 /DS 互不干扰"
    );
}

/// `/Contents`（Table 164 raw L21272–L21277）与未建模的 `/TM` 走同一条访问器
/// 形态：`/Contents` 由写端 `with_contents` 落成文本串，重新解析后按 7.9.2.2
/// 展开逐段语言；14.9.2.1（raw L31940–L31941）的 shall 在串内转义覆盖目录
/// `/Lang`。本库写端不产出 `/TM`，故同一条注解的 `/TM` 视图恒为 `Ok(None)`。
#[test]
fn builder_annotation_contents_multilingual_escape_roundtrips_through_reparsed_document() {
    let contents = format!(
        "{}First note {}Zweiter Absatz.",
        escape("en"),
        escape("deCH")
    );
    let annot = PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
        .with_contents(contents.clone())
        .with_title("Reviewer");
    let doc = annotated(annot, PdfVersion::V1_6);

    let written = annot_object(&doc, 3, 0);
    assert_eq!(
        written.get("Contents"),
        Some(&text_string_value(&contents)),
        "/Contents 按 UTF-16BE 文本串写出，转义字节原样在场"
    );

    let annotation = &doc.page_annotations(0).expect("annotations")[0];
    assert_eq!(
        annotation.contents,
        Some(text_string_value(&contents)),
        "`contents` 保留未去转义的解码原串"
    );
    assert!(annotation.has_contents_language_escape());
    let view = annotation
        .contents_semantic_text()
        .expect("two escapes")
        .expect("/Contents present");
    assert_eq!(view.escape_count(), 2);
    assert_eq!(view.plain_text(), "First note Zweiter Absatz.");
    assert_eq!(view.segments().len(), 2);
    assert_eq!(
        view.segments()[0]
            .language()
            .map(LanguageIdentifier::as_str),
        Some("en")
    );
    assert_eq!(view.segments()[0].text(), "First note ");
    assert_eq!(
        view.segments()[1]
            .language()
            .map(LanguageIdentifier::as_str),
        Some("de-CH"),
        "ISO 3166 国家码拼为区域标签（7.9.2.2 c）"
    );

    let prevailing = doc.document_language().expect("catalog /Lang (en)");
    assert_eq!(
        annotation
            .contents_language(prevailing.as_ref())
            .expect("language view"),
        Some(language("en")),
        "串内转义 shall 覆盖 prevailing Lang"
    );

    // 同一条注解的其余视图互不干扰。
    assert_eq!(
        annotation.title_text().expect("/T readable").as_deref(),
        Some("Reviewer"),
        "无转义的 /T 仍是原串"
    );
    assert_eq!(
        annotation
            .title_language(prevailing.as_ref())
            .expect("/T language"),
        prevailing,
        "无转义的 /T 沿用目录 /Lang"
    );
    assert!(!annotation.has_modified_text_language_escape());
    assert_eq!(
        annotation
            .modified_text_semantic_text()
            .expect("no /TM written by this library"),
        None,
        "本库写端不产出未建模的 /TM，故读端无视图"
    );
}
