//! CCITT Group 3/4 解码的 PDF 语义层：输出规模护栏与后端调度。
//!
//! 位流与行解码由第三方库承担，经 `ccitt/backend.rs` 隔离；本文件不含码表，
//! 也不引用任何 crate 类型。

use super::CcittParams;
use super::backend::HayroCcittBackend;

/// 输出规模上限（防止畸形 `/Columns`/`Rows` 触发超大分配）。
pub(super) const MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;

/// 解码 CCITT 码流，返回按行打包的 1 位像素数据。
pub(crate) fn decode(data: &[u8], params: &CcittParams) -> Result<Vec<u8>, String> {
    if params.columns == 0 {
        return Err("CCITTFaxDecode: /Columns must be positive".to_string());
    }
    let bytes_per_row = params.columns.div_ceil(8);
    if bytes_per_row > MAX_OUTPUT_BYTES {
        return Err("CCITTFaxDecode: /Columns too large".to_string());
    }
    if let Some(rows) = params.rows {
        let total = bytes_per_row
            .checked_mul(rows)
            .ok_or_else(|| "CCITTFaxDecode: output size overflow".to_string())?;
        if total > MAX_OUTPUT_BYTES {
            return Err("CCITTFaxDecode: /Columns * /Rows exceeds output limit".to_string());
        }
    }
    decode_with(data, params, &HayroCcittBackend)
}

/// 位流与行解码后端。
///
/// 契约：输入为 PDF `/DecodeParms` 语义参数，输出为逐行打包的 1 位位图
/// （`ceil(Columns/8)` 字节/行），且已完成 `/BlackIs1` 位反转；`Rows` 已知时
/// 不足部分补白行；一条完整行都解不出时报错。
pub(super) trait CcittBackend {
    fn decode(&self, data: &[u8], params: &CcittParams) -> Result<Vec<u8>, String>;
}

pub(super) fn decode_with<B: CcittBackend>(
    data: &[u8],
    params: &CcittParams,
    backend: &B,
) -> Result<Vec<u8>, String> {
    backend.decode(data, params)
}
