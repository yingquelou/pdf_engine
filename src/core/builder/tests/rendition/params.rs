//! rendition `/P` `/SP` 播放/屏幕参数写出（13.2.5–13.2.6 Table 279–284）。

use super::super::support::annot_object;
use super::super::*;
use super::support::*;

#[test]
fn rendition_writes_play_and_screen_parameters() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    let clip = b.embed_file(EmbeddedFile::new("clip.mp4", vec![1, 2, 3]));
    let play = MediaPlayParameters::new()
        .with_must_honour(
            MediaPlaySettings::new()
                .with_volume(75)
                .with_controller(true)
                .with_fit(MediaFit::Slice)
                .with_duration(MediaDuration::explicit(MediaTimespan::new(12.5)))
                .with_auto_play(false)
                .with_repeat_count(2.0),
        )
        .with_best_effort(MediaPlaySettings::new().with_fit(MediaFit::Other(9)));
    let screen_parameters = MediaScreenParameters::new().with_must_honour(
        MediaScreenSettings::new()
            .with_window(MediaWindow::Floating)
            .with_background([0.25, 0.5, 0.75])
            .with_opacity(0.5)
            .with_monitor(2)
            .with_floating(
                MediaFloatingWindow::new(640, 480)
                    .with_resize_target(1)
                    .with_position(5)
                    .with_offscreen(2)
                    .with_title_bar(false)
                    .with_closable(true)
                    .with_resizable(2)
                    .with_title(vec!["clip".into(), "intro".into()]),
            ),
    );
    b.add_annotation(
        p0,
        link_with(Action::Rendition(RenditionAction::play(
            screen,
            Rendition::Media(Box::new(
                MediaRendition::new(MediaClipData::new(MediaClipSource::EmbeddedFile(clip)))
                    .with_play(play)
                    .with_screen(screen_parameters),
            )),
        ))),
    );

    let doc = b.assemble_document().expect("assemble");
    let action = action_dict(&annot_object(&doc, 3, 1));
    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    let play = rendition.get("P").and_then(Value::as_dict).expect("/P");
    assert_eq!(
        play.get("Type"),
        Some(&Value::Name("MediaPlayParams".into()))
    );
    assert!(play.get("PL").is_none(), "未声明播放器时不写 /PL");
    let mh = play.get("MH").and_then(Value::as_dict).expect("/MH");
    assert_eq!(mh.get("V"), Some(&Value::Integer(75)));
    assert_eq!(mh.get("C"), Some(&Value::Bool(true)));
    assert_eq!(mh.get("F"), Some(&Value::Integer(1)));
    assert_eq!(mh.get("A"), Some(&Value::Bool(false)));
    assert_eq!(mh.get("RC"), Some(&Value::Float(2.0)));
    let duration = mh.get("D").and_then(Value::as_dict).expect("/D");
    assert_eq!(
        duration.get("Type"),
        Some(&Value::Name("MediaDuration".into()))
    );
    assert_eq!(duration.get("S"), Some(&Value::Name("T".into())));
    let timespan = duration.get("T").and_then(Value::as_dict).expect("/T");
    assert_eq!(timespan.get("Type"), Some(&Value::Name("Timespan".into())));
    assert_eq!(timespan.get("S"), Some(&Value::Name("S".into())));
    assert_eq!(timespan.get("V"), Some(&Value::Float(12.5)));
    let best = play.get("BE").and_then(Value::as_dict).expect("/BE");
    assert_eq!(best.get("F"), Some(&Value::Integer(9)));
    assert!(best.get("V").is_none(), "未设置条目省略");

    let screen_parameters = rendition.get("SP").and_then(Value::as_dict).expect("/SP");
    assert_eq!(
        screen_parameters.get("Type"),
        Some(&Value::Name("MediaScreenParams".into()))
    );
    let mh = screen_parameters
        .get("MH")
        .and_then(Value::as_dict)
        .expect("/MH");
    assert_eq!(mh.get("W"), Some(&Value::Integer(0)));
    assert_eq!(
        mh.get("B")
            .and_then(Value::as_array)
            .map(|array| array.0.as_slice()),
        Some([Value::Float(0.25), Value::Float(0.5), Value::Float(0.75)].as_slice())
    );
    assert_eq!(mh.get("O"), Some(&Value::Float(0.5)));
    assert_eq!(mh.get("M"), Some(&Value::Integer(2)));
    let floating = mh.get("F").and_then(Value::as_dict).expect("/F");
    assert_eq!(floating.get("Type"), Some(&Value::Name("FWParams".into())));
    assert_eq!(
        floating
            .get("D")
            .and_then(Value::as_array)
            .map(|array| array.0.as_slice()),
        Some([Value::Integer(640), Value::Integer(480)].as_slice())
    );
    assert_eq!(floating.get("RT"), Some(&Value::Integer(1)));
    assert_eq!(floating.get("P"), Some(&Value::Integer(5)));
    assert_eq!(floating.get("O"), Some(&Value::Integer(2)));
    assert_eq!(floating.get("T"), Some(&Value::Bool(false)));
    assert_eq!(floating.get("UC"), Some(&Value::Bool(true)));
    assert_eq!(floating.get("R"), Some(&Value::Integer(2)));
    let title = floating.get("TT").and_then(Value::as_array).expect("/TT");
    assert_eq!(title.0.len(), 2);
    assert!(screen_parameters.get("BE").is_none());
}

#[test]
fn rendition_play_parameters_reject_invalid_values() {
    let (mut b, _) = builder_with(
        action_with(with_play(
            MediaPlayParameters::new().with_must_honour(MediaPlaySettings::new().with_volume(-1)),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/V must not be negative");

    let (mut b, _) = builder_with(
        action_with(with_play(
            MediaPlayParameters::new()
                .with_best_effort(MediaPlaySettings::new().with_repeat_count(-0.5)),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/RC must be a finite non-negative number");

    let (mut b, _) = builder_with(
        action_with(with_play(
            MediaPlayParameters::new().with_must_honour(
                MediaPlaySettings::new()
                    .with_duration(MediaDuration::explicit(MediaTimespan::new(-1.0))),
            ),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/D /T /V must be a finite non-negative number");

    // 有效对照：边界值 0 合法。
    let (mut b, _) = builder_with(
        action_with(with_play(
            MediaPlayParameters::new().with_must_honour(
                MediaPlaySettings::new()
                    .with_volume(0)
                    .with_repeat_count(0.0)
                    .with_duration(MediaDuration::intrinsic()),
            ),
        )),
        PdfVersion::V1_7,
    );
    b.assemble_document().expect("valid play parameters");
}

#[test]
fn rendition_screen_parameters_reject_invalid_values() {
    let (mut b, _) = builder_with(
        action_with(with_screen(MediaScreenParameters::new().with_must_honour(
            MediaScreenSettings::new().with_background([0.0, 1.0, 1.5]),
        ))),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/B components must be within 0.0..=1.0");

    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new()
                .with_best_effort(MediaScreenSettings::new().with_opacity(1.5)),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/O must be within 0.0..=1.0");

    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new()
                .with_must_honour(MediaScreenSettings::new().with_monitor(7)),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/M must be within 0..=6");

    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new().with_must_honour(
                MediaScreenSettings::new()
                    .with_window(MediaWindow::Hidden)
                    .with_floating(MediaFloatingWindow::new(640, 480)),
            ),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/F is only valid when /W is 0");

    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new().with_must_honour(
                MediaScreenSettings::new()
                    .with_window(MediaWindow::Floating)
                    .with_floating(MediaFloatingWindow::new(-1, 480)),
            ),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/D entries must not be negative");

    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new().with_must_honour(
                MediaScreenSettings::new()
                    .with_window(MediaWindow::Floating)
                    .with_floating(MediaFloatingWindow::new(640, 480).with_position(9)),
            ),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/P must be within 0..=8");

    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new().with_must_honour(
                MediaScreenSettings::new()
                    .with_window(MediaWindow::Floating)
                    .with_floating(MediaFloatingWindow::new(640, 480).with_resize_target(4)),
            ),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/RT must be within 0..=3");

    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new().with_must_honour(
                MediaScreenSettings::new()
                    .with_window(MediaWindow::Floating)
                    .with_floating(MediaFloatingWindow::new(640, 480).with_offscreen(3)),
            ),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/O must be within 0..=2");

    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new().with_must_honour(
                MediaScreenSettings::new()
                    .with_window(MediaWindow::Floating)
                    .with_floating(MediaFloatingWindow::new(640, 480).with_resizable(3)),
            ),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/R must be within 0..=2");

    // 有效对照：完整浮动窗口与空标题数组。
    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new().with_must_honour(
                MediaScreenSettings::new()
                    .with_window(MediaWindow::Floating)
                    .with_background([0.0, 0.0, 0.0])
                    .with_opacity(1.0)
                    .with_monitor(6)
                    .with_floating(
                        MediaFloatingWindow::new(0, 0)
                            .with_resize_target(3)
                            .with_position(8)
                            .with_offscreen(2)
                            .with_resizable(2)
                            .with_title(Vec::new()),
                    ),
            ),
        )),
        PdfVersion::V1_7,
    );
    b.assemble_document().expect("valid screen parameters");
}

#[test]
fn rendition_floating_window_requires_dimensions() {
    let (mut b, _) = builder_with(
        action_with(with_screen(MediaScreenParameters::new().with_must_honour(
            MediaScreenSettings::new().with_window(MediaWindow::Floating),
        ))),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/F is required when /W is 0");

    let (mut b, _) = builder_with(
        action_with(with_screen(
            MediaScreenParameters::new().with_must_honour(
                MediaScreenSettings::new()
                    .with_window(MediaWindow::Floating)
                    .with_floating(MediaFloatingWindow::new(640, -1)),
            ),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "/D entries must not be negative");
}
