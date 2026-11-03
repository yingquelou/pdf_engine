//! Table F.4 项 1/2/4/5 的跨页派生用例：累加首对象号与页起点、主 hint 流的
//! 跳过修正、分数位置的类型化类别（**不折字节**），以及零页数/缺头部/分母 0
//! 的降级形态一律 `None` 而非 panic。位级解析本身由 [`super::page_offset`] 覆盖，
//! 这里只对「解析出来的增量如何累加」下断言。

use super::super::page_offset_derive::{SharedObjectPosition, check_page_offset_derivations};
use crate::core::reader::linearization::hints::LinearizationHints;
use crate::core::reader::linearization::hints::bits::BitWriter;
use crate::core::reader::linearization::hints::page_offset::{
    PageOffsetEntry, PageOffsetHeader, PageOffsetHintTable,
};
use crate::core::reader::linearization::hints::shared::{
    SharedObjectGroupEntry, SharedObjectHeader, SharedObjectHintTable,
};
use crate::core::reader::linearization::{HintRange, LinearizationIssue};

/// Table F.3 头部：每页最少 3 个对象、首页页对象位置 1000、最少页长 500、
/// 分子位宽 3（可表 0—7）、分母由参数给出。
fn header(denominator: u64) -> PageOffsetHeader {
    PageOffsetHeader {
        least_objects_per_page: 3,
        first_page_object_location: 1000,
        object_count_bits: 4,
        least_page_length: 500,
        page_length_bits: 6,
        least_content_offset: 40,
        content_offset_bits: 5,
        least_content_length: 60,
        content_length_bits: 7,
        shared_ref_count_bits: 3,
        shared_object_id_bits: 5,
        shared_position_numerator_bits: 3,
        shared_position_denominator: denominator,
    }
}

/// Table F.4 的一页条目（项 6/7 的增量对派生无意义，写 0）。
fn entry(
    object_delta: u64,
    length_delta: u64,
    shared_references: u64,
    identifiers: Vec<u64>,
    numerators: Vec<u64>,
) -> PageOffsetEntry {
    PageOffsetEntry {
        object_count_delta: object_delta,
        page_length_delta: length_delta,
        shared_reference_count: shared_references,
        shared_object_ids: identifiers,
        shared_position_numerators: numerators,
        content_offset_delta: 0,
        content_length_delta: 0,
    }
}

fn table(header: PageOffsetHeader, pages: Vec<PageOffsetEntry>) -> PageOffsetHintTable {
    PageOffsetHintTable {
        declared_pages: pages.len() as u64,
        bits_consumed: 288,
        header,
        pages,
    }
}

/// 三页的表：对象数 `[3, 5, 4]`、页长 `[510, 520, 530]`、共享引用 `[0, 2, 1]`、
/// 标识符 `[1, 2] / [3]`、分子 `[3, 4] / [5]`（分母 4）。
fn three_pages() -> Vec<PageOffsetEntry> {
    vec![
        entry(0, 10, 0, Vec::new(), Vec::new()),
        entry(2, 20, 2, vec![1, 2], vec![3, 4]),
        entry(1, 30, 1, vec![3], vec![5]),
    ]
}

/// 同一张表按 F.4.1 a)—g) 的交错序写成位流，再用 `parse` 读回（证明派生读的是
/// 解析产物而非手搭结构）。`denominator` 只改 Table F.3 项 13。
fn parsed_three_pages(denominator: u64) -> PageOffsetHintTable {
    let mut w = BitWriter::default();
    let fixed = header(denominator);
    w.unsigned(fixed.least_objects_per_page, 32);
    w.unsigned(fixed.first_page_object_location, 32);
    w.unsigned(u64::from(fixed.object_count_bits), 16);
    w.unsigned(fixed.least_page_length, 32);
    w.unsigned(u64::from(fixed.page_length_bits), 16);
    w.unsigned(fixed.least_content_offset, 32);
    w.unsigned(u64::from(fixed.content_offset_bits), 16);
    w.unsigned(fixed.least_content_length, 32);
    w.unsigned(u64::from(fixed.content_length_bits), 16);
    w.unsigned(u64::from(fixed.shared_ref_count_bits), 16);
    w.unsigned(u64::from(fixed.shared_object_id_bits), 16);
    w.unsigned(u64::from(fixed.shared_position_numerator_bits), 16);
    w.unsigned(fixed.shared_position_denominator, 16);
    for delta in [0, 2, 1] {
        w.unsigned(delta, 4); // 项 1
    }
    for delta in [10, 20, 30] {
        w.unsigned(delta, 6); // 项 2
    }
    for count in [0, 2, 1] {
        w.unsigned(count, 3); // 项 3
    }
    for id in [1, 2, 3] {
        w.unsigned(id, 5); // 项 4
    }
    for numerator in [3, 4, 5] {
        w.unsigned(numerator, 3); // 项 5
    }
    for _ in 0..3 {
        w.unsigned(0, 5); // 项 6
    }
    for _ in 0..3 {
        w.unsigned(0, 7); // 项 7
    }
    PageOffsetHintTable::parse(&w.bytes, 0, 3).expect("well-formed three-page table")
}

fn range(offset: u64, length: u64) -> HintRange {
    HintRange { offset, length }
}

fn hints(
    page_offset: Option<PageOffsetHintTable>,
    shared_object: Option<SharedObjectHintTable>,
    first_page_object: Option<u64>,
    primary: HintRange,
) -> LinearizationHints {
    LinearizationHints {
        primary,
        first_page_object,
        stream_bytes: 0,
        table_positions: Vec::new(),
        page_offset,
        shared_object,
        thumbnail: None,
        issues: Vec::new(),
    }
}

/// Table F.5/F.6 的最小形态：`groups` 个首页组、共享对象区空。
fn shared_table(groups: usize) -> SharedObjectHintTable {
    SharedObjectHintTable {
        header: SharedObjectHeader {
            first_object_number: 0,
            first_object_location: 0,
            first_page_entries: groups as u64,
            total_entries: groups as u64,
            group_object_bits: 0,
            least_group_length: 0,
            group_length_bits: 0,
        },
        first_page_groups: (0..groups)
            .map(|_| SharedObjectGroupEntry {
                length_delta: 0,
                signature_present: false,
                signature: None,
                object_count_minus_one: 0,
            })
            .collect(),
        shared_section_groups: Vec::new(),
    }
}

#[test]
fn derived_first_object_numbers_accumulate_over_three_pages() {
    let table = parsed_three_pages(4);
    // Table F.4 项 1：首页 = `/O`，第二页 = 1，其后累加「第二页起」各页的对象数。
    assert_eq!(table.page_first_object_number(Some(8), 0), Some(8));
    assert_eq!(table.page_first_object_number(Some(8), 1), Some(1));
    assert_eq!(
        table.page_first_object_number(Some(8), 2),
        Some(6),
        "1 + 第二页的 5 个对象"
    );
    assert_eq!(table.page_first_object_number(Some(8), 3), None);
    assert_eq!(
        table.page_first_object_number(Some(8), usize::MAX),
        None,
        "越界索引不 panic"
    );
    // 首页在另一个对象组（F.3.1），故其对象数不参与累加链。
    assert_eq!(table.second_group_max_object(), Some(9));
}

#[test]
fn missing_page_object_anchor_only_degrades_the_first_page() {
    let table = table(header(4), three_pages());
    assert_eq!(table.page_first_object_number(None, 0), None);
    assert_eq!(
        table.page_first_object_number(None, 1),
        Some(1),
        "项 1 的第二页首对象号与 `/O` 无关"
    );
    assert_eq!(table.page_first_object_number(None, 2), Some(6));
}

#[test]
fn derived_page_starts_accumulate_page_lengths_and_span() {
    let table = parsed_three_pages(4);
    // 锚点 1000 ＋ 页长 [510, 520, 530] ⇒ 起点 [1000, 1510, 2030]。
    assert_eq!(table.page_start_byte(1000, 0), Some(1000));
    assert_eq!(table.page_start_byte(1000, 1), Some(1510));
    assert_eq!(table.page_start_byte(1000, 2), Some(2030));
    assert_eq!(table.page_start_byte(1000, 3), None);
    assert_eq!(table.page_span(1000, 1), Some((1510, 520)));
    assert_eq!(table.page_end_byte(1000, 2), Some(2560));
    assert_eq!(table.page_end_byte(1000, 3), None);
}

#[test]
fn hint_stream_skipping_shifts_derived_page_starts() {
    let parsed = parsed_three_pages(4);
    // F.4 前言 NOTE 3：表内位置按「主 hint 流不存在」解读，大于其起点者补流长。
    let shifted = hints(Some(parsed.clone()), None, Some(8), range(900, 100));
    assert_eq!(shifted.page_start_byte(0), Some(1100));
    assert_eq!(shifted.page_start_byte(1), Some(1610));
    assert_eq!(shifted.page_end_byte(2), Some(2660));
    assert_eq!(shifted.page_first_object_number(2), Some(6));
    // 表内位置全都小于主 hint 流起点时不修正（同一个表、不同的 `/H`）。
    let unshifted = hints(Some(parsed.clone()), None, Some(8), range(9000, 100));
    assert_eq!(unshifted.page_start_byte(0), Some(1000));
    assert_eq!(unshifted.page_start_byte(2), Some(2030));
    // 主 hint 流起点正好等于首页位置：`position > offset` 才修正（既有 helper 的约定）。
    let boundary = hints(Some(parsed), None, Some(8), range(1000, 100));
    assert_eq!(boundary.page_start_byte(0), Some(1000));
    assert_eq!(boundary.page_start_byte(1), Some(1610));
}

#[test]
fn numerators_below_d_select_the_content_stream_fraction() {
    let table = table(header(4), three_pages());
    assert_eq!(
        table.shared_object_position(1, 0),
        Some(SharedObjectPosition::ContentStreamFraction {
            numerator: 3,
            denominator: 4
        })
    );
    assert_eq!(
        hints(Some(table), None, Some(8), range(0, 0)).shared_object_position(1, 0),
        Some(SharedObjectPosition::ContentStreamFraction {
            numerator: 3,
            denominator: 4
        })
    );
}

#[test]
fn numerator_equal_to_d_selects_before_trailing_objects() {
    let table = table(header(4), three_pages());
    assert_eq!(
        table.shared_object_position(1, 1),
        Some(SharedObjectPosition::BeforeTrailingObjects {
            numerator: 4,
            denominator: 4
        })
    );
}

#[test]
fn numerator_above_d_selects_after_trailing_objects() {
    let table = table(header(4), three_pages());
    assert_eq!(
        table.shared_object_position(2, 0),
        Some(SharedObjectPosition::AfterTrailingObjects {
            numerator: 5,
            denominator: 4
        }),
        "d+1 之后；原值保留，规范未给更大的值进一步语义"
    );
    assert_eq!(table.shared_object_position(2, 1), None, "序号越界");
    assert_eq!(table.shared_object_position(3, 0), None, "页越界");
    assert_eq!(table.shared_object_position(0, 0), None, "首页没有项 4/5");
}

#[test]
fn zero_denominator_degrades_to_none_and_never_divides() {
    let table = parsed_three_pages(0);
    assert_eq!(table.header.shared_position_denominator, 0);
    assert_eq!(table.shared_object_position(1, 0), None);
    assert_eq!(table.shared_object_position(1, 1), None);
    assert_eq!(table.shared_reference_total(), 3);
    let mut issues = Vec::new();
    check_page_offset_derivations(Some(&table), Some(8), &mut issues);
    assert_eq!(
        issues,
        vec![
            LinearizationIssue::PageOffsetObjectNumberConflict {
                first_page_object: 8,
                second_group_max: 9
            },
            LinearizationIssue::PageOffsetSharedFractionDenominatorZero { references: 3 },
        ],
        "分母 0 只报告，不做除法；派生值仍照常可取"
    );
    // 页起点/首对象号与分母无关，仍可用。
    assert_eq!(table.page_start_byte(1000, 2), Some(2030));
}

#[test]
fn degenerate_tables_return_none_instead_of_panicking() {
    // 零页数：解析成功但没有任何条目。
    let empty = table(header(4), Vec::new());
    assert_eq!(empty.page_first_object_number(Some(8), 0), None);
    assert_eq!(empty.page_start_byte(1000, 0), None);
    assert_eq!(empty.page_end_byte(1000, 0), None);
    assert_eq!(empty.page_span(1000, 0), None);
    assert_eq!(empty.shared_object_position(0, 0), None);
    assert_eq!(empty.second_group_max_object(), None);
    assert_eq!(empty.shared_reference_total(), 0);
    assert!(empty.shared_index_problems(3).is_empty());
    // 表缺失（解析失败/`/N` 不可用）：类型化访问器一律 `None`，不叠加诊断。
    let none = hints(None, None, None, range(0, 0));
    assert_eq!(none.page_first_object_number(0), None);
    assert_eq!(none.page_start_byte(0), None);
    assert_eq!(none.page_end_byte(0), None);
    assert_eq!(none.shared_object_position(0, 0), None);
    let mut issues = Vec::new();
    check_page_offset_derivations(None, None, &mut issues);
    assert!(issues.is_empty());
    // 只有首页时不存在第二对象组。
    let single = table(header(4), vec![entry(0, 10, 0, Vec::new(), Vec::new())]);
    assert_eq!(single.second_group_max_object(), None);
    assert_eq!(single.page_first_object_number(Some(8), 0), Some(8));
}

#[test]
fn accumulation_saturates_instead_of_wrapping() {
    let mut header = header(4);
    header.least_page_length = u64::MAX;
    header.least_objects_per_page = u64::MAX;
    let table = table(header, three_pages());
    assert_eq!(table.page_start_byte(1000, 1), Some(u64::MAX));
    assert_eq!(table.page_start_byte(1000, 2), Some(u64::MAX));
    assert_eq!(table.page_end_byte(1000, 2), Some(u64::MAX));
    assert_eq!(table.page_first_object_number(Some(8), 2), Some(u64::MAX));
    assert_eq!(table.second_group_max_object(), Some(u64::MAX));
    // 锚点本身饱和：不回绕成小值。
    assert_eq!(table.page_start_byte(u64::MAX, 2), Some(u64::MAX));
}

#[test]
fn shared_identifiers_beyond_the_shared_object_table_are_typed_issues() {
    let table = parsed_three_pages(4);
    // 下标是共享对象 hint 表的顺序位置（自 0 起）：3 个条目时标识符 3 越界。
    assert_eq!(table.shared_index_problems(3), vec![(2, 3)]);
    assert!(table.shared_index_problems(10).is_empty());
    let host = hints(
        Some(table.clone()),
        Some(shared_table(3)),
        Some(10),
        range(0, 0),
    );
    assert_eq!(
        host.shared_object_index_issues(),
        vec![LinearizationIssue::PageOffsetSharedObjectIndexOutOfRange {
            page: 2,
            index: 3,
            entries: 3
        }],
        "类型化诊断，不硬失败"
    );
    // 标识符全在表项数内：无诊断。
    let clean = hints(
        Some(table.clone()),
        Some(shared_table(4)),
        Some(10),
        range(0, 0),
    );
    assert!(clean.shared_object_index_issues().is_empty());
    // 共享对象 hint 表没读出来时不做该核对（不叠加假阳性）。
    let blind = hints(Some(table), None, Some(10), range(0, 0));
    assert!(blind.shared_object_index_issues().is_empty());
    // 且它不进装配期 `issues`（附录 F 夹具的项 4 越界，见方法文档）。
    let mut derived = Vec::new();
    check_page_offset_derivations(Some(&parsed_three_pages(4)), Some(10), &mut derived);
    assert!(derived.is_empty());
}

#[test]
fn object_number_conflict_is_reported_only_when_certain() {
    let table = parsed_three_pages(4);
    // 第二对象组用到对象号 1..=9，故 `/O` 必须 >= 10（F.3.1 的「紧接其后」）。
    let mut tight = Vec::new();
    check_page_offset_derivations(Some(&table), Some(9), &mut tight);
    assert_eq!(
        tight,
        vec![LinearizationIssue::PageOffsetObjectNumberConflict {
            first_page_object: 9,
            second_group_max: 9
        }]
    );
    // `/O` 偏大不报：hint 表只按页计对象数，组内还有目录等非页对象。
    let mut roomy = Vec::new();
    check_page_offset_derivations(Some(&table), Some(10), &mut roomy);
    assert!(roomy.is_empty());
    let mut unknown = Vec::new();
    check_page_offset_derivations(Some(&table), None, &mut unknown);
    assert!(unknown.is_empty(), "缺 `/O` 时无从对照");
}
