//! `CCITTFaxDecode` 解码器（ISO 32000-1 §7.4.6；编码算法见 ITU-T T.4/T.6）。
//!
//! 目录模块：
//! - `decoder`：位流、行解码、二维参考行与输出打包；
//! - `tables`：T.4 白/黑游程表与 T.6 二维模式码表。
//!
//! 本文件保留参数结构、公共入口与测试。纯 Rust 实现，不依赖第三方库。

use crate::core::types::{Dict, Value};

mod decoder;
mod tables;

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
    /// 是否期望 EOB/RTC 终止。
    ///
    /// 当前实现在 EOB/RTC、`Rows` 或数据结束处**取最早者**停止（对真实文件
    /// 更宽容），因此该标志只记录调用方期望，不改变输出；保留字段以便未来
    /// 需要严格/宽容模式切换时直接消费。
    #[allow(dead_code)]
    pub end_of_block: bool,
    /// 1 位是否表示黑色（缺省 false：0=黑、1=白）。
    pub black_is_1: bool,
    /// 允许在报错前跳过的损坏行数。
    pub damaged_rows_before_error: usize,
}

impl CcittParams {
    /// 从 `/DecodeParms` 字典读取参数；`None` 使用全部缺省值。
    pub(crate) fn from_dict(dict: Option<&Dict>) -> Result<Self, String> {
        let int = |key: &str| {
            dict.and_then(|d| d.get(key))
                .and_then(|v| v.as_integer())
        };
        let boolean = |key: &str| match dict.and_then(|d| d.get(key)) {
            Some(Value::Bool(b)) => Some(*b),
            _ => None,
        };

        let k = int("K").unwrap_or(0);
        let k = i32::try_from(k).map_err(|_| "CCITTFaxDecode: /K out of range".to_string())?;

        let columns = int("Columns").unwrap_or(1728);
        if columns <= 0 {
            return Err(format!("CCITTFaxDecode: /Columns must be positive, got {columns}"));
        }
        let columns = usize::try_from(columns)
            .map_err(|_| "CCITTFaxDecode: /Columns out of range".to_string())?;

        let rows = int("Rows").unwrap_or(0);
        if rows < 0 {
            return Err(format!("CCITTFaxDecode: /Rows must be non-negative, got {rows}"));
        }
        let rows = if rows == 0 {
            None
        } else {
            Some(usize::try_from(rows).map_err(|_| "CCITTFaxDecode: /Rows out of range".to_string())?)
        };

        let damaged = int("DamagedRowsBeforeError").unwrap_or(0);
        if damaged < 0 {
            return Err(format!(
                "CCITTFaxDecode: /DamagedRowsBeforeError must be non-negative, got {damaged}"
            ));
        }
        let damaged_rows_before_error =
            usize::try_from(damaged).map_err(|_| "CCITTFaxDecode: damaged row limit out of range".to_string())?;

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
