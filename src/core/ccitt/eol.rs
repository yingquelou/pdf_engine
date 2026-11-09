//! EOL / RTC / EOFB 的位级扫描与损坏行窗口定位。
//!
//! 契约：输入为 MSB 优先位流与 `/K`；输出为连续 EOL 的位区间、最早 EOB/RTC 的
//! 起始位，以及第 `index` 行在码流中的替换窗口。全部只依赖位流本身，不读解码结果。

use super::CcittParams;
use super::bitbuf::BitBuf;

/// 连续 EOL 的位区间与计数（6 个及以上为 RTC）。
#[derive(Debug, Clone, Copy)]
pub(super) struct EolRun {
    start: usize,
    end: usize,
    count: usize,
}

/// 损坏行在码流中的位区间：行数据起点到其后第一个 EOL 的末尾。
#[derive(Debug, Clone, Copy)]
pub(super) struct RowWindow {
    pub(super) start: usize,
    pub(super) end: usize,
}

/// Group 4 的 EOFB：连续两个 EOL（24 位，T.6 §2.4.1.1）。
const EOFB: u32 = 0x00_10_01;

/// 最早到达的 EOB/RTC 的起始位，供 `/EndOfBlock false` 截断码流。
pub(super) fn eob_start(stream: &BitBuf, params: &CcittParams) -> Option<usize> {
    if params.k < 0 {
        let mut window = 0u32;
        for index in 0..stream.bit_len() {
            window = ((window << 1) | u32::from(stream.bit(index))) & 0x00ff_ffff;
            if index >= 23 && window == EOFB {
                return Some(index - 23);
            }
        }
        None
    } else {
        // Group 3 的 RTC：连续 6 个 EOL。
        eol_runs(stream)
            .into_iter()
            .find(|run| run.count >= 6)
            .map(|run| run.start)
    }
}

/// 按位扫描 EOL（`>=11` 个 0 后跟 1），紧邻的合并为一段；遇到 RTC 即停止。
pub(super) fn eol_runs(stream: &BitBuf) -> Vec<EolRun> {
    let mut runs: Vec<EolRun> = Vec::new();
    let mut zeros = 0usize;
    let mut zero_start = 0usize;

    for index in 0..stream.bit_len() {
        if stream.bit(index) {
            if zeros >= 11 {
                match runs.last_mut() {
                    // 紧邻上一段 EOL：同一段连续 EOL。
                    Some(last) if last.end == zero_start => {
                        last.end = index + 1;
                        last.count += 1;
                    }
                    _ => runs.push(EolRun {
                        start: zero_start,
                        end: index + 1,
                        count: 1,
                    }),
                }
            }
            zeros = 0;
        } else {
            if zeros == 0 {
                zero_start = index;
            }
            zeros += 1;
        }
        if runs.last().is_some_and(|run| run.count >= 6) {
            // RTC：原实现在此终止，之后的行不再参与替换。
            break;
        }
    }
    runs
}

/// 第 `index` 行的替换区间。
///
/// 行数据从「上一条 EOL 的末尾」开始、到其后第一个 EOL 段的末尾结束。损坏
/// 行末尾的 0 位即使与其后的 EOL 合流，也会随该 EOL 一起被替换掉，与原实现
/// 「跳到下一 EOL 后用替换行继续」的恢复口径一致。
pub(super) fn damaged_window(runs: &[EolRun], index: usize) -> Option<RowWindow> {
    // G3 可以带一个前导 EOL；此时第 0 行从首个 EOL 之后开始。
    let leading = runs.first().is_some_and(|run| run.start == 0);
    let (start, eol_index) = if leading {
        (runs.get(index)?.end, index + 1)
    } else if index == 0 {
        (0, 0)
    } else {
        (runs.get(index - 1)?.end, index)
    };
    Some(RowWindow {
        start,
        end: runs.get(eol_index)?.end,
    })
}

#[cfg(test)]
mod tests {
    use super::{EolRun, damaged_window, eob_start, eol_runs};
    use crate::core::ccitt::CcittParams;
    use crate::core::ccitt::bitbuf::BitBuf;

    fn bits(pattern: &str) -> BitBuf {
        let mut buf = BitBuf::new();
        for byte in pattern.bytes() {
            buf.push_bit(byte == b'1');
        }
        buf
    }

    fn params(k: i32) -> CcittParams {
        CcittParams {
            k,
            columns: 8,
            rows: None,
            end_of_line: false,
            encoded_byte_align: false,
            end_of_block: false,
            black_is_1: false,
            damaged_rows_before_error: 0,
        }
    }

    #[test]
    fn merges_adjacent_eols_and_stops_at_rtc() {
        let eol = "000000000001";
        let stream = bits(&(eol.to_owned() + eol + "01" + &eol.repeat(6) + "1"));

        let runs = eol_runs(&stream);

        // 两段：前两行 EOL 合并为一段，RTC 为另一段；RTC 之后的位不再扫描。
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].start, runs[0].end, runs[0].count), (0, 24, 2));
        assert_eq!((runs[1].start, runs[1].end, runs[1].count), (26, 98, 6));
    }

    #[test]
    fn finds_group4_eofb_start() {
        let stream = bits(&("1".to_owned() + "000000000001" + "000000000001"));
        // EOFB 从第 1 位开始（跳过首行的 V0）。
        assert_eq!(eob_start(&stream, &params(-1)), Some(1));
    }

    #[test]
    fn finds_group3_rtc_start() {
        let eol = "000000000001";
        let stream = bits(&(eol.repeat(6) + "1"));
        assert_eq!(eob_start(&stream, &params(0)), Some(0));
    }

    #[test]
    fn window_skips_leading_eol() {
        let runs = [
            EolRun {
                start: 0,
                end: 12,
                count: 1,
            },
            EolRun {
                start: 20,
                end: 32,
                count: 1,
            },
            EolRun {
                start: 40,
                end: 52,
                count: 1,
            },
        ];

        let window = damaged_window(&runs, 0).unwrap();
        assert_eq!((window.start, window.end), (12, 32));

        let window = damaged_window(&runs, 1).unwrap();
        assert_eq!((window.start, window.end), (32, 52));

        assert!(damaged_window(&runs, 2).is_none());
    }

    #[test]
    fn window_without_leading_eol_starts_at_zero() {
        let runs = [
            EolRun {
                start: 20,
                end: 32,
                count: 1,
            },
            EolRun {
                start: 40,
                end: 52,
                count: 1,
            },
        ];

        // 无前导 EOL：第 0 行从码流起点开始。
        let window = damaged_window(&runs, 0).unwrap();
        assert_eq!((window.start, window.end), (0, 32));

        let window = damaged_window(&runs, 1).unwrap();
        assert_eq!((window.start, window.end), (32, 52));

        assert!(damaged_window(&runs, 2).is_none());
    }
}
