//! OPI 代理字典读端测试（ISO 32000-1 14.11.7 Table 368–370）。

use crate::core::reader::action::FileSpec;
use crate::core::reader::image::ImageModel;
use crate::core::reader::opi::{
    OpiColor, OpiColorType, OpiError, OpiInks, OpiProxy, OpiTagText, OpiVersion,
};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

/// 以版本名与内层字典构造 `/OPI` 条目的值。
fn opi_entry(version: &str, inner: Dict) -> Value {
    Value::Dict(Dict::new().insert(version, Value::Dict(inner)))
}

fn opi_dict(version: &str, inner: Dict) -> Dict {
    Dict::new().insert("OPI", opi_entry(version, inner))
}

fn document_with(index: i64, dict: Dict) -> Document {
    let document = Document::new();
    document.set_object(Object::new(
        index,
        0,
        Value::Stream(dict, StreamData::Owned(Vec::new())),
    ));
    document
}

fn minimal_v13() -> Dict {
    Dict::new()
        .insert("Version", Value::Float(1.3))
        .insert("F", Value::String("highres.tif".into()))
        .insert("Size", arr(vec![Value::Integer(100), Value::Integer(200)]))
        .insert(
            "CropRect",
            arr(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(90),
                Value::Integer(180),
            ]),
        )
        .insert(
            "Position",
            arr(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(10),
                Value::Integer(0),
                Value::Integer(10),
                Value::Integer(10),
                Value::Integer(0),
                Value::Integer(10),
            ]),
        )
}

#[test]
fn opi_reads_v13_entries_and_defaults() {
    let full = minimal_v13()
        .insert("Type", name("OPI"))
        .insert("ID", Value::HexString("00FF".into()))
        .insert("Comments", Value::String("proxy comment".into()))
        .insert(
            "CropFixed",
            arr(vec![
                Value::Float(1.5),
                Value::Float(2.5),
                Value::Float(89.5),
                Value::Float(179.5),
            ]),
        )
        .insert(
            "Resolution",
            arr(vec![Value::Float(300.0), Value::Float(300.0)]),
        )
        .insert("ColorType", name("Process"))
        .insert(
            "Color",
            arr(vec![
                Value::Float(0.1),
                Value::Float(0.2),
                Value::Float(0.3),
                Value::Float(0.4),
                Value::HexString("4D6167656E7461".into()),
            ]),
        )
        .insert("Tint", Value::Float(0.5))
        .insert("Overprint", Value::Bool(true))
        .insert("ImageType", arr(vec![Value::Integer(8), Value::Integer(2)]))
        .insert(
            "GrayMap",
            arr(vec![
                Value::Integer(0),
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(65535),
            ]),
        )
        .insert("Transparency", Value::Bool(false))
        .insert(
            "Tags",
            arr(vec![
                Value::Integer(1),
                Value::String("tag-a".into()),
                Value::Integer(2),
                Value::String("tag-b".into()),
            ]),
        );
    let document = document_with(20, opi_dict("1.3", full));
    let proxy = document
        .xobject_opi(&Value::Ref(20, 0))
        .expect("OPI")
        .expect("present");
    assert_eq!(proxy.version(), OpiVersion::V1_3);
    let OpiProxy::V1_3(dictionary) = &proxy else {
        panic!("expected OPI 1.3");
    };
    assert_eq!(dictionary.type_entry.as_deref(), Some("OPI"));
    assert_eq!(dictionary.version, 1.3);
    assert_eq!(
        dictionary.file,
        FileSpec {
            file: "highres.tif".into()
        }
    );
    assert_eq!(dictionary.id.as_deref(), Some(&[0_u8, 0xFF][..]));
    assert_eq!(dictionary.comments.as_deref(), Some("proxy comment"));
    assert_eq!(dictionary.size, [100, 200]);
    assert_eq!(dictionary.crop_rect, [1, 2, 90, 180]);
    assert_eq!(dictionary.crop_fixed, Some([1.5, 2.5, 89.5, 179.5]));
    assert_eq!(
        dictionary.position,
        [0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0]
    );
    assert_eq!(dictionary.resolution, Some([300.0, 300.0]));
    assert_eq!(dictionary.color_type, Some(OpiColorType::Process));
    assert_eq!(
        dictionary.color,
        Some(OpiColor {
            cmyk: [0.1, 0.2, 0.3, 0.4],
            color_name: b"Magenta".to_vec(),
        })
    );
    assert_eq!(dictionary.tint, Some(0.5));
    assert_eq!(dictionary.overprint, Some(true));
    assert_eq!(dictionary.image_type, Some([8, 2]));
    assert_eq!(dictionary.gray_map.as_deref(), Some(&[0, 1, 2, 65535][..]));
    assert_eq!(dictionary.transparency, Some(false));
    assert_eq!(
        dictionary.tags,
        vec![(1, "tag-a".to_string()), (2, "tag-b".to_string())]
    );

    // 省略的可选条目保持 `None`，不物化规范默认值。
    let minimal = document_with(21, opi_dict("1.3", minimal_v13()));
    let Some(OpiProxy::V1_3(dictionary)) = minimal.xobject_opi(&Value::Ref(21, 0)).expect("OPI")
    else {
        panic!("expected OPI 1.3");
    };
    assert_eq!(dictionary.type_entry, None);
    assert_eq!(dictionary.id, None);
    assert_eq!(dictionary.comments, None);
    assert_eq!(dictionary.crop_fixed, None);
    assert_eq!(dictionary.resolution, None);
    assert_eq!(dictionary.color_type, None);
    assert_eq!(dictionary.color, None);
    assert_eq!(dictionary.tint, None);
    assert_eq!(dictionary.overprint, None);
    assert_eq!(dictionary.image_type, None);
    assert_eq!(dictionary.gray_map, None);
    assert_eq!(dictionary.transparency, None);
    assert!(dictionary.tags.is_empty());
}

#[test]
fn opi_reads_v20_entries() {
    let full = Dict::new()
        .insert("Type", name("OPI"))
        .insert("Version", Value::Integer(2))
        .insert("F", Value::String("proxy.jpg".into()))
        .insert("MainImage", Value::HexString("6D61696E2E746966".into()))
        .insert(
            "Tags",
            arr(vec![
                Value::Integer(7),
                Value::String("single".into()),
                Value::Integer(9),
                arr(vec![
                    Value::String("first".into()),
                    Value::String("second".into()),
                ]),
            ]),
        )
        .insert("Size", arr(vec![Value::Float(200.0), Value::Float(100.0)]))
        .insert(
            "CropRect",
            arr(vec![
                Value::Float(10.0),
                Value::Float(20.0),
                Value::Float(190.0),
                Value::Float(90.0),
            ]),
        )
        .insert("Overprint", Value::Bool(true))
        .insert(
            "Inks",
            arr(vec![
                name("monochrome"),
                Value::HexString("4379616E".into()),
                Value::Float(0.5),
                Value::HexString("4D6167656E7461".into()),
                Value::Float(1.0),
            ]),
        )
        .insert(
            "IncludedImageDimensions",
            arr(vec![Value::Integer(4000), Value::Integer(2000)]),
        )
        .insert("IncludedImageQuality", Value::Integer(3));
    let document = document_with(30, opi_dict("2.0", full));
    let proxy = document
        .xobject_opi(&Value::Ref(30, 0))
        .expect("OPI")
        .expect("present");
    assert_eq!(proxy.version(), OpiVersion::V2_0);
    let OpiProxy::V2_0(dictionary) = &proxy else {
        panic!("expected OPI 2.0");
    };
    assert_eq!(dictionary.type_entry.as_deref(), Some("OPI"));
    assert_eq!(dictionary.version, 2.0);
    assert_eq!(
        dictionary.file,
        FileSpec {
            file: "proxy.jpg".into()
        }
    );
    assert_eq!(
        dictionary.main_image.as_deref(),
        Some(b"main.tif".as_slice())
    );
    assert_eq!(
        dictionary.tags,
        vec![
            (7, OpiTagText::Text("single".into())),
            (9, OpiTagText::Texts(vec!["first".into(), "second".into()])),
        ]
    );
    assert_eq!(dictionary.size, Some([200.0, 100.0]));
    assert_eq!(dictionary.crop_rect, Some([10.0, 20.0, 190.0, 90.0]));
    assert_eq!(dictionary.overprint, Some(true));
    assert_eq!(
        dictionary.inks,
        Some(OpiInks::Monochrome(vec![
            (b"Cyan".to_vec(), 0.5),
            (b"Magenta".to_vec(), 1.0),
        ]))
    );
    assert_eq!(dictionary.included_image_dimensions, Some([4000, 2000]));
    assert_eq!(dictionary.included_image_quality, Some(3));

    // Size/CropRect 成对省略，其余可选条目不物化。
    let minimal = document_with(
        31,
        opi_dict(
            "2.0",
            Dict::new()
                .insert("Version", Value::Float(2.0))
                .insert("F", Value::String("proxy.jpg".into())),
        ),
    );
    let Some(OpiProxy::V2_0(dictionary)) = minimal.xobject_opi(&Value::Ref(31, 0)).expect("OPI")
    else {
        panic!("expected OPI 2.0");
    };
    assert_eq!(dictionary.size, None);
    assert_eq!(dictionary.crop_rect, None);
    assert_eq!(dictionary.overprint, None);
    assert_eq!(dictionary.inks, None);
    assert_eq!(dictionary.included_image_dimensions, None);
    assert_eq!(dictionary.included_image_quality, None);
    assert!(dictionary.tags.is_empty());
}

#[test]
fn image_model_exposes_opi_proxy() {
    let dict = Dict::new()
        .insert("Type", name("XObject"))
        .insert("Subtype", name("Image"))
        .insert("Width", Value::Integer(1))
        .insert("Height", Value::Integer(1))
        .insert("ColorSpace", name("DeviceGray"))
        .insert("BitsPerComponent", Value::Integer(8))
        .insert("OPI", opi_entry("1.3", minimal_v13()));
    let document = Document::new();
    document.set_object(Object::new(
        40,
        0,
        Value::Stream(dict, StreamData::Owned(vec![0_u8])),
    ));
    let image: ImageModel = document.image(&Value::Ref(40, 0)).expect("image");
    let opi = image.opi.expect("OPI on image model");
    assert_eq!(opi.version(), OpiVersion::V1_3);
    let OpiProxy::V1_3(dictionary) = opi else {
        panic!("expected OPI 1.3");
    };
    assert_eq!(dictionary.size, [100, 200]);
}

#[test]
fn malformed_opi_entries_are_typed_errors() {
    fn error(document: &Document, index: i64) -> OpiError {
        document
            .xobject_opi(&Value::Ref(index, 0))
            .expect_err("expected error")
    }

    // 版本字典必须恰好一个条目。
    let two = Dict::new().insert(
        "OPI",
        Value::Dict(
            Dict::new()
                .insert("1.3", Value::Dict(minimal_v13()))
                .insert(
                    "2.0",
                    Value::Dict(
                        Dict::new()
                            .insert("Version", Value::Float(2.0))
                            .insert("F", Value::String("proxy.jpg".into())),
                    ),
                ),
        ),
    );
    match error(&document_with(50, two), 50) {
        OpiError::Malformed(message) => assert!(message.contains("exactly one entry"), "{message}"),
        other => panic!("expected Malformed, got {other:?}"),
    }

    // 未知版本键。
    let unknown = opi_dict("1.2", minimal_v13());
    match error(&document_with(51, unknown), 51) {
        OpiError::Malformed(message) => {
            assert!(message.contains("must be /1.3 or /2.0"), "{message}");
        }
        other => panic!("expected Malformed, got {other:?}"),
    }

    // 1.3 缺少必填 /F。
    let mut missing = minimal_v13();
    missing.0.remove("F");
    match error(&document_with(52, opi_dict("1.3", missing)), 52) {
        OpiError::Malformed(message) => assert!(message.contains("/F is required"), "{message}"),
        other => panic!("expected Malformed, got {other:?}"),
    }

    // /Position 四角不构成矩形。
    let bad_position = minimal_v13().insert(
        "Position",
        arr(vec![
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(10),
            Value::Integer(0),
            Value::Integer(12),
            Value::Integer(10),
            Value::Integer(0),
            Value::Integer(10),
        ]),
    );
    match error(&document_with(53, opi_dict("1.3", bad_position)), 53) {
        OpiError::Malformed(message) => assert!(message.contains("rectangle"), "{message}"),
        other => panic!("expected Malformed, got {other:?}"),
    }

    // 2.0 的 /Size 与 /CropRect 必须成对出现。
    let unpaired = opi_dict(
        "2.0",
        Dict::new()
            .insert("Version", Value::Float(2.0))
            .insert("F", Value::String("proxy.jpg".into()))
            .insert("Size", arr(vec![Value::Integer(10), Value::Integer(10)])),
    );
    match error(&document_with(54, unpaired), 54) {
        OpiError::Malformed(message) => {
            assert!(message.contains("both be present"), "{message}");
        }
        other => panic!("expected Malformed, got {other:?}"),
    }

    // /GrayMap 超过读取上限。
    let large = minimal_v13()
        .insert(
            "ImageType",
            arr(vec![Value::Integer(1), Value::Integer(13)]),
        )
        .insert("GrayMap", arr(Vec::new()));
    match error(&document_with(55, opi_dict("1.3", large)), 55) {
        OpiError::LimitExceeded(message) => assert!(message.contains("GrayMap"), "{message}"),
        other => panic!("expected LimitExceeded, got {other:?}"),
    }

    // 非流/字典容器。
    let document = Document::new();
    match document.xobject_opi(&Value::Integer(1)) {
        Err(OpiError::Malformed(message)) => {
            assert!(message.contains("stream or dictionary"), "{message}");
        }
        other => panic!("expected Malformed, got {other:?}"),
    }
}
