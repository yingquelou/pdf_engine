//! media clip section 链写出与校验（13.2.6 Table 277/278）。

use super::super::support::{annot_object, assert_text_entry};
use super::super::*;
use super::support::*;
use crate::core::builder::types::text_string_value;

/// 两级 section 链：`/C` 内联最外层 section，其 `/D` 与更深的 `/D`
/// 都解析为独立间接对象，最终落在 media clip data。
#[test]
fn rendition_writes_media_clip_section_chain() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    let inner = MediaClipSection::new(
        MediaClipData::new(MediaClipSource::Url("https://example.com/clip.mp4".into()))
            .with_name("clip"),
    )
    .with_name("inner");
    let outer = MediaClipSection::new(inner).with_name("outer");
    b.add_annotation(
        p0,
        link_with(Action::Rendition(RenditionAction::play(
            screen,
            Rendition::media(outer),
        ))),
    );

    let doc = b.assemble_document().expect("assemble");
    assert_eq!(b.rendition_nums.len(), 1, "一个内联 section 首级目标");
    assert_eq!(b.rendition_targets.len(), 2, "两级 section 各一个 /D 目标");

    let action = action_dict(&annot_object(&doc, 3, 1));
    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    let outer_dict = rendition.get("C").and_then(Value::as_dict).expect("/C");
    assert_eq!(
        outer_dict.get("Type"),
        Some(&Value::Name("MediaClip".into()))
    );
    assert_eq!(outer_dict.get("S"), Some(&Value::Name("MCS".into())));
    assert_text_entry(outer_dict, "N");

    let (inner_num, _) = outer_dict
        .get("D")
        .and_then(Value::as_ref)
        .expect("/D section reference");
    assert_eq!(inner_num, b.rendition_nums[0]);
    let inner_object = doc
        .get_object(inner_num)
        .expect("inner section object")
        .expect("exists");
    let inner_dict = inner_object.value.as_dict().expect("inner section dict");
    assert_eq!(inner_dict.get("S"), Some(&Value::Name("MCS".into())));
    assert_text_entry(inner_dict, "N");

    let (data_num, _) = inner_dict
        .get("D")
        .and_then(Value::as_ref)
        .expect("/D data reference");
    assert_ne!(inner_num, data_num, "每级 section /D 目标对象号不同");
    let data_object = doc
        .get_object(data_num)
        .expect("data object")
        .expect("exists");
    let data_dict = data_object.value.as_dict().expect("data dict");
    assert_eq!(data_dict.get("S"), Some(&Value::Name("MCD".into())));
    assert_text_entry(data_dict, "N");
    let url = data_dict.get("D").and_then(Value::as_dict).expect("/D URL");
    assert_eq!(url.get("FS"), Some(&Value::Name("URL".into())));
    assert_eq!(
        url.get("F"),
        Some(&text_string_value("https://example.com/clip.mp4"))
    );

    for num in [inner_num, data_num] {
        assert_eq!(
            doc.objects_for_write()
                .iter()
                .filter(|object| object.num == num)
                .count(),
            1,
            "section 目标对象只写一次"
        );
    }
}

/// 两个注解各带一条 MCS 动作：首级目标对象号互不重叠，且分别解析到
/// 各自的 section 链。
#[test]
fn rendition_clip_sections_allocate_distinct_objects() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    let first = MediaClipSection::new(
        MediaClipData::new(MediaClipSource::Url("first.mp4".into())).with_name("first"),
    )
    .with_name("first section");
    let second = MediaClipSection::new(
        MediaClipSection::new(
            MediaClipData::new(MediaClipSource::Url("second.mp4".into())).with_name("second"),
        )
        .with_name("second inner"),
    )
    .with_name("second section");
    b.add_annotation(
        p0,
        link_with(Action::Rendition(RenditionAction::play(
            screen,
            Rendition::media(first),
        ))),
    );
    b.add_annotation(
        p0,
        link_with(Action::Rendition(RenditionAction::play(
            screen,
            Rendition::media(second),
        ))),
    );

    let doc = b.assemble_document().expect("assemble");
    assert_eq!(b.rendition_nums.len(), 2, "两条内联 section");
    assert_eq!(b.rendition_targets.len(), 3, "1 + 2 级 /D 目标");

    let first_action = action_dict(&annot_object(&doc, 3, 1));
    let second_action = action_dict(&annot_object(&doc, 3, 2));
    let first_num = first_action
        .get("R")
        .and_then(Value::as_dict)
        .and_then(|rendition| rendition.get("C"))
        .and_then(Value::as_dict)
        .and_then(|clip| clip.get("D"))
        .and_then(Value::as_ref)
        .expect("first /D")
        .0;
    let second_num = second_action
        .get("R")
        .and_then(Value::as_dict)
        .and_then(|rendition| rendition.get("C"))
        .and_then(Value::as_dict)
        .and_then(|clip| clip.get("D"))
        .and_then(Value::as_ref)
        .expect("second /D")
        .0;
    assert_eq!(first_num, b.rendition_nums[0]);
    assert_eq!(second_num, b.rendition_nums[1]);
    assert_ne!(first_num, second_num);

    let first_data = doc
        .get_object(first_num)
        .expect("first data object")
        .expect("exists");
    let first_dict = first_data.value.as_dict().expect("first clip dict");
    assert_eq!(first_dict.get("S"), Some(&Value::Name("MCD".into())));
    let first_url = first_dict
        .get("D")
        .and_then(Value::as_dict)
        .expect("first URL");
    assert_eq!(first_url.get("F"), Some(&text_string_value("first.mp4")));

    let second_section = doc
        .get_object(second_num)
        .expect("second section object")
        .expect("exists");
    let second_dict = second_section.value.as_dict().expect("second clip dict");
    assert_eq!(second_dict.get("S"), Some(&Value::Name("MCS".into())));
    let second_data_num = second_dict
        .get("D")
        .and_then(Value::as_ref)
        .expect("second /D")
        .0;
    assert_ne!(second_data_num, first_num);
    assert_ne!(second_data_num, second_num);
    let second_data = doc
        .get_object(second_data_num)
        .expect("second data object")
        .expect("exists");
    let second_data = second_data.value.as_dict().expect("second data dict");
    assert_eq!(second_data.get("S"), Some(&Value::Name("MCD".into())));
    let second_url = second_data
        .get("D")
        .and_then(Value::as_dict)
        .expect("second URL");
    assert_eq!(second_url.get("F"), Some(&text_string_value("second.mp4")));
}

/// 选择器 rendition 的多个媒体候选中混有 data 与不同深度的 section：
/// 内联 `/C` 只消费各自首级目标，写对象阶段按同一分配顺序逐级写出。
#[test]
fn rendition_clip_sections_in_selector_follow_candidate_order() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    let selector = Rendition::selector(vec![
        Rendition::media(MediaClipData::new(MediaClipSource::Url(
            "inline.mp4".into(),
        ))),
        Rendition::media(MediaClipSection::new(MediaClipData::new(
            MediaClipSource::Url("first.mp4".into()),
        ))),
        Rendition::media(
            MediaClipSection::new(
                MediaClipSection::new(MediaClipData::new(MediaClipSource::Url(
                    "second.mp4".into(),
                )))
                .with_name("inner"),
            )
            .with_name("outer"),
        ),
    ]);
    b.add_annotation(
        p0,
        link_with(Action::Rendition(RenditionAction::play(screen, selector))),
    );

    let doc = b.assemble_document().expect("assemble");
    assert_eq!(b.rendition_nums.len(), 2, "两个 section 候选");
    assert_eq!(b.rendition_targets.len(), 3, "1 + 2 级 /D 目标");

    let action = action_dict(&annot_object(&doc, 3, 1));
    let entries = action
        .get("R")
        .and_then(Value::as_dict)
        .and_then(|rendition| rendition.get("R"))
        .and_then(Value::as_array)
        .expect("selector /R");
    assert_eq!(entries.0.len(), 3);

    let inline = entries.0[0]
        .as_dict()
        .and_then(|media| media.get("C"))
        .and_then(Value::as_dict)
        .expect("inline clip");
    assert_eq!(inline.get("S"), Some(&Value::Name("MCD".into())));

    let first = entries.0[1]
        .as_dict()
        .and_then(|media| media.get("C"))
        .and_then(Value::as_dict)
        .expect("first section");
    let first_num = first.get("D").and_then(Value::as_ref).expect("first /D").0;
    assert_eq!(first_num, b.rendition_nums[0]);
    let first_data = doc
        .get_object(first_num)
        .expect("first data object")
        .expect("exists");
    assert_eq!(
        first_data.value.as_dict().and_then(|d| d.get("S")),
        Some(&Value::Name("MCD".into()))
    );

    let second = entries.0[2]
        .as_dict()
        .and_then(|media| media.get("C"))
        .and_then(Value::as_dict)
        .expect("second section");
    let second_num = second
        .get("D")
        .and_then(Value::as_ref)
        .expect("second /D")
        .0;
    assert_eq!(second_num, b.rendition_nums[1]);
    assert_ne!(first_num, second_num);
    let inner = doc
        .get_object(second_num)
        .expect("inner section object")
        .expect("exists");
    let inner_dict = inner.value.as_dict().expect("inner section dict");
    assert_eq!(inner_dict.get("S"), Some(&Value::Name("MCS".into())));
    let data_num = inner_dict
        .get("D")
        .and_then(Value::as_ref)
        .expect("inner /D")
        .0;
    assert_ne!(data_num, second_num);
    let data = doc
        .get_object(data_num)
        .expect("second data object")
        .expect("exists");
    assert_eq!(
        data.value.as_dict().and_then(|d| d.get("S")),
        Some(&Value::Name("MCD".into()))
    );
}

/// 注解先于大纲写出：内联 section 的共享游标跨两处推进，大纲动作对象
/// 的 `/D` 取到与分配顺序一致的目标号。
#[test]
fn rendition_clip_section_annotations_and_outline_share_cursor() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let screen = screen(&mut b, p0);
    let annotation_clip = MediaClipSection::new(MediaClipData::new(MediaClipSource::Url(
        "annotation.mp4".into(),
    )));
    b.add_annotation(
        p0,
        link_with(Action::Rendition(RenditionAction::play(
            screen,
            Rendition::media(annotation_clip),
        ))),
    );
    let outline_clip = MediaClipSection::new(
        MediaClipSection::new(MediaClipData::new(MediaClipSource::Url(
            "outline.mp4".into(),
        )))
        .with_name("inner"),
    )
    .with_name("outer");
    let handle = b.action(Action::Rendition(RenditionAction::play(
        screen,
        Rendition::media(outline_clip),
    )));
    b.outline(&[OutlineEntry {
        title: "clip".into(),
        destination: None,
        action: Some(handle),
        color: None,
        flags: 0,
        expanded: false,
        children: Vec::new(),
    }]);

    let mut doc = b.assemble_document().expect("assemble");
    assert_eq!(b.rendition_cursor.get(), 1, "注解已消费第一个首级目标");
    crate::core::builder::navigation::apply(
        &mut doc,
        b.version,
        b.pages.len(),
        &b.navigation,
        crate::core::builder::types::ActionRefs {
            annotation_nums: &b.annotation_nums,
            ocg_nums: &b.ocg_nums,
            sound_nums: &b.sound_nums,
            form_nums: &b.form_nums,
            embedded_nums: &b.embedded_nums,
            rendition_nums: &b.rendition_nums,
            rendition_cursor: Some(&b.rendition_cursor),
        },
    )
    .expect("apply outline");
    assert_eq!(
        b.rendition_cursor.get(),
        b.rendition_nums.len(),
        "大纲消费剩余首级目标"
    );

    let mut actions = doc.objects_for_write().into_iter().filter(|object| {
        object
            .value
            .as_dict()
            .and_then(|dict| dict.get("S"))
            .and_then(Value::as_name)
            == Some("Rendition")
    });
    let action = actions.next().expect("outline rendition action object");
    assert!(actions.next().is_none(), "大纲动作对象只写一次");
    let clip = action
        .value
        .as_dict()
        .and_then(|dict| dict.get("R"))
        .and_then(Value::as_dict)
        .and_then(|rendition| rendition.get("C"))
        .and_then(Value::as_dict)
        .expect("outline /C");
    assert_eq!(clip.get("S"), Some(&Value::Name("MCS".into())));
    let inner_num = clip.get("D").and_then(Value::as_ref).expect("outline /D").0;
    assert_eq!(inner_num, b.rendition_nums[1]);
    let inner = doc
        .get_object(inner_num)
        .expect("outline inner section")
        .expect("exists");
    assert_eq!(
        inner.value.as_dict().and_then(|dict| dict.get("S")),
        Some(&Value::Name("MCS".into()))
    );
}

/// `/N` 为空、嵌套超过读端深度上限（section 链计入最终 data clip，
/// 选择器候选逐级加一）、嵌套 data 的嵌入文件/Form 句柄越界都在任何
/// 对象写出前拒绝。
#[test]
fn rendition_clip_section_rejects_invalid_values() {
    let empty_name =
        MediaClipSection::new(MediaClipData::new(MediaClipSource::Url("clip.mp4".into())))
            .with_name("");
    let (mut b, _) = builder_with(action_with(Rendition::media(empty_name)), PdfVersion::V1_5);
    expect_rejected(&mut b, "media clip section /N must not be empty");

    let mut deep: MediaClip =
        MediaClip::Data(MediaClipData::new(MediaClipSource::Url("clip.mp4".into())));
    for _ in 0..31 {
        deep = MediaClip::Section(MediaClipSection::new(deep));
    }
    let (mut b, _) = builder_with(action_with(Rendition::media(deep)), PdfVersion::V1_5);
    expect_rejected(&mut b, "media clip nesting exceeds the depth limit");

    let mut nested_selector =
        Rendition::media(MediaClipData::new(MediaClipSource::Url("clip.mp4".into())));
    for _ in 0..32 {
        nested_selector = Rendition::selector(vec![nested_selector]);
    }
    let (mut b, _) = builder_with(action_with(nested_selector), PdfVersion::V1_5);
    expect_rejected(&mut b, "rendition nesting exceeds the depth limit");

    let embedded = MediaClipSection::new(
        MediaClipSection::new(MediaClipData::new(MediaClipSource::EmbeddedFile(3)))
            .with_name("inner"),
    )
    .with_name("outer");
    let (mut b, _) = builder_with(action_with(Rendition::media(embedded)), PdfVersion::V1_5);
    expect_rejected(&mut b, "unknown embedded file handle 3");

    let form = MediaClipSection::new(MediaClipData::new(MediaClipSource::FormXObject(7)))
        .with_name("outer");
    let (mut b, _) = builder_with(action_with(Rendition::media(form)), PdfVersion::V1_5);
    expect_rejected(&mut b, "unknown form handle 7");
}
