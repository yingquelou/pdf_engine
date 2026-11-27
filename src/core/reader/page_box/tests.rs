//! 页面边界读端测试（ISO 32000-1 14.11.2 Table 30 与 Table 360/361）。

use crate::core::reader::page_box::{BoxStyleName, PageBoxError, Rectangle};
use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

const PAGES_OBJECT: i64 = 2;
const PAGE_OBJECT: i64 = 10;

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn float_rect(values: [f64; 4]) -> Value {
    arr(values.iter().map(|value| Value::Float(*value)).collect())
}

fn int_rect(values: [i64; 4]) -> Value {
    arr(values.iter().map(|value| Value::Integer(*value)).collect())
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 单页文档：页面对象 10、页面树 2、目录 1；页面字典自动补 `/Type` 与
/// `/Parent`（未声明时）。
fn document_with_page(page: Dict) -> Document {
    let mut doc = Document::new();
    let mut page = page.insert("Type", name("Page"));
    if page.get("Parent").is_none() {
        page = page.insert("Parent", Value::Ref(PAGES_OBJECT, 0));
    }
    insert(&doc, PAGE_OBJECT, Value::Dict(page));
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGE_OBJECT, 0)]))
                .insert("Count", Value::Integer(1)),
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

fn assert_malformed(page: Dict, needle: &str) {
    let doc = document_with_page(page);
    match doc.page_bounds(0) {
        Err(PageBoxError::Malformed(message)) => {
            assert!(
                message.contains(needle),
                "expected {needle:?}, got {message}"
            );
        }
        other => panic!("expected malformed error containing {needle:?}, got {other:?}"),
    }
}

#[test]
fn page_bounds_read_boxes_and_color_info() {
    let page = Dict::new()
        // 7.9.5：允许任意对角顺序，读端归一化为左下/右上。
        .insert("MediaBox", int_rect([595, 842, 0, 0]))
        // /CropBox 为间接引用，须解引用后解析。
        .insert("CropBox", Value::Ref(20, 0))
        .insert("BleedBox", float_rect([5.0, 5.0, 590.0, 837.0]))
        .insert("TrimBox", float_rect([20.0, 20.0, 490.0, 790.0]))
        .insert("ArtBox", float_rect([25.0, 25.0, 485.0, 785.0]))
        .insert(
            "BoxColorInfo",
            Value::Dict(
                Dict::new()
                    .insert(
                        "CropBox",
                        Value::Dict(
                            Dict::new()
                                .insert(
                                    "C",
                                    arr(vec![
                                        Value::Float(0.1),
                                        Value::Float(0.2),
                                        Value::Float(0.3),
                                    ]),
                                )
                                .insert("W", Value::Float(2.5))
                                .insert("S", name("D"))
                                .insert("D", arr(vec![Value::Integer(3), Value::Float(1.0)]))
                                .insert("CustomKey", Value::Integer(9)),
                        ),
                    )
                    // 扩展样式名按 Table 361 原样保留。
                    .insert(
                        "BleedBox",
                        Value::Dict(Dict::new().insert("S", name("VendorDash"))),
                    )
                    .insert("TrimBox", Value::Dict(Dict::new().insert("S", name("S"))))
                    .insert("XUnmodelled", Value::Integer(4)),
            ),
        )
        .insert("CustomKey", Value::Integer(7));
    let doc = document_with_page(page);
    insert(&doc, 20, float_rect([10.0, 20.0, 500.0, 800.0]));

    let bounds = doc.page_bounds(0).expect("page bounds");
    assert_eq!(bounds.media, Rectangle::new(0.0, 0.0, 595.0, 842.0));
    assert_eq!(bounds.media.width(), 595.0);
    assert_eq!(bounds.media.height(), 842.0);
    assert_eq!(bounds.crop, Some(Rectangle::new(10.0, 20.0, 500.0, 800.0)));
    assert_eq!(bounds.bleed, Some(Rectangle::new(5.0, 5.0, 590.0, 837.0)));
    assert_eq!(bounds.trim, Some(Rectangle::new(20.0, 20.0, 490.0, 790.0)));
    assert_eq!(bounds.art, Some(Rectangle::new(25.0, 25.0, 485.0, 785.0)));
    assert_eq!(
        bounds.dict.get("CustomKey"),
        Some(&Value::Integer(7)),
        "原始页面字典应保留未建模条目"
    );

    let info = bounds.box_color_info.as_ref().expect("box color info");
    let crop_style = info.crop.as_ref().expect("/CropBox style");
    assert_eq!(crop_style.color, Some([0.1, 0.2, 0.3]));
    assert_eq!(crop_style.width, Some(2.5));
    assert_eq!(crop_style.style, Some(BoxStyleName::Dashed));
    assert_eq!(crop_style.dash, Some(vec![3.0, 1.0]));
    assert_eq!(
        crop_style.dict.get("CustomKey"),
        Some(&Value::Integer(9)),
        "框样式字典应保留未建模条目"
    );
    let bleed_style = info.bleed.as_ref().expect("/BleedBox style");
    assert_eq!(
        bleed_style.style,
        Some(BoxStyleName::Other("VendorDash".into()))
    );
    assert_eq!(
        bleed_style
            .style
            .as_ref()
            .map(BoxStyleName::name)
            .expect("style name"),
        "VendorDash"
    );
    assert_eq!(
        info.trim.as_ref().and_then(|style| style.style.clone()),
        Some(BoxStyleName::Solid)
    );
    assert_eq!(info.art, None, "未声明的框样式条目为 None");
    assert_eq!(info.dict.get("XUnmodelled"), Some(&Value::Integer(4)));

    assert_eq!(BoxStyleName::from_name("S").name(), "S");
    assert_eq!(BoxStyleName::from_name("D").name(), "D");
    assert_eq!(bounds.effective_crop(), bounds.crop.unwrap());
}

#[test]
fn page_bounds_inherit_media_and_crop_through_parent_chain() {
    // MediaBox 在祖父节点、CropBox 在父节点；BleedBox 声明在父节点但
    // 不可继承，页面应读到 None。
    let page = Dict::new().insert("Parent", Value::Ref(11, 0));
    let doc = document_with_page(page);
    insert(
        &doc,
        11,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("CropBox", Value::Ref(21, 0))
                .insert("BleedBox", float_rect([0.0, 0.0, 10.0, 10.0]))
                .insert("Parent", Value::Ref(12, 0)),
        ),
    );
    insert(
        &doc,
        12,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("MediaBox", int_rect([0, 0, 600, 800])),
        ),
    );
    insert(&doc, 21, float_rect([1.0, 1.0, 200.0, 200.0]));

    let bounds = doc.page_bounds(0).expect("inherited bounds");
    assert_eq!(bounds.media, Rectangle::new(0.0, 0.0, 600.0, 800.0));
    assert_eq!(
        bounds.crop,
        Some(Rectangle::new(1.0, 1.0, 200.0, 200.0)),
        "/CropBox 沿 /Parent 链继承且解引用"
    );
    assert_eq!(bounds.bleed, None, "/BleedBox 不可继承");
    assert_eq!(bounds.trim, None);
    assert_eq!(bounds.art, None);

    // 页面自身声明 CropBox 时取最近声明者。
    let page = Dict::new()
        .insert("Parent", Value::Ref(11, 0))
        .insert("CropBox", float_rect([5.0, 5.0, 100.0, 100.0]));
    let doc = document_with_page(page);
    insert(
        &doc,
        11,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("CropBox", float_rect([1.0, 1.0, 200.0, 200.0]))
                .insert("Parent", Value::Ref(12, 0)),
        ),
    );
    insert(
        &doc,
        12,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("MediaBox", int_rect([0, 0, 600, 800])),
        ),
    );
    let bounds = doc.page_bounds(0).expect("nearest crop");
    assert_eq!(bounds.crop, Some(Rectangle::new(5.0, 5.0, 100.0, 100.0)));
}

#[test]
fn page_bounds_default_chain_and_media_intersection() {
    // 只声明 MediaBox：其它框保持 None（不物化缺省值），有效值沿缺省链。
    let doc = document_with_page(Dict::new().insert("MediaBox", int_rect([10, 20, 110, 220])));
    let bounds = doc.page_bounds(0).expect("media only");
    assert_eq!(bounds.media, Rectangle::new(10.0, 20.0, 110.0, 220.0));
    assert_eq!(bounds.crop, None);
    assert_eq!(bounds.bleed, None);
    assert_eq!(bounds.trim, None);
    assert_eq!(bounds.art, None);
    assert_eq!(bounds.box_color_info, None);
    for effective in [
        bounds.effective_crop(),
        bounds.effective_bleed(),
        bounds.effective_trim(),
        bounds.effective_art(),
    ] {
        assert_eq!(effective, bounds.media);
    }

    // 越出 MediaBox 的声明框按 14.11.2.1 缩减为与 MediaBox 的交集。
    let page = Dict::new()
        .insert("MediaBox", int_rect([0, 0, 100, 100]))
        .insert("CropBox", float_rect([-10.0, -10.0, 120.0, 120.0]))
        .insert("BleedBox", float_rect([-5.0, -5.0, 50.0, 50.0]))
        .insert("ArtBox", float_rect([110.0, 110.0, 130.0, 130.0]));
    let bounds = document_with_page(page).page_bounds(0).expect("clamped");
    // 声明值原样报告。
    assert_eq!(
        bounds.crop,
        Some(Rectangle::new(-10.0, -10.0, 120.0, 120.0))
    );
    assert_eq!(
        bounds.effective_crop(),
        Rectangle::new(0.0, 0.0, 100.0, 100.0)
    );
    assert_eq!(
        bounds.effective_bleed(),
        Rectangle::new(0.0, 0.0, 50.0, 50.0)
    );
    // 未声明 TrimBox 缺省等于 CropBox（有效值即 CropBox ∩ MediaBox）。
    assert_eq!(
        bounds.effective_trim(),
        Rectangle::new(0.0, 0.0, 100.0, 100.0)
    );
    // ArtBox 完全在 MediaBox 之外：交集退化为零尺寸矩形。
    let art = bounds.effective_art();
    assert_eq!(art, Rectangle::new(100.0, 100.0, 100.0, 100.0));
    assert_eq!(art.width(), 0.0);
    assert_eq!(art.height(), 0.0);
}

#[test]
fn page_bounds_reject_malformed_boxes_and_color_info() {
    // 矩形：数组长度、非数字、非有限数。
    assert_malformed(
        Dict::new().insert(
            "MediaBox",
            arr(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(100),
            ]),
        ),
        "exactly 4 numbers",
    );
    assert_malformed(
        Dict::new().insert(
            "MediaBox",
            arr(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(100),
                Value::String("100".into()),
            ]),
        ),
        "is not a number",
    );
    assert_malformed(
        Dict::new().insert("MediaBox", float_rect([0.0, 0.0, f64::NAN, 100.0])),
        "is not finite",
    );
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert("CropBox", Value::Integer(3)),
        "/CropBox is not an array",
    );

    // 框颜色信息：整体或条目不是字典。
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert("BoxColorInfo", Value::Integer(3)),
        "/BoxColorInfo is not a dictionary",
    );
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert(
                "BoxColorInfo",
                Value::Dict(Dict::new().insert("CropBox", Value::Integer(3))),
            ),
        "/BoxColorInfo /CropBox is not a dictionary",
    );

    // /C：长度与 0.0–1.0 范围。
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert(
                "BoxColorInfo",
                Value::Dict(Dict::new().insert(
                    "CropBox",
                    Value::Dict(
                        Dict::new().insert("C", arr(vec![Value::Float(0.0), Value::Float(0.0)])),
                    ),
                )),
            ),
        "exactly 3 numbers",
    );
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert(
                "BoxColorInfo",
                Value::Dict(Dict::new().insert(
                    "TrimBox",
                    Value::Dict(Dict::new().insert(
                        "C",
                        arr(vec![
                            Value::Float(0.0),
                            Value::Float(1.5),
                            Value::Float(0.0),
                        ]),
                    )),
                )),
            ),
        "between 0 and 1",
    );

    // /S 须为名称。
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert(
                "BoxColorInfo",
                Value::Dict(Dict::new().insert(
                    "ArtBox",
                    Value::Dict(Dict::new().insert("S", Value::Integer(1))),
                )),
            ),
        "/S is not a name",
    );

    // /D：空数组、全零、负数、非有限数。
    for (dash, needle) in [
        (arr(Vec::new()), "must not be empty"),
        (
            arr(vec![Value::Integer(0), Value::Float(0.0)]),
            "must not be all zero",
        ),
        (
            arr(vec![Value::Integer(3), Value::Integer(-1)]),
            "non-negative finite",
        ),
        (
            arr(vec![Value::Integer(3), Value::Float(f64::INFINITY)]),
            "non-negative finite",
        ),
    ] {
        assert_malformed(
            Dict::new()
                .insert("MediaBox", int_rect([0, 0, 100, 100]))
                .insert(
                    "BoxColorInfo",
                    Value::Dict(Dict::new().insert(
                        "CropBox",
                        Value::Dict(Dict::new().insert("S", name("D")).insert("D", dash)),
                    )),
                ),
            needle,
        );
    }

    // /W 须有限。
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert(
                "BoxColorInfo",
                Value::Dict(Dict::new().insert(
                    "CropBox",
                    Value::Dict(Dict::new().insert("W", Value::Float(f64::INFINITY))),
                )),
            ),
        "/W is not finite",
    );

    // /Parent 链成环。
    let doc = document_with_page(Dict::new().insert("Parent", Value::Ref(11, 0)));
    insert(
        &doc,
        11,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Parent", Value::Ref(12, 0)),
        ),
    );
    insert(
        &doc,
        12,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Parent", Value::Ref(11, 0)),
        ),
    );
    match doc.page_bounds(0) {
        Err(PageBoxError::Malformed(message)) => {
            assert!(message.contains("cycle"), "unexpected message {message}");
        }
        other => panic!("expected cycle error, got {other:?}"),
    }

    // /Parent 链超过 MAX_TREE_DEPTH（64）。
    let doc = document_with_page(Dict::new().insert("Parent", Value::Ref(11, 0)));
    for number in 11..90 {
        insert(
            &doc,
            number,
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Pages"))
                    .insert("Parent", Value::Ref(number + 1, 0)),
            ),
        );
    }
    match doc.page_bounds(0) {
        Err(PageBoxError::LimitExceeded(what)) => {
            assert_eq!(what, "page boundary inheritance");
        }
        other => panic!("expected depth limit error, got {other:?}"),
    }
}

#[test]
fn page_bounds_require_inheritable_media_box() {
    // 页面与全部祖先都没有 MediaBox。
    let doc = document_with_page(Dict::new().insert("Parent", Value::Ref(11, 0)));
    insert(
        &doc,
        11,
        Value::Dict(Dict::new().insert("Type", name("Pages"))),
    );
    match doc.page_bounds(0) {
        Err(PageBoxError::Malformed(message)) => {
            assert!(
                message.contains("page has no inheritable /MediaBox"),
                "unexpected message {message}"
            );
        }
        other => panic!("expected missing media box error, got {other:?}"),
    }

    // 页码越界与无页面树均为 Page 错误。
    let doc = document_with_page(Dict::new().insert("MediaBox", int_rect([0, 0, 100, 100])));
    match doc.page_bounds(9) {
        Err(PageBoxError::Page(_)) => {}
        other => panic!("expected page error, got {other:?}"),
    }
    let error = doc.page_bounds(9).expect_err("out of range");
    assert!(
        error.to_string().starts_with("invalid page box:"),
        "unexpected display {error}"
    );
    assert!(error.to_string().contains("page not found"), "{error}");

    match Document::new().page_bounds(0) {
        Err(PageBoxError::Page(_)) => {}
        other => panic!("expected page error on empty document, got {other:?}"),
    }
}

#[test]
fn page_bounds_reads_inherited_rotate_with_default_zero() {
    let plain = document_with_page(Dict::new().insert("MediaBox", int_rect([0, 0, 100, 100])));
    assert_eq!(plain.page_bounds(0).unwrap().rotate, 0);

    let own = document_with_page(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert("Rotate", Value::Integer(90)),
    );
    assert_eq!(own.page_bounds(0).unwrap().rotate, 90);

    // 页面未声明 /Rotate 时沿 /Parent 链继承页面树上的值。
    let doc = document_with_page(Dict::new().insert("MediaBox", int_rect([0, 0, 100, 100])));
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGE_OBJECT, 0)]))
                .insert("Count", Value::Integer(1))
                .insert("Rotate", Value::Integer(270)),
        ),
    );
    assert_eq!(doc.page_bounds(0).unwrap().rotate, 270);
}

#[test]
fn page_bounds_rejects_malformed_rotate() {
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert("Rotate", Value::Integer(45)),
        "multiple of 90",
    );
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert("Rotate", name("90")),
        "not an integer",
    );
    assert_malformed(
        Dict::new()
            .insert("MediaBox", int_rect([0, 0, 100, 100]))
            .insert("Rotate", Value::Integer(2_700_000_000)),
        "out of range",
    );
}
