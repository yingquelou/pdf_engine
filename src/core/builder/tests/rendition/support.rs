//! rendition 写端测试共享夹具。

use super::super::*;

pub(super) fn screen(b: &mut PdfBuilder, page: usize) -> usize {
    b.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Screen, [72.0, 500.0, 392.0, 740.0]),
    )
}

pub(super) fn link_with(action: Action) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 460.0, 240.0, 480.0]).with_action(action)
}

/// 在注解 `/A` 中取内联动作字典。
pub(super) fn action_dict(annot: &Dict) -> Dict {
    annot
        .get("A")
        .and_then(Value::as_dict)
        .expect("annotation /A")
        .clone()
}

/// 断言构建失败、消息命中且未写出任何字节。
pub(super) fn expect_rejected(builder: &mut PdfBuilder, fragment: &str) {
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
pub(super) fn builder_with(
    make: impl FnOnce(usize) -> Action,
    version: PdfVersion,
) -> (PdfBuilder, usize) {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    b.add_annotation(p0, link_with(make(screen)));
    (b, p0)
}

pub(super) fn url_clip() -> MediaClipData {
    MediaClipData::new(MediaClipSource::Url("clip.mp4".into()))
}

pub(super) fn with_play(play: MediaPlayParameters) -> Rendition {
    Rendition::Media(Box::new(MediaRendition::new(url_clip()).with_play(play)))
}

pub(super) fn with_screen(screen: MediaScreenParameters) -> Rendition {
    Rendition::Media(Box::new(
        MediaRendition::new(url_clip()).with_screen(screen),
    ))
}

pub(super) fn action_with(rendition: Rendition) -> impl FnOnce(usize) -> Action {
    move |screen| Action::Rendition(RenditionAction::play(screen, rendition))
}
