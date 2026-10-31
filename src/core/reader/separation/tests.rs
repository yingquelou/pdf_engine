//! 分色字典读端测试（ISO 32000-1 14.11.4 Table 364）。

use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::separation::{DeviceColorant, SeparationInfoError};
use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

const PAGES_OBJECT: i64 = 2;
const PAGE_A: i64 = 10;
const PAGE_B: i64 = 11;

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

fn shared_pages() -> Value {
    arr(vec![Value::Ref(PAGE_A, 0), Value::Ref(PAGE_B, 0)])
}

/// 指数插值 tint 变换（FunctionType 2）。
fn exponential() -> Value {
    Value::Dict(
        Dict::new()
            .insert("FunctionType", Value::Integer(2))
            .insert("Domain", arr(vec![Value::Integer(0), Value::Integer(1)]))
            .insert("C0", arr(vec![Value::Integer(0)]))
            .insert("C1", arr(vec![Value::Integer(1)]))
            .insert("N", Value::Integer(1)),
    )
}

fn separation_space(colorant: &str) -> Value {
    arr(vec![
        name("Separation"),
        name(colorant),
        name("DeviceGray"),
        exponential(),
    ])
}

fn page(info: Option<Value>) -> Dict {
    let page = Dict::new()
        .insert("Type", name("Page"))
        .insert("Parent", Value::Ref(PAGES_OBJECT, 0));
    match info {
        Some(info) => page.insert("SeparationInfo", info),
        None => page,
    }
}

/// 两页文档：Catalog 1 → Pages 2（Kids [10 11]）→ Page 10 / Page 11。
fn document_with_pages(page_a: Dict, page_b: Dict) -> Document {
    let mut doc = Document::new();
    insert(&doc, PAGE_A, Value::Dict(page_a));
    insert(&doc, PAGE_B, Value::Dict(page_b));
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert(
                    "Kids",
                    arr(vec![Value::Ref(PAGE_A, 0), Value::Ref(PAGE_B, 0)]),
                )
                .insert("Count", Value::Integer(2)),
        ),
    );
    insert(
        &doc,
        1,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES_OBJECT, 0)),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

fn assert_malformed(doc: &Document, needle: &str) {
    match doc.page_separation_info(0) {
        Err(SeparationInfoError::Malformed(message)) => {
            assert!(
                message.contains(needle),
                "expected {needle:?}, got {message}"
            );
        }
        other => panic!("expected malformed error containing {needle:?}, got {other:?}"),
    }
}

#[test]
fn separation_info_reads_pages_colorant_and_color_space() {
    let pages = shared_pages();
    let info_a = Dict::new()
        .insert("Pages", pages.clone())
        .insert("DeviceColorant", name("Cyan"))
        .insert("ColorSpace", separation_space("Cyan"));
    let info_b = Dict::new()
        .insert("Pages", pages)
        .insert("DeviceColorant", name("Cyan"));
    let doc = document_with_pages(
        page(Some(Value::Dict(info_a))),
        page(Some(Value::Dict(info_b))),
    );

    let info = doc
        .page_separation_info(0)
        .expect("separation info")
        .expect("declared");
    assert_eq!(info.pages, vec![(PAGE_A, 0), (PAGE_B, 0)]);
    assert_eq!(info.device_colorant, DeviceColorant::Name("Cyan".into()));
    assert_eq!(info.device_colorant.as_str(), "Cyan");
    match &info.color_space {
        Some(ColorSpaceModel::Separation(space)) => assert_eq!(space.colorant, "Cyan"),
        other => panic!("expected Separation colour space, got {other:?}"),
    }
    assert_eq!(
        info.dict.get("DeviceColorant"),
        Some(&name("Cyan")),
        "原始字典应保留 /DeviceColorant"
    );

    // 第二页共享相同 /Pages，同样可读。
    let info = doc
        .page_separation_info(1)
        .expect("separation info")
        .expect("declared");
    assert_eq!(info.pages, vec![(PAGE_A, 0), (PAGE_B, 0)]);
    assert_eq!(info.color_space, None);
}

#[test]
fn separation_info_accepts_text_colorant_and_indirect_dictionary() {
    let info_object = Dict::new()
        .insert("Pages", shared_pages())
        .insert("DeviceColorant", Value::String("PANTONE 35 CV".into()))
        .insert("CustomKey", Value::Integer(7));
    let doc = document_with_pages(page(Some(Value::Ref(20, 0))), page(Some(Value::Ref(20, 0))));
    insert(&doc, 20, Value::Dict(info_object));

    let info = doc
        .page_separation_info(0)
        .expect("separation info")
        .expect("declared");
    assert_eq!(
        info.device_colorant,
        DeviceColorant::Text("PANTONE 35 CV".into())
    );
    assert_eq!(info.device_colorant.as_str(), "PANTONE 35 CV");
    assert_eq!(info.color_space, None);
    assert_eq!(
        info.dict.get("DeviceColorant"),
        Some(&Value::String("PANTONE 35 CV".into())),
        "文本串色料名原样保留"
    );
    assert_eq!(info.dict.get("CustomKey"), Some(&Value::Integer(7)));

    // 间接字典由两页共享，第二页读出同一 `/Pages`。
    let other = doc
        .page_separation_info(1)
        .expect("separation info")
        .expect("declared");
    assert_eq!(other.pages, info.pages);
}

#[test]
fn separation_info_requires_pages_to_include_the_page() {
    // `/Pages` 不含当前页。
    let doc = document_with_pages(
        page(Some(Value::Dict(
            Dict::new()
                .insert("Pages", arr(vec![Value::Ref(PAGE_B, 0)]))
                .insert("DeviceColorant", name("Cyan")),
        ))),
        page(Some(Value::Dict(
            Dict::new()
                .insert("Pages", arr(vec![Value::Ref(PAGE_B, 0)]))
                .insert("DeviceColorant", name("Cyan")),
        ))),
    );
    assert_malformed(&doc, "must include the page itself");

    // 被引用页缺 `/SeparationInfo`，无法声明相同 `/Pages`。
    let doc = document_with_pages(
        page(Some(Value::Dict(
            Dict::new()
                .insert("Pages", shared_pages())
                .insert("DeviceColorant", name("Cyan")),
        ))),
        page(None),
    );
    assert_malformed(&doc, "identical /Pages");

    // 被引用页的 `/Pages` 顺序不同（对象号序列不一致）。
    let doc = document_with_pages(
        page(Some(Value::Dict(
            Dict::new()
                .insert("Pages", shared_pages())
                .insert("DeviceColorant", name("Cyan")),
        ))),
        page(Some(Value::Dict(
            Dict::new()
                .insert(
                    "Pages",
                    arr(vec![Value::Ref(PAGE_B, 0), Value::Ref(PAGE_A, 0)]),
                )
                .insert("DeviceColorant", name("Cyan")),
        ))),
    );
    assert_malformed(&doc, "not identical");
}

#[test]
fn malformed_separation_info_is_typed_errors() {
    // 缺 /Pages 与非数组 /Pages。
    assert_malformed(
        &document_with_pages(
            page(Some(Value::Dict(
                Dict::new().insert("DeviceColorant", name("Cyan")),
            ))),
            page(None),
        ),
        "/Pages is required",
    );
    assert_malformed(
        &document_with_pages(
            page(Some(Value::Dict(
                Dict::new()
                    .insert("Pages", Value::Integer(3))
                    .insert("DeviceColorant", name("Cyan")),
            ))),
            page(None),
        ),
        "/Pages is not an array",
    );

    // 条目不是间接引用。
    assert_malformed(
        &document_with_pages(
            page(Some(Value::Dict(
                Dict::new()
                    .insert(
                        "Pages",
                        arr(vec![
                            Value::Ref(PAGE_A, 0),
                            Value::Dict(Dict::new().insert("Type", name("Page"))),
                        ]),
                    )
                    .insert("DeviceColorant", name("Cyan")),
            ))),
            page(None),
        ),
        "must be an indirect reference",
    );

    // 条目解引用失败。
    assert_malformed(
        &document_with_pages(
            page(Some(Value::Dict(
                Dict::new()
                    .insert("Pages", arr(vec![Value::Ref(PAGE_A, 0), Value::Ref(99, 0)]))
                    .insert("DeviceColorant", name("Cyan")),
            ))),
            page(None),
        ),
        "entry 1",
    );

    // 与被引用页一致的合法骨架，用于检查色料与颜色空间条目。
    let valid_b = Value::Dict(
        Dict::new()
            .insert("Pages", shared_pages())
            .insert("DeviceColorant", name("Cyan")),
    );

    // /DeviceColorant 是数字。
    let info_a = Dict::new()
        .insert("Pages", shared_pages())
        .insert("DeviceColorant", Value::Integer(3));
    assert_malformed(
        &document_with_pages(page(Some(Value::Dict(info_a))), page(Some(valid_b.clone()))),
        "name or text string",
    );

    // /ColorSpace 为 DeviceRGB：合法颜色空间但不是 Separation/DeviceN。
    let info_a = Dict::new()
        .insert("Pages", shared_pages())
        .insert("DeviceColorant", name("Cyan"))
        .insert("ColorSpace", name("DeviceRGB"));
    assert_malformed(
        &document_with_pages(page(Some(Value::Dict(info_a))), page(Some(valid_b))),
        "Separation or DeviceN",
    );

    // /Pages 超过 1024 项。
    let oversized = (0..1025)
        .map(|_| Value::Ref(PAGE_A, 0))
        .collect::<Vec<Value>>();
    let doc = document_with_pages(
        page(Some(Value::Dict(
            Dict::new()
                .insert("Pages", arr(oversized))
                .insert("DeviceColorant", name("Cyan")),
        ))),
        page(None),
    );
    match doc.page_separation_info(0) {
        Err(SeparationInfoError::LimitExceeded(what)) => {
            assert_eq!(what, "page /SeparationInfo /Pages");
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}
