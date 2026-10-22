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

#[test]
fn pdf_rect_parses_and_serializes_pdf_arrays() {
    let value = Value::Array(Array(vec![
        Value::Integer(10),
        Value::Float(20.5),
        Value::Integer(30),
        Value::Float(40.25),
    ]));
    let rect = PdfRect::from_value(&value).unwrap();

    assert_eq!(rect.llx(), 10.0);
    assert_eq!(rect.lly(), 20.5);
    assert_eq!(rect.urx(), 30.0);
    assert_eq!(rect.ury(), 40.25);
    assert_eq!(
        rect.to_value(),
        Value::Array(Array(vec![
            Value::Float(10.0),
            Value::Float(20.5),
            Value::Float(30.0),
            Value::Float(40.25),
        ]))
    );
}

#[test]
fn pdf_rect_preserves_reversed_coordinates() {
    let rect = PdfRect::from_array(&Array(vec![
        Value::Integer(30),
        Value::Integer(40),
        Value::Integer(10),
        Value::Integer(20),
    ]))
    .unwrap();

    assert_eq!(
        (rect.llx(), rect.lly(), rect.urx(), rect.ury()),
        (30.0, 40.0, 10.0, 20.0)
    );
}

#[test]
fn pdf_rect_rejects_wrong_array_length() {
    let error = PdfRect::from_array(&Array(vec![
        Value::Integer(1),
        Value::Integer(2),
        Value::Integer(3),
    ]))
    .unwrap_err();

    assert_eq!(error, PdfRectError::InvalidArrayLength);
}

#[test]
fn pdf_rect_rejects_non_numeric_coordinates() {
    let error = PdfRect::from_array(&Array(vec![
        Value::Integer(1),
        Value::Name("not-a-number".to_owned()),
        Value::Integer(3),
        Value::Integer(4),
    ]))
    .unwrap_err();

    assert_eq!(error, PdfRectError::NonNumericCoordinate { index: 1 });
}

#[test]
fn pdf_text_string_decodes_ascii_and_pdf_doc_encoding() {
    assert_eq!(
        PdfTextString::decode(b"Hello PDF").unwrap().as_str(),
        "Hello PDF"
    );

    let special = PdfTextString::decode(&[0xA0, 0x80, 0x93, 0x18]).unwrap();
    assert_eq!(special.as_str(), "\u{20AC}\u{2022}\u{FB01}\u{02D8}");
    assert_eq!(special.encode(), [0xA0, 0x80, 0x93, 0x18]);
}

#[test]
fn pdf_text_string_uses_utf16be_for_unrepresentable_characters() {
    let text = PdfTextString::from("A🦀B");
    let encoded = text.encode();
    assert_eq!(
        encoded,
        [0xFE, 0xFF, 0x00, 0x41, 0xD8, 0x3E, 0xDD, 0x80, 0x00, 0x42]
    );
    assert_eq!(PdfTextString::decode(&encoded).unwrap(), text);
}

#[test]
fn pdf_text_string_avoids_pdfdocencoding_bom_prefix_ambiguity() {
    let text = PdfTextString::from("þÿ");
    let encoded = text.encode();

    assert_eq!(encoded, [0xFE, 0xFF, 0x00, 0xFE, 0x00, 0xFF]);
    assert_eq!(PdfTextString::decode(&encoded).unwrap(), text);
}

#[test]
fn pdf_text_string_rejects_undefined_pdf_doc_encoding_bytes() {
    for byte in [0x7F, 0x9F] {
        assert_eq!(
            PdfTextString::decode(&[byte]).unwrap_err(),
            PdfTextStringError::UndefinedPdfDocEncodingByte { byte }
        );
    }
}

#[test]
fn pdf_text_string_rejects_malformed_utf16be() {
    assert_eq!(
        PdfTextString::decode(&[0xFE, 0xFF, 0x00]).unwrap_err(),
        PdfTextStringError::InvalidUtf16ByteLength { length: 1 }
    );

    for (encoded, byte_index, unit) in [
        (&[0xFE, 0xFF, 0xD8, 0x00][..], 2, 0xD800),
        (&[0xFE, 0xFF, 0xDC, 0x00][..], 2, 0xDC00),
        (&[0xFE, 0xFF, 0xD8, 0x00, 0x00, 0x41][..], 4, 0x0041),
    ] {
        assert_eq!(
            PdfTextString::decode(encoded).unwrap_err(),
            PdfTextStringError::InvalidUtf16Surrogate { byte_index, unit }
        );
    }
}
