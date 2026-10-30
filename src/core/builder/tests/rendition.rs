//! 多媒体 rendition 与 Rendition 动作写端（13.2 Table 266–278、12.6.4.13
//! Table 214）：media clip 数据来源引用、选择器嵌套、Screen 注解目标与
//! PDF 1.5 门禁。

use super::support::{annot_object, assert_text_entry};
use super::*;
use crate::core::types::PdfFeature;

fn screen(b: &mut PdfBuilder, page: usize) -> usize {
    b.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Screen, [72.0, 500.0, 392.0, 740.0]),
    )
}

fn link_with(action: Action) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 460.0, 240.0, 480.0]).with_action(action)
}

/// 在注解 `/A` 中取内联动作字典。
fn action_dict(annot: &Dict) -> Dict {
    annot
        .get("A")
        .and_then(Value::as_dict)
        .expect("annotation /A")
        .clone()
}

#[test]
fn rendition_action_writes_embedded_file_clip() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    let clip = b.embed_file(EmbeddedFile::new("clip.mp4", vec![1, 2, 3]));
    b.add_annotation(
        p0,
        link_with(Action::Rendition(RenditionAction::play(
            screen,
            Rendition::media(
                MediaClipData::new(MediaClipSource::EmbeddedFile(clip))
                    .with_name("intro clip")
                    .with_content_type("video/mp4")
                    .with_permissions(MediaPermissions::new(TemporaryFilePermission::Extract))
                    .with_base_url("https://example.com/media/"),
            )
            .with_name("intro"),
        ))),
    );

    let doc = b.assemble_document().expect("assemble");
    assert!(doc.features.borrow().contains(PdfFeature::Renditions));
    let annot = annot_object(&doc, 3, 1);
    assert_eq!(
        annot.get("Subtype"),
        Some(&Value::Name("Link".into())),
        "动作挂在 Link 注解上"
    );
    let action = action_dict(&annot);
    assert_eq!(action.get("S"), Some(&Value::Name("Rendition".into())));
    assert_eq!(action.get("OP"), Some(&Value::Integer(0)));
    let target = action
        .get("AN")
        .and_then(Value::as_ref)
        .expect("/AN indirect reference");
    assert_eq!(target.0, b.annotation_nums[screen]);
    assert!(action.get("JS").is_none(), "未声明脚本时不物化 /JS");

    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    assert_eq!(
        rendition.get("Type"),
        Some(&Value::Name("Rendition".into()))
    );
    assert_eq!(rendition.get("S"), Some(&Value::Name("MR".into())));
    assert_text_entry(rendition, "N");
    let clip_dict = rendition.get("C").and_then(Value::as_dict).expect("/C");
    assert_eq!(
        clip_dict.get("Type"),
        Some(&Value::Name("MediaClip".into()))
    );
    assert_eq!(clip_dict.get("S"), Some(&Value::Name("MCD".into())));
    let data = clip_dict
        .get("D")
        .and_then(Value::as_ref)
        .expect("/D filespec reference");
    assert_eq!(data.0, b.embedded_nums[clip]);
    let filespec = doc
        .get_object(data.0)
        .expect("filespec object")
        .expect("exists");
    assert_eq!(
        filespec.value.as_dict().and_then(|d| d.get("Type")),
        Some(&Value::Name("Filespec".into()))
    );
    let permissions = clip_dict
        .get("P")
        .and_then(Value::as_dict)
        .expect("/P permissions");
    assert_eq!(
        permissions.get("TF"),
        Some(&Value::Name("TEMPEXTRACT".into()))
    );
    let handler = clip_dict.get("MH").and_then(Value::as_dict).expect("/MH");
    assert!(handler.get("BU").is_some(), "/BU 基准 URL");
}

#[test]
fn rendition_action_writes_url_clip_selector_and_script() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    let selector = Rendition::selector(vec![
        Rendition::media(MediaClipData::new(MediaClipSource::Url(
            "https://example.com/clip.mp4".into(),
        ))),
        Rendition::selector(Vec::new()),
    ]);
    b.add_annotation(
        p0,
        link_with(Action::Rendition(
            RenditionAction::play_if_paused(screen, selector).with_javascript("app.alert('play');"),
        )),
    );

    let doc = b.assemble_document().expect("assemble");
    let action = action_dict(&annot_object(&doc, 3, 1));
    assert_eq!(action.get("OP"), Some(&Value::Integer(4)));
    assert!(action.get("JS").is_some(), "/JS 与 /OP 并存");
    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    assert_eq!(rendition.get("S"), Some(&Value::Name("SR".into())));
    let entries = rendition
        .get("R")
        .and_then(Value::as_array)
        .expect("/R array");
    assert_eq!(entries.0.len(), 2, "选择器嵌套与空候选数组都写出");
    let media = entries.0[0].as_dict().expect("media rendition");
    assert_eq!(media.get("S"), Some(&Value::Name("MR".into())));
    let data = media
        .get("C")
        .and_then(Value::as_dict)
        .and_then(|clip| clip.get("D"))
        .and_then(Value::as_dict)
        .expect("/D URL filespec");
    assert_eq!(data.get("FS"), Some(&Value::Name("URL".into())));
    assert!(
        matches!(
            data.get("F"),
            Some(Value::HexString(_)) | Some(Value::String(_))
        ),
        "URL 以文本串写出"
    );
    let nested = entries.0[1].as_dict().expect("nested selector");
    assert_eq!(nested.get("S"), Some(&Value::Name("SR".into())));
    assert_eq!(
        nested.get("R").and_then(Value::as_array).map(|a| a.0.len()),
        Some(0)
    );
}

#[test]
fn rendition_action_writes_form_xobject_clip_and_allocates_object() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    let form = b.form_xobject(&FormXObject::new(
        [0.0, 0.0, 320.0, 240.0],
        ContentStream::new(),
    ));
    b.add_annotation(
        p0,
        link_with(Action::Rendition(RenditionAction::play(
            screen,
            Rendition::media(MediaClipData::new(MediaClipSource::FormXObject(form))),
        ))),
    );

    let doc = b.assemble_document().expect("assemble");
    assert!(
        b.form_nums[form] > 0,
        "仅被 rendition 引用的表单也分配对象号"
    );
    let action = action_dict(&annot_object(&doc, 3, 1));
    let data = action
        .get("R")
        .and_then(Value::as_dict)
        .and_then(|r| r.get("C"))
        .and_then(Value::as_dict)
        .and_then(|c| c.get("D"))
        .and_then(Value::as_ref)
        .expect("/D form reference");
    assert_eq!(data.0, b.form_nums[form]);
    let form_dict = doc
        .get_object(data.0)
        .expect("form object")
        .expect("exists");
    assert_eq!(
        form_dict.value.as_dict().and_then(|d| d.get("Subtype")),
        Some(&Value::Name("Form".into()))
    );
}

#[test]
fn rendition_action_writes_script_only_form() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with(Action::Rendition(RenditionAction::script(
            "this.pageNum = 0;",
        ))),
    );

    let doc = b.assemble_document().expect("assemble");
    let action = action_dict(&annot_object(&doc, 3, 0));
    assert_eq!(action.get("S"), Some(&Value::Name("Rendition".into())));
    assert!(action.get("OP").is_none(), "无 /OP 时不物化");
    assert!(action.get("AN").is_none(), "脚本形态不要求 /AN");
    assert!(action.get("R").is_none(), "脚本形态不要求 /R");
    assert!(matches!(action.get("JS"), Some(Value::HexString(_))));
}

/// 断言构建失败、消息命中且未写出任何字节。
fn expect_rejected(builder: &mut PdfBuilder, fragment: &str) {
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("rendition action contract violation");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains(fragment), "{message}");
    assert_eq!(out.len(), 0);
}

/// 构造带单个动作的最小文档并返回构建器与页面号；`make` 接收 Screen
/// 注解句柄。
fn builder_with(make: impl FnOnce(usize) -> Action, version: PdfVersion) -> (PdfBuilder, usize) {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    b.add_annotation(p0, link_with(make(screen)));
    (b, p0)
}

#[test]
fn rendition_action_rejects_invalid_configurations() {
    // 空脚本。
    let (mut b, _) = builder_with(
        |_| Action::Rendition(RenditionAction::script("")),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/JS must not be empty");

    // /AN 越界。
    let (mut b, _) = builder_with(
        |_| Action::Rendition(RenditionAction::script("x").with_annotation(99)),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "unknown annotation handle 99");

    // /AN 目标不是 Screen 注解：句柄 0 为 Text 注解。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let text = b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]),
    );
    b.add_annotation(
        p0,
        link_with(Action::Rendition(
            RenditionAction::script("x").with_annotation(text),
        )),
    );
    expect_rejected(&mut b, "not a Screen annotation");

    // /D 引用未知嵌入文件。
    let (mut b, _) = builder_with(
        |screen| {
            Action::Rendition(RenditionAction::play(
                screen,
                Rendition::media(MediaClipData::new(MediaClipSource::EmbeddedFile(7))),
            ))
        },
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "unknown embedded file handle 7");

    // /D URL 为空。
    let (mut b, _) = builder_with(
        |screen| {
            Action::Rendition(RenditionAction::play(
                screen,
                Rendition::media(MediaClipData::new(MediaClipSource::Url(String::new()))),
            ))
        },
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "URL must be non-empty printable ASCII");

    // /D 引用未知 Form XObject。
    let (mut b, _) = builder_with(
        |screen| {
            Action::Rendition(RenditionAction::play(
                screen,
                Rendition::media(MediaClipData::new(MediaClipSource::FormXObject(3))),
            ))
        },
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "unknown form handle 3");
}

#[test]
fn rendition_action_requires_pdf_15() {
    let (mut b, _) = builder_with(
        |screen| {
            Action::Rendition(RenditionAction::play(
                screen,
                Rendition::media(MediaClipData::new(MediaClipSource::Url("clip.mp4".into()))),
            ))
        },
        PdfVersion::V1_4,
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.4 rejects renditions");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(error.to_string().contains("renditions"), "{error}");
    assert_eq!(out.len(), 0);
}
