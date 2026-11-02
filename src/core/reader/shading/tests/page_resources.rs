//! 页面 `/Resources` 中 `Shading`/`Pattern` 命名条目的解析；单页文档
//! 夹具仅服务本主题，随此文件保留。

use super::super::*;
use super::support::{axial_dict, dict, name, reals};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

fn make_page_doc(resources: Dict) -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Resources", Value::Dict(resources)),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };
    doc
}

#[test]
fn page_shading_and_pattern_resources_resolve_named_entries() {
    let doc = make_page_doc(
        Dict::new()
            .insert(
                "Shading",
                Value::Dict(Dict::new().insert("Sh1", axial_dict(&[]))),
            )
            .insert(
                "Pattern",
                Value::Dict(
                    Dict::new()
                        .insert(
                            "P1",
                            dict(&[("PatternType", Value::Integer(2)), ("Shading", name("Sh1"))]),
                        )
                        .insert(
                            "P2",
                            Value::Stream(
                                Dict::new()
                                    .insert("PaintType", Value::Integer(1))
                                    .insert("TilingType", Value::Integer(2))
                                    .insert("BBox", reals(&[0.0, 0.0, 2.0, 2.0]))
                                    .insert("XStep", Value::Float(2.0))
                                    .insert("YStep", Value::Float(2.0)),
                                StreamData::Owned(b"n".to_vec()),
                            ),
                        ),
                ),
            ),
    );
    let model = doc.page_shading(0, "Sh1").unwrap();
    assert!(matches!(model, Some(ShadingModel::Axial(_))));
    assert!(doc.page_shading(0, "Missing").unwrap().is_none());

    let named_pattern = doc.page_pattern(0, "P1").unwrap();
    let Some(PatternModel::Shading(pattern)) = named_pattern else {
        panic!("expected shading pattern resolved through page resources");
    };
    assert_eq!(pattern.shading.shading_type(), 2);

    let tiling = doc.page_pattern(0, "P2").unwrap();
    let Some(PatternModel::Tiling(tiling)) = tiling else {
        panic!("expected tiling pattern");
    };
    assert_eq!(tiling.tiling_type, TilingType::NonAdaptiveVariableSpacing);
    assert!(doc.page_pattern(0, "Absent").unwrap().is_none());
}
