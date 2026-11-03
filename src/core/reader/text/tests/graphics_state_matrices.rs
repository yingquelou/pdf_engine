//! `q`/`Q` 保存并恢复文本矩阵 `Tm` 与文本行矩阵 `Tlm`。
//!
//! 规范依据（ISO 32000-1，本地副本 `docs/PDF32000_2008.pdf`）：
//! - 8.4.2 Graphics state stack：「The q operator shall push a copy of the
//!   entire graphics state onto the stack.」「The Q operator shall restore the
//!   entire graphics state to its former value by popping it from the
//!   stack.」（两个算子列于 Table 57 - Graphics State Operators）
//! - 9.3.1 General：「The text state comprises those graphics state
//!   parameters that only affect text」（Table 104 - Text state parameters）
//! - 9.4.1 General：`Tm`、`Tlm`、`Trm` 是只在文本对象内出现的三个参数，
//!   「shall not persist from one text object to the next」；Table 107 的 `BT`
//!   把 `Tm` 与 `Tlm` 初始化为单位矩阵，`ET` 丢弃文本矩阵
//! - 9.4.2 Text-Positioning Operators：Table 108 的 `Tm` 同时设置 `Tm` 与
//!   `Tlm`，`Tlm` 捕获行首的 `Tm`
//!
//! 位置断言走 [`Document::extract_text_fragments`]：片段原点即
//! `Tm × CTM` 作用于 `(0, Ts)` 的默认用户空间坐标；`Tlm` 无法直接观察，
//! 由其后第一条 `Td`/`T*` 移动后的位置体现。

use crate::core::types::Document;

use super::super::MAX_STATE_DEPTH;
use super::support::{resources_with_font, winansi_type1_font};

/// 提取带位置的文本片段并展开成 `(text, x, y)`，便于逐条断言。
fn positions(content: &[u8]) -> Vec<(String, f64, f64)> {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    document
        .extract_text_fragments(content, &resources)
        .into_iter()
        .map(|fragment| (fragment.text, fragment.x, fragment.y))
        .collect()
}

#[test]
fn save_and_restore_wrap_the_text_matrix() {
    // `q` 区域内 `Tm` 被改写；`Q` 必须把 `q` 之前的 `Tm` 还原回来。
    assert_eq!(
        positions(
            b"BT /F1 12 Tf 1 0 0 1 100 200 Tm \
               q 1 0 0 1 300 400 Tm (in) Tj Q (out) Tj ET"
        ),
        vec![
            ("in".to_owned(), 300.0, 400.0),
            ("out".to_owned(), 100.0, 200.0),
        ],
        "Q 应恢复 q 之前的 Tm"
    );
}

#[test]
fn save_and_restore_wrap_the_line_matrix_seen_by_td() {
    // `Tm` 同时写 `Tlm`，故 `Q` 后第一条 `Td` 必须从恢复的行矩阵起步。
    assert_eq!(
        positions(
            b"BT /F1 12 Tf 1 0 0 1 10 20 Tm \
               q 1 0 0 1 200 300 Tm Q 0 -5 Td (moved) Tj ET"
        ),
        vec![("moved".to_owned(), 10.0, 15.0)],
        "Td 读 Tlm：恢复后的行矩阵才给出 10 15"
    );
}

#[test]
fn nested_save_restore_pairs_restore_each_text_matrix() {
    assert_eq!(
        positions(
            b"BT /F1 12 Tf 1 0 0 1 1 1 Tm \
               q 1 0 0 1 2 2 Tm q 1 0 0 1 3 3 Tm \
               (deep) Tj Q (middle) Tj Q (outer) Tj ET"
        ),
        vec![
            ("deep".to_owned(), 3.0, 3.0),
            ("middle".to_owned(), 2.0, 2.0),
            ("outer".to_owned(), 1.0, 1.0),
        ],
        "嵌套 q/Q 逐层恢复 Tm"
    );
}

#[test]
fn nested_save_restore_pairs_restore_each_line_matrix() {
    assert_eq!(
        positions(
            b"BT /F1 12 Tf 1 0 0 1 1 1 Tm \
               q 1 0 0 1 2 2 Tm q 1 0 0 1 3 3 Tm Q \
               0 -10 Td (middle) Tj Q 0 -10 Td (outer) Tj ET"
        ),
        vec![
            ("middle".to_owned(), 2.0, -8.0),
            ("outer".to_owned(), 1.0, -9.0),
        ],
        "内层 Q 后 Tlm 回到 2 2，外层 Q 后回到 1 1"
    );
}

#[test]
fn extra_restore_is_a_no_op_for_the_text_matrices() {
    // 多于 `q` 的 `Q`：既不 panic，也不得改写当前矩阵（与今天一致）。
    assert_eq!(
        positions(b"BT /F1 12 Tf 1 0 0 1 50 60 Tm Q Q (a) Tj 0 -10 Td (b) Tj ET"),
        vec![("a".to_owned(), 50.0, 60.0), ("b".to_owned(), 50.0, 50.0)]
    );
    // 多于 `Q` 的 `q`：未配平的 `q` 不改变当前值。
    assert_eq!(
        positions(b"BT /F1 12 Tf q q 1 0 0 1 5 6 Tm (a) Tj ET"),
        vec![("a".to_owned(), 5.0, 6.0)]
    );
}

#[test]
fn deep_q_nesting_beyond_the_limit_stays_silent() {
    // `MAX_STATE_DEPTH` 策略不变：只是同一栈深度多带两个字段。
    let mut content = b"BT /F1 12 Tf 1 0 0 1 1 1 Tm ".to_vec();
    for _ in 0..MAX_STATE_DEPTH + 44 {
        content.extend_from_slice(b"q ");
    }
    content.extend_from_slice(b"1 0 0 1 9 9 Tm (in) Tj ");
    for _ in 0..MAX_STATE_DEPTH + 44 {
        content.extend_from_slice(b"Q ");
    }
    content.extend_from_slice(b"(after) Tj ET");
    assert_eq!(
        positions(&content),
        vec![("in".to_owned(), 9.0, 9.0), ("after".to_owned(), 1.0, 1.0)],
        "被上限丢弃的 q 不影响配平后的恢复"
    );
}

#[test]
fn begin_text_after_restore_still_resets_the_matrices_to_identity() {
    // Table 107：`BT` 无条件把 `Tm`/`Tlm` 置为单位矩阵，与 `Q` 的恢复无关。
    assert_eq!(
        positions(
            b"q BT 1 0 0 1 7 8 Tm (before) Tj ET \
               Q BT (after) Tj 20 20 Td (moved) Tj ET"
        ),
        vec![
            ("before".to_owned(), 7.0, 8.0),
            ("after".to_owned(), 0.0, 0.0),
            ("moved".to_owned(), 20.0, 20.0),
        ]
    );
}

#[test]
fn restore_brings_the_text_matrices_back_with_the_other_parameters() {
    // 原有五个字段（字体名/字号/CTM/leading/rise）的恢复语义不变，
    // `Tm`/`Tlm` 与它们一道恢复。
    assert_eq!(
        positions(
            b"1 TL q 2 TL 5 Ts 1 0 0 1 3 4 cm \
               BT /F1 12 Tf 1 0 0 1 100 100 Tm (a) Tj Q (b) Tj T* (c) Tj ET"
        ),
        vec![
            // Tm(100 100) × CTM(3 4) 再抬 rise 5。
            ("a".to_owned(), 103.0, 109.0),
            // Q 后：Tm 回到 q 时刻的单位矩阵，CTM/rise 同步回退。
            ("b".to_owned(), 0.0, 0.0),
            // T* 用恢复后的 leading = 1，从恢复后的 Tlm 起算。
            ("c".to_owned(), 0.0, -1.0),
        ]
    );
}

#[test]
fn td_inside_a_saved_region_is_undone_by_restore() {
    // `Td` 改动的是 `Tm`/`Tlm`，故属于被 `q`/`Q` 保存的图形状态。
    assert_eq!(
        positions(b"BT /F1 12 Tf 1 0 0 1 40 40 Tm q 0 -20 Td (in) Tj Q (out) Tj ET"),
        vec![
            ("in".to_owned(), 40.0, 20.0),
            ("out".to_owned(), 40.0, 40.0)
        ]
    );
}
