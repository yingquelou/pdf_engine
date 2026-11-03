//! 懒加载与增量更新子主题（7.5.5 `/Prev` 历史链、最小对象链遍历）。

use super::*;

/// 懒加载核心验证：打开 Document 后不应解析任何对象；
/// 只在 `get_object` 调用时才解析被请求的对象并缓存。
///
/// 默认 1.7 输出会把非流对象（Catalog、Pages）打包进一个对象流（ObjStm），
/// 因此取一个打包对象时会连带解析其**容器**对象流（对象 4），故计数为
/// 「被请求对象 + 其对象流」。其余无关对象（内容流对象 3）仍不被解析。
#[test]
fn lazy_load_only_requested_objects() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let doc2 = parse_buf(&buf);

    // 打开后缓存应为空：未解析任何对象
    assert_eq!(
        doc2.cached_object_count(),
        0,
        "no objects should be parsed on open"
    );

    // 请求对象 2（打包在对象流内）：解析对象 2 及其对象流容器，共 2 个；
    // 对象 1/3 仍未被解析。
    let _pages = doc2.get_object(2).expect("parse").expect("obj 2 present");
    assert_eq!(
        doc2.cached_object_count(),
        2,
        "only the requested object and its object stream should be parsed"
    );

    // 再次请求对象 2：命中缓存，不新增解析
    let _pages2 = doc2.get_object(2).expect("parse").expect("obj 2 present");
    assert_eq!(
        doc2.cached_object_count(),
        2,
        "cache hit should not reparse"
    );

    // 请求对象 1（打包在同一对象流内）：对象流已缓存，只再解析对象 1，共 3 个
    let _cat = doc2.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(doc2.cached_object_count(), 3);

    // 内容流对象 3 至此仍未被解析（即便它在文件中且 xref 有可直接定位的条目）
    assert_eq!(doc2.cached_object_count(), 3);
}

/// 渲染指定页面的典型链路：沿 Root → Pages → Page，
/// 只解析这条链上的对象，不触及无关对象。
#[test]
fn render_one_page_walks_minimal_chain() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let doc2 = parse_buf(&buf);

    // 渲染第 0 页的最小对象链
    let root_num = doc2.trailer().root().expect("root").0;
    let catalog = doc2.get_object(root_num).expect("parse").expect("catalog");
    let pages_ref = catalog
        .value
        .as_dict()
        .unwrap()
        .get("Pages")
        .and_then(|v| v.as_ref())
        .expect("Pages ref");
    let pages = doc2.get_object(pages_ref.0).expect("parse").expect("pages");
    let first_kid = pages
        .value
        .as_dict()
        .unwrap()
        .get("Kids")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .as_ref()
        .unwrap();
    let _page = doc2.get_object(first_kid.0).expect("parse").expect("page");

    // 默认 1.7 下 Catalog/Pages 打包在同一对象流（对象 4）内，故本链解析
    // 了「对象流容器 + Catalog + Pages + Page」共 4 个对象；已无其它无关对象
    // 被触及（文件内不再有第 5 个内容对象）。
    assert_eq!(
        doc2.cached_object_count(),
        4,
        "rendering one page should parse only Catalog/Pages/Page and their object stream"
    );
}

/// 增量更新 + `/Prev` 链：初始修订定义对象 1=100、对象 2；尾追修订
/// 只更新对象 1=200（其 xref 仅含对象 1），trailer `/Prev` 指向初始 xref。
/// 期望：`get_object(1)` 得最新的 200（最新修订优先），对象 2 仍由历史
/// 修订补充可见（最新修订未提及）。
#[test]
fn incremental_update_prev_chain_merges_history() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n100\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Marker /History >>\nendobj\n");
    // 初始修订：经典 xref 表，覆盖对象 0..=2。
    let off_xref1 = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{off_xref1}\n%%EOF\n").as_bytes(),
    );
    // 尾追修订：更新对象 1=200，xref 仅含对象 1，trailer /Prev 指向初始 xref。
    let off1b = buf.len();
    buf.extend_from_slice(b"1 0 obj\n200\nendobj\n");
    let off_xref2 = buf.len();
    buf.extend_from_slice(b"xref\n1 1\n");
    buf.extend_from_slice(format!("{off1b:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 3 /Root 1 0 R /Prev {off_xref1} >>\nstartxref\n{off_xref2}\n%%EOF\n"
        )
        .as_bytes(),
    );

    let doc = parse_buf(&buf);
    // 最新修订更新了对象 1 → 取最新值 200（历史 100 不覆盖）。
    let o1 = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(o1.value, Value::Integer(200));
    // 最新修订未提及对象 2 → 由历史修订提供。
    let o2 = doc
        .get_object(2)
        .expect("parse")
        .expect("obj 2 via history");
    assert_eq!(
        o2.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("History")
    );
}

/// 增量更新的删除语义：尾追修订把对象 1 标为自由条目（`f`），其 xref
/// 仅含该自由条目、trailer `/Prev` 指向初始 xref。期望 `get_object(1)`
/// 返回 `None`——**不得**回退到历史修订里的旧值 100。
#[test]
fn incremental_update_free_entry_does_not_fall_back() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n100\nendobj\n");
    // 初始修订：经典 xref 表，覆盖对象 0..=1，对象 1 在用。
    let off_xref1 = buf.len();
    buf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 >>\nstartxref\n{off_xref1}\n%%EOF\n").as_bytes(),
    );
    // 尾追修订：对象 1 标为 free（删除），xref 仅含该 free 条目。
    let off_xref2 = buf.len();
    buf.extend_from_slice(b"xref\n1 1\n0000000000 00001 f \n");
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 /Prev {off_xref1} >>\nstartxref\n{off_xref2}\n%%EOF\n")
            .as_bytes(),
    );

    let doc = parse_buf(&buf);
    // 最新修订删除对象 1 → 必须返回 None（free 条目也是「首个命中」）。
    assert!(
        doc.get_object(1)
            .expect("get_object must not error")
            .is_none(),
        "free entry in newest revision must not fall back to history"
    );
}

/// `/Prev` 回溯到历史 **XRef 流**：初始修订的 xref 是 XRef 流对象 3，
/// 尾追修订用经典 xref 表更新对象 1=200，trailer `/Prev` 指向该 XRef 流。
/// 期望历史 XRef 流被回溯解析，其提供的对象 2 可见，且最新修订优先。
#[test]
fn incremental_update_prev_to_xref_stream() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n100\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Marker /History >>\nendobj\n");
    // 初始修订的 xref 为 XRef 流对象 3，覆盖对象 0..=3。
    let off_stm = buf.len();
    let mut xs: Vec<u8> = Vec::new();
    let mut push = |t: u8, f2: u16, f3: u8| {
        xs.push(t);
        xs.extend_from_slice(&f2.to_be_bytes());
        xs.push(f3);
    };
    push(0, 0, 255); // 0：自由条目
    push(1, off1 as u16, 0); // 1
    push(1, off2 as u16, 0); // 2
    push(1, off_stm as u16, 0); // 3：XRef 流自身
    let hdr = format!(
        "3 0 obj\n<< /Type /XRef /Size 4 /Index [0 4] /W [1 2 1] /Length {} /Root 1 0 R >>\nstream\n",
        xs.len()
    );
    buf.extend_from_slice(hdr.as_bytes());
    buf.extend_from_slice(&xs);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    // 尾追修订：更新对象 1=200 的经典 xref 表，/Prev 指向历史 XRef 流。
    let off1b = buf.len();
    buf.extend_from_slice(b"1 0 obj\n200\nendobj\n");
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n1 1\n");
    buf.extend_from_slice(format!("{off1b:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 4 /Root 1 0 R /Prev {off_stm} >>\nstartxref\n{off_xref}\n%%EOF\n"
        )
        .as_bytes(),
    );

    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    // 最新修订优先：对象 1 = 200。
    let o1 = doc.get_object(1).expect("parse").expect("obj 1");
    assert_eq!(o1.value, Value::Integer(200));
    // /Prev 回溯到历史 XRef 流：对象 2 仅由历史 xref 提供。
    let o2 = doc
        .get_object(2)
        .expect("parse")
        .expect("obj 2 via history xref stream");
    assert_eq!(
        o2.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("History")
    );
}
