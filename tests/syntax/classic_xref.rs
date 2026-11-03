//! 经典交叉引用表子主题（7.5.4 xref 表往返、偏移有效性与 `/XRefStm` 条目覆盖）。

use super::*;

#[test]
fn roundtrip_preserves_structure() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let doc2 = parse_buf(&buf);

    assert_eq!(doc2.trailer().root(), Some((1, 0)));
    let cat = doc2.get_object(1).expect("parse").expect("catalog present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    // Pages.Kids[0] == 3 0 R
    let pages = doc2.get_object(2).expect("parse").expect("pages present");
    let kids = pages
        .value
        .as_dict()
        .unwrap()
        .get("Kids")
        .and_then(|v| v.as_array())
        .unwrap();
    assert_eq!(kids.iter().next().unwrap().as_ref(), Some((3, 0)));
    // 流内容存在且非空（按需从文件读取，不常驻内存）
    let page = doc2.get_object(3).expect("parse").expect("page present");
    let stream_bytes = doc2.stream_bytes(&page.value).expect("stream bytes");
    assert!(!stream_bytes.is_empty());
}

#[test]
fn xref_offsets_are_valid() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    // 逐条目校验：
    // - 普通条目（type 1，按偏移定位）偏移处应能读到 "N G obj"；
    // - 对象流内条目（type 2，≥1.5 打包）没有文件偏移，改为验证按该条目
    //   能取回对象、且其对象流自身是可直接定位的流对象。
    let doc2 = parse_buf(&buf);
    for sub in &doc2.xref().0 {
        for (i, e) in sub.entries.iter().enumerate() {
            if !e.in_use() {
                continue;
            }
            let num = sub.start + i as i64;
            match &e.kind {
                XrefKind::Normal => {
                    let head = &buf[e.offset as usize..];
                    let line_end = head.iter().position(|&b| b == b'\n').unwrap_or(head.len());
                    let line = std::str::from_utf8(&head[..line_end]).unwrap_or("");
                    assert!(
                        line.starts_with(&format!("{num} {} obj", e.generation)),
                        "offset {:#x}: line = {:?}",
                        e.offset,
                        line
                    );
                }
                XrefKind::InObjStream { stream_num, .. } => {
                    let obj = doc2
                        .get_object(num)
                        .expect("parse")
                        .expect("packed object present");
                    assert_eq!(obj.num, num);
                    let stm = doc2
                        .get_object(*stream_num)
                        .expect("parse")
                        .expect("object stream present");
                    assert!(
                        matches!(stm.value, Value::Stream(_, _)),
                        "对象流自身应是可直接定位的流对象"
                    );
                }
                XrefKind::Free => {}
            }
        }
    }
}

/// 目标版本 1.4：保持经典 xref 表 + trailer，不输出 XRef 流/对象流。
#[test]
fn classic_xref_kept_below_1_5() {
    let doc = sample_doc();
    let buf = try_write(&doc, PdfVersion::V1_4).expect("write");
    let out = String::from_utf8_lossy(&buf);

    assert!(out.contains("%PDF-1.4"));
    assert!(out.contains("\nxref\n"), "1.4 应输出经典 xref 表");
    assert!(out.contains("trailer"), "1.4 应输出 trailer");
    assert!(!out.contains("/Type /XRef"), "1.4 不应输出 XRef 流");
    assert!(!out.contains("/Type /ObjStm"), "1.4 不应输出对象流");
    // 所有对象均顶层写出。
    assert!(out.contains("1 0 obj"));
    assert!(out.contains("3 0 obj"));

    let doc2 = parse_buf(&buf);
    assert_eq!(doc2.source_version(), PdfVersion::V1_4);
    assert!(doc2.get_object(1).expect("parse").is_some());
}

/// 回归（#6）：解析含 ObjStm + XRef 流的 1.5 PDF 后以**同版本 1.5** 再写出，
/// writer 自身生成的对象流 / XRef 流不得与源文件残留的结构性对象叠加。
///
/// 修复前：源里的 `/Type /ObjStm`、`/Type /XRef` 容器对象被当作普通顶层
/// 对象保留，导致输出出现 2×ObjStm、2×XRef，体积显著膨胀。
#[test]
fn rewrite_at_same_version_has_single_structural_objects() {
    let buf = obj_stm_pdf("", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);

    let out = try_write(&doc, PdfVersion::V1_5).expect("1.5 再写出");
    let text = String::from_utf8_lossy(&out);

    assert_eq!(
        text.matches("/Type /ObjStm").count(),
        1,
        "同版本再写出应恰有 1 个对象流，不得叠加源残留：\n{text}"
    );
    assert_eq!(
        text.matches("/Type /XRef").count(),
        1,
        "同版本再写出应恰有 1 个 XRef 流，不得叠加源残留：\n{text}"
    );
}

/// 混合引用读端优先级固化（7.5.8.4，Table 19）：主经典表把对象 3 登记为
/// generation 65535 自由条目（旧读端视其为 null 引用），更新段 trailer 的
/// `/XRefStm` 补充流给出对象 3 的真实条目——合并查找必须让 `/XRefStm`
/// 条目**覆盖**主表自由条目，`get_object(3)` 应返回真实对象。
#[test]
fn xref_stm_entry_overrides_main_table_free_entry() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let off3 = buf.len();
    buf.extend_from_slice(
        b"3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /HiddenFont >>\nendobj\n",
    );
    // 主经典表：0..3 全条目；对象 3 是自由条目（链 0 -> 3 -> 0，gen 65535）。
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n");
    buf.extend_from_slice(b"0000000003 65535 f \r\n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \r\n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \r\n").as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \r\n");
    // 主 trailer：不含 /XRefStm（Table 19 shall）。
    buf.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n");
    buf.extend_from_slice(format!("{off_xref}\n%%EOF\n").as_bytes());
    // 补充交叉引用流对象 4：type 1 条目给出对象 3 的真实偏移。
    let off_stm = buf.len();
    let mut stream: Vec<u8> = Vec::new();
    stream.push(1u8);
    stream.extend_from_slice(&(off3 as u16).to_be_bytes());
    stream.push(0u8);
    buf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Type /XRef /Size 5 /Index [3 1] /W [1 2 1] /Length {} >>\nstream\n",
            stream.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&stream);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    // 更新段：空经典表 + trailer（/Prev 指主表、/XRefStm 指补充流）。
    let off_update = buf.len();
    buf.extend_from_slice(b"xref\n0 0\ntrailer\n");
    buf.extend_from_slice(
        format!("<< /Size 5 /Root 1 0 R /Prev {off_xref} /XRefStm {off_stm} >>\n").as_bytes(),
    );
    buf.extend_from_slice(b"startxref\n");
    buf.extend_from_slice(format!("{off_update}\n%%EOF\n").as_bytes());

    let doc = parse_buf(&buf);
    // /XRefStm 条目覆盖主表 65535 自由条目：对象 3 必须可按真实条目取回。
    let hidden = doc
        .get_object(3)
        .expect("parse")
        .expect("obj 3 present: /XRefStm entry must override the main-table free entry");
    assert_eq!(
        hidden
            .value
            .as_dict()
            .and_then(|d| d.get("BaseFont"))
            .and_then(|v| v.as_name()),
        Some("HiddenFont")
    );
    // 主表自身的 n 条目不受影响。
    assert!(doc.get_object(1).expect("parse").is_some());
    assert!(doc.get_object(2).expect("parse").is_some());
}
