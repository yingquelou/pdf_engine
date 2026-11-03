//! 附录 F.4.3 缩略图 hint 表的集成验证：在 [`super`] 的分部装配器上把一张
//! 真实的 Table F.7/F.8 缩略图表插到页偏移表与共享对象表之间，按字节相邻给出
//! Table F.2 键 `T` 与 `S` 的位置，再从 `Document::linearization_hints()` 解析
//! 回类型化数据；畸形形态一律给类型化诊断而非 panic。
//!
//! 本文件是 `tests/main/linearization_hints.rs` 的子模块（同目录即可，无须改
//! `tests/main/mod.rs`），复用父模块的位写手与两轮收敛的布局迭代。

use super::{
    BitWriter, build, page_offset_bytes, page_offset_table_bits, shared_object_table_bits,
};
use crate::support::parse_buf;
use pdf_engine::core::reader::linearization::LinearizationIssue;

/// Table F.7 头部 12 项的定值（项 1/2/5/7/9/10/11/12）；项 3 是条目数、
/// 项 4/6/8 是三个位宽，由参数驱动。
fn thumbnail_header(
    w: &mut BitWriter,
    entries: u64,
    gap_bits: u64,
    length_bits: u64,
    object_bits: u64,
) {
    w.unsigned(11, 32); // 项 1：首张缩略图的对象号
    w.unsigned(900, 32); // 项 2：首张缩略图的位置
    w.unsigned(entries, 32); // 项 3：有缩略图的页数
    w.unsigned(gap_bits, 16); // 项 4：Table F.8 项 1 的位宽
    w.unsigned(400, 32); // 项 5：缩略图最少长度
    w.unsigned(length_bits, 16); // 项 6：Table F.8 项 3 的位宽
    w.unsigned(1, 32); // 项 7：缩略图最少对象数
    w.unsigned(object_bits, 16); // 项 8：Table F.8 项 2 的位宽
    w.unsigned(50, 32); // 项 9：缩略图共享对象区首对象号
    w.unsigned(9000, 32); // 项 10：该区首对象位置
    w.unsigned(4, 32); // 项 11：该区对象数
    w.unsigned(1500, 32); // 项 12：该区长度
}

/// 两个条目（页 0 与页 1，中间无缺图页）的缩略图表，项 4 位宽由 `gap_bits`
/// 给定（合规值 2；给 40 即声明宽度越界形态）。
///
/// 336 位头部 + 2×(gap_bits+3+6) 位三列。
fn thumbnail_table_bits(gap_bits: u64) -> Vec<u8> {
    let mut w = BitWriter::default();
    thumbnail_header(&mut w, 2, gap_bits, 6, 3);
    // a) 项 1 列 → b) 项 2 列 → c) 项 3 列（F.4.3 的列序，35740—35743）。
    for _ in 0..2 {
        w.unsigned(0, gap_bits as u32);
    }
    for objects in [1u64, 2] {
        w.unsigned(objects, 3);
    }
    for length in [5u64, 63] {
        w.unsigned(length, 6);
    }
    w.finish()
}

/// 项 3 声明 0 条目的缩略图表（只剩 336 位头部）：`T` 声称有表、缩略图区却
/// 一个条目都没有，与「shall include one or more per-page entries」矛盾。
fn empty_thumbnail_table_bits() -> Vec<u8> {
    let mut w = BitWriter::default();
    thumbnail_header(&mut w, 0, 2, 6, 3);
    w.finish()
}

/// 页偏移表 → 缩略图表 → 共享对象表，三表字节相邻（F.4 前言：各表以字节边界
/// 开始且互不重叠，故相邻位置即后一张表的下界）。
fn thumbnail_payload(table: &[u8]) -> Vec<u8> {
    let mut payload = page_offset_table_bits(0);
    payload.extend_from_slice(table);
    payload.extend(shared_object_table_bits());
    payload
}

/// Table F.2 键 `T`/`S` 的定位条目：`T` 紧跟页偏移表，`S` 紧跟缩略图表。
fn thumbnail_entries(table_len: usize) -> String {
    format!(
        " /T {} /S {}",
        page_offset_bytes(),
        page_offset_bytes() + table_len
    )
}

#[test]
fn handbuilt_linearized_file_exposes_thumbnail_hint_table() {
    let table_bytes = thumbnail_table_bits(2);
    let payload = thumbnail_payload(&table_bytes);
    let entries = thumbnail_entries(table_bytes.len());
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
    // 表按 Table F.2 键 `T` 定位：起点即页偏移表的末字节。
    assert_eq!(hints.position_of("T"), Some(page_offset_bytes() as u64));
    let table = hints.thumbnail.as_ref().expect("parsed thumbnail table");
    assert_eq!(table.start_byte, page_offset_bytes() as u64);
    assert_eq!(table.declared_entries, 2);

    // Table F.7 的 12 项逐项归位，含声明而未建模的缩略图共享对象区（项 9—12）。
    assert_eq!(table.header.first_thumbnail_object_number, 11);
    assert_eq!(table.header.first_thumbnail_object_location, 900);
    assert_eq!(table.header.pages_with_thumbnails, 2);
    assert_eq!(table.header.gap_count_bits, 2);
    assert_eq!(table.header.least_image_length, 400);
    assert_eq!(table.header.image_length_bits, 6);
    assert_eq!(table.header.least_object_count, 1);
    assert_eq!(table.header.object_count_bits, 3);
    assert_eq!(table.header.first_shared_object_number, 50);
    assert_eq!(table.header.first_shared_object_location, 9000);
    assert_eq!(table.header.shared_object_count, 4);
    assert_eq!(table.header.shared_section_length, 1500);
    assert!(table.width_problems().is_empty());

    // Table F.8 的三列按页序归位；项 2/3 是增量，须加回头部项 7/项 5。
    assert_eq!(table.entries.len(), 2);
    assert_eq!(table.entries[0].preceding_pages_without_thumbnail, 0);
    assert_eq!(table.entries[0].object_count_delta, 1);
    assert_eq!(table.entries[0].image_length_delta, 5);
    assert_eq!(table.entries[1].object_count_delta, 2);
    assert_eq!(table.entries[1].image_length_delta, 63);
    assert_eq!(table.entries[0].object_count(&table.header), 2);
    assert_eq!(table.entries[1].object_count(&table.header), 3);
    assert_eq!(table.entries[0].image_length(&table.header), 405);
    assert_eq!(table.entries[1].image_length(&table.header), 463);
    assert_eq!(table.page_of(0), Some(0));
    assert_eq!(table.page_of(1), Some(1));

    // 位置核对：336+22=358 位补齐 45 字节，`/S` 恰好落在本表末字节之后。
    assert_eq!(table.bits_consumed, 358);
    assert_eq!(table.end_byte(), 44 + 45);
    assert_eq!(hints.thumbnail_end_byte(), Some(table.end_byte()));
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert_eq!(hints.stream_bytes, payload.len());

    // 前两家不受影响：页偏移表与共享对象表都还在原位。
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
    // 表内位置仍按 F.4 前言 NOTE 3 修正（项 2 的 900 大于主流起点）。
    let adjusted = hints.adjust_position(table.header.first_thumbnail_object_location);
    assert_eq!(
        adjusted,
        table.header.first_thumbnail_object_location + hints.primary.length
    );

    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

#[test]
fn position_inside_the_thumbnail_table_is_a_typed_issue() {
    // `/S` 声称的位置落进缩略图表体内：表互不重叠，故该键没有可信的解析起点。
    let table_bytes = thumbnail_table_bits(2);
    let payload = thumbnail_payload(&table_bytes);
    let inside = page_offset_bytes() + 16;
    let entries = format!(" /T {} /S {inside}", page_offset_bytes());
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTablePositionInvalid { key, problem }
                if key == "S" && problem.contains("inside the thumbnail hint table")
        )),
        "{:#?}",
        hints.issues
    );
    assert_eq!(
        hints.position_of("S"),
        None,
        "落在表体里的位置须从位置表剔除"
    );
    assert!(hints.thumbnail.is_some(), "本表自身仍按 `T` 正常解析");
    assert_eq!(hints.thumbnail_end_byte(), Some(44 + 45));
    assert!(hints.shared_object.is_none());
    // 位置核对不牵连已解析的家族。
    assert!(hints.page_offset.is_some());
    assert_eq!(document.page_count().expect("page count"), 2);
}

#[test]
fn malformed_thumbnail_table_is_a_typed_issue_not_a_panic() {
    // 截在缩略图表体内（含只剩表头字节、以及一个字节都不剩）：类型化诊断，
    // 本家族整体退回 `None`，页偏移表照常；越界的 `/S` 另给位置诊断，不在此断言。
    let table_bytes = thumbnail_table_bits(2);
    let full = thumbnail_payload(&table_bytes);
    let entries = thumbnail_entries(table_bytes.len());
    for prefix in [44usize, 45, 50, 88] {
        let bytes = build(&full[..prefix], &entries);
        let document = parse_buf(&bytes);
        let hints = document
            .linearization_hints()
            .expect("no I/O error")
            .expect("hint stream present");
        assert!(
            hints
                .issues
                .iter()
                .any(|issue| matches!(issue, LinearizationIssue::ThumbnailTableCorrupt { .. })),
            "prefix {prefix} must yield a typed thumbnail issue: {:#?}",
            hints.issues
        );
        assert!(hints.thumbnail.is_none());
        assert!(hints.page_offset.is_some());
        assert_eq!(document.page_count().expect("page count"), 2);
    }

    // `T` 声称有表但条目整段缺失（Table F.7 项 3 为 0）。
    let empty = empty_thumbnail_table_bits();
    let payload = thumbnail_payload(&empty);
    let entries = thumbnail_entries(empty.len());
    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::ThumbnailHintTableMissing),
        "{:#?}",
        hints.issues
    );
    assert_eq!(hints.thumbnail.map(|table| table.entries.len()), Some(0));

    // 声明位宽越界：原值保留、按声明值读完，只给类型化报告；后续家族照常。
    let wide = thumbnail_table_bits(40);
    let payload = thumbnail_payload(&wide);
    let entries = thumbnail_entries(wide.len());
    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::ThumbnailWidthOutOfRange {
                item: "item 4",
                bits: 40
            }),
        "{:#?}",
        hints.issues
    );
    let table = hints.thumbnail.as_ref().expect("declared width kept");
    assert_eq!(table.header.gap_count_bits, 40);
    assert_eq!(table.entries.len(), 2);
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert!(hints.shared_object.is_some());
    assert_eq!(document.page_count().expect("page count"), 2);
}
