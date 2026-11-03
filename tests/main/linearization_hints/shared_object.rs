//! F.4.2 共享对象 hint 表（Table F.5/F.6）的集成用例：正表逐项归位、
//! `S` 键截断/缺失的类型化诊断。

use super::*;

#[test]
fn handbuilt_linearized_file_exposes_shared_object_hint_table() {
    let payload = hint_payload(0, 2);
    let entries = hint_entries();
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized as linearized");
    assert!(info.is_consistent(), "structure issues: {:?}", info.issues);
    assert!(!info.treated_as_ordinary());

    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);
    assert_eq!(hints.stream_bytes, payload.len());
    assert_eq!(hints.position_of("S"), Some(page_offset_bytes() as u64));
    assert!(hints.page_offset.is_some());

    let table = hints.shared_object.as_ref().expect("parsed table");
    assert_eq!(table.header.first_object_number, 20);
    assert_eq!(table.header.first_object_location, 5000);
    assert_eq!(table.header.first_page_entries, 2);
    assert_eq!(table.header.total_entries, 3);
    assert_eq!(table.header.group_object_bits, 8);
    assert_eq!(table.header.least_group_length, 100);
    assert_eq!(table.header.group_length_bits, 12);
    assert_eq!(table.first_page_groups.len(), 2);
    assert_eq!(table.first_page_groups[0].length_delta, 5);
    assert_eq!(table.first_page_groups[0].object_count_minus_one, 3);
    let signature: Vec<u8> = (0x00u8..=0x0F).collect();
    assert_eq!(
        table.first_page_groups[0].signature,
        signature.as_slice().try_into().ok()
    );
    assert_eq!(table.first_page_groups[1].length_delta, 7);
    assert_eq!(table.first_page_groups[1].signature, None);
    assert_eq!(table.shared_section_groups.len(), 1);
    assert_eq!(table.shared_section_groups[0].length_delta, 9);
    assert_eq!(table.shared_section_groups[0].object_count_minus_one, 1);

    // F.4 前言 NOTE 1/NOTE 3 的分工：Table F.2 的 `/S` 是**流内**位置，
    // 原样使用；表内的对象位置（大于主流起点者）须补上主流长度。
    let shared_at = page_offset_bytes() as u64;
    assert_eq!(hints.adjust_position(shared_at), shared_at);
    assert_eq!(
        hints.adjust_position(hints.primary.offset),
        hints.primary.offset
    );
    let after_stream = hints.primary.offset + 1;
    assert_eq!(
        hints.adjust_position(after_stream),
        after_stream + hints.primary.length
    );
    assert_eq!(
        hints.adjust_position(table.header.first_object_location),
        table.header.first_object_location + hints.primary.length,
        "表内位置 5000 大于主流起点，须按 NOTE 3 补上主流长度"
    );

    // 识别与解码都不改变普通路径：两页可读、零告警。
    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

#[test]
fn truncated_hint_bytes_are_a_typed_issue_not_a_panic() {
    // 表位仍在流内（页偏移表之后），但 Table F.5 头部只写出一半。
    let full = hint_payload(0, 2);
    let payload = &full[..page_offset_bytes() + 6];
    let entries = hint_entries();
    let bytes = build(payload, &entries);
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTableCorrupt { key, .. } if key == "S"
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.shared_object.is_none());
    // 前一家族的完整字节不受后一表截断的影响。
    assert!(hints.page_offset.is_some());
    assert_eq!(
        hints.page_offset_end_byte(),
        Some(page_offset_bytes() as u64)
    );
    // 解码失败不影响文档按普通路径读取。
    assert_eq!(document.page_count().expect("page count"), 2);
}

#[test]
fn missing_required_shared_object_table_key_is_reported() {
    // Table F.2 的 `S` 是 Required：主 hint 流字典缺它即给类型化诊断。
    let payload = hint_payload(0, 2);
    let bytes = build(&payload, "");
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::SharedHintTableMissing),
        "{:#?}",
        hints.issues
    );
    assert!(hints.table_positions.is_empty());
    assert_eq!(hints.shared_object, None::<SharedObjectHintTable>);
    // 页偏移表不靠 Table F.2 键定位（前言钉死在偏移 0），故缺 `S` 也照常解析。
    assert!(hints.page_offset.is_some());
}
