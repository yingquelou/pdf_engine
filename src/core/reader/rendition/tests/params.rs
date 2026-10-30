//! rendition `/P` `/SP` 参数读端测试（13.2.5–13.2.6 Table 279–284）。

use super::*;

fn play_parameters(play: &Value) -> MediaPlayParameters {
    let rendition = Document::new()
        .rendition(play)
        .expect("media play parameters");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    media.play.expect("/P")
}

fn screen_parameters(screen: &Value) -> MediaScreenParameters {
    let rendition = Document::new()
        .rendition(screen)
        .expect("media screen parameters");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    media.screen.expect("/SP")
}

#[test]
fn parses_media_play_parameters_and_durations() {
    let play = play_parameters(&rendition_with_play(settings(
        Dict::new()
            .insert("Type", name("MediaPlayParams"))
            .insert("PL", Value::Dict(Dict::new()))
            .insert(
                "MH",
                settings(
                    Dict::new()
                        .insert("V", Value::Integer(80))
                        .insert("C", Value::Bool(true))
                        .insert("F", Value::Integer(1))
                        .insert("D", duration("T", Some(timespan(Value::Float(12.5)))))
                        .insert("A", Value::Bool(false))
                        .insert("RC", Value::Integer(2)),
                ),
            )
            .insert(
                "BE",
                settings(
                    Dict::new()
                        .insert("F", Value::Integer(9))
                        .insert("D", duration("I", None))
                        .insert("RC", Value::Float(0.0)),
                ),
            ),
    )));
    assert!(play.players.is_some());
    let must = play.must_honour.expect("MH");
    assert_eq!(must.volume, Some(80));
    assert_eq!(must.controller, Some(true));
    assert_eq!(must.fit, Some(MediaFit::Slice));
    let explicit = must.duration.expect("explicit duration");
    assert_eq!(explicit.kind, MediaDurationKind::Explicit);
    assert_eq!(explicit.timespan.expect("/T").seconds, 12.5);
    assert_eq!(must.auto_play, Some(false));
    assert_eq!(must.repeat_count, Some(2.0));
    let best = play.best_effort.expect("BE");
    assert_eq!(best.fit, Some(MediaFit::Other(9)), "未识别 /F 保留原值");
    assert_eq!(
        best.duration.expect("intrinsic duration").kind,
        MediaDurationKind::Intrinsic
    );
    assert_eq!(best.repeat_count, Some(0.0));
    assert!(best.volume.is_none());

    let infinite = play_parameters(&rendition_with_play(settings(
        Dict::new().insert("MH", settings(Dict::new().insert("D", duration("F", None)))),
    )));
    assert_eq!(
        infinite
            .must_honour
            .and_then(|settings| settings.duration)
            .expect("infinite duration")
            .kind,
        MediaDurationKind::Infinity
    );

    let doc = Document::new();
    insert(
        &doc,
        60,
        Value::Dict(
            Dict::new()
                .insert("Type", name("MediaPlayParams"))
                .insert("MH", settings(Dict::new().insert("V", Value::Integer(50)))),
        ),
    );
    let rendition = doc
        .rendition(&Value::Dict(
            media_rendition(playback_clip()).insert("P", Value::Ref(60, 0)),
        ))
        .expect("indirect play parameters");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    assert_eq!(
        media.play.expect("/P").must_honour.expect("MH").volume,
        Some(50)
    );
}

#[test]
fn parses_media_screen_parameters_and_floating_window() {
    let screen = screen_parameters(&rendition_with_screen(settings(
        Dict::new()
            .insert("Type", name("MediaScreenParams"))
            .insert(
                "MH",
                settings(
                    Dict::new()
                        .insert("W", Value::Integer(0))
                        .insert(
                            "B",
                            arr(vec![
                                Value::Float(0.1),
                                Value::Float(0.2),
                                Value::Float(0.3),
                            ]),
                        )
                        .insert("O", Value::Float(0.75))
                        .insert("M", Value::Integer(2))
                        .insert(
                            "F",
                            floating_window(
                                Dict::new()
                                    .insert(
                                        "D",
                                        arr(vec![Value::Integer(640), Value::Integer(480)]),
                                    )
                                    .insert("RT", Value::Integer(1))
                                    .insert("P", Value::Integer(8))
                                    .insert("O", Value::Integer(2))
                                    .insert("T", Value::Bool(false))
                                    .insert("UC", Value::Bool(true))
                                    .insert("R", Value::Integer(2))
                                    .insert(
                                        "TT",
                                        arr(vec![
                                            Value::String("title".into()),
                                            Value::String("subtitle".into()),
                                        ]),
                                    ),
                            ),
                        ),
                ),
            )
            .insert("BE", settings(Dict::new())),
    )));
    let must = screen.must_honour.expect("MH");
    assert_eq!(must.window, Some(MediaWindow::Floating));
    assert_eq!(must.background, Some([0.1, 0.2, 0.3]));
    assert_eq!(must.opacity, Some(0.75));
    assert_eq!(must.monitor, Some(2));
    let floating = must.floating.expect("floating window");
    assert_eq!(floating.dimensions, [640, 480]);
    assert_eq!(floating.resize_target, Some(1));
    assert_eq!(floating.position, Some(8));
    assert_eq!(floating.offscreen, Some(2));
    assert_eq!(floating.title_bar, Some(false));
    assert_eq!(floating.closable, Some(true));
    assert_eq!(floating.resizable, Some(2));
    assert_eq!(
        floating.title.as_deref(),
        Some(["title".to_owned(), "subtitle".to_owned()].as_slice())
    );
    let best = screen.best_effort.expect("BE");
    assert!(best.window.is_none());
    assert!(best.background.is_none());
    assert!(best.floating.is_none());

    let doc = Document::new();
    insert(
        &doc,
        61,
        Value::Dict(
            Dict::new().insert(
                "MH",
                settings(
                    Dict::new().insert("W", Value::Integer(0)).insert(
                        "F",
                        floating_window(
                            Dict::new()
                                .insert("D", arr(vec![Value::Integer(1), Value::Integer(1)]))
                                .insert("TT", arr(vec![])),
                        ),
                    ),
                ),
            ),
        ),
    );
    let rendition = doc
        .rendition(&Value::Dict(
            media_rendition(playback_clip()).insert("SP", Value::Ref(61, 0)),
        ))
        .expect("indirect screen parameters");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    let floating = media
        .screen
        .expect("/SP")
        .must_honour
        .expect("MH")
        .floating
        .expect("/F");
    assert_eq!(floating.title, Some(Vec::new()), "空 /TT 数组合法");
}

#[test]
fn play_and_screen_parameters_default_to_absent() {
    let rendition = Document::new()
        .rendition(&Value::Dict(media_rendition(playback_clip())))
        .expect("rendition");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    assert!(media.play.is_none());
    assert!(media.screen.is_none());

    let rendition = Document::new()
        .rendition(&Value::Dict(
            media_rendition(playback_clip())
                .insert("P", Value::Dict(Dict::new()))
                .insert("SP", Value::Dict(Dict::new())),
        ))
        .expect("rendition");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    let play = media.play.expect("empty /P");
    assert!(play.players.is_none());
    assert!(play.must_honour.is_none());
    assert!(play.best_effort.is_none());
    let screen = media.screen.expect("empty /SP");
    assert!(screen.must_honour.is_none());
    assert!(screen.best_effort.is_none());

    let play = play_parameters(&rendition_with_play(settings(
        Dict::new()
            .insert("MH", settings(Dict::new()))
            .insert("BE", settings(Dict::new())),
    )));
    let must = play.must_honour.expect("empty MH");
    assert!(must.volume.is_none());
    assert!(must.controller.is_none());
    assert!(must.fit.is_none());
    assert!(must.duration.is_none());
    assert!(must.auto_play.is_none());
    assert!(must.repeat_count.is_none());
    assert!(play.best_effort.expect("empty BE").fit.is_none());
}

#[test]
fn malformed_play_and_screen_parameters_are_typed_errors() {
    let doc = Document::new();
    let screen_with = |entries: Dict| rendition_with_screen(settings(entries));
    for (value, message) in [
        (
            rendition_with_play(Value::Integer(1)),
            "media play parameters is not a dictionary",
        ),
        (
            rendition_with_play(settings(Dict::new().insert("Type", name("MediaClip")))),
            "/Type is MediaClip, expected MediaPlayParams",
        ),
        (
            rendition_with_play(settings(Dict::new().insert("MH", Value::Integer(1)))),
            "media play parameters /MH is not a dictionary",
        ),
        (
            rendition_with_play(settings(
                Dict::new().insert("MH", settings(Dict::new().insert("V", Value::Integer(-1)))),
            )),
            "/V must not be negative",
        ),
        (
            rendition_with_play(settings(
                Dict::new().insert("MH", settings(Dict::new().insert("RC", Value::Float(-0.5)))),
            )),
            "/RC must not be negative",
        ),
        (
            rendition_with_play(settings(Dict::new().insert(
                "MH",
                settings(
                    Dict::new().insert("D", duration("T", Some(timespan(Value::Integer(-1))))),
                ),
            ))),
            "timespan /V must not be negative",
        ),
        (
            rendition_with_play(settings(
                Dict::new().insert("MH", settings(Dict::new().insert("D", duration("T", None)))),
            )),
            "explicit media duration is missing /T",
        ),
        (
            rendition_with_play(settings(Dict::new().insert(
                "MH",
                settings(Dict::new().insert("D", Value::Dict(Dict::new()))),
            ))),
            "media duration is missing a name /S",
        ),
        (
            rendition_with_play(settings(
                Dict::new().insert(
                    "MH",
                    settings(
                        Dict::new().insert(
                            "D",
                            Value::Dict(
                                Dict::new().insert("S", name("T")).insert(
                                    "T",
                                    Value::Dict(
                                        Dict::new()
                                            .insert("S", name("X"))
                                            .insert("V", Value::Integer(1)),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
            )),
            "timespan /S is X, expected S",
        ),
        (
            rendition_with_screen(Value::Integer(1)),
            "media screen parameters is not a dictionary",
        ),
        (
            rendition_with_screen(settings(
                Dict::new().insert("Type", name("MediaPlayParams")),
            )),
            "/Type is MediaPlayParams, expected MediaScreenParams",
        ),
        (
            screen_with(Dict::new().insert("MH", settings(Dict::new().insert("B", arr(vec![]))))),
            "must contain exactly 3 numbers",
        ),
        (
            screen_with(Dict::new().insert(
                "MH",
                settings(Dict::new().insert(
                    "B",
                    arr(vec![
                        Value::Integer(0),
                        Value::Integer(1),
                        Value::Float(1.5),
                    ]),
                )),
            )),
            "entries must be within 0.0..=1.0",
        ),
        (
            screen_with(
                Dict::new().insert("MH", settings(Dict::new().insert("O", Value::Float(2.0)))),
            ),
            "/O must be within 0.0..=1.0",
        ),
        (
            screen_with(
                Dict::new().insert("MH", settings(Dict::new().insert("M", Value::Integer(7)))),
            ),
            "/M must be within 0..=6",
        ),
        (
            screen_with(
                Dict::new().insert("MH", settings(Dict::new().insert("W", Value::Integer(0)))),
            ),
            "/F is required when /W is 0",
        ),
        (
            screen_with(Dict::new().insert(
                "MH",
                settings(Dict::new().insert("W", Value::Integer(0)).insert(
                    "F",
                    floating_window(Dict::new().insert("D", arr(vec![Value::Integer(640)]))),
                )),
            )),
            "must contain exactly 2 integers",
        ),
        (
            screen_with(Dict::new().insert(
                "MH",
                settings(Dict::new().insert("W", Value::Integer(0)).insert(
                    "F",
                    floating_window(
                        Dict::new().insert("D", arr(vec![Value::Integer(-1), Value::Integer(480)])),
                    ),
                )),
            )),
            "/D entries must not be negative",
        ),
        (
            screen_with(
                Dict::new().insert(
                    "MH",
                    settings(
                        Dict::new().insert("W", Value::Integer(0)).insert(
                            "F",
                            floating_window(
                                Dict::new()
                                    .insert(
                                        "D",
                                        arr(vec![Value::Integer(640), Value::Integer(480)]),
                                    )
                                    .insert("RT", Value::Integer(4)),
                            ),
                        ),
                    ),
                ),
            ),
            "/RT must be within 0..=3",
        ),
        (
            screen_with(
                Dict::new().insert(
                    "MH",
                    settings(
                        Dict::new().insert("W", Value::Integer(0)).insert(
                            "F",
                            floating_window(
                                Dict::new()
                                    .insert(
                                        "D",
                                        arr(vec![Value::Integer(640), Value::Integer(480)]),
                                    )
                                    .insert("TT", arr(vec![Value::Integer(1)])),
                            ),
                        ),
                    ),
                ),
            ),
            "/TT entries are not text strings",
        ),
    ] {
        let error = doc.rendition(&value).expect_err("malformed parameters");
        assert!(
            error.to_string().contains(message),
            "expected {message:?} in {error}"
        );
    }
}
