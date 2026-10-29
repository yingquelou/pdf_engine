//! E2 外观特性字典：Table 189 的 `/MK` 条目与 Table 247 的图标适配字典 `/IF`
//! 写出 → 解析往返（挂在 Screen 注解上）。

use super::*;

/// 写入 → 重新解析：`/R` `/BC` `/BG` `/CA` `/RC` `/AC` `/TP` 与 `/IF`
/// 回读为读端类型化字段。
#[test]
fn builder_appearance_characteristics_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, AppearanceCharacteristics, IconFit as BuilderIconFit,
        IconScaleWhen as BuilderIconScaleWhen, IconScaling as BuilderIconScaling, PageSize,
        PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::annotation::{AnnotationSubtype, IconScaleWhen, IconScaling};

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Screen, [72.0, 700.0, 200.0, 720.0])
            .with_appearance_characteristics(
                AppearanceCharacteristics::new()
                    .with_rotation(270)
                    .with_border_color(vec![0.1, 0.2, 0.3])
                    .with_background_color(vec![0.5])
                    .with_caption("play")
                    .with_rollover_caption("hover")
                    .with_alternate_caption("press")
                    .with_text_position(4)
                    .with_icon_fit(
                        BuilderIconFit::new()
                            .with_scale_when(BuilderIconScaleWhen::Smaller)
                            .with_scaling(BuilderIconScaling::Anamorphic)
                            .with_leftover(0.25, 0.75)
                            .with_fit_border(true),
                    ),
            ),
    );
    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build screen with /MK entries");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(annots.len(), 1);
    let screen = &annots[0];
    assert_eq!(screen.subtype, AnnotationSubtype::Screen);
    let mk = screen
        .appearance_characteristics
        .as_ref()
        .expect("/MK 解析为外观特性字典");
    assert_eq!(mk.rotation, Some(270));
    assert_eq!(mk.border_color, Some(vec![0.1, 0.2, 0.3]));
    assert_eq!(mk.background_color, Some(vec![0.5]));
    assert_eq!(mk.caption.as_deref(), Some("play"));
    assert_eq!(mk.rollover_caption.as_deref(), Some("hover"));
    assert_eq!(mk.alternate_caption.as_deref(), Some("press"));
    assert_eq!(mk.text_position, Some(4));
    let icon_fit = mk.icon_fit.as_ref().expect("/IF 图标适配字典");
    assert_eq!(icon_fit.scale_when, Some(IconScaleWhen::Smaller));
    assert_eq!(icon_fit.scaling, Some(IconScaling::Anamorphic));
    assert_eq!(icon_fit.leftover, Some([0.25, 0.75]));
    assert_eq!(icon_fit.fit_border, Some(true));
}

/// 写入 → 重新解析：`/I` `/RI` `/IX` 回读为间接引用并指向写出的 Form XObject，
/// 三个图标表单未被页面绘制仍随文档写出。
#[test]
fn builder_appearance_characteristic_icons_reference_form_xobjects() {
    use pdf_engine::core::builder::{
        AnnotationKind, AppearanceCharacteristics, FormXObject, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let make_form = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ));
    let hover_form = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ));
    let press_form = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Screen, [72.0, 700.0, 200.0, 720.0])
            .with_appearance_characteristics(
                AppearanceCharacteristics::new()
                    .with_icon(make_form)
                    .with_rollover_icon(hover_form)
                    .with_alternate_icon(press_form),
            ),
    );
    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build screen with /MK icon forms");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    let mk = annots[0]
        .appearance_characteristics
        .as_ref()
        .expect("/MK 解析为外观特性字典");
    let mut numbers = Vec::new();
    for icon in [&mk.icon, &mk.rollover_icon, &mk.alternate_icon] {
        let (num, _) = icon
            .as_ref()
            .and_then(|value| value.as_ref())
            .expect("图标须写成间接引用");
        let object = doc.get_object(num).expect("object").expect("exists");
        match &object.value {
            Value::Stream(dict, _) => assert_eq!(
                dict.get("Subtype"),
                Some(&Value::Name("Form".into())),
                "/MK 图标须指向 Form XObject"
            ),
            other => panic!("/MK icon references a non-stream object: {other:?}"),
        }
        numbers.push(num);
    }
    numbers.sort_unstable();
    numbers.dedup();
    assert_eq!(numbers.len(), 3, "三个图标各自引用一个 Form XObject");
}
