//! 标记类注解 Table 170 附加条目的读端测试（ISO 32000-1 12.5.6.2）。
//!
//! 覆盖 `/Subj`（L21706–L21707）文本串、`/IRT`（L21698–L21705）间接引用与
//! `/RT`（L21708–L21717）回复类型名的类型化解析：缺失一律 `None`（规范
//! 「Default value: R」不物化，未知名按 `LinkHighlightMode` 的先例原样保留），
//! 畸形输入返回 `AnnotationError::Malformed`。

use crate::core::reader::annotation::*;
use crate::core::types::{Dict, Object, Value};

use super::support::*;

fn annotation_with(dict: Dict) -> Annotation {
    let document = doc(vec![
        catalog_object(),
        Object::new(7, 0, Value::Dict(base_annotation("Text"))),
    ]);
    parse_annotation(&document, 0, None, &dict).expect("annotation parses")
}

fn assert_malformed(dict: Dict, fragment: &str) {
    let document = doc(vec![catalog_object()]);
    let error = parse_annotation(&document, 0, None, &dict).expect_err("malformed markup entry");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains(fragment)),
        "expected message containing {fragment:?}, got {error:?}"
    );
}

#[test]
fn markup_annotation_reads_subject_in_reply_to_and_reply_type() {
    let annotation = annotation_with(
        base_annotation("Text")
            .insert("Subj", Value::String("Invoice total".into()))
            .insert("IRT", Value::Ref(7, 0))
            .insert("RT", Value::Name("Group".into())),
    );

    assert_eq!(
        annotation.subject,
        Some(Value::String("Invoice total".into()))
    );
    assert_eq!(annotation.in_reply_to, Some((7, 0)));
    assert_eq!(annotation.reply_type, Some(ReplyType::Group));
    assert_eq!(
        annotation.reply_type.as_ref().map(ReplyType::name),
        Some("Group")
    );
}

#[test]
fn absent_markup_reply_entries_parse_as_none_not_defaults() {
    // Table 170 的 `/RT` 有「Default value: R」，但缺省不物化：读端只报缺失。
    let annotation = annotation_with(base_annotation("Highlight"));
    assert_eq!(annotation.subject, None, "/Subj 缺失即 None");
    assert_eq!(annotation.in_reply_to, None, "/IRT 缺失即 None");
    assert_eq!(
        annotation.reply_type, None,
        "/RT 缺失不物化为 Reply（缺省由消费方按规范解释）"
    );

    // 只带 `/IRT` 的回复注解同样不得凭空补出 `/RT`。
    let annotation = annotation_with(base_annotation("Text").insert("IRT", Value::Ref(7, 0)));
    assert_eq!(annotation.in_reply_to, Some((7, 0)));
    assert_eq!(annotation.reply_type, None);
}

#[test]
fn reply_type_maps_standard_names_and_keeps_unknown_names() {
    assert_eq!(ReplyType::from_name("R"), ReplyType::Reply);
    assert_eq!(ReplyType::from_name("Group"), ReplyType::Group);
    assert_eq!(
        ReplyType::from_name("Thread"),
        ReplyType::Other("Thread".into())
    );
    assert_eq!(ReplyType::Reply.name(), "R");
    assert_eq!(ReplyType::Group.name(), "Group");
    assert_eq!(ReplyType::Other("Thread".into()).name(), "Thread");

    // 读端对未知名不报错，原样保留（与 `LinkHighlightMode` 同一口径）。
    let annotation = annotation_with(
        base_annotation("Text")
            .insert("IRT", Value::Ref(7, 0))
            .insert("RT", Value::Name("Thread".into())),
    );
    assert_eq!(
        annotation.reply_type,
        Some(ReplyType::Other("Thread".into())),
        "非标准 /RT 名保留原文"
    );
}

#[test]
fn malformed_markup_reply_entries_are_typed_errors() {
    // `/Subj` 是文本串（Table 170 的 text string 列）。
    assert_malformed(
        base_annotation("Text").insert("Subj", Value::Integer(3)),
        "annotation /Subj must be a text string",
    );
    // `/IRT` 是注解的间接引用；直接字典与整数都不是引用形态。
    assert_malformed(
        base_annotation("Text").insert("IRT", Value::Dict(base_annotation("Text"))),
        "annotation /IRT must be an indirect reference",
    );
    assert_malformed(
        base_annotation("Text").insert("IRT", Value::Integer(7)),
        "annotation /IRT must be an indirect reference",
    );
    // `/RT` 是名字。
    assert_malformed(
        base_annotation("Text").insert("RT", Value::String("Group".into())),
        "annotation /RT must be a name",
    );
}
