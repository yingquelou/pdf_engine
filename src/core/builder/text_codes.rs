//! `PdfBuilder` 的 CID 文本方法：Identity CID、显式字节字符码（1–4 字节）
//! 与预定义 CMap Unicode 文本。

use super::{CidFont, PdfBuilder};
use crate::core::reader::font::predefined_cmaps::resolve_predefined_cmap;

impl PdfBuilder {
    /// 在指定页上用 CIDFont 按显式字节字符码写一段文字。
    ///
    /// `codes` 中每一项是一个完整的源字符码（大端字节序列），长度须为
    /// 1..=4，与 [`CidEncoding::mapping_bytes`] 登记的码长对应；长度非法
    /// 的条目不会写入内容流，并使 [`PdfBuilder::build`] 返回
    /// [`WriteError::InvalidObject`]（零字节输出），以暴露调用错误。
    /// CIDFont 未配置编码时等价于 `/Identity-H`（2 字节码即 CID）。字符码
    /// 可不在编码映射中，此时读端按 CMap 语义得到 CID 0。
    ///
    /// [`CidEncoding::mapping_bytes`]: crate::core::builder::CidEncoding::mapping_bytes
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    /// [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject
    pub fn text_code_bytes(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        codes: &[&[u8]],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get(cid_font_idx) {
            Some(f) => f.size,
            None => return self,
        };
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::new();
        for code in codes {
            if (1..=4).contains(&code.len()) {
                bytes.extend_from_slice(code);
            } else if p.invalid_code_length.is_none() {
                p.invalid_code_length = Some(code.len());
            }
        }
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上按显式字节字符码写文字，并登记字符码 → Unicode 映射。
    ///
    /// `glyphs` 为 `(字符码字节序列, Unicode 文本)` 序列：字符码原样写入
    /// 内容流，同时以**完整字节序列**为键登记到该 CIDFont 的
    /// `/ToUnicode` CMap（键长度与字符码一致）。长度非法的条目不会写入
    /// 内容流或 `/ToUnicode`，并使 [`PdfBuilder::build`] 返回
    /// [`WriteError::InvalidObject`]（零字节输出）；同一字符码以最后一次
    /// 登记为准。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    /// [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject
    pub fn text_code_bytes_unicode(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        glyphs: &[(&[u8], &str)],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let mut invalid_code_length = None;
        let size = match p.cid_fonts.get_mut(cid_font_idx) {
            Some(f) => {
                for &(code, text) in glyphs {
                    if (1..=4).contains(&code.len()) {
                        f.to_unicode_bytes.insert(code.to_vec(), text.to_owned());
                    } else if invalid_code_length.is_none() {
                        invalid_code_length = Some(code.len());
                    }
                }
                f.size
            }
            None => return self,
        };
        p.invalid_code_length = p.invalid_code_length.or(invalid_code_length);
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::new();
        for &(code, _) in glyphs {
            if (1..=4).contains(&code.len()) {
                bytes.extend_from_slice(code);
            }
        }
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }
}

/// 把 16-bit 字符码（或 Identity 语义下的 CID）按 2 字节大端序
/// 追加到内容流字节缓冲。
fn push_two_byte_codes(bytes: &mut Vec<u8>, codes: impl IntoIterator<Item = u16>) {
    for code in codes {
        bytes.push((code >> 8) as u8);
        bytes.push((code & 0xFF) as u8);
    }
}

impl PdfBuilder {
    /// 在指定页上注册一个 CIDFont 并返回该页内的字体索引（0-based）。
    ///
    /// 索引供 [`PdfBuilder::text_cid`] 与 [`PdfBuilder::text_cid_unicode`]
    /// 使用。同一 CIDFont（整体结构相等判定）在同一页内去重。
    pub fn cid_font(&mut self, page: usize, cid_font: &CidFont) -> usize {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return 0,
        };
        match p.cid_fonts.iter().position(|f| f == cid_font) {
            Some(i) => i,
            None => {
                p.cid_fonts.push(cid_font.clone());
                p.cid_fonts.len() - 1
            }
        }
    }

    /// 在指定页上用 CIDFont 写一段文字（Identity 语义）。
    ///
    /// `cids` 为 CID 序列（16-bit），直接按 `/Identity-H` 语义（字符码
    /// 即 CID）写成 2 字节大端序十六进制字符串。若字体配置了自定义
    /// [`CidEncoding`](crate::core::builder::CidEncoding)，请改用 [`PdfBuilder::text_codes`] 传入源字符码，
    /// 否则内容流中的字节会被嵌入 CMap 当作字符码重新映射。
    /// 坐标原点在页面左下角，单位 pt。
    pub fn text_cid(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        cids: &[u16],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get(cid_font_idx) {
            Some(f) => f.size,
            None => return self,
        };
        // 简单字体用 /F1..，CIDFont 用 /CF1.. 避免命名冲突。
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::with_capacity(cids.len() * 2);
        push_two_byte_codes(&mut bytes, cids.iter().copied());
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上用 CIDFont 写一段文字，并登记 CID → Unicode 映射
    /// （Identity 语义）。
    ///
    /// `glyphs` 为 `(CID, Unicode 文本)` 序列：CID 按 Identity-H 写成
    /// 2 字节大端序十六进制串，同时写入该 CIDFont 的 `/ToUnicode` CMap
    /// （`bfchar`/`bfrange` 条目，目标为 UTF-16BE）。同一 CID 的映射以
    /// 最后一次登记为准；CIDFont 未注册时忽略本次调用。配置自定义
    /// [`CidEncoding`](crate::core::builder::CidEncoding) 时应改用 [`PdfBuilder::text_codes_unicode`]。
    pub fn text_cid_unicode(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        glyphs: &[(u16, &str)],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get_mut(cid_font_idx) {
            Some(f) => {
                for &(cid, text) in glyphs {
                    f.to_unicode.insert(cid, text.to_owned());
                }
                f.size
            }
            None => return self,
        };
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::with_capacity(glyphs.len() * 2);
        push_two_byte_codes(&mut bytes, glyphs.iter().map(|&(cid, _)| cid));
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上用 CIDFont 按 2 字节字符码写一段文字。
    ///
    /// `codes` 是内容流中的**源字符码**，逐个按 2 字节大端序写成十六进制
    /// 字符串。CIDFont 配置了 [`CidEncoding`](crate::core::builder::CidEncoding) 时由嵌入 CMap 把字符码映射
    /// 为 CID；未配置时等价于 `/Identity-H`（字符码即 CID）。字符码可以不
    /// 在编码映射中，此时读端按 CMap 语义得到 CID 0。
    pub fn text_codes(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        codes: &[u16],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get(cid_font_idx) {
            Some(f) => f.size,
            None => return self,
        };
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::with_capacity(codes.len() * 2);
        push_two_byte_codes(&mut bytes, codes.iter().copied());
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上用 CIDFont 按 2 字节字符码写文字，并登记
    /// 字符码 → Unicode 映射。
    ///
    /// `glyphs` 为 `(字符码, Unicode 文本)` 序列：字符码按 2 字节大端序
    /// 写入内容流，同时以**字符码**为键登记到该 CIDFont 的 `/ToUnicode`
    /// CMap（读端按源字符码查询，即使嵌入 CMap 把它映射到不同 CID）。
    /// 同一字符码的映射以最后一次登记为准；CIDFont 未注册时忽略本次调用。
    pub fn text_codes_unicode(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        glyphs: &[(u16, &str)],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get_mut(cid_font_idx) {
            Some(f) => {
                for &(code, text) in glyphs {
                    f.to_unicode.insert(code, text.to_owned());
                }
                f.size
            }
            None => return self,
        };
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::with_capacity(glyphs.len() * 2);
        push_two_byte_codes(&mut bytes, glyphs.iter().map(|&(code, _)| code));
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上用配置了预定义 CMap 的 CIDFont 写一段 Unicode 文本。
    ///
    /// 每个字符取其 Unicode 码位作为字符码，按 2 字节大端序写入内容流，
    /// 并以字符码为键登记到该 CIDFont 的 `/ToUnicode` CMap，保持
    /// 「写入→解析→提取」可还原。字符码必须是目标 CMap **2 字节码空间**
    /// 内的值（如 `UniGB-UCS2-H` 覆盖全部 BMP；`Adobe-GB1` 等混合码空间
    /// CMap 只接受其 2 字节区段内的码位）。
    ///
    /// 取舍说明（本 API 返回 `&mut Self`，无法逐字报错，参照现有
    /// 「无法表示」约定跳过而非替换）：
    /// - 补充平面字符（码位 > U+FFFF）无法用 2 字节码表示，跳过；
    /// - 码位不在目标 CMap 2 字节码空间内时，写出后读端会把字节拆成
    ///   较短码或拒绝解码，ToUnicode 无法对应，故同样跳过；
    ///   被跳过的字符既不写内容流也不登记 ToUnicode。
    ///
    /// CIDFont 未注册、未配置 [`CidFont::predefined_encoding`] 或名称不在
    /// 读端名册内时忽略本次调用（未知名称仍会在 [`PdfBuilder::build`]
    /// 阶段报错）。
    pub fn text_predefined_unicode(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        text: &str,
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let font = match p.cid_fonts.get_mut(cid_font_idx) {
            Some(f) => f,
            None => return self,
        };
        let Some(name) = font.predefined_encoding.as_deref() else {
            return self;
        };
        // 名称合法性由 build 统一校验；此处解析失败直接忽略本次文本。
        let Ok(cmap) = resolve_predefined_cmap(name) else {
            return self;
        };
        let size = font.size;
        let mut bytes = Vec::with_capacity(text.len() * 2);
        for ch in text.chars() {
            let code = u32::from(ch);
            // 补充平面无法表示为 2 字节码，跳过。
            if code > u32::from(u16::MAX) {
                continue;
            }
            // 仅接受落在 2 字节码空间内的码位，保证读端按 2 字节解码。
            let in_codespace = cmap.codespace_contains(2, code);
            if !in_codespace {
                continue;
            }
            let code = code as u16;
            bytes.push((code >> 8) as u8);
            bytes.push((code & 0xFF) as u8);
            font.to_unicode.insert(code, ch.to_string());
        }
        let font_name = format!("CF{}", cid_font_idx + 1);
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }
}
