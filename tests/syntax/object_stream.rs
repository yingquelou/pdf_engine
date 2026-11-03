//! 对象流子主题（7.5.8.2 ObjStm 打包对象解析与过滤器失败降级）。

use super::*;

/// 未压缩对象流（ObjStm）正常路径：XRef 流 type 2 条目指向对象流内
/// 的对象，`get_object` 应能解析并取值正确。
#[test]
fn uncompressed_obj_stm_resolves_inner_object() {
    let buf = obj_stm_pdf("", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);

    // 顶层对象（type 1）仍可正常取出，并正确引用对象 5。
    let cat = doc.get_object(1).expect("parse").expect("catalog present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_ref()),
        Some((5, 0))
    );

    // 对象 5 位于对象流内，type 2 条目应驱动对象流解析。
    let inner = doc
        .get_object(5)
        .expect("parse")
        .expect("object inside object stream");
    assert_eq!(inner.num, 5);
    assert_eq!(inner.generation, 0);
    assert_eq!(
        inner
            .value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("InObjStm")
    );
    // 再次请求命中缓存，不重复解析。
    assert!(doc.get_object(5).expect("parse").is_some());

    assert!(
        doc.warnings().is_empty(),
        "unexpected warnings: {:?}",
        doc.warnings()
    );
}

/// 压缩对象流（`/Filter /FlateDecode`，真压缩）：对象流内对象应可解码取出
/// （修复前记 warning + 返回 None）。
#[test]
fn flate_compressed_obj_stm_resolves_inner_object() {
    let buf = obj_stm_pdf("FlateDecode", true);
    let doc = parse_buf(&buf);
    let inner = doc
        .get_object(5)
        .expect("parse")
        .expect("object inside flate-compressed object stream");
    assert_eq!(inner.num, 5);
    assert_eq!(
        inner
            .value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("InObjStm")
    );
    assert!(
        doc.warnings().is_empty(),
        "flate object stream should decode cleanly: {:?}",
        doc.warnings()
    );
}

/// 不可解码过滤器（`/Filter /DCTDecode`）的对象流：不得 panic，
/// `get_object` 返回 None 且 warnings 有说明。
#[test]
fn undecodable_filter_obj_stm_fails_gracefully() {
    let buf = obj_stm_pdf("DCTDecode", false);
    let doc = parse_buf(&buf);
    let got = doc.get_object(5).expect("get_object must not error");
    assert!(
        got.is_none(),
        "undecodable object stream should not resolve"
    );
    assert!(
        doc.warnings()
            .iter()
            .any(|w| w.contains("DCTDecode") && w.contains("compressed")),
        "expected an explanatory warning about DCTDecode, got: {:?}",
        doc.warnings()
    );
}

/// 目标版本 1.5：非流对象（Catalog/Pages）打包进 ObjStm（type 2 条目），
/// 流对象仍是顶层对象（type 1 条目）；回读能取到打包对象与流对象。
#[test]
fn object_stream_packs_non_stream_objects_at_1_5() {
    let doc = sample_doc();
    let buf = try_write(&doc, PdfVersion::V1_5).expect("write");
    let out = String::from_utf8_lossy(&buf);

    assert!(out.contains("/Type /ObjStm"), "应输出对象流：{out}");
    assert!(out.contains("/N 2"), "对象流应打包 2 个非流对象");
    assert!(out.contains("/First "), "对象流应含 /First");
    // 流对象（内容流）仍在顶层，未被塞进对象流。
    assert!(
        !out[..out.find("/Type /ObjStm").unwrap()].contains("endobj\n1 0 obj"),
        "对象 1 不应作为顶层对象单独写出"
    );

    let doc2 = parse_buf(&buf);
    // 打包对象：Catalog(1)、Pages(2) → type 2（对象流内）。
    for num in [1i64, 2] {
        match doc2.xref().get(num).map(|e| &e.kind) {
            Some(XrefKind::InObjStream { stream_num, index }) => {
                assert!(*stream_num > 0, "对象流对象号应为正");
                assert!(*index >= 0);
            }
            other => panic!("object {num} should be in an object stream, got {other:?}"),
        }
        assert!(doc2.get_object(num).expect("parse").is_some());
    }
    let pages = doc2.get_object(2).expect("parse").expect("pages");
    assert_eq!(
        pages
            .value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Pages")
    );
    // 流对象（内容流，对象 3）仍是顶层对象（type 1）且字节可取。
    let page = doc2.get_object(3).expect("parse").expect("content stream");
    assert!(matches!(page.value, Value::Stream(_, _)));
    assert!(!doc2.stream_bytes(&page.value).expect("bytes").is_empty());
}

// ── 架构修复新增测试 ──────────────────────────────────────────────

/// 读含 ObjStm + XRef 流的 1.5 PDF → writer 以 1.3 写出 → 回读断言：
/// 所有对象可见（load_all_objects 预解包后顶层写出）、输出经典 xref 表、
/// 无 ObjStm / XRef 流残留（结构性特性自动降级）。
#[test]
fn obj_stm_xref_stream_automatically_downgraded_below_1_5() {
    // 1.5 PDF：含对象流（对象 5 打包在对象流 6 里）+ XRef 流
    let buf = obj_stm_pdf("", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);

    // Features 初始：XRef 流来自 trailer.dict（懒加载就能扫到）；
    // ObjectStream 需要解析对象流对象本身才会被扫到（懒加载路径不会触发）。
    // 先调 load_all_objects 预解包全部对象 → 触发 ObjStm 字典扫描。
    doc.load_all_objects().expect("预加载全部对象");

    let feats = doc.features.borrow();
    assert!(feats.contains(PdfFeature::XRefStream), "应检测到 XRef 流");
    assert!(feats.contains(PdfFeature::ObjectStream), "应检测到对象流");
    drop(feats);

    // 以 1.3 写出 → 策略选择 PlainObjectLayout + ClassicXrefSerializer
    let mut out = Vec::new();
    core::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_3)
        .build(&doc, &mut out)
        .expect("1.3 目标下结构性特性应自动降级，不应报错");

    // 输出应是经典 xref 表形态，1.3 头
    let text = String::from_utf8_lossy(&out);
    eprintln!("=== writer 输出头 ===\n{}", &text[..300.min(text.len())]);
    eprintln!("=== 尾部 ===\n{}", &text[text.len().saturating_sub(300)..]);
    assert!(
        text.contains("%PDF-1.3"),
        "输出应为 1.3 头：{}",
        &text[..10.min(text.len())]
    );
    assert!(text.contains("xref\n"), "应含经典 xref 关键字");
    assert!(!text.contains("/Type /XRef"), "降级后不应有 XRef 流");
    assert!(!text.contains("/Type /ObjStm"), "降级后不应有对象流");

    // 回读：所有对象应直接可见（对象流内的对象已解包为顶层）
    let doc2 = parse_buf(&out);
    assert_eq!(doc2.source_version(), PdfVersion::V1_3);
    let cat = doc2.get_object(1).expect("1.3 回读").expect("Catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    // 对象 5 原本在对象流里，降级后应作为顶层对象可直接取
    let inner = doc2
        .get_object(5)
        .expect("1.3 回读")
        .expect("对象流内对象应在降级后可见");
    assert_eq!(
        inner
            .value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("InObjStm")
    );
}
