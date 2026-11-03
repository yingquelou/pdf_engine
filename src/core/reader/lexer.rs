//! PDF 窗口词法器。
//!
//! token 形态覆盖 ISO 32000-1 第 7.2 节，另含一项**非 ISO 32000-1
//! 语法**：`<~ ... ~>` ASCII85 字符串——Adobe 的扩展字面量形态，标准
//! 第 7 章只定义 ASCII85 *过滤器*（7.4.3）而未定义该字面量。此处按
//! 生产者实态提供互操作读取（本库写端 `writer/value.rs` 亦产出该形态），
//! 不把它建模为 ISO 特性，故不纳入 `PdfFeature` 版本门禁。

use super::{Result, err, find_subslice};

/// PDF 词法分析器。
///
/// 持有一个**窗口**字节切片（非整个文件）与当前游标 `pos`，外加 `base`
/// （窗口在文件中的绝对起始偏移）。错误偏移报告为 `base + pos`（绝对
/// 文件偏移），便于跨窗口定位。提供「跳过空白注释、读一个 token、按需
/// 回退」的最小能力，供自由解析函数组合使用。
pub struct Lexer<'a> {
    input: &'a [u8],
    pub(super) pos: usize,
    base: u64,
}

/// 词法 token。仅刻画语法类别，不试图在词法层完成语义。
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Token {
    /// 普通关键字，如 `obj` / `endobj` / `trailer` / `startxref` /
    /// `xref` / `true` / `false` / `null` / `R` / `stream` / `endstream`。
    Keyword(String),
    /// 数字：整数或浮点（保留原字符串）。
    Number(String),
    /// Name，已去掉前导 `/`。
    Name(String),
    /// 字面字符串，已去掉 `()` 包裹。
    Literal(String),
    /// 十六进制字符串，已去掉 `<>` 包裹。
    Hex(String),
    /// ASCII85 字符串，已去掉 `<~` `~>` 包裹。
    ///
    /// **非 ISO 32000-1 语法**：`<~ ... ~>` 是 Adobe 的 ASCII85 字符串
    /// 扩展形态，标准第 7 章未定义（规范只定义 ASCII85 *过滤器*，
    /// 7.4.3）。此处按生产者实态提供互操作读取，不是 ISO 特性，
    /// 故不纳入 `PdfFeature` 版本门禁。
    Ascii85(String),
    /// `<<`。
    DictOpen,
    /// `>>`。
    DictClose,
    /// `[`。
    ArrayOpen,
    /// `]`。
    ArrayClose,
}

impl<'a> Lexer<'a> {
    #[cfg(test)]
    pub(super) fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            base: 0,
        }
    }

    /// 在指定绝对偏移构造窗口词法器。`base` 即 `offset`。
    pub(super) fn at(input: &'a [u8], base: u64) -> Self {
        Self {
            input,
            pos: 0,
            base,
        }
    }

    /// 当前游标的**绝对文件偏移**。
    pub(super) fn pos(&self) -> u64 {
        self.base + self.pos as u64
    }

    pub(super) fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    pub(super) fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.input.get(self.pos + ahead).copied()
    }

    pub(super) fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    /// PDF 空白字符集（ISO 32000-1 §7.2）：0x00 NUL、0x09 TAB、
    /// 0x0A LF、0x0C FF、0x0D CR、0x20 空格。
    /// 与 `filters` 的 `is_whitespace` 保持一致（NUL 计入空白）。
    pub(super) fn is_whitespace(b: u8) -> bool {
        matches!(b, 0x00 | b'\t' | b'\n' | 0x0c | b'\r' | b' ')
    }

    pub(super) fn is_delim(b: u8) -> bool {
        matches!(
            b,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    /// 跳过空白与 `% 注释`（至行尾）。
    pub(super) fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                Some(b) if Self::is_whitespace(b) => {
                    self.pos += 1;
                }
                Some(b'%') => {
                    // 注释到 \r 或 \n
                    while let Some(b) = self.peek() {
                        if b == b'\r' || b == b'\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// 读取下一个 token；到达输入末尾返回 `Err`。
    pub(super) fn next_token(&mut self) -> Result<Token> {
        self.skip_ws();
        let b = self
            .peek()
            .ok_or_else(|| err(self.pos(), "unexpected EOF"))?;
        match b {
            b'(' => {
                self.pos += 1;
                let s = self.read_literal()?;
                Ok(Token::Literal(s))
            }
            b'<' => match self.peek_at(1) {
                Some(b'<') => {
                    self.pos += 2;
                    Ok(Token::DictOpen)
                }
                // `<~`：非 ISO 32000-1 的 ASCII85 字符串扩展形态。必须在
                // 十六进制分支之前判别，否则 `<~` 会被当成 `<` 起始的
                // 十六进制串、把 `~` 当作数据字符吞进去。
                Some(b'~') => {
                    self.pos += 2;
                    let s = self.read_ascii85()?;
                    Ok(Token::Ascii85(s))
                }
                _ => {
                    self.pos += 1;
                    let s = self.read_hex()?;
                    Ok(Token::Hex(s))
                }
            },
            b'>' => match self.peek_at(1) {
                Some(b'>') => {
                    self.pos += 2;
                    Ok(Token::DictClose)
                }
                _ => Err(err(self.pos(), "stray '>'")),
            },
            b'[' => {
                self.pos += 1;
                Ok(Token::ArrayOpen)
            }
            b']' => {
                self.pos += 1;
                Ok(Token::ArrayClose)
            }
            b'/' => {
                self.pos += 1;
                let s = self.read_name();
                Ok(Token::Name(s))
            }
            // 数字：以 0-9 或 . 开头，或以 + - 后接数字/点
            b'0'..=b'9' | b'.' | b'+' | b'-' => {
                let s = self.read_number();
                if s == "+" || s == "-" || s == "." {
                    // 仅符号/小数点：当成普通关键字处理
                    Ok(Token::Keyword(s))
                } else {
                    Ok(Token::Number(s))
                }
            }
            _ => {
                // 关键字：连续非分隔符字符
                let s = self.read_keyword();
                if s.is_empty() {
                    Err(err(self.pos(), format!("unexpected byte {:#x}", b)))
                } else {
                    Ok(Token::Keyword(s))
                }
            }
        }
    }

    /// 字面字符串：处理嵌套 `()` 与 `\` 转义。
    pub(super) fn read_literal(&mut self) -> Result<String> {
        let mut out = String::new();
        let mut depth = 1i32;
        while let Some(b) = self.bump() {
            match b {
                b'(' => {
                    depth += 1;
                    out.push('(');
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                    out.push(')');
                }
                b'\\' => match self.bump() {
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'b') => out.push('\u{0008}'),
                    Some(b'f') => out.push('\u{000c}'),
                    Some(b'(') => out.push('('),
                    Some(b')') => out.push(')'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'\n') => {}
                    Some(b'\r') => {
                        if self.peek() == Some(b'\n') {
                            self.pos += 1;
                        }
                    }
                    Some(d) if d.is_ascii_digit() => {
                        // 至多 3 位八进制
                        let mut oct = String::from(d as char);
                        for _ in 0..2 {
                            match self.peek() {
                                Some(x) if x.is_ascii_digit() => {
                                    oct.push(x as char);
                                    self.pos += 1;
                                }
                                _ => break,
                            }
                        }
                        let code = u32::from_str_radix(&oct, 8).unwrap_or(0);
                        // 截到 Latin-1 范围
                        out.push((code & 0xFF) as u8 as char);
                    }
                    other => {
                        // 未知转义：保留后字符
                        if let Some(c) = other {
                            out.push(c as char);
                        }
                    }
                },
                _ => out.push(b as char),
            }
        }
        Err(err(self.pos(), "unterminated literal string"))
    }

    /// 十六进制字符串：到 `>` 为止，已去掉 `<>`。
    pub(super) fn read_hex(&mut self) -> Result<String> {
        let mut out = String::new();
        while let Some(b) = self.bump() {
            match b {
                b'>' => return Ok(out),
                b if b.is_ascii_whitespace() => continue,
                b => out.push(b as char),
            }
        }
        Err(err(self.pos(), "unterminated hex string"))
    }

    /// ASCII85 字符串（非 ISO 32000-1 扩展语法）：从 `<~` 之后读到 `~>`。
    ///
    /// 语义口径——与写端 `writer/value.rs` 的 `Value::Ascii85String` 分支
    /// **逐字对称**：写端把 payload 原样夹在 `<~` 与 `~>` 之间、不做任何
    /// 转义，故此处也**逐字**收集其间全部字符（含空白、换行与杂散 `~`），
    /// 只剥掉两个定界符。payload 是 ASCII85 **编码文本**（不是解码后的
    /// 字节）；取字节请走既有公共入口 `filters::decode_stream`，本词法层
    /// 不重复实现 base-85 解码。
    ///
    /// 结束标记鲁棒性：`~` 在 ASCII85 里本身是「数据结束」标记，故 `~>`
    /// 之前允许出现杂散 `~`（`~~>` 形态）、尾部允许混入空白/换行。只在
    /// 真正看到 `~>` 时停止：其余 `~` 一律留在 payload 内，既不 panic，
    /// 也不会吃掉后续 token。
    ///
    /// 未闭合（窗口末尾没有 `~>`）返回类型化词法错误，与
    /// `unterminated literal string` / `unterminated hex string` 同风格；
    /// 不 panic、不无限循环，也不静默把后续对象吞进串里当作成功值。
    pub(super) fn read_ascii85(&mut self) -> Result<String> {
        let mut out = String::new();
        while let Some(b) = self.bump() {
            if b == b'~' {
                if self.peek() == Some(b'>') {
                    self.pos += 1;
                    return Ok(out);
                }
                out.push('~');
                continue;
            }
            out.push(b as char);
        }
        Err(err(self.pos(), "unterminated ascii85 string"))
    }

    /// Name：到分隔符/空白为止，已去掉前导 `/`。
    /// `#xx` 转义按规范解码。
    pub(super) fn read_name(&mut self) -> String {
        let mut out = String::new();
        while let Some(b) = self.peek() {
            if Self::is_whitespace(b) || Self::is_delim(b) {
                break;
            }
            self.pos += 1;
            if b == b'#' {
                let hi = self.peek();
                let lo = self.peek_at(1);
                if let (Some(hi), Some(lo)) = (hi, lo)
                    && hi.is_ascii_hexdigit()
                    && lo.is_ascii_hexdigit()
                {
                    let s = format!("{}{}", hi as char, lo as char);
                    let code = u8::from_str_radix(&s, 16).unwrap_or(0);
                    out.push(code as char);
                    self.pos += 2;
                    continue;
                }
            }
            out.push(b as char);
        }
        out
    }

    /// 数字字符串：连续的 `0-9 . + -`。
    pub(super) fn read_number(&mut self) -> String {
        let mut out = String::new();
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || b == b'.' || b == b'+' || b == b'-' {
                out.push(b as char);
                self.pos += 1;
            } else {
                break;
            }
        }
        out
    }

    /// 关键字字符串：连续的非空白、非分隔符。
    pub(super) fn read_keyword(&mut self) -> String {
        let mut out = String::new();
        while let Some(b) = self.peek() {
            if Self::is_whitespace(b) || Self::is_delim(b) {
                break;
            }
            out.push(b as char);
            self.pos += 1;
        }
        out
    }

    /// 当前位置起的字节是否以指定前缀开头（不消耗）。
    pub(super) fn starts_with(&self, prefix: &[u8]) -> bool {
        self.input[self.pos..].starts_with(prefix)
    }

    /// 下一 token 是否为 `stream` 关键字。不消耗。
    pub(super) fn peek_is_stream_keyword(&mut self) -> bool {
        let saved = self.pos;
        self.skip_ws();
        let r = self.starts_with(b"stream");
        self.pos = saved;
        r
    }

    /// 计算流字节在文件中的**绝对偏移 + 长度**（不读字节，只算范围）。
    ///
    /// 前置条件：当前 `pos` 已指向 `stream` 关键字之后、流字节起始处
    /// （调用方已消费 `stream` 并剥掉一个 EOL）。若提供 `/Length`，按
    /// 声明长度定位并让调用方验证其后是 `endstream`；这可避免把流内容
    /// 中偶然出现的 `endstream` 字节序列当作结束标记。缺少长度时才回退
    /// 到扫描标记并剥掉之前的一个 EOL。
    ///
    /// 若窗口内找不到 `endstream`，返回 `Err`——这通常意味着对象跨过
    /// 了读取窗口边界，调用方应扩大窗口重试。
    pub(super) fn compute_stream_range(
        &mut self,
        declared_length: Option<u64>,
    ) -> Result<(u64, u64)> {
        let start = self.pos;
        if let Some(length) = declared_length {
            let length = usize::try_from(length)
                .map_err(|_| err(self.pos(), "stream /Length exceeds addressable size"))?;
            let end = start
                .checked_add(length)
                .filter(|end| *end <= self.input.len())
                .ok_or_else(|| err(self.pos(), "stream /Length exceeds available window"))?;
            self.pos = end;
            return Ok((self.base + start as u64, length as u64));
        }

        let end = find_subslice(self.input, b"endstream", start)
            .ok_or_else(|| err(self.pos(), "endstream not found in window"))?;
        let mut len = end - start;
        // 规范允许 endstream 前有 EOL，剥掉一个
        if len >= 2 && self.input[start + len - 2] == b'\r' && self.input[start + len - 1] == b'\n'
        {
            len -= 2;
        } else if len >= 1 && self.input[start + len - 1] == b'\n' {
            len -= 1;
        }
        // 只推进到 endstream 起始，不消费 endstream 本身。
        self.pos = end;
        Ok((self.base + start as u64, len as u64))
    }
}
