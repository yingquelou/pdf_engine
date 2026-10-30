//! 媒体播放器 `/PL` 读端测试（13.2.7 Table 290–293）。

use super::*;

fn software(entries: Dict) -> Value {
    Value::Dict(entries.insert("Type", name("SoftwareIdentifier")))
}

fn player(entries: Dict) -> Value {
    Value::Dict(entries.insert("Type", name("MediaPlayerInfo")))
}

fn player_with_software(software: Value) -> Value {
    player(Dict::new().insert("PID", software))
}

fn players(entries: Dict) -> Value {
    Value::Dict(entries.insert("Type", name("MediaPlayers")))
}

fn players_with_must_use(entry: Value) -> Value {
    players(Dict::new().insert("MU", arr(vec![entry])))
}

fn software_uri() -> Value {
    Value::String("vnd.adobe.swname:ADBE_Acrobat".into())
}

fn with_play_players(players: Value) -> Value {
    Value::Dict(
        media_rendition(playback_clip())
            .insert("P", Value::Dict(Dict::new().insert("PL", players))),
    )
}

fn with_clip_players(players: Value) -> Value {
    Value::Dict(media_rendition(Value::Dict(
        clip_data(Value::Dict(filespec("clip.mov"))).insert("PL", players),
    )))
}

#[test]
fn parses_media_players_in_clip_data_and_play_parameters() {
    let doc = Document::new();
    insert(
        &doc,
        69,
        Value::String("vnd.adobe.swname:ADBE_Acrobat".into()),
    );
    insert(
        &doc,
        70,
        Value::Dict(
            Dict::new()
                .insert("Type", name("SoftwareIdentifier"))
                .insert("U", Value::Ref(69, 0))
                .insert("L", arr(vec![Value::Integer(2), Value::Integer(0)]))
                .insert("LI", Value::Bool(false))
                .insert("H", arr(vec![Value::Integer(3), Value::Integer(1)]))
                .insert("HI", Value::Bool(false))
                .insert(
                    "OS",
                    arr(vec![
                        Value::String("WIN".into()),
                        Value::HexString("4D4143".into()),
                    ]),
                ),
        ),
    );
    insert(
        &doc,
        71,
        Value::Dict(
            Dict::new()
                .insert("Type", name("MediaPlayerInfo"))
                .insert("PID", Value::Ref(70, 0))
                .insert(
                    "MH",
                    Value::Dict(Dict::new().insert("X", Value::Integer(1))),
                )
                .insert(
                    "BE",
                    Value::Dict(Dict::new().insert("Y", Value::Bool(true))),
                ),
        ),
    );
    insert(
        &doc,
        72,
        Value::Dict(
            Dict::new()
                .insert("Type", name("SoftwareIdentifier"))
                .insert("U", Value::String("quicktime".into()))
                .insert("OS", arr(vec![])),
        ),
    );
    insert(
        &doc,
        73,
        Value::Dict(
            Dict::new()
                .insert("Type", name("MediaPlayerInfo"))
                .insert("PID", Value::Ref(72, 0)),
        ),
    );
    insert(
        &doc,
        74,
        Value::Dict(
            Dict::new()
                .insert("Type", name("SoftwareIdentifier"))
                .insert("U", Value::String("vlc://player".into()))
                .insert("L", arr(vec![])),
        ),
    );
    insert(
        &doc,
        75,
        Value::Dict(
            Dict::new()
                .insert("Type", name("MediaPlayerInfo"))
                .insert("PID", Value::Ref(74, 0)),
        ),
    );
    insert(
        &doc,
        76,
        players(
            Dict::new()
                .insert("MU", arr(vec![Value::Ref(71, 0), Value::Ref(73, 0)]))
                .insert("A", arr(vec![]))
                .insert("NU", arr(vec![Value::Ref(75, 0)])),
        ),
    );
    insert(
        &doc,
        77,
        Value::Dict(
            Dict::new()
                .insert("Type", name("SoftwareIdentifier"))
                .insert("U", Value::String("vnd.adobe.swname:ADBE_Reader".into()))
                .insert("H", arr(vec![Value::Integer(8)])),
        ),
    );
    insert(
        &doc,
        78,
        Value::Dict(
            Dict::new()
                .insert("Type", name("MediaPlayerInfo"))
                .insert("PID", Value::Ref(77, 0)),
        ),
    );
    insert(
        &doc,
        79,
        players(Dict::new().insert("A", arr(vec![Value::Ref(78, 0)]))),
    );
    insert(
        &doc,
        80,
        Value::Dict(clip_data(Value::Dict(filespec("clip.mov"))).insert("PL", Value::Ref(76, 0))),
    );
    insert(
        &doc,
        81,
        Value::Dict(
            Dict::new()
                .insert("Type", name("MediaPlayParams"))
                .insert("PL", Value::Ref(79, 0)),
        ),
    );
    insert(
        &doc,
        82,
        Value::Dict(media_rendition(Value::Ref(80, 0)).insert("P", Value::Ref(81, 0))),
    );

    let rendition = doc.rendition(&Value::Ref(82, 0)).expect("rendition");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    let MediaClip::Data(data) = media.clip.expect("/C") else {
        panic!("expected media clip data");
    };
    let players = data.players.expect("/PL");
    assert_eq!(players.must_use.len(), 2);
    assert!(players.any.is_empty(), "空 /A 数组保持为空");
    assert_eq!(players.not_used.len(), 1);

    let detailed = &players.must_use[0];
    assert_eq!(detailed.software.uri, "vnd.adobe.swname:ADBE_Acrobat");
    assert_eq!(detailed.software.lower, Some(vec![2, 0]));
    assert_eq!(detailed.software.lower_inclusive, Some(false));
    assert_eq!(detailed.software.upper, Some(vec![3, 1]));
    assert_eq!(detailed.software.upper_inclusive, Some(false));
    assert_eq!(
        detailed.software.operating_systems,
        Some(vec![b"WIN".to_vec(), b"MAC".to_vec()])
    );
    assert_eq!(
        detailed.mh,
        Some(Value::Dict(Dict::new().insert("X", Value::Integer(1))))
    );
    assert_eq!(
        detailed.be,
        Some(Value::Dict(Dict::new().insert("Y", Value::Bool(true))))
    );

    let minimal = &players.must_use[1];
    assert_eq!(minimal.software.uri, "quicktime");
    assert_eq!(minimal.software.lower, None, "缺省 /L 不物化");
    assert_eq!(minimal.software.lower_inclusive, None);
    assert_eq!(minimal.software.upper, None);
    assert_eq!(minimal.software.upper_inclusive, None);
    assert_eq!(
        minimal.software.operating_systems,
        Some(Vec::new()),
        "显式空 /OS 数组保留"
    );
    assert!(minimal.mh.is_none());
    assert!(minimal.be.is_none());

    let third = &players.not_used[0];
    assert_eq!(third.software.uri, "vlc://player");
    assert_eq!(
        third.software.lower,
        Some(Vec::new()),
        "空 /L 数组表示无穷大"
    );

    let play = media.play.expect("/P");
    let play_players = play.players.expect("/PL");
    assert!(play_players.must_use.is_empty());
    assert!(play_players.not_used.is_empty());
    assert_eq!(play_players.any.len(), 1);
    let any = &play_players.any[0];
    assert_eq!(any.software.uri, "vnd.adobe.swname:ADBE_Reader");
    assert_eq!(any.software.upper, Some(vec![8]));
    assert_eq!(any.software.lower, None);
    assert!(any.mh.is_none());
}

#[test]
fn odd_length_hex_operating_systems_are_padded() {
    let doc = Document::new();
    let value = with_play_players(players_with_must_use(player_with_software(software(
        Dict::new()
            .insert("U", software_uri())
            .insert("OS", arr(vec![Value::HexString("57494".into())])),
    ))));
    let rendition = doc.rendition(&value).expect("rendition");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    let players = media.play.expect("/P").players.expect("/PL");
    assert_eq!(
        players.must_use[0].software.operating_systems,
        Some(vec![b"WI@".to_vec()]),
        "奇数位十六进制末位补 0"
    );
}

#[test]
fn malformed_media_players_are_typed_errors() {
    let doc = Document::new();
    for (value, message) in [
        (
            with_clip_players(Value::Integer(1)),
            "media players is not a dictionary",
        ),
        (
            with_play_players(Value::Dict(
                Dict::new().insert("Type", name("MediaPlayParams")),
            )),
            "media players /Type is MediaPlayParams, expected MediaPlayers",
        ),
        (
            with_play_players(players(Dict::new().insert("MU", Value::Integer(1)))),
            "media players /MU is not an array",
        ),
        (
            with_play_players(players_with_must_use(Value::Integer(1))),
            "media player info is not a dictionary",
        ),
        (
            with_play_players(players_with_must_use(player(Dict::new()))),
            "media player info is missing required /PID",
        ),
        (
            with_play_players(players_with_must_use(player_with_software(Value::Integer(
                1,
            )))),
            "software identifier is not a dictionary",
        ),
        (
            with_play_players(players_with_must_use(player_with_software(software(
                Dict::new(),
            )))),
            "software identifier is missing required /U",
        ),
        (
            with_play_players(players_with_must_use(player_with_software(software(
                Dict::new().insert("U", Value::String("软件".into())),
            )))),
            "software identifier /U is not ASCII",
        ),
        (
            with_play_players(players_with_must_use(player_with_software(software(
                Dict::new()
                    .insert("U", software_uri())
                    .insert("L", Value::Integer(1)),
            )))),
            "software identifier /L is not an array",
        ),
        (
            with_play_players(players_with_must_use(player_with_software(software(
                Dict::new()
                    .insert("U", software_uri())
                    .insert("L", arr(vec![Value::Name("two".into())])),
            )))),
            "software identifier /L entries are not integers",
        ),
        (
            with_play_players(players_with_must_use(player_with_software(software(
                Dict::new()
                    .insert("U", software_uri())
                    .insert("L", arr(vec![Value::Integer(-1)])),
            )))),
            "software identifier /L entries must not be negative",
        ),
        (
            with_play_players(players_with_must_use(player_with_software(software(
                Dict::new()
                    .insert("U", software_uri())
                    .insert("LI", Value::Integer(1)),
            )))),
            "software identifier /LI is not a boolean",
        ),
        (
            with_play_players(players_with_must_use(player_with_software(software(
                Dict::new()
                    .insert("U", software_uri())
                    .insert("OS", arr(vec![Value::Integer(1)])),
            )))),
            "software identifier /OS entries are not byte strings",
        ),
    ] {
        let error = doc.rendition(&value).expect_err("malformed media players");
        assert!(
            error.to_string().contains(message),
            "expected {message:?} in {error}"
        );
    }
}
