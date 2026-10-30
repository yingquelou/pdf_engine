//! 多媒体 rendition（13.2 Table 266–278）与 Rendition 动作（12.6.4.13
//! Table 214）写出 → 解析 → 回读往返。

use super::*;

/// 嵌入文件作为 media clip `/D`：Screen 注解目标、播放操作与 clip 条目
/// 经读取端还原为同一模型；懒加载解析后特性画像登记 rendition（PDF 1.5）。
#[test]
fn builder_rendition_action_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, EmbeddedFile, MediaClipData, MediaClipSource, MediaPermissions,
        PageSize, PdfAnnotation, PdfBuilder, Rendition, RenditionAction, TemporaryFilePermission,
    };
    use pdf_engine::core::reader::action::{Action as ReadAction, RenditionOperation};
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::reader::rendition::{
        MediaClip, MediaClipDataSource, Rendition as ReadRendition,
        TemporaryFilePermission as ReadTemporaryFilePermission,
    };
    use pdf_engine::core::types::PdfFeature;

    let mut builder = PdfBuilder::with_version(pdf_engine::core::types::PdfVersion::V1_5);
    let p0 = builder.page(PageSize::A4);
    let screen = builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Screen, [72.0, 500.0, 392.0, 740.0]),
    );
    let clip = builder.embed_file(EmbeddedFile::new("clip.mp4", vec![0, 1, 2, 3]));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 460.0, 240.0, 480.0]).with_action(
            Action::Rendition(RenditionAction::play(
                screen,
                Rendition::media(
                    MediaClipData::new(MediaClipSource::EmbeddedFile(clip))
                        .with_name("intro clip")
                        .with_content_type("video/mp4")
                        .with_permissions(MediaPermissions::new(TemporaryFilePermission::Access))
                        .with_base_url("https://example.com/media/"),
                )
                .with_name("intro"),
            )),
        ),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build rendition");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);
    assert_eq!(annotations[0].subtype, AnnotationSubtype::Screen);
    assert!(doc.features.borrow().contains(PdfFeature::Renditions));

    let action = doc
        .action(annotations[1].action.as_ref().expect("/A"))
        .expect("typed action");
    let ReadAction::Rendition(action) = action else {
        panic!("expected a rendition action, got {action:?}");
    };
    assert_eq!(action.operation, Some(RenditionOperation::Play));
    assert_eq!(action.annotation, annotations[0].reference);
    assert!(action.javascript.is_none());

    let ReadRendition::Media(media) = action.rendition.expect("/R") else {
        panic!("expected a media rendition");
    };
    assert_eq!(media.name.as_deref(), Some("intro"));
    let Some(MediaClip::Data(clip)) = media.clip else {
        panic!("expected media clip data");
    };
    assert_eq!(clip.name.as_deref(), Some("intro clip"));
    assert_eq!(clip.content_type.as_deref(), Some("video/mp4"));
    assert_eq!(
        clip.permissions
            .as_ref()
            .map(|permissions| permissions.temporary_file.clone()),
        Some(ReadTemporaryFilePermission::Access)
    );
    let MediaClipDataSource::FileSpec(filespec) = clip.data else {
        panic!("expected a filespec data source");
    };
    assert_eq!(filespec.short_name.as_deref(), Some("clip.mp4"));
    assert_eq!(filespec.file_system, None);
    assert_eq!(filespec.embedded.len(), 1, "嵌入文件流随文件规格写出");
    assert_eq!(
        clip.mh
            .as_ref()
            .and_then(|handler| handler.base_url.as_deref()),
        Some("https://example.com/media/")
    );
}

/// 选择器 rendition（含空候选数组）、URL 文件规格与 `/OP` + `/JS` 并存。
#[test]
fn builder_rendition_selector_and_script_roundtrip() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, MediaClipData, MediaClipSource, PageSize, PdfAnnotation,
        PdfBuilder, Rendition, RenditionAction,
    };
    use pdf_engine::core::reader::action::{Action as ReadAction, RenditionOperation};
    use pdf_engine::core::reader::rendition::{
        MediaClip, MediaClipDataSource, Rendition as ReadRendition,
    };
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = builder.page(PageSize::A4);
    let screen = builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Screen, [0.0, 0.0, 320.0, 240.0]),
    );
    let selector = Rendition::selector(vec![
        Rendition::media(MediaClipData::new(MediaClipSource::Url(
            "https://example.com/clip.mp4".into(),
        ))),
        Rendition::selector(Vec::new()),
    ]);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 250.0, 100.0, 270.0]).with_action(
            Action::Rendition(
                RenditionAction::play_if_paused(screen, selector)
                    .with_javascript("app.alert('play');"),
            ),
        ),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build selector rendition");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    let action = doc
        .action(annotations[1].action.as_ref().expect("/A"))
        .expect("typed action");
    let ReadAction::Rendition(action) = action else {
        panic!("expected a rendition action, got {action:?}");
    };
    assert_eq!(action.operation, Some(RenditionOperation::PlayIfPaused));
    assert_eq!(action.javascript.as_deref(), Some("app.alert('play');"));

    let ReadRendition::Selector(selector) = action.rendition.expect("/R") else {
        panic!("expected a selector rendition");
    };
    assert_eq!(selector.renditions.len(), 2);
    let ReadRendition::Media(media) = &selector.renditions[0] else {
        panic!("expected a nested media rendition");
    };
    let Some(MediaClip::Data(clip)) = &media.clip else {
        panic!("expected media clip data");
    };
    let MediaClipDataSource::FileSpec(filespec) = &clip.data else {
        panic!("expected a filespec data source");
    };
    assert_eq!(filespec.file_system.as_deref(), Some("URL"));
    assert_eq!(
        filespec.short_name.as_deref(),
        Some("https://example.com/clip.mp4")
    );
    let ReadRendition::Selector(empty) = &selector.renditions[1] else {
        panic!("expected a nested empty selector");
    };
    assert!(empty.renditions.is_empty());
}

/// 仅 `/JS` 的 rendition 动作不写 `/OP` `/AN` `/R`。
#[test]
fn builder_rendition_script_only_roundtrip() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, RenditionAction,
    };
    use pdf_engine::core::reader::action::Action as ReadAction;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 100.0, 20.0]).with_action(
            Action::Rendition(RenditionAction::script("this.pageNum = 0;")),
        ),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build script rendition");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    let action = doc
        .action(annotations[0].action.as_ref().expect("/A"))
        .expect("typed action");
    let ReadAction::Rendition(action) = action else {
        panic!("expected a rendition action, got {action:?}");
    };
    assert_eq!(action.operation, None);
    assert_eq!(action.annotation, None);
    assert_eq!(action.rendition, None);
    assert_eq!(action.javascript.as_deref(), Some("this.pageNum = 0;"));
}

/// 播放参数（Table 279–281）与屏幕参数（Table 282–284）经写端嵌套
/// 字典写出后由读取端还原为同一类型化模型。
#[test]
fn builder_rendition_play_and_screen_parameters_roundtrip() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, EmbeddedFile, MediaClipData, MediaClipSource, MediaDuration,
        MediaFit, MediaFloatingWindow, MediaPlayParameters, MediaPlaySettings, MediaRendition,
        MediaScreenParameters, MediaScreenSettings, MediaTimespan, MediaWindow, PageSize,
        PdfAnnotation, PdfBuilder, Rendition, RenditionAction,
    };
    use pdf_engine::core::reader::action::Action as ReadAction;
    use pdf_engine::core::reader::rendition::{
        MediaDurationKind, MediaFit as ReadMediaFit, MediaWindow as ReadMediaWindow,
        Rendition as ReadRendition,
    };
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = builder.page(PageSize::A4);
    let screen = builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Screen, [72.0, 500.0, 392.0, 740.0]),
    );
    let clip = builder.embed_file(EmbeddedFile::new("clip.mp4", vec![0, 1, 2, 3]));
    let play = MediaPlayParameters::new().with_must_honour(
        MediaPlaySettings::new()
            .with_volume(75)
            .with_fit(MediaFit::Slice)
            .with_duration(MediaDuration::explicit(MediaTimespan::new(12.5)))
            .with_repeat_count(2.0),
    );
    let screen_parameters = MediaScreenParameters::new().with_must_honour(
        MediaScreenSettings::new()
            .with_window(MediaWindow::Floating)
            .with_background([0.25, 0.5, 0.75])
            .with_opacity(0.75)
            .with_floating(
                MediaFloatingWindow::new(640, 480)
                    .with_title_bar(false)
                    .with_title(vec!["clip".into(), "intro".into()]),
            ),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 460.0, 240.0, 480.0]).with_action(
            Action::Rendition(RenditionAction::play(
                screen,
                Rendition::Media(Box::new(
                    MediaRendition::new(MediaClipData::new(MediaClipSource::EmbeddedFile(clip)))
                        .with_play(play)
                        .with_screen(screen_parameters),
                )),
            )),
        ),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build rendition parameters");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    let action = doc
        .action(annotations[1].action.as_ref().expect("/A"))
        .expect("typed action");
    let ReadAction::Rendition(action) = action else {
        panic!("expected a rendition action, got {action:?}");
    };
    let ReadRendition::Media(media) = action.rendition.expect("/R") else {
        panic!("expected a media rendition");
    };

    let play = media.play.expect("/P");
    let must = play.must_honour.expect("/MH");
    assert_eq!(must.volume, Some(75));
    assert_eq!(must.fit, Some(ReadMediaFit::Slice));
    assert_eq!(must.repeat_count, Some(2.0));
    assert!(must.controller.is_none(), "未设置条目保持缺省");
    let duration = must.duration.expect("/D");
    assert_eq!(duration.kind, MediaDurationKind::Explicit);
    assert_eq!(duration.timespan.expect("/T").seconds, 12.5);

    let screen = media.screen.expect("/SP");
    let must = screen.must_honour.expect("/MH");
    assert_eq!(must.window, Some(ReadMediaWindow::Floating));
    assert_eq!(must.background, Some([0.25, 0.5, 0.75]));
    assert_eq!(must.opacity, Some(0.75));
    let floating = must.floating.expect("/F");
    assert_eq!(floating.dimensions, [640, 480]);
    assert_eq!(floating.title_bar, Some(false));
    assert_eq!(
        floating.title.as_deref(),
        Some(["clip".to_owned(), "intro".to_owned()].as_slice())
    );
}
