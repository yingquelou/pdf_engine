//! CMap 流字典 `/UseCMap`（9.7.5.3，Table 120）的写→解析→提取往返。
//!
//! Table 120 允许 `/UseCMap` 取「预定义 CMap 的名字」或「一个含 CMap 的流」。
//! 这里用手搭的最小文档验证流形态：`/Encoding` 与 `/ToUnicode` 都是内嵌 CMap
//! 流，且各自的流字典 `/UseCMap` 指向另一个内嵌 CMap 基座流。断言只在基座里
//! 声明的字符码 `<0042>` 仍能取到继承的 CID 与 Unicode——即文本确实来自
//! 继承映射，而不是本地映射。

use super::*;

/// `/Encoding` 基座流：`<0041>`、`<0042>` 两个码。
const ENCODING_BASE: &str = "\
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
2 begincidchar
<0041> 1
<0042> 2
endcidchar";

/// `/Encoding` 引用流：只声明与基座不同的 `<0041>`（覆盖）与 `<0043>`（新增）。
const ENCODING_LOCAL: &str = "\
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
2 begincidchar
<0041> 9
<0043> 3
endcidchar";

/// `/ToUnicode` 基座流：`<0042>` 只在基座里出现。
const TOUNICODE_BASE: &str = "\
2 beginbfchar
<0041> <004D>
<0042> <004E>
endbfchar";

/// `/ToUnicode` 引用流：`<0041>` 覆盖基座、`<0043>` 只在本地。
const TOUNICODE_LOCAL: &str = "\
2 beginbfchar
<0041> <0050>
<0043> <004F>
endbfchar";

/// 带 `/UseCMap` 指向 `base` 对象号的 CMap 流字典（Table 120 的 stream 形态）。
fn cmap_stream_dict(base: i64) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("CMap".into()))
        .insert("CMapName", Value::Name("Example-UseCMap".into()))
        .insert("CMapType", Value::Integer(2))
        .insert(
            "CIDSystemInfo",
            Value::Dict(
                Dict::new()
                    .insert("Registry", Value::String("Adobe".into()))
                    .insert("Ordering", Value::String("Identity".into()))
                    .insert("Supplement", Value::Integer(0)),
            ),
        )
        .insert("UseCMap", Value::Ref(base, 0))
}

fn cmap_stream(base: i64, body: &str) -> Value {
    Value::Stream(
        cmap_stream_dict(base),
        StreamData::Owned(body.as_bytes().to_vec()),
    )
}

#[test]
fn use_cmap_stream_bases_survive_the_writer_and_supply_inherited_mappings() {
    let mut content = core::content::ContentStream::new();
    content
        .begin_text()
        .set_font("F1", 18.0)
        .move_next_line(72.0, 700.0)
        .show_text_hex([0x00u8, 0x41, 0x00, 0x42, 0x00, 0x43])
        .end_text();

    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Contents", Value::Ref(5, 0))
                .insert(
                    "Resources",
                    Value::Dict(Dict::new().insert(
                        "Font",
                        Value::Dict(Dict::new().insert("F1", Value::Ref(4, 0))),
                    )),
                ),
        ),
    ));
    // /Encoding 与 /ToUnicode 都是「带 /UseCMap 的内嵌 CMap 流」。
    doc.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type0".into()))
                .insert("BaseFont", Value::Name("ExampleComposite".into()))
                .insert("Encoding", Value::Ref(7, 0))
                .insert(
                    "DescendantFonts",
                    Value::Array(Array::new().push(Value::Ref(6, 0))),
                )
                .insert("ToUnicode", Value::Ref(9, 0)),
        ),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(content.to_bytes())),
    ));
    doc.set_object(Object::new(
        6,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("CIDFontType2".into()))
                .insert("BaseFont", Value::Name("ExampleComposite".into()))
                .insert(
                    "CIDSystemInfo",
                    Value::Dict(
                        Dict::new()
                            .insert("Registry", Value::String("Adobe".into()))
                            .insert("Ordering", Value::String("Identity".into()))
                            .insert("Supplement", Value::Integer(0)),
                    ),
                )
                .insert("CIDToGIDMap", Value::Name("Identity".into()))
                .insert("FontDescriptor", Value::Ref(11, 0)),
        ),
    ));
    doc.set_object(Object::new(7, 0, cmap_stream(8, ENCODING_LOCAL)));
    doc.set_object(Object::new(
        8,
        0,
        Value::Stream(
            Dict::new(),
            StreamData::Owned(ENCODING_BASE.as_bytes().to_vec()),
        ),
    ));
    doc.set_object(Object::new(9, 0, cmap_stream(10, TOUNICODE_LOCAL)));
    doc.set_object(Object::new(
        10,
        0,
        Value::Stream(
            Dict::new(),
            StreamData::Owned(TOUNICODE_BASE.as_bytes().to_vec()),
        ),
    ));
    doc.set_object(Object::new(
        11,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontName", Value::Name("ExampleComposite".into())),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(12)),
    };

    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let reparsed = parse_buf(&buf);

    // 文本提取：P（本地覆盖）、N（只有基座有）、O（本地新增）。
    assert_eq!(
        reparsed.extract_page_text(0).as_deref(),
        Some("PNO"),
        "只由 /UseCMap 基座提供的映射必须参与文本提取"
    );

    // 编码链：9（本地覆盖）、2（只有基座有）、3（本地新增）。
    let font_dict = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&font_dict)
        .expect("基座可解析时字体应可读")
        .expect("应为 Type 0 字体");
    let decoded = font
        .decode_codes(&[0x00, 0x41, 0x00, 0x42, 0x00, 0x43])
        .unwrap();
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        vec![9, 2, 3],
        "只由 /UseCMap 基座提供的码必须取到继承 CID"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.clone())
            .collect::<Vec<_>>(),
        vec![Some("P".into()), Some("N".into()), Some("O".into())]
    );
}

#[test]
fn use_cmap_dangling_base_still_extracts_local_text_through_parser() {
    // 悬空基座：按「无基座」降级，本地映射必须照旧可用（不得让此前可读的
    // 字体变成不可读）。只测本地已映射的两个码，避免与既有的按 CID 兜底行为纠缠。
    let mut content = core::content::ContentStream::new();
    content
        .begin_text()
        .set_font("F1", 18.0)
        .move_next_line(72.0, 700.0)
        .show_text_hex([0x00u8, 0x41, 0x00, 0x43])
        .end_text();

    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Contents", Value::Ref(5, 0))
                .insert(
                    "Resources",
                    Value::Dict(Dict::new().insert(
                        "Font",
                        Value::Dict(Dict::new().insert("F1", Value::Ref(4, 0))),
                    )),
                ),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type0".into()))
                .insert("BaseFont", Value::Name("ExampleComposite".into()))
                .insert("Encoding", Value::Name("Identity-H".into()))
                .insert(
                    "DescendantFonts",
                    Value::Array(Array::new().push(Value::Ref(6, 0))),
                )
                .insert("ToUnicode", Value::Ref(9, 0)),
        ),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(content.to_bytes())),
    ));
    doc.set_object(Object::new(
        6,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("CIDFontType2".into()))
                .insert("BaseFont", Value::Name("ExampleComposite".into()))
                .insert(
                    "CIDSystemInfo",
                    Value::Dict(
                        Dict::new()
                            .insert("Registry", Value::String("Adobe".into()))
                            .insert("Ordering", Value::String("Identity".into()))
                            .insert("Supplement", Value::Integer(0)),
                    ),
                )
                .insert("CIDToGIDMap", Value::Name("Identity".into()))
                .insert("FontDescriptor", Value::Ref(7, 0)),
        ),
    ));
    doc.set_object(Object::new(
        7,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontName", Value::Name("ExampleComposite".into())),
        ),
    ));
    // 9 号流的 /UseCMap 指向不存在的 99 号；本地只有 <0041>。
    doc.set_object(Object::new(9, 0, cmap_stream(99, TOUNICODE_LOCAL)));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(10)),
    };

    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let reparsed = parse_buf(&buf);
    assert_eq!(
        reparsed.extract_page_text(0).as_deref(),
        Some("PO"),
        "坏基座只应丢掉继承部分（对照上一例的 PNO），本地映射仍要能提取"
    );
}
