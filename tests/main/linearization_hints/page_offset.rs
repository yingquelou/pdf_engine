//! F.4.1 页偏移 hint 表（Table F.3/F.4）的集成用例：正表逐项归位与畸形
//! 形态的类型化诊断。

use super::*;

#[test]
fn handbuilt_linearized_file_exposes_page_offset_hint_table() {
    let payload = hint_payload(0, 2);
    let entries = hint_entries();
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized as linearized");
    assert!(info.is_consistent(), "structure issues: {:?}", info.issues);

    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);
    assert!(
        hints.shared_object_index_issues().is_empty(),
        "合规的项 4 标识符在独立视图里同样无诊断"
    );
    let table = hints
        .page_offset
        .as_ref()
        .expect("the page offset table sits at stream offset 0");

    // Table F.3 头部 13 项逐项归位；本表没有 Table F.2 键，纯按前言定位。
    assert_eq!(table.declared_pages, PAGE_COUNT as u64);
    assert_eq!(table.header.least_objects_per_page, 3);
    assert_eq!(table.header.first_page_object_location, 700);
    assert_eq!(table.header.object_count_bits, 3);
    assert_eq!(table.header.least_page_length, 300);
    assert_eq!(table.header.page_length_bits, 6);
    assert_eq!(table.header.least_content_offset, 20);
    assert_eq!(table.header.content_offset_bits, 5);
    assert_eq!(table.header.least_content_length, 50);
    assert_eq!(table.header.content_length_bits, 8);
    assert_eq!(table.header.shared_ref_count_bits, 2);
    assert_eq!(table.header.shared_object_id_bits, 4);
    assert_eq!(table.header.shared_position_numerator_bits, 3);
    assert_eq!(table.header.shared_position_denominator, 4);
    assert!(table.width_problems().is_empty());

    // Table F.4 项 1/2/6/7 存的是**增量**：类型化访问器须加回头部最小值。
    assert_eq!(table.pages.len(), PAGE_COUNT);
    assert_eq!(table.pages[0].object_count_delta, 0);
    assert_eq!(table.pages[0].page_length_delta, 10);
    assert_eq!(table.pages[0].content_offset_delta, 4);
    assert_eq!(table.pages[0].content_length_delta, 11);
    assert_eq!(table.pages[0].object_count(&table.header), 3);
    assert_eq!(table.pages[0].page_length(&table.header), 310);
    assert_eq!(table.pages[0].content_offset(&table.header), 24);
    assert_eq!(table.pages[0].content_length(&table.header), 61);
    assert_eq!(table.pages[1].object_count(&table.header), 4);
    assert_eq!(table.pages[1].page_length(&table.header), 325);
    assert_eq!(table.pages[1].content_offset(&table.header), 27);
    assert_eq!(table.pages[1].content_length(&table.header), 63);

    // F.4.1 d)/e)：共享对象两列自第二页起，按页归位、不跨页混排。
    assert_eq!(table.pages[0].shared_reference_count, 0);
    assert!(table.pages[0].shared_object_ids.is_empty());
    assert!(table.pages[0].shared_position_numerators.is_empty());
    assert_eq!(table.pages[1].shared_reference_count, 2);
    assert_eq!(table.pages[1].shared_object_ids, vec![1, 2]);
    assert_eq!(table.pages[1].shared_position_numerators, vec![2, 0]);

    // 表体长度即下一张表的下界：350 位补齐到 44 字节，`/S` 恰好落在其上。
    assert_eq!(table.bits_consumed, 350);
    assert_eq!(table.end_byte(), page_offset_bytes() as u64);
    assert_eq!(hints.page_offset_end_byte(), Some(table.end_byte()));
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert_eq!(hints.stream_bytes, payload.len());

    // F.4.1 是 F.4.2 的前置：本表解析不挤掉后面的共享对象表。
    assert!(hints.shared_object.is_some());

    // 识别与解码都不改变普通路径：两页可读、零告警。
    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

#[test]
fn malformed_page_offset_table_is_a_typed_issue_not_a_panic() {
    let full = hint_payload(0, 2);
    // 截在页偏移表体内（含空表）：一律类型化诊断，页偏移家族整体退回 `None`。
    for prefix in [0usize, 1, 13, 43] {
        let bytes = build(&full[..prefix], " /S 9999");
        let document = parse_buf(&bytes);
        let hints = document
            .linearization_hints()
            .expect("no I/O error")
            .expect("hint stream present");
        assert!(
            hints
                .issues
                .iter()
                .any(|issue| matches!(issue, LinearizationIssue::PageOffsetTableCorrupt { .. })),
            "prefix {prefix}/{} must yield a typed page offset issue: {:#?}",
            full.len(),
            hints.issues
        );
        assert!(hints.page_offset.is_none());
        // 解码失败不影响文档按普通路径读取。
        assert_eq!(document.page_count().expect("page count"), 2);
    }

    // Table F.4 项 3 的首页 shall：非零值原样保留、列不错位读、给类型化诊断。
    let payload = hint_payload(3, 2);
    let entries = hint_entries();
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::PageOffsetFirstPageSharedRefs { count: 3 }),
        "{:#?}",
        hints.issues
    );
    let table = hints.page_offset.as_ref().expect("violated value kept");
    assert_eq!(table.pages[0].shared_reference_count, 3);
    assert!(table.pages[0].shared_object_ids.is_empty());
    assert_eq!(table.pages[1].shared_object_ids, vec![1, 2]);
    // 违规值不影响后续家族的可读性。
    assert!(hints.shared_object.is_some());
}

#[test]
fn out_of_range_shared_object_id_is_a_typed_issue() {
    // F.4.2（35604）：Table F.4 项 4 的标识符须是共享对象 hint 表自 0 起的
    // 顺序下标；本脚手架的共享对象表有 3 个条目，`hint_payload(0, 5)` 的
    // 末位标识符刻意越界（缺省夹具取合规值 2）。
    let payload = hint_payload(0, 5);
    let entries = hint_entries();
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::PageOffsetSharedObjectIndexOutOfRange {
                page: 1,
                index: 5,
                entries: 3
            }),
        "merged hint issues must report the out-of-range id: {:#?}",
        hints.issues
    );
    // 原值保留在类型化条目里，家族不硬失败。
    let table = hints.page_offset.as_ref().expect("violated id kept");
    assert_eq!(table.pages[1].shared_object_ids, vec![1, 5]);
    assert!(hints.shared_object.is_some());
}
