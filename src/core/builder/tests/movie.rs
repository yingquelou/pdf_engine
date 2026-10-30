//! 电影对象（13.4 Table 295）、Movie 注解（12.5.6.17 Table 186）与 Movie
//! 动作（12.6.4.9 Table 209）写端：内联电影字典、播放设置、注解目标解析
//! 与 PDF 1.2 门禁。

use super::support::annot_object;
use super::*;
use crate::core::reader::movie::{MovieMode, MovieTime, MovieTimeValue};
use crate::core::types::{Array, PdfFeature};

fn movie_annotation(movie: Movie) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Movie, [72.0, 600.0, 240.0, 700.0]).with_movie(movie)
}

fn link_with_action(action: Action) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 500.0, 240.0, 520.0]).with_action(action)
}

#[test]
fn movie_annotation_writes_dictionary_and_activation_playback() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        movie_annotation(
            Movie::new("clip.mov")
                .with_aspect(640, 480)
                .with_rotate(90)
                .with_poster(true),
        )
        .with_movie_activation(MoviePlayback::Activation(
            MovieActivation::new()
                .with_start(MovieTime {
                    value: MovieTimeValue::Integer(0),
                    time_scale: None,
                })
                .with_volume(0.5)
                .with_mode(MovieMode::Repeat),
        )),
    );

    let doc = b.assemble_document().expect("assemble");
    assert!(doc.features.borrow().contains(PdfFeature::MovieObjects));
    let annot = annot_object(&doc, 3, 0);
    assert_eq!(annot.get("Subtype"), Some(&Value::Name("Movie".into())));

    let movie = annot.get("Movie").and_then(Value::as_dict).expect("/Movie");
    assert_eq!(movie.get("Type"), None, "电影字典没有 /Type 条目");
    let filespec = movie.get("F").and_then(Value::as_dict).expect("/F");
    assert_eq!(filespec.get("Type"), Some(&Value::Name("Filespec".into())));
    assert_eq!(filespec.get("F"), Some(&Value::String("clip.mov".into())));
    assert_eq!(
        movie.get("Aspect"),
        Some(&Value::Array(Array(vec![
            Value::Integer(640),
            Value::Integer(480),
        ])))
    );
    assert_eq!(movie.get("Rotate"), Some(&Value::Integer(90)));
    assert_eq!(movie.get("Poster"), Some(&Value::Bool(true)));

    let playback = annot.get("A").and_then(Value::as_dict).expect("/A");
    assert_eq!(playback.get("Start"), Some(&Value::Integer(0)));
    assert_eq!(playback.get("Volume"), Some(&Value::Float(0.5)));
    assert_eq!(playback.get("Mode"), Some(&Value::Name("Repeat".into())));
    assert_eq!(playback.get("Rate"), None, "缺省条目不物化");
}

#[test]
fn movie_annotation_boolean_playback_is_written_as_flag() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        movie_annotation(Movie::new("clip.mov")).with_movie_activation(MoviePlayback::Flag(false)),
    );

    let doc = b.assemble_document().expect("assemble");
    let annot = annot_object(&doc, 3, 0);
    assert_eq!(annot.get("A"), Some(&Value::Bool(false)));
    let movie = annot.get("Movie").and_then(Value::as_dict).expect("/Movie");
    for key in ["Aspect", "Rotate", "Poster"] {
        assert_eq!(movie.get(key), None, "缺省 {key} 不物化");
    }
}

#[test]
fn movie_action_writes_annotation_reference_and_activation() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    let target = b.add_annotation(p0, movie_annotation(Movie::new("clip.mov")));
    b.add_annotation(
        p0,
        link_with_action(Action::Movie(
            MovieAction::annotation(target)
                .with_activation(MovieActivation::new().with_synchronous(true)),
        )),
    );

    let doc = b.assemble_document().expect("assemble");
    let (movie_num, _) = super::support::annot_object_with_num(&doc, 3, 0);
    let link = annot_object(&doc, 3, 1);
    let action = link.get("A").and_then(Value::as_dict).expect("/A");
    assert_eq!(action.get("S"), Some(&Value::Name("Movie".into())));
    assert_eq!(action.get("Annotation"), Some(&Value::Ref(movie_num, 0)));
    assert_eq!(action.get("Synchronous"), Some(&Value::Bool(true)));
    assert_eq!(action.get("T"), None);
}

#[test]
fn movie_action_title_target_is_written_as_text_string() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with_action(Action::Movie(MovieAction::title("Intro"))),
    );

    let doc = b.assemble_document().expect("assemble");
    let link = annot_object(&doc, 3, 0);
    let action = link.get("A").and_then(Value::as_dict).expect("/A");
    assert_eq!(action.get("S"), Some(&Value::Name("Movie".into())));
    assert!(
        matches!(action.get("T"), Some(Value::HexString(_))),
        "标题写成文本串"
    );
    assert_eq!(action.get("Annotation"), None);
}

#[test]
fn movie_action_target_must_be_movie_annotation() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    let text = b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 120.0, 720.0]),
    );
    b.add_annotation(
        p0,
        link_with_action(Action::Movie(MovieAction::annotation(text))),
    );

    let mut buf = Vec::new();
    let error = b.build(&mut buf).expect_err("non-Movie target");
    assert!(
        error.to_string().contains("not a Movie annotation"),
        "{error}"
    );
    assert!(buf.is_empty(), "校验失败时零字节输出");
}

#[test]
fn movie_action_rejects_unknown_annotation_handle() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with_action(Action::Movie(MovieAction::annotation(5))),
    );

    let mut buf = Vec::new();
    let error = b.build(&mut buf).expect_err("unknown handle");
    assert!(
        error.to_string().contains("unknown annotation handle 5"),
        "{error}"
    );
    assert!(buf.is_empty(), "校验失败时零字节输出");
}

#[test]
fn movie_action_rejects_empty_title() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with_action(Action::Movie(MovieAction::title("  "))),
    );

    let mut buf = Vec::new();
    let error = b.build(&mut buf).expect_err("empty title");
    assert!(error.to_string().contains("must not be empty"), "{error}");
    assert!(buf.is_empty(), "校验失败时零字节输出");
}

#[test]
fn movie_annotation_requires_movie_dictionary() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Movie, [72.0, 600.0, 240.0, 700.0]),
    );

    let mut buf = Vec::new();
    let error = b.build(&mut buf).expect_err("missing /Movie");
    assert!(error.to_string().contains("requires /Movie"), "{error}");
    assert!(buf.is_empty(), "校验失败时零字节输出");
}

#[test]
fn movie_annotation_rejects_generic_action() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        movie_annotation(Movie::new("clip.mov"))
            .with_action(Action::Uri("https://example.com".into())),
    );

    let mut buf = Vec::new();
    let error = b.build(&mut buf).expect_err("/A conflict");
    assert!(
        error.to_string().contains("cannot carry an action"),
        "{error}"
    );
    assert!(buf.is_empty());
}

#[test]
fn movie_entries_rejected_before_pdf_1_2() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, movie_annotation(Movie::new("clip.mov")));

    let mut buf = Vec::new();
    let error = b.build(&mut buf).expect_err("version gate");
    assert!(
        matches!(
            error,
            crate::core::writer::WriteError::UnsupportedVersion(_)
        ),
        "unexpected error: {error}"
    );
    assert!(buf.is_empty(), "版本不足时零字节输出");

    // 无 Movie 注解、仅标题目标的 Movie 动作同样按 PDF 1.2 门禁。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with_action(Action::Movie(MovieAction::title("Intro"))),
    );
    let mut buf = Vec::new();
    let error = b.build(&mut buf).expect_err("action version gate");
    assert!(
        error.to_string().contains("movie objects"),
        "unexpected error: {error}"
    );
    assert!(buf.is_empty());
}

#[test]
fn movie_activation_validates_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    let movie = Movie::new("clip.mov");
    b.add_annotation(
        p0,
        movie_annotation(movie).with_movie_activation(MoviePlayback::Activation(
            MovieActivation::new().with_mode(MovieMode::Other("Bounce".into())),
        )),
    );

    let mut buf = Vec::new();
    let error = b.build(&mut buf).expect_err("unknown mode");
    assert!(error.to_string().contains("Bounce"), "{error}");
    assert!(buf.is_empty());

    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, movie_annotation(Movie::new("clip.mov").with_rotate(45)));
    let mut buf = Vec::new();
    let error = b.build(&mut buf).expect_err("bad rotate");
    assert!(error.to_string().contains("multiple of 90"), "{error}");
    assert!(buf.is_empty());
}
