//! 页面演示条目的写出 → 解析 → 回读往返：`/Dur` 停留时长与 `/Trans`
//! 页面过渡（12.4.4 Presentations，Table 30/162）。

use super::*;

/// 写入 → 重新解析：两页分别声明 `/Dur`，首页额外声明 Split 过渡；
/// 读端 `page_duration` / `page_transition` 还原秒数与过渡字典，
/// 未声明的 `/Trans` 回读为 `None`。
#[test]
fn builder_page_presentation_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, Transition, TransitionDimension, TransitionMotion,
        TransitionStyle,
    };
    use pdf_engine::core::reader::transition::{
        TransitionDimension as ReadDimension, TransitionMotion as ReadMotion,
        TransitionStyle as ReadStyle,
    };
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = builder.page(PageSize::A4);
    let p1 = builder.page(PageSize::A4);
    builder.text(p1, &Font::helvetica(12.0), 72.0, 700.0, "Second");
    builder.set_page_duration(p0, 5.0);
    builder.set_page_transition(
        p0,
        &Transition::new(TransitionStyle::Split)
            .with_duration(3.5)
            .with_dimension(TransitionDimension::Vertical)
            .with_motion(TransitionMotion::Outward),
    );
    builder.set_page_duration(p1, 2.5);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build page presentation");

    let doc = parse_buf(&buf);
    assert_eq!(doc.page_duration(0).expect("duration"), Some(5.0));
    let transition = doc
        .page_transition(0)
        .expect("transition")
        .expect("page 0 declares /Trans");
    assert_eq!(transition.style, ReadStyle::Split);
    assert_eq!(transition.duration, Some(3.5));
    assert_eq!(transition.dimension, Some(ReadDimension::Vertical));
    assert_eq!(transition.motion, Some(ReadMotion::Outward));
    assert_eq!(transition.direction, None);
    assert_eq!(transition.scale, None);
    assert_eq!(transition.border, None);

    assert_eq!(doc.page_duration(1).expect("duration"), Some(2.5));
    assert!(
        doc.page_transition(1).expect("transition").is_none(),
        "second page declares no /Trans"
    );
}
