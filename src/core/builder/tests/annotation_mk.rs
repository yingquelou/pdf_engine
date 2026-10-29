//! 外观特性字典 `/MK`（12.5.2，Table 189）与图标适配字典 `/IF`（Table 247）写端：
//! `/R` 须 90 的倍数、`/BC` `/BG` 限 0/1/3/4 个 0.0–1.0 分量、`/TP` 限 0–6、
//! `/IF /A` 限两个 0.0–1.0 数值、`/I` `/RI` `/IX` 写成 Form XObject 间接引用、
//! 条目只许挂在 `/Screen` 上（Widget 端的 `/MK` 归 E4）；违反一律
//! `InvalidObject` 且零字节输出。子类型自 PDF 1.5 引入（Table 169），
//! `/MK`（1.2）与 `/IF /FB`（1.5）的版本门禁随之生效。

use super::support::annot_object;
use super::*;
use crate::core::builder::{AppearanceCharacteristics, IconFit, IconScaleWhen, IconScaling};
use crate::core::content::ContentStream;
use crate::core::types::Value;

/// 注册一个最小 Form XObject 并返回句柄。
fn form(b: &mut PdfBuilder) -> usize {
    b.form_xobject(&FormXObject::new(
        [0.0, 0.0, 10.0, 10.0],
        ContentStream::new(),
    ))
}

fn screen(rect: [f64; 4]) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Screen, rect)
}

/// 把单条 Screen 注解挂到一页文档并按指定版本写出。
fn build_one(annot: PdfAnnotation, version: PdfVersion) -> (Result<(), WriteError>, Vec<u8>) {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    let result = b.build(&mut out);
    (result, out)
}

/// 断言构建失败、消息命中且未写出任何字节。
fn expect_rejected(annot: PdfAnnotation, fragment: &str) {
    let (result, out) = build_one(annot, PdfVersion::V1_7);
    let error = result.expect_err("appearance characteristics contract violation");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains(fragment), "unexpected message: {message}");
    assert_eq!(out.len(), 0, "rejected build must not emit bytes");
}

/// 取写出注解字典里的 `/MK` 子字典。
fn written_mk(doc: &crate::core::types::Document) -> crate::core::types::Dict {
    let dict = annot_object(doc, 3, 0);
    dict.get("MK")
        .and_then(|value| value.as_dict())
        .expect("/MK dictionary")
        .clone()
}

/// Table 189 的标量条目一次性写出：`/R` `/BC` `/BG` `/CA` `/RC` `/AC` `/TP`。
#[test]
fn appearance_characteristics_writes_table_189_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        screen([72.0, 700.0, 200.0, 720.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new()
                .with_rotation(90)
                .with_border_color(vec![0.1, 0.2, 0.3])
                .with_background_color(vec![0.5])
                .with_caption("play")
                .with_rollover_caption("hover")
                .with_alternate_caption("press")
                .with_text_position(4),
        ),
    );
    let doc = b.assemble_document().expect("assemble");
    let mk = written_mk(&doc);
    assert_eq!(mk.get("R"), Some(&Value::Integer(90)));
    assert_eq!(mk.get("TP"), Some(&Value::Integer(4)));
    for key in ["BC", "BG"] {
        let array = mk.get(key).and_then(|value| value.as_array());
        assert!(array.is_some(), "/{key} array should be written");
    }
    for key in ["CA", "RC", "AC"] {
        match mk.get(key) {
            Some(Value::String(_)) | Some(Value::HexString(_)) => {}
            other => panic!("expected text string at /MK /{key}, got {other:?}"),
        }
    }
    assert!(mk.get("I").is_none() && mk.get("IF").is_none());

    // 未声明的外观特性不写 `/MK`。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, screen([72.0, 700.0, 200.0, 720.0]));
    let doc = b.assemble_document().expect("plain screen");
    let plain = annot_object(&doc, 3, 0);
    assert_eq!(plain.get("Subtype"), Some(&Value::Name("Screen".into())));
    assert!(plain.get("MK").is_none(), "未声明外观特性时不写 /MK");
}

/// `/I` `/RI` `/IX` 写成 Form XObject 间接引用，未在页面绘制的表单也有对象号。
#[test]
fn appearance_characteristics_writes_icon_form_references() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let normal = form(&mut b);
    let rollover = form(&mut b);
    let alternate = form(&mut b);
    b.add_annotation(
        p0,
        screen([72.0, 700.0, 200.0, 720.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new()
                .with_icon(normal)
                .with_rollover_icon(rollover)
                .with_alternate_icon(alternate),
        ),
    );
    let doc = b.assemble_document().expect("three icons");
    let mk = written_mk(&doc);
    for key in ["I", "RI", "IX"] {
        let (num, _) = mk
            .get(key)
            .and_then(|value| value.as_ref())
            .unwrap_or_else(|| panic!("/MK /{key} must be an indirect reference"));
        let object = doc.get_object(num).expect("object").expect("exists");
        match &object.value {
            Value::Stream(dict, _) => assert_eq!(
                dict.get("Subtype"),
                Some(&Value::Name("Form".into())),
                "/MK /{key} 指向的应是 Form XObject"
            ),
            other => panic!("/MK /{key} references a non-stream object: {other:?}"),
        }
    }
}

/// Table 247 的图标适配字典按四项写出。
#[test]
fn icon_fit_dictionary_writes_table_247_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        screen([72.0, 700.0, 200.0, 720.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new().with_icon_fit(
                IconFit::new()
                    .with_scale_when(IconScaleWhen::Smaller)
                    .with_scaling(IconScaling::Anamorphic)
                    .with_leftover(0.25, 0.75)
                    .with_fit_border(true),
            ),
        ),
    );
    let doc = b.assemble_document().expect("icon fit");
    let fit = written_mk(&doc)
        .get("IF")
        .and_then(|value| value.as_dict())
        .expect("/MK /IF dictionary")
        .clone();
    assert_eq!(fit.get("SW"), Some(&Value::Name("S".into())));
    assert_eq!(fit.get("S"), Some(&Value::Name("A".into())));
    assert_eq!(fit.get("FB"), Some(&Value::Bool(true)));
    let leftover = fit
        .get("A")
        .and_then(|value| value.as_array())
        .expect("/A array");
    assert_eq!(leftover.0.len(), 2);

    // 模式名映射可往返（写出的名字与读端 `from_name` 一致）。
    assert_eq!(IconScaleWhen::Always.name(), "A");
    assert_eq!(IconScaleWhen::Bigger.name(), "B");
    assert_eq!(IconScaleWhen::Never.name(), "N");
    assert_eq!(IconScaling::Proportional.name(), "P");
}

/// `/R` 须为 90 的倍数（Table 189）。
#[test]
fn rotation_must_be_a_multiple_of_ninety() {
    for degrees in [45_i64, -45, 100] {
        expect_rejected(
            screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
                AppearanceCharacteristics::new().with_rotation(degrees),
            ),
            "multiple of 90",
        );
    }
    // 负的多倍数按 Table 189 合法。
    let (result, _) = build_one(
        screen([0.0, 0.0, 10.0, 10.0])
            .with_appearance_characteristics(AppearanceCharacteristics::new().with_rotation(-90)),
        PdfVersion::V1_7,
    );
    result.expect("-90 degrees is a valid rotation");
}

/// `/BC` `/BG` 分量个数限 0、1、3、4，取值限 0.0–1.0。
#[test]
fn appearance_characteristics_colors_are_constrained() {
    for components in [vec![0.5, 0.5], vec![0.5, 0.5, 0.5, 0.5, 0.5]] {
        expect_rejected(
            screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
                AppearanceCharacteristics::new().with_border_color(components.clone()),
            ),
            "0, 1, 3 or 4 components",
        );
    }
    expect_rejected(
        screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new().with_background_color(vec![1.5, 0.0, 0.0]),
        ),
        "within 0.0–1.0",
    );
    expect_rejected(
        screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new().with_border_color(vec![f64::NAN, 0.0, 0.0]),
        ),
        "must be finite",
    );
    // 空数组按 Table 189 表示无色，合法。
    let (result, _) = build_one(
        screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new().with_border_color(Vec::new()),
        ),
        PdfVersion::V1_7,
    );
    result.expect("transparent border colour");
}

/// `/TP` 位置码限 0–6（Table 189）。
#[test]
fn text_position_must_be_a_valid_code() {
    expect_rejected(
        screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new().with_text_position(7),
        ),
        "/TP must be 0–6",
    );
    for position in 0..=6_u8 {
        let (result, _) = build_one(
            screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
                AppearanceCharacteristics::new().with_text_position(position),
            ),
            PdfVersion::V1_7,
        );
        result.unwrap_or_else(|error| panic!("/TP {position} must be accepted: {error}"));
    }
}

/// `/IF /A` 须是两个 0.0–1.0 数值（Table 247）。
#[test]
fn icon_fit_leftover_must_be_unit_numbers() {
    expect_rejected(
        screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new().with_icon_fit(IconFit::new().with_leftover(0.5, 1.5)),
        ),
        "/A must hold two finite numbers",
    );
    expect_rejected(
        screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new()
                .with_icon_fit(IconFit::new().with_leftover(f64::INFINITY, 0.5)),
        ),
        "/A must hold two finite numbers",
    );
}

/// 外观特性字典只许挂在 Screen 注解上（Table 187/188）。
#[test]
fn appearance_characteristics_requires_a_screen_annotation() {
    for kind in [
        AnnotationKind::Text,
        AnnotationKind::Stamp,
        AnnotationKind::Link,
    ] {
        let (result, out) = build_one(
            PdfAnnotation::new(kind, [0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
                AppearanceCharacteristics::new().with_rotation(90),
            ),
            PdfVersion::V1_7,
        );
        let error = result.expect_err("/MK on a non-Screen annotation");
        let message = error.to_string();
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "expected InvalidObject, got {message}"
        );
        assert!(message.contains("specific to Screen"), "{message}");
        assert_eq!(out.len(), 0);
    }
}

/// 图标句柄越界在写出前拒绝，消息指出 `/MK`。
#[test]
fn unknown_icon_handles_fail_the_build() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
            AppearanceCharacteristics::new().with_rollover_icon(9_999),
        ),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("unknown /MK icon handle");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(
        message.contains("/MK references unknown form handle"),
        "{message}"
    );
    assert_eq!(out.len(), 0, "rejected build must not emit bytes");
}

/// `/Screen` 自 PDF 1.5 引入（Table 169），低版本拒绝且不写 `/MK` 门禁。
#[test]
fn screen_annotations_require_pdf_1_5() {
    for version in [PdfVersion::V1_0, PdfVersion::V1_4] {
        let (result, out) = build_one(
            screen([0.0, 0.0, 10.0, 10.0]).with_appearance_characteristics(
                AppearanceCharacteristics::new().with_caption("play"),
            ),
            version,
        );
        let error = result.expect_err("screen below PDF 1.5");
        let message = error.to_string();
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "expected UnsupportedVersion, got {message}"
        );
        assert!(message.contains("require PDF 1.5"), "{message}");
        assert_eq!(out.len(), 0);
    }
    let (result, _) = build_one(
        screen([0.0, 0.0, 10.0, 10.0])
            .with_appearance_characteristics(AppearanceCharacteristics::new().with_rotation(180)),
        PdfVersion::V1_5,
    );
    result.expect("screen is writable from PDF 1.5");
}
