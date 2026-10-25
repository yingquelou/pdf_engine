//! Type 3 字形程序（`/CharProcs` 流）开头 `d0`/`d1` 操作符的最小化扫描。
//!
//! 不复用 `reader::text` 的内容流分词器：它对数字采用宽容解析（失败按 0
//! 处理），无法区分非法操作数；这里按 `d0`/`d1` 的操作数要求做严格解析。

use super::super::{FontError, FontResult};
use super::Type3GlyphMetrics;

/// 扫描字形程序流的第一个操作符，解释 `d0`/`d1` 声明的指标。
///
/// 显式迭代（不使用递归）：逐个分词，数值 token 压入操作数列表，遇到首个
/// 非数值 token 即首操作符，校验后立即返回；不解释后续图形操作符。
pub(super) fn scan_glyph_metrics_header(
    bytes: &[u8],
    glyph: &str,
) -> FontResult<Type3GlyphMetrics> {
    let mut operands: Vec<f64> = Vec::new();
    let mut pos = 0usize;
    loop {
        skip_ws_and_comments(bytes, &mut pos);
        if pos >= bytes.len() {
            return Err(FontError::Invalid(format!(
                "character procedure /{glyph} ends before its d0/d1 operator"
            )));
        }
        let first = bytes[pos];
        if is_delimiter(first) {
            // 字符串、数组、字典、名称等都以分隔符开头，`d0`/`d1` 只接受
            // 数值操作数，出现即非法。
            return Err(FontError::Invalid(format!(
                "character procedure /{glyph} has a non-numeric operand starting with {:?}",
                char::from(first)
            )));
        }
        // 扫描一个常规 token（到空白或分隔符为止）。
        let start = pos;
        while pos < bytes.len() && is_regular(bytes[pos]) && !is_delimiter(bytes[pos]) {
            pos += 1;
        }
        let token = &bytes[start..pos];
        if token == b"d0" || token == b"d1" {
            let expected = if token == b"d0" { 2 } else { 6 };
            if operands.len() != expected {
                return Err(FontError::Invalid(format!(
                    "character procedure /{glyph}: {} takes {expected} numeric operands, found {}",
                    String::from_utf8_lossy(token),
                    operands.len()
                )));
            }
            let width = [operands[0], operands[1]];
            // 惰性求值：d0 时不得索引不存在的操作数。
            let bbox =
                (token == b"d1").then(|| [operands[2], operands[3], operands[4], operands[5]]);
            return Ok(Type3GlyphMetrics { width, bbox });
        }
        if is_number_start(first) {
            let text = std::str::from_utf8(token).map_err(|_| {
                FontError::Invalid(format!(
                    "character procedure /{glyph} has an invalid operand"
                ))
            })?;
            match text.parse::<f64>() {
                Ok(value) if value.is_finite() => operands.push(value),
                _ => {
                    return Err(FontError::Invalid(format!(
                        "character procedure /{glyph} has an invalid numeric operand {text:?}"
                    )));
                }
            }
        } else {
            // 首个非数值 token 即首操作符，按规范必须是 d0/d1。
            return Err(FontError::Invalid(format!(
                "first operator of character procedure /{glyph} is {:?}, expected d0 or d1",
                String::from_utf8_lossy(token)
            )));
        }
    }
}

/// 跳过 PDF 空白字符与 `%` 注释。
fn skip_ws_and_comments(input: &[u8], pos: &mut usize) {
    while *pos < input.len() {
        let b = input[*pos];
        if is_whitespace(b) {
            *pos += 1;
        } else if b == b'%' {
            while *pos < input.len() && input[*pos] != b'\n' && input[*pos] != b'\r' {
                *pos += 1;
            }
        } else {
            break;
        }
    }
}

fn is_whitespace(b: u8) -> bool {
    matches!(b, 0x00 | 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_regular(b: u8) -> bool {
    b > 0x20 && b != 0x7F
}

/// token 首字节是否可能是数值（符号、数字或小数点）。
fn is_number_start(b: u8) -> bool {
    matches!(b, b'0'..=b'9' | b'+' | b'-' | b'.')
}
