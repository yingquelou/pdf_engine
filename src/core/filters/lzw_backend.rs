//! `weezl` 的唯一适配点：把 LZW 位流与字典解码挡在 PDF 语义层之外。
//!
//! 上层只经 [`LzwBackend`](super::lzw::LzwBackend) 交互；换库或改回本地实现
//! 时只需替换本文件，`lzw.rs` 的 `/EarlyChange` 与 Predictor 语义不受影响。

use weezl::BitOrder;
use weezl::decode::Decoder;

use super::lzw::LzwBackend;

/// 基于 `weezl` 的 LZW 解码后端。
pub(super) struct WeezlLzwBackend;

impl LzwBackend for WeezlLzwBackend {
    fn decode(&self, data: &[u8], early_change: bool) -> Result<Vec<u8>, String> {
        // PDF 码流高位优先；`/EarlyChange 1` 与 TIFF 的「提前一个码切换」
        // 口径一致，`/EarlyChange 0` 走标准口径。
        let mut decoder = if early_change {
            Decoder::with_tiff_size_switch(BitOrder::Msb, 8)
        } else {
            Decoder::new(BitOrder::Msb, 8)
        };
        let mut out = Vec::new();
        let result = decoder.into_vec(&mut out).decode(data);
        result
            .status
            .map_err(|err| format!("LZWDecode: invalid code (outside table): {err}"))?;
        Ok(out)
    }
}
