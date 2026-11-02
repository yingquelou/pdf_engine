//! 标记类注解 Table 170 附加条目（12.5.6.2）写端测试：`/Subj` `/IRT` `/RT`
//! 的序列化、缺省不物化、子类型归属与依赖校验、`/IRT` 跨注解同页校验，以及
//! PDF 1.5 / 1.6 版本门禁。全部失败须在分配对象号前拒绝且零字节写出；
//! 末尾的集成往返经临时文件重新解析，断言读端模型取回同值。

use super::support::{annot_object, annot_object_with_num};
use super::*;
use crate::core::reader::annotation::ReplyType as ReaderReplyType;
use crate::core::types::{PdfFeature, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

/// 最小标记类注解（`Text`）；`shift` 只是让两条注解的 `/Rect` 不同。
fn markup(shift: f64) -> PdfAnnotation {
    PdfAnnotation::new(
        AnnotationKind::Text,
        [72.0 + shift, 700.0, 200.0 + shift, 720.0],
    )
}

/// 两页 A4；`slots[i].0` 是注解挂载页下标，`slots[i].1` 是注解本身。
fn builder_with(version: PdfVersion, slots: &[(usize, PdfAnnotation)]) -> PdfBuilder {
    let mut b = PdfBuilder::with_version(version);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let pages = [first, second];
    for (page, annot) in slots {
        b.add_annotation(pages[*page], annot.clone());
    }
    b
}

/// 写出（含目标页为第二页的场景）；失败时返回错误，用于零字节契约断言。
fn build_with(
    version: PdfVersion,
    slots: Vec<(usize, PdfAnnotation)>,
) -> Result<Vec<u8>, WriteError> {
    let mut b = builder_with(version, &slots);
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

/// 单页挂载全部注解并装配为 `Document`（不写盘）。
fn assemble_with(version: PdfVersion, annotations: Vec<PdfAnnotation>) -> Document {
    let slots: Vec<(usize, PdfAnnotation)> =
        annotations.into_iter().map(|annot| (0, annot)).collect();
    let mut b = builder_with(version, &slots);
    b.assemble_document().expect("assemble Table 170 entries")
}

#[test]
fn markup_annotation_serializes_subject_in_reply_to_and_reply_type() {
    let doc = assemble_with(
        PdfVersion::V1_6,
        vec![
            markup(0.0),
            markup(10.0)
                .with_subject("Invoice total")
                .with_in_reply_to(0)
                .with_reply_type(ReplyType::Group),
        ],
    );

    // 目标注解的对象号由分配阶段确定，断言按引用值比对而非硬编码。
    let (target_num, target) = annot_object_with_num(&doc, 3, 0);
    assert!(target.get("IRT").is_none(), "主注解不写 /IRT");
    assert!(
        target.get("RT").is_none(),
        "Table 170 的缺省回复类型 R 不物化"
    );
    assert!(target.get("Subj").is_none(), "未声明的 /Subj 不物化");

    let dict = annot_object(&doc, 3, 1);
    // 文本串按 PDFDocEncoding（纯 ASCII）写成大写十六进制串。
    assert_eq!(
        dict.get("Subj"),
        Some(&Value::HexString("496E766F69636520746F74616C".into()))
    );
    assert_eq!(dict.get("IRT"), Some(&Value::Ref(target_num, 0)));
    assert_eq!(dict.get("RT"), Some(&Value::Name("Group".into())));

    let features = doc.features.borrow();
    for feature in [
        PdfFeature::AnnotationSubject,
        PdfFeature::AnnotationInReplyTo,
        PdfFeature::AnnotationReplyType,
    ] {
        assert!(
            features.contains(feature),
            "feature profile must record {feature}"
        );
    }
}

#[test]
fn markup_annotation_writes_only_declared_entries_and_the_r_reply_type() {
    let doc = assemble_with(
        PdfVersion::V1_6,
        vec![
            markup(0.0),
            markup(10.0)
                .with_in_reply_to(0)
                .with_reply_type(ReplyType::Reply),
            markup(20.0).with_subject("Notes"),
        ],
    );

    let (target_num, _) = annot_object_with_num(&doc, 3, 0);
    let reply = annot_object(&doc, 3, 1);
    assert_eq!(reply.get("IRT"), Some(&Value::Ref(target_num, 0)));
    assert_eq!(reply.get("RT"), Some(&Value::Name("R".into())));
    assert!(reply.get("Subj").is_none());

    let subject_only = annot_object(&doc, 3, 2);
    assert!(
        subject_only
            .get("Subj")
            .is_some_and(|value| matches!(value, Value::HexString(_))),
        "/Subj 写成文本串"
    );
    assert!(subject_only.get("IRT").is_none());
    assert!(subject_only.get("RT").is_none());

    let features = doc.features.borrow();
    assert!(features.contains(PdfFeature::AnnotationSubject));
    assert!(features.contains(PdfFeature::AnnotationInReplyTo));
    assert!(features.contains(PdfFeature::AnnotationReplyType));
}

#[test]
fn table_170_entries_rejected_on_non_markup_annotation() {
    // Table 170 标题即「Additional entries specific to markup annotations」，
    // Link/Popup 等非标记类（Table 169 的 Markup 列为假）一律拒绝。
    // `/IRT` 用例注册两条 Link：句柄有效、同页时跨注解前序校验放行，
    // 拒绝点落在逐条归属校验（validate）上。
    for slots in [
        vec![PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0]).with_subject("x")],
        vec![
            PdfAnnotation::new(AnnotationKind::Link, [20.0, 0.0, 30.0, 10.0])
                .with_in_reply_to(1)
                .with_reply_type(ReplyType::Group),
            PdfAnnotation::new(AnnotationKind::Link, [40.0, 0.0, 50.0, 10.0]),
        ],
    ] {
        let mut out = Vec::new();
        let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
        let p0 = b.page(PageSize::A4);
        for annot in slots {
            b.add_annotation(p0, annot);
        }
        let error = b.build(&mut out).expect_err("non-markup host");
        assert!(
            matches!(&error, WriteError::InvalidObject(message)
                if message.contains("/Subj, /IRT and /RT are only permitted on markup annotations")
                    && message.contains("Table 170")),
            "{error}"
        );
        assert_eq!(out.len(), 0, "validation must precede any bytes");
    }
}

#[test]
fn reply_type_without_in_reply_to_is_rejected() {
    // `/IRT` 行（L21698）是「Required if an RT entry is present」。
    for annot in [
        markup(0.0).with_reply_type(ReplyType::Group),
        markup(0.0)
            .with_subject("x")
            .with_reply_type(ReplyType::Reply),
    ] {
        let mut out = Vec::new();
        let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
        let p0 = b.page(PageSize::A4);
        b.add_annotation(p0, annot);
        let error = b.build(&mut out).expect_err("/RT without /IRT");
        assert!(
            matches!(&error, WriteError::InvalidObject(message)
                if message.contains("/RT requires an /IRT entry")),
            "{error}"
        );
        assert_eq!(out.len(), 0, "validation must precede any bytes");
    }
}

#[test]
fn in_reply_to_requires_another_annotation_on_the_same_page() {
    // 自引用。
    let error = build_with(PdfVersion::V1_5, vec![(0, markup(0.0).with_in_reply_to(0))])
        .expect_err("/IRT self reference");
    assert!(
        matches!(&error, WriteError::InvalidObject(message)
            if message.contains("/IRT must not reference the annotation itself")),
        "{error}"
    );

    // 未注册的句柄。
    let error = build_with(
        PdfVersion::V1_5,
        vec![(0, markup(0.0).with_in_reply_to(99))],
    )
    .expect_err("unknown /IRT handle");
    assert!(
        matches!(&error, WriteError::InvalidObject(message)
            if message.contains("/IRT references unknown annotation handle 99")),
        "{error}"
    );

    // 跨页：Table 170 的 shall「Both annotations shall be on the same page」。
    let error = build_with(
        PdfVersion::V1_5,
        vec![(0, markup(0.0)), (1, markup(10.0).with_in_reply_to(0))],
    )
    .expect_err("cross-page /IRT");
    assert!(
        matches!(&error, WriteError::InvalidObject(message)
            if message.contains("/IRT target must be on the same page")),
        "{error}"
    );
}

#[test]
fn group_reply_must_target_a_primary_annotation_without_in_reply_to() {
    // 12.5.6.2 L21732–L21734：分组的从属注解指向「shall not have an IRT entry」
    // 的主注解；被 `/RT /Group` 指向的注解自己又回复第三条时必须拒绝。
    let error = build_with(
        PdfVersion::V1_6,
        vec![
            (0, markup(0.0).with_in_reply_to(2)),
            (
                0,
                markup(10.0)
                    .with_in_reply_to(0)
                    .with_reply_type(ReplyType::Group),
            ),
            (0, markup(20.0)),
        ],
    )
    .expect_err("/RT Group targets a subordinate annotation");
    assert!(
        matches!(&error, WriteError::InvalidObject(message)
            if message.contains("/RT Group must reference a primary annotation")),
        "{error}"
    );
}

#[test]
fn every_table_170_rejection_writes_no_bytes() {
    // 逐条复跑：任一 `/IRT` 校验失败都在写出任何字节之前返回错误。
    for slots in [
        vec![(0, markup(0.0).with_in_reply_to(0))],
        vec![(0, markup(0.0).with_in_reply_to(7))],
        vec![(0, markup(0.0)), (1, markup(10.0).with_in_reply_to(0))],
    ] {
        let mut out = Vec::new();
        let mut b = builder_with(PdfVersion::V1_5, &slots);
        assert!(b.build(&mut out).is_err(), "expected rejection");
        assert_eq!(out.len(), 0, "validation must precede any bytes");
    }
}

#[test]
fn subject_and_in_reply_to_require_pdf_1_5_and_reply_type_requires_pdf_1_6() {
    // `/Subj` `/IRT` 标注 PDF 1.5，`/RT` 标注 PDF 1.6。
    let error = build_with(PdfVersion::V1_4, vec![(0, markup(0.0).with_subject("x"))])
        .expect_err("PDF 1.4 rejects /Subj");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("annotation subject (/Subj)")
                && message.contains("1.5")
                && message.contains("1.4")),
        "{error}"
    );

    let error = build_with(
        PdfVersion::V1_4,
        vec![(0, markup(0.0)), (0, markup(10.0).with_in_reply_to(0))],
    )
    .expect_err("PDF 1.4 rejects /IRT");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("annotation in reply to (/IRT)")),
        "{error}"
    );

    let error = build_with(
        PdfVersion::V1_5,
        vec![
            (0, markup(0.0)),
            (
                0,
                markup(10.0)
                    .with_in_reply_to(0)
                    .with_reply_type(ReplyType::Group),
            ),
        ],
    )
    .expect_err("PDF 1.5 rejects /RT");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("annotation reply type (/RT)") && message.contains("1.6")),
        "{error}"
    );

    // 门禁通过时同一条目在目标版本上可写出。
    assert!(
        build_with(
            PdfVersion::V1_6,
            vec![
                (0, markup(0.0)),
                (
                    0,
                    markup(10.0)
                        .with_in_reply_to(0)
                        .with_reply_type(ReplyType::Group)
                ),
            ],
        )
        .is_ok()
    );
}

/// 写出并重新解析（经 target 目录临时文件，与 `annotation_text_markup` 同法），
/// 返回读端 `Document` 供类型化断言。
fn build_file_and_parse(version: PdfVersion, slots: &[(usize, PdfAnnotation)]) -> Document {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let out = build_with(version, slots.to_vec()).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-table170-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

#[test]
fn table_170_entries_roundtrip_through_reparsed_document() {
    // 主注解 + 回复注解（带全三条条目）写出为真实字节后重新解析，
    // 读端模型必须取回同值：/Subj 文本、/IRT 引用目标、/RT 类型化枚举。
    let doc = build_file_and_parse(
        PdfVersion::V1_6,
        &[
            (0, markup(0.0)),
            (
                0,
                markup(10.0)
                    .with_subject("Invoice total")
                    .with_in_reply_to(0)
                    .with_reply_type(ReplyType::Group),
            ),
        ],
    );
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let primary = &annotations[0];
    assert_eq!(primary.subject, None, "主注解回读无 /Subj");
    assert_eq!(primary.in_reply_to, None, "主注解回读无 /IRT");
    assert_eq!(primary.reply_type, None, "主注解回读无 /RT");

    let reply = &annotations[1];
    assert_eq!(
        reply
            .subject_text()
            .expect("/Subj 语言视图可解析")
            .as_deref(),
        Some("Invoice total"),
        "/Subj 文本回读同值"
    );
    assert_eq!(reply.reply_type, Some(ReaderReplyType::Group));

    // /IRT 的引用号须解析回主注解自身的字典（同一对象，而非任意注解）。
    let (target_num, target_gen) = reply.in_reply_to.expect("/IRT 回读为引用");
    let target = doc
        .get_object(target_num)
        .expect("resolve /IRT target")
        .expect("target object present");
    assert_eq!(target.generation, target_gen);
    match &target.value {
        Value::Dict(dict) => assert_eq!(&dict.0, &primary.dict.0, "/IRT 指向主注解字典"),
        other => panic!("/IRT target shall be a dictionary, got {other:?}"),
    }
}

#[test]
fn reply_type_r_roundtrips_and_absent_entries_stay_none_after_reparse() {
    // 显式声明的 /RT /R（规范缺省值）写出后回读为 Reply；只带 /IRT 的注解
    // 回读后 /RT 仍为 None（缺省不物化在读端同样成立）。
    let doc = build_file_and_parse(
        PdfVersion::V1_6,
        &[
            (0, markup(0.0)),
            (
                0,
                markup(10.0)
                    .with_in_reply_to(0)
                    .with_reply_type(ReplyType::Reply),
            ),
            (0, markup(20.0).with_in_reply_to(0)),
        ],
    );
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations[1].reply_type, Some(ReaderReplyType::Reply));
    assert!(annotations[1].in_reply_to.is_some());
    assert_eq!(annotations[2].reply_type, None, "缺省 R 不物化");
    assert_eq!(annotations[2].subject, None);
    assert_eq!(
        annotations[0].in_reply_to, None,
        "被回复的主注解自身不写 /IRT"
    );
}
