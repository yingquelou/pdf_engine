//! 电影对象（13.4 Table 295）、Movie 注解（12.5.6.17 Table 186）与
//! Movie 动作（12.6.4.9 Table 209）写出 → 解析 → 回读往返。

use super::*;

/// Movie 注解携带内联电影字典与激活播放参数；Link `/A` Movie 动作按
/// 注解对象号回读为同一 Movie 注解。时间值的 8 字节形态经十六进制串
/// 往返无损。
#[test]
fn builder_movie_annotation_and_action_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, Movie, MovieAction, MovieActivation, MoviePlayback, PageSize,
        PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::action::Action as ReadAction;
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::reader::movie::{
        MovieMode, MoviePlayback as ReadMoviePlayback, MovieTime, MovieTimeValue, Poster,
    };

    let bytes = [0, 0, 0, 0, 0, 0, 0x03, 0xE8];
    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let movie = builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Movie, [72.0, 600.0, 240.0, 700.0])
            .with_movie(
                Movie::new("clip.mov")
                    .with_aspect(640, 480)
                    .with_rotate(270)
                    .with_poster(true),
            )
            .with_movie_activation(MoviePlayback::Activation(
                MovieActivation::new()
                    .with_start(MovieTime {
                        value: MovieTimeValue::Bytes(bytes),
                        time_scale: Some(60),
                    })
                    .with_volume(0.5)
                    .with_mode(MovieMode::Palindrome)
                    .with_fw_position(0.25, 0.75),
            )),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 500.0, 240.0, 520.0]).with_action(
            Action::Movie(
                MovieAction::annotation(movie).with_activation(
                    MovieActivation::new()
                        .with_synchronous(true)
                        .with_show_controls(true),
                ),
            ),
        ),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build movie");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);
    // 注解对象为懒加载对象，解析后特性画像应登记电影对象（PDF 1.2）。
    assert!(
        doc.features
            .borrow()
            .contains(pdf_engine::core::types::PdfFeature::MovieObjects)
    );
    let annotation = &annotations[0];
    assert_eq!(annotation.subtype, AnnotationSubtype::Movie);
    let parsed_movie = annotation.movie.as_ref().expect("/Movie");
    assert_eq!(parsed_movie.file.short_name.as_deref(), Some("clip.mov"));
    assert_eq!(parsed_movie.aspect, Some((640, 480)));
    assert_eq!(parsed_movie.rotate, 270);
    assert_eq!(parsed_movie.poster, Poster::FromMovieFile);
    match annotation.movie_playback.as_ref().expect("/A") {
        ReadMoviePlayback::Activation(activation) => {
            assert_eq!(
                activation.start,
                Some(MovieTime {
                    value: MovieTimeValue::Bytes(bytes),
                    time_scale: Some(60),
                })
            );
            assert_eq!(activation.volume, Some(0.5));
            assert_eq!(activation.mode, MovieMode::Palindrome);
            assert_eq!(activation.fw_position, Some((0.25, 0.75)));
        }
        other => panic!("unexpected playback: {other:?}"),
    }

    let link = &annotations[1];
    assert_eq!(link.subtype, AnnotationSubtype::Link);
    let action = link.action.as_ref().expect("/A");
    match doc.action(action).expect("typed action") {
        ReadAction::Movie(action) => {
            assert_eq!(action.annotation, annotation.reference);
            assert_eq!(action.title, None);
            assert_eq!(action.activation.synchronous, Some(true));
            assert_eq!(action.activation.show_controls, Some(true));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

/// 标题形态的 Movie 动作与新布尔播放设置往返；`/T` 标题写成文本串。
#[test]
fn builder_movie_title_action_and_flag_playback_roundtrip() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, Movie, MovieAction, MoviePlayback, PageSize, PdfAnnotation,
        PdfBuilder,
    };
    use pdf_engine::core::reader::action::Action as ReadAction;
    use pdf_engine::core::reader::movie::MoviePlayback as ReadMoviePlayback;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Movie, [72.0, 600.0, 240.0, 700.0])
            .with_movie(Movie::new("clip.mov"))
            .with_title("Intro")
            .with_movie_activation(MoviePlayback::Flag(false)),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 500.0, 240.0, 520.0])
            .with_action(Action::Movie(MovieAction::title("Intro"))),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build movie");
    let doc = parse_buf(&buf);

    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(
        annotations[0].movie_playback,
        Some(ReadMoviePlayback::Flag(false))
    );
    match doc
        .action(annotations[1].action.as_ref().expect("/A"))
        .expect("typed action")
    {
        ReadAction::Movie(action) => {
            assert_eq!(action.annotation, None);
            assert_eq!(action.title.as_deref(), Some("Intro"));
            assert_eq!(action.activation.volume, None);
        }
        other => panic!("unexpected action: {other:?}"),
    }
}
