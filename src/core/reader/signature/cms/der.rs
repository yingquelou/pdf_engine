//! CMS 解析用的最小 DER 读取器：只接受定长编码，并累计上限状态。
//!
//! - 长度超过剩余字节、截断、indefinite length 一律类型化错误；
//! - 高位标签号（`tag & 0x1F == 0x1F`）不支持，按坏标签拒绝；
//! - 每个 TLV 计入元素数；构造容器按需调用 [`ParseState::enter`] 计深度；
//! - 未知/暂不建模的条目（证书、CRL、unsignedAttrs）只按声明长度跳过，
//!   不做内容扫描。

use super::{CmsError, CmsLimits};

/// 跨嵌套游标共享的解析状态（深度、元素计数与上限）。
pub(super) struct ParseState {
    limits: CmsLimits,
    depth: usize,
    elements: usize,
}

impl ParseState {
    pub(super) fn new(limits: CmsLimits) -> Self {
        Self {
            limits,
            depth: 0,
            elements: 0,
        }
    }

    /// 进入一层构造容器。
    pub(super) fn enter(&mut self) -> Result<(), CmsError> {
        self.depth += 1;
        if self.depth > self.limits.max_depth {
            return Err(CmsError::DepthLimit {
                limit: self.limits.max_depth,
            });
        }
        Ok(())
    }

    /// 离开一层构造容器（出错路径无需调用）。
    pub(super) fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn count_element(&mut self) -> Result<(), CmsError> {
        self.elements += 1;
        if self.elements > self.limits.max_elements {
            return Err(CmsError::ElementLimit {
                limit: self.limits.max_elements,
            });
        }
        Ok(())
    }
}

/// 一个完整的 DER TLV：标签、内容、原始字节与在整体 DER 中的偏移。
pub(super) struct Tlv<'a> {
    pub tag: u8,
    pub content: &'a [u8],
    pub raw: &'a [u8],
    pub offset: usize,
}

impl Tlv<'_> {
    /// 内容首个字节在整体 DER 中的偏移。
    pub(super) fn content_offset(&self) -> usize {
        self.offset + (self.raw.len() - self.content.len())
    }
}

/// 在字节切片上顺序读取 TLV 的游标；`base` 把局部偏移换算为整体偏移。
pub(super) struct Cursor<'a> {
    data: &'a [u8],
    base: usize,
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(data: &'a [u8], base: usize) -> Self {
        Self { data, base, pos: 0 }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// 已消费字节数（局部偏移）。
    pub(super) fn position(&self) -> usize {
        self.pos
    }

    /// 下一个待读 TLV 在整体 DER 中的偏移。
    pub(super) fn next_offset(&self) -> usize {
        self.base + self.pos
    }

    /// 下一个 TLV 的标签字节（不消费）。
    pub(super) fn peek_tag(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    /// 读取下一个 TLV；尾部截断、非法长度均返回类型化错误。
    pub(super) fn read(&mut self, state: &mut ParseState) -> Result<Tlv<'a>, CmsError> {
        let start = self.pos;
        let offset = self.base + start;
        let Some(&tag) = self.data.get(start) else {
            return Err(CmsError::Truncated { offset });
        };
        if tag & 0x1F == 0x1F {
            return Err(CmsError::UnexpectedTag {
                expected: "single-byte tag",
                tag,
                offset,
            });
        }
        let mut cursor = start + 1;
        let Some(&first) = self.data.get(cursor) else {
            return Err(CmsError::Truncated {
                offset: self.base + cursor,
            });
        };
        cursor += 1;
        let length = if first < 0x80 {
            u64::from(first)
        } else if first == 0x80 {
            return Err(CmsError::IndefiniteLength { offset });
        } else {
            let count = usize::from(first & 0x7F);
            if count > 8 {
                return Err(CmsError::LengthOverflow { offset });
            }
            let end = cursor + count;
            if end > self.data.len() {
                return Err(CmsError::Truncated {
                    offset: self.base + self.data.len(),
                });
            }
            let mut value: u64 = 0;
            for &byte in &self.data[cursor..end] {
                value = value
                    .checked_mul(256)
                    .and_then(|value| value.checked_add(u64::from(byte)))
                    .ok_or(CmsError::LengthOverflow { offset })?;
            }
            cursor = end;
            value
        };
        let remaining = self.data.len() - cursor;
        if length > remaining as u64 {
            return Err(CmsError::LengthExceedsRemaining {
                offset,
                length,
                remaining,
            });
        }
        state.count_element()?;
        let content_start = cursor;
        let end = cursor + length as usize;
        self.pos = end;
        Ok(Tlv {
            tag,
            content: &self.data[content_start..end],
            raw: &self.data[start..end],
            offset,
        })
    }

    /// 读取并校验标签；标签不符返回 `UnexpectedTag`。
    pub(super) fn read_tag(
        &mut self,
        tag: u8,
        expected: &'static str,
        state: &mut ParseState,
    ) -> Result<Tlv<'a>, CmsError> {
        let tlv = self.read(state)?;
        if tlv.tag != tag {
            return Err(CmsError::UnexpectedTag {
                expected,
                tag: tlv.tag,
                offset: tlv.offset,
            });
        }
        Ok(tlv)
    }

    /// 要求游标处于末尾。
    pub(super) fn finish(&self) -> Result<(), CmsError> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err(CmsError::TrailingData {
                offset: self.base + self.pos,
                len: self.data.len() - self.pos,
            })
        }
    }
}

/// 解码 OBJECT IDENTIFIER 内容为点分数字串。
pub(super) fn decode_oid(content: &[u8], offset: usize) -> Result<String, CmsError> {
    if content.is_empty() {
        return Err(CmsError::InvalidOid { offset });
    }
    let mut arcs: Vec<u64> = Vec::new();
    let mut value: u64 = 0;
    let mut pending = false;
    for &byte in content {
        value = value
            .checked_mul(128)
            .and_then(|value| value.checked_add(u64::from(byte & 0x7F)))
            .ok_or(CmsError::InvalidOid { offset })?;
        pending = true;
        if byte & 0x80 == 0 {
            if arcs.is_empty() {
                let (first, second) = if value < 40 {
                    (0, value)
                } else if value < 80 {
                    (1, value - 40)
                } else {
                    (2, value - 80)
                };
                arcs.push(first);
                arcs.push(second);
            } else {
                arcs.push(value);
            }
            value = 0;
            pending = false;
        }
    }
    if pending {
        return Err(CmsError::InvalidOid { offset });
    }
    Ok(arc_join(&arcs))
}

fn arc_join(arcs: &[u64]) -> String {
    let mut out = String::new();
    for (index, arc) in arcs.iter().enumerate() {
        if index > 0 {
            out.push('.');
        }
        out.push_str(&arc.to_string());
    }
    out
}

/// 解码小整数（version 等）；超过 8 字节或越界返回错误。
pub(super) fn decode_integer(content: &[u8], offset: usize) -> Result<i64, CmsError> {
    if content.is_empty() || content.len() > 16 {
        return Err(CmsError::IntegerOverflow { offset });
    }
    let mut value: i128 = if content[0] & 0x80 != 0 { -1 } else { 0 };
    for &byte in content {
        value = (value << 8) | i128::from(byte);
    }
    i64::try_from(value).map_err(|_| CmsError::IntegerOverflow { offset })
}

/// 解码 IA5/ASCII 文本（UTCTime/GeneralizedTime 等）。
pub(super) fn decode_text(
    content: &[u8],
    offset: usize,
    field: &'static str,
) -> Result<String, CmsError> {
    std::str::from_utf8(content)
        .map(str::to_string)
        .map_err(|_| CmsError::InvalidText { field, offset })
}
