//! 内联图像（Table 94/95）：缩写键还原、缺省值与页面资源 `/CS` 解析。

use super::support::{dict, document_with_page};
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::content::InlineImage;
use crate::core::types::{Array, Document, Value};

#[test]
fn inline_image_parses_abbreviated_keys_and_decodes_filters() {
    let doc = Document::new();
    let image = doc
        .inline_image(&InlineImage {
            dict: vec![
                ("W".into(), Value::Float(3.0)),
                ("H".into(), Value::Float(1.0)),
                ("BPC".into(), Value::Float(8.0)),
                ("CS".into(), Value::Name("G".into())),
                ("F".into(), Value::Name("AHx".into())),
            ],
            data: b"00FF7F>".to_vec(),
        })
        .expect("inline image should parse");
    assert_eq!((image.width, image.height), (3, 1));
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceGray));
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.filters, vec!["ASCIIHexDecode".to_string()]);
    assert_eq!(image.data, vec![0x00, 0xFF, 0x7F]);
    assert_eq!(
        image.samples().unwrap().values,
        vec![0.0, 1.0, 127.0 / 255.0]
    );
}

#[test]
fn inline_image_mask_defaults_bits_and_has_no_color_space() {
    let doc = Document::new();
    let image = doc
        .inline_image(&InlineImage {
            dict: vec![
                ("W".into(), Value::Float(2.0)),
                ("H".into(), Value::Float(1.0)),
                ("IM".into(), Value::Bool(true)),
            ],
            data: vec![0b1000_0000],
        })
        .expect("inline image mask should parse");
    assert!(image.image_mask);
    assert_eq!(image.color_space, None);
    assert_eq!(image.bits_per_component, 1);
    assert_eq!(image.samples().unwrap().values, vec![1.0, 0.0]);
}

#[test]
fn page_inline_image_resolves_named_color_space() {
    let indexed_space = Value::Array(Array(vec![
        Value::Name("Indexed".into()),
        Value::Name("DeviceRGB".into()),
        Value::Integer(1),
        Value::HexString("FF000000FF00".into()),
    ]));
    let resources = Value::Dict(dict(vec![(
        "ColorSpace",
        Value::Dict(dict(vec![("CS0", indexed_space)])),
    )]));
    let doc = document_with_page(Some(resources), Vec::new());

    let inline = InlineImage {
        dict: vec![
            ("W".into(), Value::Float(2.0)),
            ("H".into(), Value::Float(1.0)),
            ("BPC".into(), Value::Float(8.0)),
            ("CS".into(), Value::Name("CS0".into())),
        ],
        data: vec![0, 1],
    };
    let image = doc
        .page_inline_image(0, &inline)
        .expect("page resources should resolve /CS0");
    assert!(matches!(
        image.color_space,
        Some(ColorSpaceModel::Indexed(_))
    ));
    assert_eq!(
        image.samples().unwrap().values,
        vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
    );

    assert!(
        doc.inline_image(&inline).is_err(),
        "without page context the resource name must not resolve"
    );
}
