//! 媒体播放器 `/PL` 写出（13.2.7 Table 290–293）。

use super::super::support::annot_object;
use super::super::*;
use super::support::*;

fn one_software(software: SoftwareIdentifier) -> MediaPlayers {
    MediaPlayers::new().with_must_use(vec![MediaPlayerInfo::new(software)])
}

fn clip_with(players: MediaPlayers) -> Rendition {
    Rendition::media(
        MediaClipData::new(MediaClipSource::Url("clip.mp4".into())).with_players(players),
    )
}

#[test]
fn rendition_writes_media_players_on_clip_and_play() {
    let clip_players = MediaPlayers::new()
        .with_must_use(vec![MediaPlayerInfo::new(
            SoftwareIdentifier::new("vnd.adobe.swname:ADBE_Acrobat")
                .with_lower(vec![2, 0])
                .with_lower_inclusive(false)
                .with_upper(vec![3, 1])
                .with_upper_inclusive(false)
                .with_operating_systems(vec![b"WIN".to_vec(), b"MAC".to_vec()]),
        )])
        .with_not_used(vec![MediaPlayerInfo::new(SoftwareIdentifier::new(
            "quicktime",
        ))]);
    let play_players = MediaPlayers::new().with_any(vec![MediaPlayerInfo::new(
        SoftwareIdentifier::new("vnd.adobe.swname:ADBE_Reader").with_upper(vec![8]),
    )]);
    let rendition = Rendition::Media(Box::new(
        MediaRendition::new(
            MediaClipData::new(MediaClipSource::Url("clip.mp4".into())).with_players(clip_players),
        )
        .with_play(MediaPlayParameters::new().with_players(play_players)),
    ));
    let (mut b, _) = builder_with(action_with(rendition), PdfVersion::V1_5);

    let doc = b.assemble_document().expect("assemble");
    let action = action_dict(&annot_object(&doc, 3, 1));
    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    let clip = rendition.get("C").and_then(Value::as_dict).expect("/C");
    let players = clip.get("PL").and_then(Value::as_dict).expect("/C /PL");
    assert_eq!(
        players.get("Type"),
        Some(&Value::Name("MediaPlayers".into()))
    );
    assert!(players.get("A").is_none(), "空 /A 数组不写出");
    let must = players.get("MU").and_then(Value::as_array).expect("/MU");
    assert_eq!(must.0.len(), 1);
    let info = must.0[0].as_dict().expect("media player info");
    assert_eq!(
        info.get("Type"),
        Some(&Value::Name("MediaPlayerInfo".into()))
    );
    assert!(info.get("MH").is_none(), "/MH 无定义条目不写出");
    assert!(info.get("BE").is_none(), "/BE 无定义条目不写出");
    let pid = info.get("PID").and_then(Value::as_dict).expect("/PID");
    assert_eq!(
        pid.get("Type"),
        Some(&Value::Name("SoftwareIdentifier".into()))
    );
    assert!(
        matches!(
            pid.get("U"),
            Some(Value::HexString(_)) | Some(Value::String(_))
        ),
        "/U 以文本串写出"
    );
    assert_eq!(
        pid.get("L")
            .and_then(Value::as_array)
            .map(|array| array.0.as_slice()),
        Some([Value::Integer(2), Value::Integer(0)].as_slice())
    );
    assert_eq!(pid.get("LI"), Some(&Value::Bool(false)));
    assert_eq!(
        pid.get("H")
            .and_then(Value::as_array)
            .map(|array| array.0.as_slice()),
        Some([Value::Integer(3), Value::Integer(1)].as_slice())
    );
    assert_eq!(pid.get("HI"), Some(&Value::Bool(false)));
    assert_eq!(
        pid.get("OS")
            .and_then(Value::as_array)
            .map(|array| array.0.as_slice()),
        Some(
            [
                Value::HexString("57494E".into()),
                Value::HexString("4D4143".into())
            ]
            .as_slice()
        ),
        "/OS 以字节串写出"
    );

    let not_used = players.get("NU").and_then(Value::as_array).expect("/NU");
    assert_eq!(not_used.0.len(), 1);
    let info = not_used.0[0].as_dict().expect("not-used player");
    let pid = info.get("PID").and_then(Value::as_dict).expect("/PID");
    assert!(pid.get("L").is_none(), "未声明的 /L 不物化");
    assert!(pid.get("LI").is_none());
    assert!(pid.get("H").is_none());
    assert!(pid.get("HI").is_none());
    assert!(pid.get("OS").is_none());

    let play = rendition.get("P").and_then(Value::as_dict).expect("/P");
    let players = play.get("PL").and_then(Value::as_dict).expect("/P /PL");
    assert!(players.get("MU").is_none());
    assert!(players.get("NU").is_none());
    let any = players.get("A").and_then(Value::as_array).expect("/A");
    assert_eq!(any.0.len(), 1);
    let pid = any.0[0]
        .as_dict()
        .and_then(|info| info.get("PID"))
        .and_then(Value::as_dict)
        .expect("/PID");
    assert_eq!(
        pid.get("H")
            .and_then(Value::as_array)
            .map(|array| array.0.as_slice()),
        Some([Value::Integer(8)].as_slice())
    );
    assert!(pid.get("L").is_none(), "缺省 /L 不物化");
}

#[test]
fn rendition_media_players_reject_invalid_values() {
    let (mut b, _) = builder_with(
        action_with(clip_with(one_software(SoftwareIdentifier::new("")))),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "software identifier /U must be non-empty ASCII");

    let (mut b, _) = builder_with(
        action_with(clip_with(one_software(SoftwareIdentifier::new("软件")))),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "software identifier /U must be non-empty ASCII");

    let (mut b, _) = builder_with(
        action_with(clip_with(one_software(
            SoftwareIdentifier::new("vnd.adobe.swname:ADBE_Acrobat").with_lower(vec![-1]),
        ))),
        PdfVersion::V1_7,
    );
    expect_rejected(
        &mut b,
        "software identifier /L entries must not be negative",
    );

    let (mut b, _) = builder_with(
        action_with(clip_with(one_software(
            SoftwareIdentifier::new("vnd.adobe.swname:ADBE_Acrobat")
                .with_lower(vec![2])
                .with_upper(vec![1]),
        ))),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "software identifier /L must not exceed /H");

    let (mut b, _) = builder_with(
        action_with(with_play(
            MediaPlayParameters::new().with_players(one_software(
                SoftwareIdentifier::new("vnd.adobe.swname:ADBE_Acrobat")
                    .with_operating_systems(vec![b"WIN".to_vec(), b"WIN".to_vec()]),
            )),
        )),
        PdfVersion::V1_7,
    );
    expect_rejected(&mut b, "software identifier /OS entries must be unique");
}
