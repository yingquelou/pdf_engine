//! AcroForm `/DR` 默认资源测试：类型化读取与非法形态报错（Table 221）。

use super::*;

#[test]
fn acro_form_default_resources_are_typed() {
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new()
            .insert("Fields", arr(Vec::new()))
            .insert("DR", Value::Ref(30, 0)),
    ));
    insert(
        &doc,
        30,
        Value::Dict(
            Dict::new()
                .insert(
                    "Font",
                    Value::Dict(Dict::new().insert("Helv", Value::Ref(31, 0))),
                )
                .insert(
                    "ExtGState",
                    Value::Dict(
                        Dict::new().insert(
                            "GS1",
                            Value::Dict(
                                Dict::new()
                                    .insert("Type", name("ExtGState"))
                                    .insert("CA", Value::Float(0.5))
                                    .insert("ca", Value::Float(0.25))
                                    .insert("BM", name("Multiply")),
                            ),
                        ),
                    ),
                )
                .insert(
                    "ColorSpace",
                    Value::Dict(Dict::new().insert("CS1", name("DeviceRGB")).insert(
                        "CS2",
                        arr(vec![
                            name("Indexed"),
                            name("DeviceRGB"),
                            Value::Integer(1),
                            Value::HexString("000000FFFFFF".into()),
                        ]),
                    )),
                )
                .insert("ProcSet", arr(vec![name("PDF"), name("Text")]))
                .insert("Pattern", Value::Dict(Dict::new())),
        ),
    );
    insert(
        &doc,
        31,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Font"))
                .insert("Subtype", name("Type1"))
                .insert("BaseFont", name("Helvetica")),
        ),
    );

    let form = doc.acro_form().expect("parse").expect("present");
    let dr = form.default_resources.as_ref().expect("/DR present");
    assert_eq!(dr.fonts.get("Helv"), Some(&Value::Ref(31, 0)));
    let gs = dr.ext_g_states.get("GS1").expect("/DR /ExtGState /GS1");
    assert_eq!(gs.stroke_alpha, Some(0.5));
    assert_eq!(gs.fill_alpha, Some(0.25));
    assert_eq!(gs.blend_mode, Some(BlendMode::Name("Multiply".into())));
    assert_eq!(
        dr.color_spaces.get("CS1"),
        Some(&ColorSpaceModel::DeviceRGB)
    );
    let Some(ColorSpaceModel::Indexed(indexed)) = dr.color_spaces.get("CS2") else {
        panic!("/CS2 must be Indexed");
    };
    assert_eq!(*indexed.base, ColorSpaceModel::DeviceRGB);
    assert_eq!(indexed.hival, 1);
    assert_eq!(indexed.lookup, vec![0, 0, 0, 0xFF, 0xFF, 0xFF]);
    assert_eq!(dr.proc_set, ["PDF", "Text"]);
    // 未建模条目保留在完整字典中。
    assert_eq!(dr.dict.get("Pattern"), Some(&Value::Dict(Dict::new())));
}

#[test]
fn malformed_default_resources_are_typed_errors() {
    let cases = [
        // /DR 不是字典。
        Value::Integer(1),
        // /Font 不是字典。
        Value::Dict(Dict::new().insert("Font", name("Helv"))),
        // /ExtGState 条目取值非法（/CA 越界）。
        Value::Dict(Dict::new().insert(
            "ExtGState",
            Value::Dict(Dict::new().insert(
                "GS1",
                Value::Dict(Dict::new().insert("CA", Value::Float(2.0))),
            )),
        )),
        // /ColorSpace 名字不认识。
        Value::Dict(Dict::new().insert(
            "ColorSpace",
            Value::Dict(Dict::new().insert("CS1", name("Bogus"))),
        )),
        // /ProcSet 条目不是名字。
        Value::Dict(Dict::new().insert("ProcSet", arr(vec![Value::Integer(1)]))),
    ];
    for dr in cases {
        let error = form_error(Value::Dict(
            Dict::new()
                .insert("Fields", arr(Vec::new()))
                .insert("DR", dr),
        ));
        assert!(matches!(error, FormError::Malformed(_)), "{error}");
    }
}
