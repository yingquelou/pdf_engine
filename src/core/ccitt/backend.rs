//! `hayro-ccitt` 的唯一适配点：参数映射与损坏行恢复的编排。
//!
//! 上层只经 [`CcittBackend`](super::decoder::CcittBackend) 交互。位流扫描见
//! `eol`，替换行的重新编码见 `encode`，行打包与 `/BlackIs1`、行末与 `/Rows`
//! 补白见 `sink`；本文件只保留 `/DecodeParms` 到库设置的映射、EOB/RTC 截断，
//! 以及「出错 → 替换损坏行 → 重新解码」的编排。
//!
//! `/DamagedRowsBeforeError` 的恢复不依赖 hayro 的内部状态：出错后按位扫描
//! EOL、定位损坏行所在的位区间，按上一行的换色位置重新编码（或编码为全白行），
//! 替换该区间与行尾 EOL 后重新解码。行解码仍由 hayro 承担。

use hayro_ccitt::{
    DecodeSettings, Decoder as HayroDecoder, DecoderContext, EncodingMode, decode as hayro_decode,
};

use super::CcittParams;
use super::bitbuf::BitBuf;
use super::decoder::CcittBackend;
use super::eol::{damaged_window, eob_start, eol_runs};
use super::encode::{row_transitions, substitute_row};
use super::sink::RowSink;

/// 基于 `hayro-ccitt` 的 CCITT 后端。
pub(super) struct HayroCcittBackend;

impl CcittBackend for HayroCcittBackend {
    fn decode(&self, data: &[u8], params: &CcittParams) -> Result<Vec<u8>, String> {
        let settings = decode_settings(params)?;
        let mut stream = BitBuf::from_bytes(data);
        // `/EndOfBlock false` 时 hayro 会越过 EOB/RTC 继续解码；为使 EOB/RTC
        // 仍参与「三者最早」终止，先按位定位 EOB/RTC 并把码流截断到该处。
        if !params.end_of_block
            && let Some(eob) = eob_start(&stream, params)
        {
            stream.truncate(eob);
        }
        let mut damaged_left = params.damaged_rows_before_error;
        // 最近一次被替换的行号：识别连续损坏并防止对同一行反复替换。
        let mut last_substitute: Option<usize> = None;

        loop {
            let mut sink = RowSink::new(
                params.columns.div_ceil(8),
                params.rows,
                params.black_is_1,
            );
            let mut ctx = DecoderContext::new(settings);
            let error = match hayro_decode(stream.bytes(), &mut sink, &mut ctx) {
                Ok(_) => return sink.finish(),
                Err(error) => error,
            };
            // 一行也没解出来：错误原样上报（原实现同样直接报错）。
            if sink.rows == 0 {
                return Err(format!("CCITTFaxDecode: {error}"));
            }

            // 与原实现一致：只有 G3 + `/EndOfLine` 且还有可替换行时才恢复。
            if params.k < 0 || !params.end_of_line || damaged_left == 0 {
                return sink.finish();
            }

            let index = sink.rows;
            // 同一行替换后仍失败：停止，避免反复改写同一区间。
            if last_substitute == Some(index) {
                return sink.finish();
            }
            let runs = eol_runs(&stream);
            // 损坏行之后必须还有一个 EOL（原实现的 `seek_to_eol` 条件）。
            let Some(window) = damaged_window(&runs, index) else {
                return sink.finish();
            };

            // 连续损坏时按原实现口径输出空白行，否则复刻上一行。
            let previous_damaged = last_substitute == Some(index - 1);
            let reference = row_transitions(
                sink.row_bytes(index - 1),
                params.columns,
                params.black_is_1,
            );
            let replacement = substitute_row(params, &reference, !previous_damaged);
            stream.splice(window.start, window.end - window.start, &replacement);

            damaged_left -= 1;
            last_substitute = Some(index);
        }
    }
}

fn decode_settings(params: &CcittParams) -> Result<DecodeSettings, String> {
    Ok(DecodeSettings {
        columns: u32::try_from(params.columns)
            .map_err(|_| "CCITTFaxDecode: /Columns out of range".to_string())?,
        rows: match params.rows {
            Some(rows) => u32::try_from(rows)
                .map_err(|_| "CCITTFaxDecode: /Rows out of range".to_string())?,
            // 高度未预定义：交给 EOFB/RTC 或数据结束终止。
            None => u32::MAX,
        },
        // PDF 语义与原实现一致：EOB/RTC、`/Rows`、数据结束三者最早者终止。
        // `/EndOfBlock false` 只表示不*期望*显式 EOB，EOB/RTC 出现时仍须在此
        // 终止——该情形由 `decode` 先把码流截断到 EOB/RTC 再交给 hayro。
        end_of_block: params.end_of_block,
        end_of_line: params.end_of_line,
        rows_are_byte_aligned: params.encoded_byte_align,
        encoding: if params.k < 0 {
            EncodingMode::Group4
        } else if params.k == 0 {
            EncodingMode::Group3_1D
        } else {
            EncodingMode::Group3_2D { k: params.k as u32 }
        },
        // `/BlackIs1` 的位反转在打包阶段完成，库保持 0=黑、1=白。
        invert_black: false,
    })
}

/// `hayro-ccitt` 行回调的唯一适配实现：原样转发到中性的 [`RowSink`]。
///
/// 本文件是 `hayro_ccitt` 在本仓库登记的唯一引用点；行打包逻辑本身不接触
/// crate 类型，见 `sink`。
impl HayroDecoder for RowSink {
    fn push_pixel(&mut self, white: bool) {
        RowSink::push_pixel(self, white);
    }

    fn push_pixel_chunk(&mut self, white: bool, chunk_count: u32) {
        RowSink::push_pixel_chunk(self, white, chunk_count);
    }

    fn next_line(&mut self) {
        RowSink::next_line(self);
    }
}
