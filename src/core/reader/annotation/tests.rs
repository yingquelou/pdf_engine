use super::*;
use crate::core::types::{Array, Object, Trailer, Xref};

fn catalog_object() -> Object {
    Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    )
}

fn pages_object(kids: Vec<Value>) -> Object {
    Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array(kids))),
        ),
    )
}

fn page_object(annots: Option<Vec<Value>>) -> Object {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Page".into()))
        .insert("Parent", Value::Ref(2, 0));
    if let Some(annots) = annots {
        dict = dict.insert("Annots", Value::Array(Array(annots)));
    }
    Object::new(3, 0, Value::Dict(dict))
}

fn doc(objects: Vec<Object>) -> Document {
    let mut trailer = Trailer::new();
    trailer.dict = trailer.dict.clone().insert("Root", Value::Ref(1, 0));
    Document::from_objects(objects, Xref::new(), trailer, Default::default())
}

fn doc_with_annots(annots: Vec<Value>, extra: Vec<Object>) -> Document {
    let mut objects = vec![
        catalog_object(),
        pages_object(vec![Value::Ref(3, 0)]),
        page_object(Some(annots)),
    ];
    objects.extend(extra);
    doc(objects)
}

fn base_annotation(subtype: &str) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("Annot".into()))
        .insert("Subtype", Value::Name(subtype.into()))
        .insert(
            "Rect",
            Value::Array(Array(vec![
                Value::Integer(10),
                Value::Float(20.5),
                Value::Integer(30),
                Value::Float(40.0),
            ])),
        )
}

#[test]
fn reads_direct_and_indirect_page_annotations() {
    let direct = Value::Dict(base_annotation("Caret"));
    let indirect = Value::Dict(
        base_annotation("Stamp")
            .insert("Contents", Value::String("rubber stamp".into()))
            .insert("NM", Value::String("annot-7".into())),
    );
    let document = doc_with_annots(
        vec![direct, Value::Ref(9, 0)],
        vec![Object::new(9, 0, indirect)],
    );
    let annotations = document.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);
    assert_eq!(annotations[0].subtype, AnnotationSubtype::Caret);
    assert_eq!(annotations[0].reference, None);
    assert_eq!(annotations[0].page_index, 0);
    assert_eq!(annotations[0].rect, [10.0, 20.5, 30.0, 40.0]);
    assert_eq!(annotations[1].subtype, AnnotationSubtype::Stamp);
    assert_eq!(annotations[1].reference, Some((9, 0)));
    assert_eq!(
        annotations[1].contents,
        Some(Value::String("rubber stamp".into()))
    );
    assert_eq!(annotations[1].name, Some(Value::String("annot-7".into())));
}

#[test]
fn pages_without_annots_are_empty_and_page_index_is_checked() {
    let document = doc(vec![
        catalog_object(),
        pages_object(vec![Value::Ref(3, 0)]),
        page_object(None),
    ]);
    assert!(
        document
            .page_annotations(0)
            .expect("page exists")
            .is_empty()
    );
    assert_eq!(
        document.page_annotations(1),
        Err(AnnotationError::PageNotFound)
    );
}

#[test]
fn annotation_flags_decode_standard_bits() {
    let annotation = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Text").insert("F", Value::Integer(0b10_1001_0010)),
    )
    .expect("flags parse");
    assert!(annotation.flags.hidden());
    assert!(annotation.flags.no_rotate());
    assert!(annotation.flags.locked());
    assert!(annotation.flags.locked_contents());
    assert!(!annotation.flags.no_view());
    assert!(!annotation.flags.read_only());
    assert!(!annotation.visible());

    let default_flags = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Text"),
    )
    .expect("defaults");
    assert_eq!(default_flags.flags, AnnotationFlags::default());
    assert!(default_flags.visible());

    let invisible = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Text").insert("F", Value::Integer(AnnotationFlags::NO_VIEW as i64)),
    )
    .expect("no-view");
    assert!(!invisible.visible());

    let bad_flags = base_annotation("Text").insert("F", Value::Name("hidden".into()));
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_flags),
        Err(AnnotationError::Malformed(_))
    ));
}

#[test]
fn appearance_streams_expose_normal_down_rollover_and_state() {
    let ap = Dict::new()
        .insert("N", Value::Ref(20, 0))
        .insert("D", Value::Ref(21, 0))
        .insert("R", Value::Dict(Dict::new().insert("S", Value::Ref(22, 0))));
    let annotation = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Widget")
            .insert("AP", Value::Dict(ap))
            .insert("AS", Value::Name("S".into())),
    )
    .expect("appearance");
    let streams = annotation.appearance.expect("AP present");
    assert_eq!(streams.normal, Some(Value::Ref(20, 0)));
    assert_eq!(streams.down, Some(Value::Ref(21, 0)));
    assert!(matches!(streams.state("R"), Some(Value::Dict(_))));
    assert!(streams.state("X").is_none());
    assert_eq!(annotation.appearance_state.as_deref(), Some("S"));
}

#[test]
fn appearance_without_states_is_dropped() {
    let annotation = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Widget").insert("AP", Value::Dict(Dict::new())),
    )
    .expect("empty AP");
    assert!(annotation.appearance.is_none());
}

#[test]
fn markup_annotation_reads_quad_points_and_keeps_raw_entries() {
    let quad = Value::Array(Array((0..8).map(Value::Integer).collect()));
    let ink_list = Value::Array(Array(vec![Value::Array(Array(vec![
        Value::Integer(1),
        Value::Integer(2),
    ]))]));
    let annotation = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Highlight")
            .insert("QuadPoints", quad)
            .insert(
                "C",
                Value::Array(Array(vec![
                    Value::Integer(1),
                    Value::Integer(1),
                    Value::Integer(0),
                ])),
            )
            .insert("CA", Value::Float(0.5))
            .insert("InkList", ink_list.clone())
            .insert("M", Value::String("D:19900428".into()))
            .insert("T", Value::HexString("0048".into())),
    )
    .expect("markup");
    assert!(annotation.subtype.is_markup());
    assert_eq!(annotation.quad_points.expect("quad").len(), 8);
    assert_eq!(annotation.color, Some(vec![1.0, 1.0, 0.0]));
    assert_eq!(annotation.opacity, Some(0.5));
    assert_eq!(
        annotation.modified_date,
        Some(Value::String("D:19900428".into()))
    );
    assert_eq!(annotation.title, Some(Value::HexString("0048".into())));
    assert_eq!(annotation.dict.get("InkList"), Some(&ink_list));

    let bad_quad = base_annotation("Highlight").insert(
        "QuadPoints",
        Value::Array(Array(vec![Value::Integer(1); 3])),
    );
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_quad),
        Err(AnnotationError::Malformed(_))
    ));
}

#[test]
fn link_annotation_keeps_action_and_destination_raw() {
    let action = Value::Dict(
        Dict::new()
            .insert("S", Value::Name("URI".into()))
            .insert("URI", Value::String("https://example.com".into())),
    );
    let dest = Value::Array(Array(vec![
        Value::Ref(3, 0),
        Value::Name("XYZ".into()),
        Value::Integer(0),
    ]));
    let annotation = parse_annotation(
        &doc(vec![]),
        0,
        Some((7, 0)),
        &base_annotation("Link")
            .insert("A", action.clone())
            .insert("Dest", dest.clone())
            .insert("Popup", Value::Ref(8, 0)),
    )
    .expect("link");
    assert_eq!(annotation.action, Some(action));
    assert_eq!(annotation.destination, Some(dest));
    assert_eq!(annotation.popup, Some((8, 0)));

    let bad_popup = base_annotation("Link").insert("Popup", Value::Integer(3));
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_popup),
        Err(AnnotationError::Malformed(_))
    ));
}

#[test]
fn border_prefers_bs_dictionary_over_legacy_array() {
    let bs_dict = Dict::new()
        .insert("W", Value::Float(2.0))
        .insert("S", Value::Name("Dashed".into()))
        .insert(
            "D",
            Value::Array(Array(vec![Value::Integer(3), Value::Integer(2)])),
        );
    let with_bs = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Square")
            .insert("BS", Value::Dict(bs_dict))
            .insert("BM", Value::Name("Multiply".into()))
            .insert(
                "Border",
                Value::Array(Array(vec![Value::Integer(4), Value::Integer(9)])),
            ),
    )
    .expect("bs");
    let border = with_bs.border.expect("border");
    assert_eq!(border.width, Some(2.0));
    assert_eq!(border.style.as_deref(), Some("Dashed"));
    assert_eq!(border.dash, vec![3.0, 2.0]);
    assert_eq!(border.blend_mode.as_deref(), Some("Multiply"));
    assert_eq!(border.corner_radius, None);

    let legacy = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Circle").insert(
            "Border",
            Value::Array(Array(vec![
                Value::Integer(2),
                Value::Integer(1),
                Value::Array(Array(vec![Value::Integer(5), Value::Integer(5)])),
            ])),
        ),
    )
    .expect("legacy border");
    let border = legacy.border.expect("border");
    assert_eq!(border.corner_radius, Some(2.0));
    assert_eq!(border.width, Some(1.0));
    assert_eq!(border.dash, vec![5.0, 5.0]);
    assert_eq!(border.style, None);

    let blend_only = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Line").insert("BM", Value::Name("Normal".into())),
    )
    .expect("blend only");
    let border = blend_only.border.expect("border");
    assert_eq!(border.width, None);
    assert_eq!(border.blend_mode.as_deref(), Some("Normal"));

    let bad_border = base_annotation("Circle").insert(
        "Border",
        Value::Array(Array((0..4).map(Value::Integer).collect())),
    );
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_border),
        Err(AnnotationError::Malformed(_))
    ));

    let bad_ca = base_annotation("Text").insert("CA", Value::Float(1.5));
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_ca),
        Err(AnnotationError::Malformed(_))
    ));
}

#[test]
fn malformed_annotation_dictionaries_are_typed_errors() {
    let document = doc(vec![]);
    let cases = vec![
        // 缺 /Type
        Dict::new().insert("Subtype", Value::Name("Text".into())),
        // /Type 非 Annot
        base_annotation("Text").insert("Type", Value::Name("Annot2".into())),
        // 缺 /Subtype
        Dict::new()
            .insert("Type", Value::Name("Annot".into()))
            .insert("Rect", Value::Array(Array(vec![Value::Integer(0); 4]))),
        // /Rect 缺项
        Dict::new()
            .insert("Type", Value::Name("Annot".into()))
            .insert("Subtype", Value::Name("Text".into()))
            .insert("Rect", Value::Array(Array(vec![Value::Integer(0); 3]))),
        // /Rect 含名称
        Dict::new()
            .insert("Type", Value::Name("Annot".into()))
            .insert("Subtype", Value::Name("Text".into()))
            .insert(
                "Rect",
                Value::Array(Array(vec![
                    Value::Integer(0),
                    Value::Integer(0),
                    Value::Name("wide".into()),
                    Value::Integer(0),
                ])),
            ),
        // /Contents 非文本串
        base_annotation("Text").insert("Contents", Value::Dict(Dict::new())),
        // /AS 非名称
        base_annotation("Text").insert("AS", Value::Integer(1)),
    ];
    for case in cases {
        assert!(
            matches!(
                parse_annotation(&document, 0, None, &case),
                Err(AnnotationError::Malformed(_))
            ),
            "expected malformed error for {case:?}"
        );
    }
}

#[test]
fn unknown_subtype_is_preserved() {
    let annotation =
        parse_annotation(&doc(vec![]), 0, None, &base_annotation("Watermark")).expect("unknown");
    assert_eq!(
        annotation.subtype,
        AnnotationSubtype::Unknown("Watermark".into())
    );
    assert_eq!(annotation.subtype.name(), "Watermark");
    assert!(!annotation.subtype.is_markup());
}

#[test]
fn annotation_count_limit_is_enforced() {
    let annots: Vec<Value> = (0..MAX_ANNOTATIONS_PER_PAGE + 1)
        .map(|_| Value::Dict(base_annotation("Text")))
        .collect();
    let document = doc_with_annots(annots, Vec::new());
    assert_eq!(
        document.page_annotations(0),
        Err(AnnotationError::LimitExceeded("annotations per page"))
    );
}

#[test]
fn page_tree_cycles_and_depth_are_typed_errors() {
    // 环：Pages 节点 2 的子节点指回自身。
    let cyclic = doc(vec![
        catalog_object(),
        pages_object(vec![Value::Ref(2, 0)]),
        page_object(None),
    ]);
    assert!(matches!(
        cyclic.page_annotations(0),
        Err(AnnotationError::Malformed(_))
    ));

    // 非父子形状：/Kids 不是数组。
    let bad_kids = doc(vec![
        catalog_object(),
        Object::new(
            2,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Pages".into()))
                    .insert("Kids", Value::Integer(1)),
            ),
        ),
        page_object(None),
    ]);
    assert!(matches!(
        bad_kids.page_annotations(0),
        Err(AnnotationError::Malformed(_))
    ));

    // 目录缺失。
    let no_catalog = doc(vec![
        pages_object(vec![Value::Ref(3, 0)]),
        page_object(None),
    ]);
    assert_eq!(
        no_catalog.page_annotations(0),
        Err(AnnotationError::CatalogMissing)
    );
}

#[test]
fn annotation_entry_referencing_non_dictionary_is_malformed() {
    let document = doc_with_annots(
        vec![Value::Ref(9, 0)],
        vec![Object::new(9, 0, Value::Integer(4))],
    );
    assert!(matches!(
        document.page_annotations(0),
        Err(AnnotationError::Malformed(_))
    ));
}

#[test]
fn text_annotation_reads_icon_open_flag_and_markup_state() {
    let dict = base_annotation("Text")
        .insert("Name", Value::Name("Paragraph".into()))
        .insert("Open", Value::Bool(false))
        .insert("State", Value::String("Accepted".into()))
        .insert("StateModel", Value::HexString("4D61726B6564".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("text annotation");
    assert_eq!(annotation.icon_name.as_deref(), Some("Paragraph"));
    assert_eq!(annotation.is_open, Some(false));
    assert_eq!(annotation.state.as_deref(), Some("Accepted"));
    assert_eq!(annotation.state_model.as_deref(), Some("Marked"));
    assert_eq!(annotation.symbol, None);
    assert!(annotation.ink_paths.is_empty());
}

#[test]
fn caret_annotation_reads_symbol_and_absent_entries_stay_none() {
    let dict = base_annotation("Caret").insert("Symbol", Value::Name("NewLine".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("caret annotation");
    assert_eq!(annotation.symbol.as_deref(), Some("NewLine"));
    assert!(annotation.icon_name.is_none());
    assert!(annotation.is_open.is_none());
    assert!(annotation.state.is_none());
    assert!(annotation.state_model.is_none());

    // `/Symbol` 也可写成文本串（真实文件偶见），按文本解码。
    let loose = base_annotation("Caret").insert("Symbol", Value::String("Cross".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &loose).expect("loose symbol");
    assert_eq!(annotation.symbol.as_deref(), Some("Cross"));
}

#[test]
fn ink_annotation_reads_ink_list_paths() {
    let dict = base_annotation("Ink").insert(
        "InkList",
        Value::Array(Array(vec![
            Value::Array(Array(vec![
                Value::Integer(10),
                Value::Float(20.5),
                Value::Integer(30),
                Value::Float(40.0),
            ])),
            Value::Array(Array(vec![
                Value::Float(1.5),
                Value::Float(2.5),
                Value::Integer(3),
                Value::Integer(4),
                Value::Integer(5),
                Value::Integer(6),
            ])),
        ])),
    );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("ink annotation");
    assert_eq!(
        annotation.ink_paths,
        vec![
            vec![10.0, 20.5, 30.0, 40.0],
            vec![1.5, 2.5, 3.0, 4.0, 5.0, 6.0]
        ]
    );
}

#[test]
fn malformed_subtype_entries_are_typed_errors() {
    let document = doc(vec![catalog_object()]);
    let cases = vec![
        base_annotation("Text").insert("Open", Value::Integer(1)),
        base_annotation("Text").insert("State", Value::Integer(2)),
        base_annotation("Caret").insert("Symbol", Value::Integer(0)),
        base_annotation("Ink").insert("InkList", Value::Integer(3)),
        base_annotation("Ink").insert("InkList", Value::Array(Array(vec![Value::Integer(1)]))),
        base_annotation("Ink").insert(
            "InkList",
            Value::Array(Array(vec![Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]))])),
        ),
        base_annotation("Ink").insert(
            "InkList",
            Value::Array(Array(vec![Value::Array(Array(vec![
                Value::Integer(1),
                Value::Float(f64::NAN),
            ]))])),
        ),
    ];
    for dict in cases {
        assert!(
            matches!(
                parse_annotation(&document, 0, None, &dict),
                Err(AnnotationError::Malformed(_))
            ),
            "expect malformed for {dict:?}"
        );
    }
}

/// `/Dest` 目的地数组、PDF 1.1 的 `/D` 字典与命名目标均解析为导航层目标模型。
#[test]
fn annotation_destination_resolves_array_dictionary_and_named_forms() {
    use crate::core::reader::navigation::{Destination, OutlineDestination};

    let array = Value::Array(Array(vec![
        Value::Ref(3, 0),
        Value::Name("XYZ".into()),
        Value::Null,
        Value::Null,
        Value::Integer(0),
    ]));
    let explicit = Value::Dict(base_annotation("Link").insert("Dest", array.clone()));
    let legacy = Value::Dict(
        base_annotation("Link").insert("Dest", Value::Dict(Dict::new().insert("D", array))),
    );
    let named = Value::Dict(base_annotation("Link").insert("Dest", Value::Name("intro".into())));
    let absent = Value::Dict(base_annotation("Link"));
    let document = doc_with_annots(vec![explicit, legacy, named, absent], Vec::new());
    let annotations = document.page_annotations(0).expect("annotations");

    let expected = OutlineDestination::Explicit(Destination::Xyz {
        page: 0,
        left: None,
        top: None,
        zoom: Some(0.0),
    });
    for index in 0..2 {
        assert_eq!(
            document
                .annotation_destination(&annotations[index])
                .expect("destination parses"),
            Some(expected.clone()),
            "array form and /D dictionary form must resolve identically"
        );
    }
    assert_eq!(
        document
            .annotation_destination(&annotations[2])
            .expect("named destination parses"),
        Some(OutlineDestination::Named("intro".into()))
    );
    assert_eq!(
        document
            .annotation_destination(&annotations[3])
            .expect("annotation without /Dest"),
        None
    );
}

/// 命名目标可继续经目录 `/Dests` 名称树查得实际页面目标（12.3.2.1）。
#[test]
fn annotation_named_destination_key_resolves_through_dests_tree() {
    use crate::core::reader::navigation::{Destination, OutlineDestination};

    let dests = Value::Dict(Dict::new().insert(
        "Names",
        Value::Array(Array(vec![
            Value::String("intro".into()),
            Value::Array(Array(vec![Value::Ref(3, 0), Value::Name("Fit".into())])),
        ])),
    ));
    let catalog = Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert("Names", Value::Dict(Dict::new().insert("Dests", dests))),
        ),
    );
    let document = doc_with_annots(
        vec![Value::Dict(
            base_annotation("Link").insert("Dest", Value::String("intro".into())),
        )],
        vec![catalog],
    );
    // 自定义 catalog 占用对象号 1，与默认辅助冲突时以最后一份为准。
    let annotation = &document.page_annotations(0).expect("annotations")[0];
    let target = document
        .annotation_destination(annotation)
        .expect("destination parses")
        .expect("/Dest present");
    assert_eq!(target, OutlineDestination::Named("intro".into()));
    assert_eq!(
        document.named_destination("intro").expect("name tree"),
        Some(Destination::Fit { page: 0 })
    );
}

/// 非法 `/Dest`（类型错误、缺 `/D`、页面不在树中）返回类型化错误。
#[test]
fn malformed_annotation_destinations_are_typed_errors() {
    let cases = vec![
        base_annotation("Link").insert("Dest", Value::Integer(3)),
        base_annotation("Link").insert("Dest", Value::Dict(Dict::new())),
        base_annotation("Link").insert(
            "Dest",
            Value::Array(Array(vec![Value::Ref(99, 0), Value::Name("Fit".into())])),
        ),
        base_annotation("Link").insert(
            "Dest",
            Value::Array(Array(vec![Value::Ref(3, 0), Value::Name("Bogus".into())])),
        ),
    ];
    let document = doc_with_annots(cases.iter().cloned().map(Value::Dict).collect(), Vec::new());
    let annotations = document.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 4);
    for annotation in &annotations {
        let error = document
            .annotation_destination(annotation)
            .expect_err("malformed /Dest must error");
        assert!(
            matches!(
                error,
                AnnotationError::Malformed(_) | AnnotationError::PageNotFound
            ),
            "unexpected error {error:?}"
        );
    }
}
