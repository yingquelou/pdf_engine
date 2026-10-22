//! types 模块测试。

use super::*;

/// 能力模型：各特性的最低版本阈值边界（引入版本前不支持、
/// 引入版本起支持），覆盖 `PdfFeature::min_version` 全部分支。
#[test]
fn feature_thresholds_match_iso_32000() {
    // XRefStream / ObjectStream：1.5 引入，4/1.5 边界
    assert!(!PdfVersion::V1_4.supports(PdfFeature::XRefStream));
    assert!(PdfVersion::V1_5.supports(PdfFeature::XRefStream));
    assert!(!PdfVersion::V1_4.supports(PdfFeature::ObjectStream));
    assert!(PdfVersion::V1_5.supports(PdfFeature::ObjectStream));

    // JBIG2Decode / Transparency：1.4 引入
    assert!(!PdfVersion::V1_3.supports(PdfFeature::JBIG2Decode));
    assert!(PdfVersion::V1_4.supports(PdfFeature::JBIG2Decode));
    assert!(!PdfVersion::V1_3.supports(PdfFeature::Transparency));
    assert!(PdfVersion::V1_4.supports(PdfFeature::Transparency));

    // JPXDecode / OptionalContent：1.5 引入
    assert!(!PdfVersion::V1_4.supports(PdfFeature::JPXDecode));
    assert!(PdfVersion::V1_5.supports(PdfFeature::JPXDecode));
    assert!(!PdfVersion::V1_4.supports(PdfFeature::OptionalContent));
    assert!(PdfVersion::V1_7.supports(PdfFeature::OptionalContent));

    // EncryptionRC4：1.1 引入
    assert!(!PdfVersion::V1_0.supports(PdfFeature::EncryptionRC4));
    assert!(PdfVersion::V1_1.supports(PdfFeature::EncryptionRC4));

    // 最低版本与 supports 一一对应
    assert_eq!(PdfFeature::XRefStream.min_version(), PdfVersion::V1_5);
    assert_eq!(PdfFeature::ObjectStream.min_version(), PdfVersion::V1_5);
    assert_eq!(PdfFeature::EncryptionRC4.min_version(), PdfVersion::V1_1);
    assert_eq!(PdfFeature::JBIG2Decode.min_version(), PdfVersion::V1_4);
    assert_eq!(PdfFeature::JPXDecode.min_version(), PdfVersion::V1_5);
    assert_eq!(PdfFeature::Transparency.min_version(), PdfVersion::V1_4);
    assert_eq!(PdfFeature::OptionalContent.min_version(), PdfVersion::V1_5);
}

/// 便捷包装必须与能力模型一致；2.0 支持全部已建模特性。
#[test]
fn convenience_wrappers_track_capability_model() {
    for v in [
        PdfVersion::V1_0,
        PdfVersion::V1_3,
        PdfVersion::V1_4,
        PdfVersion::V1_5,
        PdfVersion::V1_6,
        PdfVersion::V1_7,
    ] {
        assert_eq!(
            v.supports_xref_streams(),
            v.supports(PdfFeature::XRefStream)
        );
        assert_eq!(
            v.supports_object_streams(),
            v.supports(PdfFeature::ObjectStream)
        );
    }
    // 2.0 高于所有已建模特性的最低版本
    assert!(PdfVersion::V2_0.supports(PdfFeature::XRefStream));
    assert!(PdfVersion::V2_0.supports(PdfFeature::ObjectStream));
    assert!(PdfVersion::V2_0.supports(PdfFeature::EncryptionRC4));
    assert!(PdfVersion::V2_0.supports(PdfFeature::JBIG2Decode));
    assert!(PdfVersion::V2_0.supports(PdfFeature::JPXDecode));
    assert!(PdfVersion::V2_0.supports(PdfFeature::Transparency));
    assert!(PdfVersion::V2_0.supports(PdfFeature::OptionalContent));
}

/// 版本解析：识别 2.0，拒绝 2.1/3.0 等。
#[test]
fn version_parsing_recognizes_2_0() {
    assert_eq!(PdfVersion::from_name("2.0"), Some(PdfVersion::V2_0));
    assert_eq!(PdfVersion::parse(b"%PDF-2.0"), Some(PdfVersion::V2_0));
    assert_eq!(PdfVersion::from_name("1.7"), Some(PdfVersion::V1_7));
    assert_eq!(PdfVersion::from_name("1.0"), Some(PdfVersion::V1_0));
    // 超出范围的不识别
    assert_eq!(PdfVersion::from_name("2.1"), None);
    assert_eq!(PdfVersion::from_name("3.0"), None);
    assert_eq!(PdfVersion::from_name("1.8"), None);
}

/// 派生 `Ord`：`V1_7 < V2_0`，且默认版本为 1.7。
#[test]
fn version_ordering_and_default() {
    assert!(PdfVersion::V1_7 < PdfVersion::V2_0);
    assert!(PdfVersion::V2_0 > PdfVersion::V1_7);
    assert_eq!(PdfVersion::default(), PdfVersion::V1_7);
}

#[test]
fn pdf_date_parses_partial_fields_and_timezone_forms() {
    let date: PdfDate = "D:2024".parse().unwrap();
    assert_eq!(date.year(), 2024);
    assert_eq!(date.month(), None);
    assert_eq!(date.timezone(), PdfDateTimezone::Local);
    assert_eq!(date.to_pdf_string(), "D:2024");

    let date = PdfDate::parse("D:20240229").unwrap();
    assert_eq!(date.month(), Some(2));
    assert_eq!(date.day(), Some(29));
    assert_eq!(date.to_pdf_string(), "D:20240229");

    for (input, timezone) in [
        ("D:20240102030405Z", PdfDateTimezone::Utc),
        (
            "D:20240102030405+05'30'",
            PdfDateTimezone::Offset {
                positive: true,
                hours: 5,
                minutes: 30,
            },
        ),
        (
            "D:20240102030405-08'00'",
            PdfDateTimezone::Offset {
                positive: false,
                hours: 8,
                minutes: 0,
            },
        ),
    ] {
        let date = PdfDate::parse(input).unwrap();
        assert_eq!(date.timezone(), timezone);
        assert_eq!(date.to_string(), input);
    }
}

#[test]
fn pdf_date_rejects_invalid_components_and_syntax() {
    for input in [
        "",
        "20240101",
        "D:24",
        "D:2024x1",
        "D:202413",
        "D:20240230",
        "D:20230229",
        "D:2024010124",
        "D:202401010160",
        "D:20240101000060",
        "D:20240101000000+24'00'",
        "D:20240101000000+01'60'",
        "D:20240101000000+0100",
        "D:20240101000000z",
        "D:20240101000000Zx",
    ] {
        assert!(PdfDate::parse(input).is_err(), "accepted {input:?}");
    }
}
