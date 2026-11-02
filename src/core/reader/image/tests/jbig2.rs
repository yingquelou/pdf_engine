//! §7.4.7 JBIG2Decode：段语法层元数据（字节取自规范 worked example）——
//! 段链、`/JBIG2Globals` 解析与类型化一致性发现。

use super::support::{dict, doc_with, document_with_page, image_dict, stream};
use crate::core::reader::content::InlineImage;
use crate::core::reader::image::{ImageError, Jbig2Finding, Jbig2SegmentKind};
use crate::core::types::{Array, Dict, Document, Value};

/// Example 1 图像 XObject 流（对象 5）：页面信息段 + 立即文本区域段，72 字节。
const SPEC_JBIG2_PAGE_STREAM: &[u8] = &[
    0x00, 0x00, 0x00, 0x01, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00, 0x34, 0x00,
    0x00, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x02, 0x06, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x34, 0x00, 0x00,
    0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x10, 0x00, 0x00, 0x00,
    0x02, 0x31, 0xDB, 0x51, 0xCE, 0x51, 0xFF, 0xAC,
];

/// Example 1 全局段流（对象 6）：符号字典段（页关联 0，载荷 50 字节），61 字节。
const SPEC_JBIG2_GLOBALS_STREAM: &[u8] = &[
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x03, 0xFF, 0xFD,
    0xFF, 0x02, 0xFE, 0xFE, 0xFE, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x2A, 0xE2, 0x25,
    0xAE, 0xA9, 0xA5, 0xA5, 0x38, 0xB4, 0xD9, 0x99, 0x9C, 0x5C, 0x8E, 0x56, 0xEF, 0x0F, 0x87, 0x27,
    0xF2, 0xB5, 0x3D, 0x4E, 0x37, 0xEF, 0x79, 0x5C, 0xC5, 0x50, 0x6D, 0xFF, 0xAC,
];

/// Example 2 的 end-of-page 段与 end-of-file 段（EOF 按 §7.2.1 补全为 11 字节，
/// 规范印刷只有 10 字节）。
const SPEC_JBIG2_TAIL_SEGMENTS: &[u8] = &[
    0x00, 0x00, 0x00, 0x03, 0x31, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x33,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
];

fn jbig2_image_dict() -> Dict {
    image_dict(52, 66, "DeviceGray", 1).insert("Filter", Value::Name("JBIG2Decode".into()))
}

#[test]
fn jbig2_image_exposes_segment_chain_and_reports_missing_globals() {
    let doc = doc_with(vec![(
        10,
        stream(jbig2_image_dict(), SPEC_JBIG2_PAGE_STREAM),
    )]);
    let image = doc.image(&Value::Ref(10, 0)).expect("jbig2 image parses");
    // JBIG2 步骤透传：data 是原始段字节而不是样本。
    assert_eq!(image.data, SPEC_JBIG2_PAGE_STREAM);
    assert_eq!(image.filters, vec!["JBIG2Decode".to_string()]);
    let jbig2 = image.jbig2.as_ref().expect("jbig2 view present");
    let kinds: Vec<Jbig2SegmentKind> = jbig2.page.segments.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![
            Jbig2SegmentKind::PageInformation,
            Jbig2SegmentKind::ImmediateTextRegion,
        ]
    );
    // 文本区域段（段 2）以 1 字节引用表指向符号字典（段 0）。
    assert_eq!(jbig2.page.segments[1].referred_to_segments, vec![0]);
    assert_eq!(jbig2.page_segment_count(), 2);
    assert_eq!(jbig2.global_segment_count(), 0);
    let info = jbig2.page_info().expect("page information parsed");
    assert_eq!((info.width, info.height), (52, 66));
    // 引用悬空且无 /JBIG2Globals：类型化发现，而不是解析失败。
    assert_eq!(
        jbig2.findings,
        vec![Jbig2Finding::Jbig2GlobalsMissing {
            dangling_reference_count: 1,
        }]
    );
}

#[test]
fn jbig2_globals_decode_parm_is_resolved_through_indirect_reference() {
    let params = Value::Dict(dict(vec![("JBIG2Globals", Value::Ref(11, 0))]));
    let doc = doc_with(vec![
        (
            10,
            stream(
                jbig2_image_dict().insert("DecodeParms", params),
                SPEC_JBIG2_PAGE_STREAM,
            ),
        ),
        (11, stream(Dict::new(), SPEC_JBIG2_GLOBALS_STREAM)),
    ]);
    let image = doc.image(&Value::Ref(10, 0)).expect("jbig2 image parses");
    let jbig2 = image.jbig2.as_ref().expect("jbig2 view present");
    assert_eq!(jbig2.globals_reference, Some((11, 0)));
    let globals = jbig2.globals.as_ref().expect("globals stream resolved");
    assert_eq!(globals.segments.len(), 1);
    let symbol = &globals.segments[0];
    assert_eq!(symbol.kind, Jbig2SegmentKind::SymbolDictionary);
    assert!(symbol.is_global());
    assert_eq!(symbol.data_size, 50);
    assert_eq!(symbol.data.len(), 50);
    assert_eq!(jbig2.global_segment_count(), 1);
    assert!(jbig2.findings.is_empty());
}

#[test]
fn decode_parms_array_with_null_entry_still_finds_jbig2_globals() {
    // Example 1 原样：/Filter [ /ASCIIHexDecode /JBIG2Decode ]，
    // /DecodeParms [ null << /JBIG2Globals 6 0 R >> ]。
    let filter = Value::Array(Array(vec![
        Value::Name("ASCIIHexDecode".into()),
        Value::Name("JBIG2Decode".into()),
    ]));
    let params = Value::Array(Array(vec![
        Value::Null,
        Value::Dict(dict(vec![("JBIG2Globals", Value::Ref(11, 0))])),
    ]));
    let dict = jbig2_image_dict()
        .insert("Filter", filter)
        .insert("DecodeParms", params);
    // 流数据是段字节的 ASCIIHex 形式（含换行与 '>' 终止符）。
    let hex_text: String = SPEC_JBIG2_PAGE_STREAM
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .chunks(16)
        .map(|row| row.join(""))
        .collect::<Vec<_>>()
        .join("\n");
    let doc = doc_with(vec![
        (10, stream(dict, format!("{hex_text}>\n"))),
        (11, stream(Dict::new(), SPEC_JBIG2_GLOBALS_STREAM)),
    ]);
    let image = doc
        .image(&Value::Ref(10, 0))
        .expect("hex+jbig2 chain parses");
    // ASCIIHex 步骤照常解码；JBIG2 步骤透传原始段字节。
    assert_eq!(image.data, SPEC_JBIG2_PAGE_STREAM);
    assert_eq!(
        image.filters,
        vec!["ASCIIHexDecode".to_string(), "JBIG2Decode".to_string()]
    );
    let jbig2 = image.jbig2.as_ref().expect("jbig2 view present");
    assert_eq!(jbig2.globals_reference, Some((11, 0)));
    assert_eq!(
        jbig2
            .globals
            .as_ref()
            .expect("globals resolved from array entry")
            .segments
            .len(),
        1
    );
    assert!(jbig2.findings.is_empty());
}

#[test]
fn image_model_exposes_jbig2_geometry_through_public_api() {
    let doc = document_with_page(
        Some(Value::Dict(dict(vec![(
            "XObject",
            Value::Dict(dict(vec![("Im1", Value::Ref(10, 0))])),
        )]))),
        vec![(10, stream(jbig2_image_dict(), SPEC_JBIG2_PAGE_STREAM))],
    );
    let image = doc
        .page_image(0, "Im1")
        .expect("lookup")
        .expect("image exists");
    let jbig2 = image.jbig2.as_ref().expect("jbig2 view present");
    let info = jbig2.page_info().expect("page information parsed");
    assert_eq!((info.width, info.height), (52, 66));
    assert_eq!((info.x_resolution, info.y_resolution), (0, 0));
    assert_eq!(info.combination_operator, 0);
    assert_eq!(info.combination_operator_name(), "OR");
    assert!(info.combination_operator_override);
    assert!(!info.striped);
    // 页关联只出现 1（0/1 之外无违例）。
    assert!(jbig2.unexpected_page_associations().is_empty());
    assert_eq!(jbig2.page_segment_count(), 2);
}

#[test]
fn jbig2_global_segment_embedded_in_image_stream_is_reported() {
    let mut data = SPEC_JBIG2_GLOBALS_STREAM.to_vec();
    data.extend_from_slice(SPEC_JBIG2_PAGE_STREAM);
    data.extend_from_slice(SPEC_JBIG2_TAIL_SEGMENTS);
    let doc = doc_with(vec![(10, stream(jbig2_image_dict(), data))]);
    let image = doc
        .image(&Value::Ref(10, 0))
        .expect("non-conforming stream still models");
    let jbig2 = image.jbig2.as_ref().expect("jbig2 view present");
    assert_eq!(jbig2.page.segments.len(), 5);
    assert!(
        jbig2
            .findings
            .contains(&Jbig2Finding::GlobalSegmentInImageStream { segment_number: 0 })
    );
    // 文件级禁用段「存在但上报」：图像级发现与段链发现都保留。
    assert!(
        jbig2
            .page
            .findings
            .contains(&Jbig2Finding::EndOfPageSegmentPresent { segment_number: 3 })
    );
    assert!(
        jbig2
            .page
            .findings
            .contains(&Jbig2Finding::EndOfFileSegmentPresent { segment_number: 4 })
    );
}

#[test]
fn jbig2_samples_are_rejected_as_unsupported() {
    let doc = doc_with(vec![(
        10,
        stream(jbig2_image_dict(), SPEC_JBIG2_PAGE_STREAM),
    )]);
    let image = doc.image(&Value::Ref(10, 0)).expect("jbig2 image parses");
    let error = image
        .samples()
        .expect_err("segment bytes are not raw samples");
    assert!(
        matches!(error, ImageError::Unsupported(_)),
        "unexpected error: {error}"
    );
    assert!(
        error.to_string().contains("JBIG2Decode"),
        "unexpected error: {error}"
    );
}

#[test]
fn jbig2_inline_image_is_rejected_by_the_spec_rule() {
    // §7.4.7：JBIG2Decode shall not be used with inline images。
    let doc = Document::new();
    let error = doc
        .inline_image(&InlineImage {
            dict: vec![
                ("W".into(), Value::Float(52.0)),
                ("H".into(), Value::Float(66.0)),
                ("BPC".into(), Value::Float(1.0)),
                ("CS".into(), Value::Name("G".into())),
                ("F".into(), Value::Name("JBIG2Decode".into())),
            ],
            data: SPEC_JBIG2_PAGE_STREAM.to_vec(),
        })
        .expect_err("inline JBIG2 must be rejected");
    assert!(
        matches!(error, ImageError::Invalid(_)),
        "unexpected error: {error}"
    );
    assert!(
        error.to_string().contains("inline images"),
        "unexpected error: {error}"
    );
}
