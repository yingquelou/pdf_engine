use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::thumbnail::ThumbnailError;
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

fn dict(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

fn image(width: i64, height: i64, space: &str) -> Value {
    let channels = if space == "DeviceGray" { 1 } else { 3 };
    Value::Stream(
        dict(vec![
            ("Type", Value::Name("XObject".into())),
            ("Subtype", Value::Name("Image".into())),
            ("Width", Value::Integer(width)),
            ("Height", Value::Integer(height)),
            ("ColorSpace", Value::Name(space.into())),
            ("BitsPerComponent", Value::Integer(8)),
        ]),
        StreamData::Owned(vec![0_u8; (width * height * channels) as usize]),
    )
}

fn document_with_page(entries: Vec<(&str, Value)>, extra: Vec<(i64, Value)>) -> Document {
    let mut document = Document::new();
    document.set_object(Object::new(
        1,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Catalog".into())),
            ("Pages", Value::Ref(2, 0)),
        ])),
    ));
    document.set_object(Object::new(
        2,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Pages".into())),
            ("Kids", Value::Array(Array(vec![Value::Ref(3, 0)]))),
            ("Count", Value::Integer(1)),
        ])),
    ));
    let mut page = dict(vec![
        ("Type", Value::Name("Page".into())),
        ("Parent", Value::Ref(2, 0)),
    ]);
    for (key, value) in entries {
        page = page.insert(key, value);
    }
    document.set_object(Object::new(3, 0, Value::Dict(page)));
    for (num, value) in extra {
        document.set_object(Object::new(num, 0, value));
    }
    document.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(10)),
    };
    document
}

#[test]
fn page_thumbnail_reads_image_entries() {
    let document = document_with_page(
        vec![("Thumb", Value::Ref(4, 0))],
        vec![(4, image(2, 2, "DeviceRGB"))],
    );
    let thumbnail = document
        .page_thumbnail(0)
        .expect("thumbnail")
        .expect("present");
    assert_eq!(thumbnail.width, 2);
    assert_eq!(thumbnail.height, 2);
    assert_eq!(thumbnail.bits_per_component, 8);
    assert_eq!(thumbnail.data.len(), 12);
    assert!(
        matches!(thumbnail.color_space, Some(ColorSpaceModel::DeviceRGB)),
        "color space {:?}",
        thumbnail.color_space
    );
    assert!(thumbnail.decode.is_none());
}

#[test]
fn missing_page_thumbnail_reads_as_none() {
    let document = document_with_page(Vec::new(), Vec::new());
    assert!(document.page_thumbnail(0).expect("thumbnail").is_none());
}

#[test]
fn thumbnail_images_are_limited_to_gray_rgb_and_indexed() {
    let document = document_with_page(
        vec![("Thumb", Value::Ref(4, 0))],
        vec![(4, image(2, 2, "DeviceCMYK"))],
    );
    match document.page_thumbnail(0) {
        Err(ThumbnailError::Invalid(message)) => {
            assert!(message.contains("color space"), "{message}");
        }
        other => panic!("expected unsupported color space, got {other:?}"),
    }

    let mask = Value::Stream(
        dict(vec![
            ("Type", Value::Name("XObject".into())),
            ("Subtype", Value::Name("Image".into())),
            ("Width", Value::Integer(8)),
            ("Height", Value::Integer(1)),
            ("ImageMask", Value::Bool(true)),
            ("BitsPerComponent", Value::Integer(1)),
        ]),
        StreamData::Owned(vec![0_u8; 1]),
    );
    let document = document_with_page(vec![("Thumb", Value::Ref(4, 0))], vec![(4, mask)]);
    match document.page_thumbnail(0) {
        Err(ThumbnailError::Invalid(message)) => {
            assert!(message.contains("image mask"), "{message}");
        }
        other => panic!("expected image mask rejection, got {other:?}"),
    }
}

#[test]
fn malformed_page_thumbnail_is_a_typed_error() {
    let document = document_with_page(vec![("Thumb", Value::Integer(1))], Vec::new());
    match document.page_thumbnail(0) {
        Err(ThumbnailError::Image(error)) => {
            assert!(error.to_string().contains("must be a stream"), "{error}");
        }
        other => panic!("expected non-stream /Thumb, got {other:?}"),
    }

    let document = document_with_page(vec![("Thumb", Value::Ref(9, 0))], Vec::new());
    match document.page_thumbnail(0) {
        Err(ThumbnailError::Image(error)) => {
            assert!(
                error.to_string().contains("unresolved object reference"),
                "{error}"
            );
        }
        other => panic!("expected unresolved /Thumb, got {other:?}"),
    }

    let document = document_with_page(Vec::new(), Vec::new());
    assert!(matches!(
        document.page_thumbnail(5),
        Err(ThumbnailError::Page(_))
    ));
}
