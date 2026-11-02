//! 附录 F 线性化 PDF 测试：验证线性化文件按普通路径完整可读。

use super::support::*;
use super::*;
use crate::core::filters::decode_stream;
use crate::core::types::{PdfVersion, Value};

// ── 附录 F：线性化容忍验证 ─────────────────────────────────────────────
//
// 线性化 PDF（"fast web view"）把线性化参数字典放在文件第一个对象、首页
// 对象组放前部、末尾携带 hint 流。本库读端**不利用 hint 流、不解读线性化
// 字典**——本节专项验证线性化文件按普通 PDF 路径即可完整读取：解析无
// 错误、无告警回退、全部对象可解析、流可解码。样本由构造器手工生成，
// `/L /H /E /T` 按实际布局取真实值（定点迭代收敛），保证样本在规范意义
// 下成立（ISO 32000-1 附录 F）。

/// 主验收：经典 xref 线性化 PDF 按普通路径完整可读——无错误、无告警、
/// 全部对象可解析、内容流与 hint 流均可作为普通流解码，`/L` 与真实文件
/// 长度一致（构造器收敛证据）。
#[test]
fn linearized_classic_xref_readable_via_normal_path() {
    let hint_payload: &[u8] = b"first-page hint data (never parsed by this library)";
    let raw = build_linearized_classic(2, hint_payload, false);
    let (f, _path) = write_tmp(&raw, "linearized-classic");
    let mut p = Parser::new(f).expect("parser");
    let doc = p
        .parse_document()
        .expect("linearized pdf must parse via normal path");
    assert!(
        doc.warnings().is_empty(),
        "unexpected warnings: {:?}",
        doc.warnings()
    );
    assert_eq!(doc.source_version(), PdfVersion::V1_4);
    assert_eq!(doc.trailer().root(), Some((3, 0)));

    // 对象 1：线性化参数字典按普通间接对象解析；/L 为真实文件总长。
    let lin = doc
        .get_object(1)
        .expect("parse")
        .expect("linearization dict");
    let d = lin.value.as_dict().unwrap();
    assert_eq!(d.get("Linearized").and_then(|v| v.as_integer()), Some(1));
    assert_eq!(d.get("N").and_then(|v| v.as_integer()), Some(1));
    assert_eq!(d.get("O").and_then(|v| v.as_integer()), Some(5));
    assert_eq!(
        d.get("L").and_then(|v| v.as_integer()),
        Some(raw.len() as i64)
    );
    assert_eq!(
        d.get("H").and_then(|v| v.as_array()).map(|a| a.len()),
        Some(4)
    );

    // 首页对象组与其余对象均可按号解析。
    for (num, ty) in [(3i64, "Catalog"), (4, "Pages"), (5, "Page"), (7, "Font")] {
        let o = doc.get_object(num).expect("parse").expect("object");
        assert_eq!(
            o.value
                .as_dict()
                .unwrap()
                .get("Type")
                .and_then(|v| v.as_name()),
            Some(ty)
        );
    }

    // 内容流与 hint 流：均按普通流读取并解码回原始载荷。
    assert_eq!(decoded_stream(&doc, 6), LINEARIZED_CONTENT_OPS.to_vec());
    assert_eq!(decoded_stream(&doc, 2), hint_payload.to_vec());

    // 全量预载：7 个对象全部入缓存，仍无告警。
    doc.load_all_objects().expect("load all");
    assert_eq!(doc.cached_object_count(), 7);
    assert!(doc.warnings().is_empty());
}

/// `/H` 的两种合法形态（仅首页 hint 对 / 首页 + 整体 hint 对）均可读取。
#[test]
fn linearized_hint_pair_variants_accepted() {
    for pairs in [1usize, 2] {
        let raw = build_linearized_classic(pairs, b"hint", false);
        let (f, _path) = write_tmp(&raw, "linearized-h-variants");
        let mut p = Parser::new(f).expect("parser");
        let doc = p.parse_document().expect("parse");
        assert!(
            doc.warnings().is_empty(),
            "pairs={pairs}: unexpected warnings: {:?}",
            doc.warnings()
        );
        let lin = doc.get_object(1).expect("parse").expect("dict");
        let h = lin
            .value
            .as_dict()
            .unwrap()
            .get("H")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(h.len(), pairs * 2);
    }
}

/// hint 流声明 `/FlateDecode` 但载荷损坏：文档级读取完全不受影响（读端
/// 不解析 hint 语义），流对象仍可解析、原始字节可取；仅显式解码该流时
/// 才报 `DecodeError`。
#[test]
fn linearized_corrupt_hint_stream_tolerated() {
    let raw = build_linearized_classic(1, b"unused", true);
    let (f, _path) = write_tmp(&raw, "linearized-corrupt-hint");
    let mut p = Parser::new(f).expect("parser");
    let doc = p.parse_document().expect("parse");
    assert!(
        doc.warnings().is_empty(),
        "unexpected warnings: {:?}",
        doc.warnings()
    );

    // hint 流对象可解析、原始（仍编码的）字节可取。
    let hint = doc.get_object(2).expect("parse").expect("hint stream");
    let raw_bytes = doc.stream_bytes(&hint.value).expect("raw stream bytes");
    assert!(!raw_bytes.is_empty());
    match &hint.value {
        Value::Stream(dict, _) => {
            let e = decode_stream(dict, &raw_bytes).expect_err("corrupt hint must not decode");
            assert!(!e.0.is_empty());
        }
        other => panic!("object 2 is not a stream: {other:?}"),
    }

    // 其余对象不受影响：Page 可解析、内容流可解码。
    let page = doc.get_object(5).expect("parse").expect("page");
    assert_eq!(
        page.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Page")
    );
    assert_eq!(decoded_stream(&doc, 6), LINEARIZED_CONTENT_OPS.to_vec());
}

/// PDF 1.5 变体：线性化字典仍为文件第一个普通对象（不打入 ObjStm），
/// Catalog/Pages/Page/Font 打包在对象流内，trailer 语义由 XRef 流承载。
/// 按普通路径读取同样无错误、无告警。
#[test]
fn linearized_xref_stream_and_objstm_readable() {
    let hint_payload: &[u8] = b"page and shared hint groups";
    let raw = build_linearized_xref_stream(hint_payload);
    let (f, _path) = write_tmp(&raw, "linearized-xref-stream");
    let mut p = Parser::new(f).expect("parser");
    let doc = p.parse_document().expect("parse");
    assert!(
        doc.warnings().is_empty(),
        "unexpected warnings: {:?}",
        doc.warnings()
    );
    assert_eq!(doc.source_version(), PdfVersion::V1_5);

    // 线性化字典（普通对象）与 /L 真实性。
    let lin = doc
        .get_object(1)
        .expect("parse")
        .expect("linearization dict");
    let d = lin.value.as_dict().unwrap();
    assert_eq!(d.get("Linearized").and_then(|v| v.as_integer()), Some(1));
    assert_eq!(
        d.get("L").and_then(|v| v.as_integer()),
        Some(raw.len() as i64)
    );

    // ObjStm 内对象（type 2 条目驱动）与普通流均可解析。
    for (num, ty) in [(3i64, "Catalog"), (4, "Pages"), (5, "Page"), (7, "Font")] {
        let o = doc.get_object(num).expect("parse").expect("object");
        assert_eq!(
            o.value
                .as_dict()
                .unwrap()
                .get("Type")
                .and_then(|v| v.as_name()),
            Some(ty)
        );
    }
    assert_eq!(decoded_stream(&doc, 6), LINEARIZED_CONTENT_OPS.to_vec());
    assert_eq!(decoded_stream(&doc, 2), hint_payload.to_vec());

    // 全量预载：对象 1–9 共 9 个入缓存（0 为 free 跳过）。
    doc.load_all_objects().expect("load all");
    assert_eq!(doc.cached_object_count(), 9);
    assert!(doc.warnings().is_empty());
}
