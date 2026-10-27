//! Type 3 字体字典与字符过程测试。

use super::*;
use crate::core::types::{Array, Dict, StreamData, Value};

#[test]
fn type3_font_rejects_subset_type1_glyphs() {
    let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
    let font = Font::type3(12.0, type3).with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("Type 3 字体不应接受 subset_type1_glyphs");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("cannot be subset"),
        "错误信息应说明无字体程序可裁剪: {error}"
    );
    assert!(
        error.to_string().contains("subset_type1_glyphs"),
        "错误信息应指出被拒绝的字段: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type3_font_rejects_unknown_encoding_name() {
    let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
    let font = Font::type3(12.0, type3).with_encoding(FontEncoding::named("UnknownEncoding"));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("Type 3 未知编码名应构建失败");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("UnknownEncoding"),
        "错误信息应指出未知编码名: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type3_font_rejects_explicit_widths() {
    let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
    let font = Font::type3(12.0, type3).with_widths(65, &[500.0]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("Type 3 字体的 /Widths 由字形定义推导");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("explicit widths"),
        "错误信息应说明 /Widths 来源: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type3_font_rejects_invalid_glyph_widths() {
    for width in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", width, b"500 0 d0"));
        let font = Font::type3(12.0, type3);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "A");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法字形宽度应构建失败");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(
            error.to_string().contains("invalid width"),
            "错误信息应指出字形宽度: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn type3_font_rejects_non_finite_bbox_or_matrix() {
    for (bbox, matrix, label) in [
        (
            [0.0, f64::NAN, 1000.0, 900.0],
            [0.001, 0.0, 0.0, 0.001, 0.0, 0.0],
            "FontBBox",
        ),
        (
            [0.0, 0.0, 1000.0, 900.0],
            [0.001, 0.0, 0.0, f64::INFINITY, 0.0, 0.0],
            "FontMatrix",
        ),
    ] {
        let type3 = Type3Font::new()
            .with_bbox(bbox)
            .with_matrix(matrix)
            .glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
        let font = Font::type3(12.0, type3);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "A");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非有限几何参数应构建失败");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(
            error.to_string().contains(label),
            "错误信息应指出 {label}: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn type3_font_writes_char_procs_widths_and_encoding() {
    let type3 = Type3Font::new()
        .with_bbox([0.0, -200.0, 1000.0, 900.0])
        .with_matrix([0.002, 0.0, 0.0, 0.002, 0.0, 0.0])
        .glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"))
        .glyph(Type3Glyph::new(67, "C", 700.0, b"700 0 d0"));
    let font = Font::type3(12.0, type3);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "AC");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type3");
    assert!(
        font_dict.get("BaseFont").is_none(),
        "Type 3 字体不应写 /BaseFont"
    );
    assert!(
        font_dict.get("FontDescriptor").is_none(),
        "Type 3 字体不应写 /FontDescriptor"
    );
    assert_eq!(
        font_dict.get("FontBBox").map(numbers),
        Some(vec![0.0, -200.0, 1000.0, 900.0])
    );
    assert_eq!(
        font_dict.get("FontMatrix").map(numbers),
        Some(vec![0.002, 0.0, 0.0, 0.002, 0.0, 0.0])
    );
    assert_eq!(
        font_dict.get("FirstChar").and_then(Value::as_integer),
        Some(65)
    );
    assert_eq!(
        font_dict.get("LastChar").and_then(Value::as_integer),
        Some(67)
    );
    // 字符码 66 是空隙，宽度补 0。
    assert_eq!(
        font_dict.get("Widths").map(numbers),
        Some(vec![500.0, 0.0, 700.0])
    );

    // /Encoding 只含按字符码升序的 /Differences（无 /BaseEncoding）。
    let encoding = font_dict
        .get("Encoding")
        .and_then(Value::as_dict)
        .expect("Type 3 字体应写 /Encoding");
    assert!(
        encoding.get("BaseEncoding").is_none(),
        "Font::type3 只写 /Differences"
    );
    assert_eq!(
        encoding.get("Differences"),
        Some(&Value::Array(
            Array::new()
                .push(Value::Integer(65))
                .push(Value::Name("A".into()))
                .push(Value::Integer(67))
                .push(Value::Name("C".into()))
        ))
    );

    // /CharProcs 按字形名索引，流对象号按字形列表顺序连续分配。
    let char_procs = font_dict
        .get("CharProcs")
        .and_then(Value::as_dict)
        .expect("/CharProcs");
    let mut proc_nums = Vec::new();
    for (name, expected) in [("A", b"500 0 d0".as_slice()), ("C", b"700 0 d0".as_slice())] {
        let (proc_num, _) = char_procs
            .get(name)
            .and_then(Value::as_ref)
            .unwrap_or_else(|| panic!("缺少字符过程 {name}"));
        proc_nums.push(proc_num);
        let object = doc
            .get_object(proc_num)
            .expect("字符过程查找")
            .expect("字符过程对象");
        match object.value {
            Value::Stream(ref stream_dict, StreamData::Owned(ref bytes)) => {
                assert_eq!(bytes, expected, "{name} 的字符过程字节");
                assert_eq!(
                    stream_dict.get("Length").and_then(Value::as_integer),
                    Some(expected.len() as i64),
                    "{name} 流的 /Length"
                );
            }
            _ => panic!("字符过程应为 Owned 流"),
        }
    }
    assert_eq!(
        proc_nums[1],
        proc_nums[0] + 1,
        "CharProcs 流应按字形列表顺序连续编号"
    );

    // Type 3 字体不写字体程序（含描述符内的 /FontFile*）。
    assert!(!render(&mut b).contains("/FontFile"), "不应写字体程序");
}

#[test]
fn type3_font_requires_glyphs() {
    let font = Font::type3(12.0, Type3Font::new());
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("空字形列表应构建失败");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("at least one glyph"),
        "错误信息应说明缺少字形: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type3_font_rejects_duplicate_codes_or_names() {
    for (glyphs, needle) in [
        (
            vec![
                Type3Glyph::new(65, "A", 500.0, b"500 0 d0"),
                Type3Glyph::new(65, "B", 600.0, b"600 0 d0"),
            ],
            "duplicate character code 65",
        ),
        (
            vec![
                Type3Glyph::new(65, "A", 500.0, b"500 0 d0"),
                Type3Glyph::new(66, "A", 600.0, b"600 0 d0"),
            ],
            "duplicate glyph name",
        ),
    ] {
        let mut type3 = Type3Font::new();
        for glyph in glyphs {
            type3 = type3.glyph(glyph);
        }
        let font = Font::type3(12.0, type3);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "AB");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("重复定义应构建失败");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(
            error.to_string().contains(needle),
            "错误信息应包含 {needle:?}: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn type3_font_rejects_embedded_font_program() {
    let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
    let font = Font::type3(12.0, type3)
        .with_font_program(FontProgram::Type1(b"%!PS-AdobeFont-1.0: TestFont".to_vec()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("Type 3 字体不应接受嵌入字体程序");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("must not embed a font program"),
        "错误信息应说明不允许字体程序: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type3_font_rejects_subset_glyphs() {
    let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
    let font = Font::type3(12.0, type3).with_subset_glyphs(&[0]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("Type 3 字体不应接受 subset_glyphs");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("cannot be subset"),
        "错误信息应说明无字体程序可裁剪: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

/// 单字形 Type 3 字体（字符码 65，字形名 A）。
fn single_glyph_type3() -> Type3Font {
    Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"))
}

/// 字形程序可引用的示例资源字典。
fn sample_resources() -> Dict {
    Dict::new()
        .insert(
            "XObject",
            Value::Dict(Dict::new().insert("Im1", Value::Ref(40, 0))),
        )
        .insert(
            "ProcSet",
            Value::Array(Array::new().push(Value::Name("PDF".into()))),
        )
}

#[test]
fn type3_font_writes_resources() {
    let resources = sample_resources();
    let font = Font::type3(
        12.0,
        single_glyph_type3().with_resources(Value::Dict(resources.clone())),
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type3");
    assert_eq!(
        font_dict.get("Resources"),
        Some(&Value::Dict(resources)),
        "字体字典应原样写出 /Resources"
    );
    let s = render(&mut b);
    assert!(
        s.contains("/XObject"),
        "序列化结果应含 /Resources 的嵌套条目: {s}"
    );
    assert!(
        s.contains("/ProcSet"),
        "序列化结果应含 /Resources 的嵌套条目: {s}"
    );
}

#[test]
fn type3_font_writes_indirect_resources_as_is() {
    // 间接引用原样写出，不尝试注册或解析目标对象。
    let font = Font::type3(12.0, single_glyph_type3().with_resources(Value::Ref(9, 0)));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type3");
    assert_eq!(
        font_dict.get("Resources"),
        Some(&Value::Ref(9, 0)),
        "间接引用应原样写出"
    );
}

#[test]
fn type3_font_omits_resources_when_unset() {
    let font = Font::type3(12.0, single_glyph_type3());

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type3");
    assert!(
        font_dict.get("Resources").is_none(),
        "未设置时不应写 /Resources"
    );
}

#[test]
fn type3_font_rejects_non_dict_resources() {
    for resources in [
        Value::Integer(1),
        Value::Name("R".into()),
        Value::Array(Array::new()),
        Value::Null,
    ] {
        let font = Font::type3(12.0, single_glyph_type3().with_resources(resources));

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "A");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非字典 /Resources 应构建失败");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(
            error
                .to_string()
                .contains("/Resources must be a dictionary"),
            "错误信息应说明 /Resources 类型不合法: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn type3_font_resources_round_trip() {
    let resources = sample_resources();
    let font = Font::type3(
        12.0,
        single_glyph_type3().with_resources(Value::Dict(resources.clone())),
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type3");
    let parsed = doc
        .type3_font(&Value::Dict(font_dict))
        .expect("解析写出的字体字典")
        .expect("应为 Type 3 字体");
    assert_eq!(
        parsed.resources(&doc).expect("读端解析 /Resources"),
        Some(resources),
        "读端应还原同一资源字典"
    );
}

/// Table 112 可选条目：`/Name`、`/FontDescriptor`（无 FontFile）与 `/ToUnicode`。
#[test]
fn type3_font_writes_name_descriptor_and_to_unicode() {
    let type3 = single_glyph_type3()
        .with_name("SampleType3")
        .with_to_unicode(65, "Ω");
    let descriptor = FontDescriptor::new()
        .with_family("Sample")
        .with_ascent(800.0);
    let font = Font::type3(12.0, type3).with_descriptor(descriptor);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type3");
    assert_eq!(
        font_dict.get("Name"),
        Some(&Value::Name("SampleType3".into()))
    );

    let (descriptor_num, _) = font_dict
        .get("FontDescriptor")
        .and_then(Value::as_ref)
        .expect("应写 /FontDescriptor");
    let descriptor_object = doc
        .get_object(descriptor_num)
        .expect("descriptor lookup")
        .expect("descriptor object");
    let Value::Dict(descriptor_dict) = descriptor_object.value else {
        panic!("descriptor 应为字典");
    };
    assert_eq!(
        descriptor_dict.get("Type").and_then(Value::as_name),
        Some("FontDescriptor")
    );
    assert_eq!(
        descriptor_dict.get("FontName").and_then(Value::as_name),
        Some("Type3")
    );
    assert_eq!(
        descriptor_dict.get("FontFamily"),
        Some(&Value::String("Sample".into()))
    );
    assert!(
        matches!(descriptor_dict.get("Ascent"), Some(Value::Float(value)) if (*value - 800.0).abs() < 1e-9),
        "Ascent 应为 800"
    );
    for key in ["FontFile", "FontFile2", "FontFile3"] {
        assert!(
            descriptor_dict.get(key).is_none(),
            "Type 3 描述符不应写 {key}"
        );
    }

    let (cmap_num, _) = font_dict
        .get("ToUnicode")
        .and_then(Value::as_ref)
        .expect("应写 /ToUnicode");
    let cmap_object = doc
        .get_object(cmap_num)
        .expect("cmap lookup")
        .expect("cmap object");
    let Value::Stream(_, StreamData::Owned(bytes)) = cmap_object.value else {
        panic!("ToUnicode 应为流");
    };
    let cmap = String::from_utf8_lossy(&bytes);
    assert!(
        cmap.contains("beginbfchar"),
        "ToUnicode CMap 应含 bfchar: {cmap}"
    );
    assert!(cmap.contains("<41> <03A9>"), "字符码 65 应映射到 Ω: {cmap}");
}

/// 非法 `/Name` 在构建期报错，且不写出任何字节。
#[test]
fn type3_font_rejects_invalid_name() {
    for name in ["", "Bad Name", "Bad#Name", "Bad/Name"] {
        let font = Font::type3(12.0, single_glyph_type3().with_name(name));

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "A");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法 /Name 应构建失败");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(
            error.to_string().contains("invalid /Name"),
            "错误信息应说明 /Name 非法: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}
