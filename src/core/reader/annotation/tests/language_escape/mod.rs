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

mod contents;
mod default_style;
mod modified_text;
mod name;
mod subject;
mod title;

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

/// 单条 `Text` 注解，`/Contents` 为给定的文本串值（Table 164 的通用条目，
/// 解析器已建模为 `Annotation::contents`，原串不去转义）。
fn annotation_with_contents(contents: Value) -> Annotation {
    let document = doc(vec![catalog_object()]);
    parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Text").insert("Contents", contents),
    )
    .expect("annotation parses")
}

/// 单条 `Text` 注解，未建模条目 `/TM` 为给定的字典值。
///
/// `parse_annotation` 不读 `TM` 键（ISO 32000-1 无此条目），值原样留在
/// `Annotation::dict`，供 `language_escape_modified_text.rs` 的视图回查。
fn annotation_with_modified_text(modified_text: Value) -> Annotation {
    let document = doc(vec![catalog_object()]);
    parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Text").insert("TM", modified_text),
    )
    .expect("annotation parses")
}

/// 既没有 `/Contents` 也没有 `/TM` 的单条 `Text` 注解。
fn annotation_without_contents_or_modified_text() -> Annotation {
    let document = doc(vec![catalog_object()]);
    parse_annotation(&document, 0, None, &base_annotation("Text")).expect("annotation parses")
}
