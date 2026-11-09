//! `CCITTFaxDecode` 解码器（ISO 32000-1 §7.4.6；编码算法见 ITU-T T.4/T.6）。
//!
//! 目录模块：
//! - `decoder`：PDF 语义层（输出规模护栏、后端调度与输出契约）；
//! - `backend`：`hayro-ccitt` 的唯一适配点（参数映射与损坏行恢复编排）；
//! - `bitbuf`：MSB 优先位缓冲与带规范 EOL 的区间替换；
//! - `eol`：EOL/RTC/EOFB 位扫描与损坏行窗口定位；
//! - `encode`：换色位置提取与替换行的 T.4/T.6 编码（含游程码表）；
//! - `sink`：逐行打包、`/BlackIs1` 反转与行/`/Rows` 补白。
//!
//! 本文件保留参数结构、公共入口与测试。

use crate::core::types::{Dict, Value};

mod backend;
mod bitbuf;
mod decoder;
mod eol;
mod encode;
mod sink;

pub(crate) use decoder::decode;

/// `CCITTFaxDecode` 的可选参数（`/DecodeParms`，缺省值按 ISO 32000-1 表 11）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct CcittParams {
    /// `<0` Group 4；`0` Group 3 1-D；`>0` Group 3 2-D。
    pub k: i32,
    /// 图像宽度（像素）。
    pub columns: usize,
    /// 图像高度；`None` 表示未预定义（按 EOB/数据结束）。
    pub rows: Option<usize>,
    /// 每行是否应有 EOL（解码端始终容忍 EOL）。
    pub end_of_line: bool,
    /// 每行是否按字节边界对齐。
    pub encoded_byte_align: bool,
    /// 是否期望显式 EOB/RTC（`/EndOfBlock`）。
    ///
    /// 与原实现一致：EOB/RTC、`/Rows` 与数据结束三者中最早到达者总是终止；
    /// `/EndOfBlock false` 只表示不期望显式 EOB，适配层会先把码流截断到 EOB/RTC。
    pub end_of_block: bool,
    /// 1 位是否表示黑色（缺省 false：0=黑、1=白）。
    pub black_is_1: bool,
    /// 允许在报错前跳过的损坏行数。
    ///
    /// 由适配层恢复原语义：出错后跳到下一 EOL，用上一行（连续损坏时用空白行）
    /// 替换损坏行，最多重复该次数；达到上限或无法重同步时才截断并按 `/Rows`
    /// 补白行。
    pub damaged_rows_before_error: usize,
}

impl CcittParams {
    /// 从 `/DecodeParms` 字典读取参数；`None` 使用全部缺省值。
    pub(crate) fn from_dict(dict: Option<&Dict>) -> Result<Self, String> {
        let int = |key: &str| dict.and_then(|d| d.get(key)).and_then(|v| v.as_integer());
        let boolean = |key: &str| match dict.and_then(|d| d.get(key)) {
            Some(Value::Bool(b)) => Some(*b),
            _ => None,
        };

        let k = int("K").unwrap_or(0);
        let k = i32::try_from(k).map_err(|_| "CCITTFaxDecode: /K out of range".to_string())?;

        let columns = int("Columns").unwrap_or(1728);
        if columns <= 0 {
            return Err(format!(
                "CCITTFaxDecode: /Columns must be positive, got {columns}"
            ));
        }
        let columns = usize::try_from(columns)
            .map_err(|_| "CCITTFaxDecode: /Columns out of range".to_string())?;

        let rows = int("Rows").unwrap_or(0);
        if rows < 0 {
            return Err(format!(
                "CCITTFaxDecode: /Rows must be non-negative, got {rows}"
            ));
        }
        let rows = if rows == 0 {
            None
        } else {
            Some(
                usize::try_from(rows)
                    .map_err(|_| "CCITTFaxDecode: /Rows out of range".to_string())?,
            )
        };

        let damaged = int("DamagedRowsBeforeError").unwrap_or(0);
        if damaged < 0 {
            return Err(format!(
                "CCITTFaxDecode: /DamagedRowsBeforeError must be non-negative, got {damaged}"
            ));
        }
        let damaged_rows_before_error = usize::try_from(damaged)
            .map_err(|_| "CCITTFaxDecode: damaged row limit out of range".to_string())?;

        Ok(Self {
            k,
            columns,
            rows,
            end_of_line: boolean("EndOfLine").unwrap_or(false),
            encoded_byte_align: boolean("EncodedByteAlign").unwrap_or(false),
            end_of_block: boolean("EndOfBlock").unwrap_or(true),
            black_is_1: boolean("BlackIs1").unwrap_or(false),
            damaged_rows_before_error,
        })
    }
}

#[cfg(test)]
mod tests;
