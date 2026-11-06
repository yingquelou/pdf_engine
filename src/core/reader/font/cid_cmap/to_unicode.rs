//! ToUnicode CMap：码→Unicode 映射与 `usecmap` 基座继承。

use std::collections::BTreeMap;

use super::invalid_cmap;
use super::token::{Token, bytes_to_code, section_count, section_end, source_code, tokenize};
use super::{CodeSpace, FontResult, MAX_CMAP_BYTES, MAX_CMAP_ENTRIES, MAX_UNICODE_BYTES};

/// 内置恒等基座名：源码值即 Unicode 标量（仅 2 字节码）。
const IDENTITY_UCS_NAME: &str = "Adobe-Identity-UCS";

/// 预定义 ToUnicode 基座对源码的读端解释方式。
///
/// PDF 规范要求 `usecmap` 基座是 ToUnicode CMap（CMapType 2），但 Adobe
/// 发布的 `UniGB/UniCNS/UniJIS/UniKS` 编码 CMap 均为 CMapType 1。本库按
/// **名称编码**宽松解释：源码按对应 Unicode 编码精确解码，不借用基座里的
/// code→CID 映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::core::reader::font) enum UnicodeBase {
    /// `Adobe-Identity-UCS` 与各 ordering 的 `UCS2`（含 `UCS2-HW`）：
    /// 2 字节码值即 Unicode 标量。
    Identity,
    /// 各 ordering 的 `UTF16`：源码按 UTF-16BE 解码（2 字节或 4 字节代理对）。
    Utf16,
    /// 各 ordering 的 `UTF32`：源码按 UTF-32BE 解码（4 字节）。
    Utf32,
    /// 各 ordering 的 `UTF8`：源码按 UTF-8 解码（1–4 字节）。
    Utf8,
}

/// 预定义 Unicode 基座家族前缀（`UniJIS` 与 `UniJIS2004` 等按前缀区分）。
const UNICODE_CMAP_FAMILIES: &[&str] = &[
    "UniGB",
    "UniCNS",
    "UniJIS",
    "UniJIS2004",
    "UniJISPro",
    "UniJISX0213",
    "UniJISX02132004",
    "UniKS",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::core::reader::font) struct ToUnicodeMap {
    mappings: BTreeMap<Vec<u8>, String>,
    /// 未显式映射的源码交由该基座解码；`None` 表示无基座。
    base: Option<UnicodeBase>,
    /// 基座生效的 codespace 过滤层：每一层都必须包含源码（交集）。
    ///
    /// 第一层是预定义基座自身的 codespace（`Adobe-Identity-UCS` 除外，它
    /// 没有数据文件，按 2 字节码隐式约束）；本地声明的 codespace 非空时
    /// 追加为第二层。
    space_filters: Vec<Vec<CodeSpace>>,
}

impl ToUnicodeMap {
    /// Parse a standalone ToUnicode CMap, ignoring a declared `usecmap` base.
    #[cfg(test)]
    pub fn parse(data: &[u8]) -> FontResult<Self> {
        Ok(Self {
            mappings: parse_to_unicode_sections(data)?.mappings,
            base: None,
            space_filters: Vec::new(),
        })
    }

    /// Parse a ToUnicode CMap leniently for text extraction.
    ///
    /// The built-in `Adobe-Identity-UCS` base is resolved lazily: two-byte
    /// codes without a local mapping resolve to the Unicode scalar with the
    /// same value, clipped to the locally declared codespace ranges. Other
    /// base CMaps have no available registry and are ignored so that local
    /// mappings remain usable. Malformed streams yield `None`.
    ///
    /// 流字典 `/UseCMap` 指向的内嵌基座（Table 120）由 `parse_with_base` 传入，
    /// 本入口只看流内 `usecmap`。
    pub fn parse_lenient(data: &[u8]) -> Option<Self> {
        Self::parse_lenient_with(data, builtin_base)
    }

    /// Parse a ToUnicode CMap, resolving a `/usecmap` base through `resolve`.
    ///
    /// Mappings defined here take precedence over inherited ones. When this
    /// CMap declares codespace ranges, inherited mappings whose source code
    /// falls outside those ranges are dropped. An unresolvable base is
    /// ignored so that local mappings remain usable for extraction.
    pub fn parse_lenient_with(
        data: &[u8],
        resolve: impl FnOnce(&str) -> FontResult<ToUnicodeMap>,
    ) -> Option<Self> {
        Self::parse_with_base(data, None, resolve)
    }

    /// Parse a ToUnicode CMap that may also inherit its base from the stream
    /// dictionary's `/UseCMap` entry (Table 120: "The name of a predefined
    /// CMap, or a stream containing a CMap"), supplied as `dict_base`.
    ///
    /// Rule (a) of 9.7.5.3 requires an in-stream `usecmap` reference and
    /// `/UseCMap` to name the same CMap; when both are present the stream
    /// dictionary entry is used, because it is the only form that can carry an
    /// embedded base (library policy). Local mappings win over inherited ones,
    /// and an unresolvable base is ignored so that local mappings stay usable.
    pub(in crate::core::reader::font) fn parse_with_base(
        data: &[u8],
        dict_base: Option<ToUnicodeMap>,
        resolve: impl FnOnce(&str) -> FontResult<ToUnicodeMap>,
    ) -> Option<Self> {
        let ParsedToUnicode {
            mappings,
            code_spaces,
            base: in_stream_base,
        } = parse_to_unicode_sections(data).ok()?;
        let inherited = match dict_base {
            Some(base) => Some(base),
            None => match &in_stream_base {
                Some(name) => resolve(name).ok(),
                None => None,
            },
        };
        Some(match inherited {
            Some(base) => merge_mappings(mappings, &code_spaces, base),
            None => Self {
                mappings,
                base: None,
                space_filters: Vec::new(),
            },
        })
    }

    /// Return a mapping defined in this stream only.
    pub fn unicode_for(&self, source_code: &[u8]) -> Option<&str> {
        self.mappings.get(source_code).map(String::as_str)
    }

    /// Resolve a source code through local mappings, then the inherited base.
    pub fn resolve_unicode(&self, source_code: &[u8]) -> Option<String> {
        if let Some(text) = self.mappings.get(source_code) {
            return Some(text.clone());
        }
        let base = self.base?;
        if !self.base_allows(source_code) {
            return None;
        }
        decode_base(base, source_code)
    }

    /// 基座是否作用于该源码：先过隐式 2 字节约束（恒等基座无数据文件），
    /// 再要求每一层 codespace 过滤都命中。
    fn base_allows(&self, source_code: &[u8]) -> bool {
        if self.space_filters.is_empty() {
            return source_code.len() == 2;
        }
        let code = bytes_to_code(source_code);
        self.space_filters.iter().all(|spaces| {
            spaces
                .iter()
                .any(|space| space.contains(source_code.len(), code))
        })
    }
}

/// 按基座的编码语义精确解码源码；解码失败或产生空串时返回 `None`。
fn decode_base(base: UnicodeBase, source_code: &[u8]) -> Option<String> {
    match base {
        UnicodeBase::Identity => {
            let [high, low] = source_code else {
                return None;
            };
            char::from_u32(u32::from(u16::from_be_bytes([*high, *low]))).map(String::from)
        }
        UnicodeBase::Utf16 => {
            if !source_code.len().is_multiple_of(2) {
                return None;
            }
            let units: Vec<u16> = source_code
                .chunks(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            let text = String::from_utf16(&units).ok()?;
            (!text.is_empty()).then_some(text)
        }
        UnicodeBase::Utf32 => {
            let [a, b, c, d] = source_code else {
                return None;
            };
            char::from_u32(u32::from_be_bytes([*a, *b, *c, *d])).map(String::from)
        }
        UnicodeBase::Utf8 => {
            let text = std::str::from_utf8(source_code).ok()?;
            (!text.is_empty()).then(|| text.to_owned())
        }
    }
}

/// Resolve the built-in ToUnicode base roster.
///
/// 供 `parse_lenient` 与 `use_cmap`（Table 120 的 name 形态）共用同一份名册。
/// `Adobe-Identity-UCS` 内建；`UniGB/UniCNS/UniJIS/UniKS` 的 47 个 Unicode
/// 编码变体经 `resolve_predefined_cmap` 取真实 codespace，按名称编码解码源码。
/// 其余名称返回 `Err`，基座降级为空。
pub(super) fn builtin_base(name: &str) -> FontResult<ToUnicodeMap> {
    if name == IDENTITY_UCS_NAME {
        return Ok(ToUnicodeMap {
            mappings: BTreeMap::new(),
            base: Some(UnicodeBase::Identity),
            space_filters: Vec::new(),
        });
    }
    let Some(codec) = predefined_codec(name) else {
        return Err(invalid_cmap("unknown ToUnicode base CMap"));
    };
    let cmap = crate::core::reader::font::predefined_cmaps::resolve_predefined_cmap(name)?;
    Ok(ToUnicodeMap {
        mappings: BTreeMap::new(),
        base: Some(codec),
        space_filters: vec![cmap.code_spaces().to_vec()],
    })
}

/// 识别 `Uni{GB,CNS,JIS,JIS2004,JISPro,JISX0213,JISX02132004,KS}-
/// {UCS2,UCS2-HW,UTF8,UTF16,UTF32}-{H,V}` 对应的源码编码。
fn predefined_codec(name: &str) -> Option<UnicodeBase> {
    let rest = UNICODE_CMAP_FAMILIES
        .iter()
        .find_map(|family| name.strip_prefix(family)?.strip_prefix('-'))?;
    let (codec, orientation) = rest.rsplit_once('-')?;
    if !matches!(orientation, "H" | "V") {
        return None;
    }
    match codec {
        "UCS2" | "UCS2-HW" => Some(UnicodeBase::Identity),
        "UTF16" => Some(UnicodeBase::Utf16),
        "UTF32" => Some(UnicodeBase::Utf32),
        "UTF8" => Some(UnicodeBase::Utf8),
        _ => None,
    }
}

fn merge_mappings(
    local: BTreeMap<Vec<u8>, String>,
    code_spaces: &[CodeSpace],
    base: ToUnicodeMap,
) -> ToUnicodeMap {
    let ToUnicodeMap {
        mappings: mut merged,
        base,
        mut space_filters,
    } = base;
    if !code_spaces.is_empty() {
        merged.retain(|source, _| {
            code_spaces
                .iter()
                .any(|space| space.contains(source.len(), bytes_to_code(source)))
        });
        // 本地 codespace 作为额外过滤层，与基座 codespace 取交集。
        space_filters.push(code_spaces.to_vec());
    }
    merged.extend(local);
    ToUnicodeMap {
        mappings: merged,
        base,
        space_filters,
    }
}

/// Sections of a ToUnicode CMap before a possible `usecmap` merge.
struct ParsedToUnicode {
    mappings: BTreeMap<Vec<u8>, String>,
    code_spaces: Vec<CodeSpace>,
    base: Option<String>,
}

fn unicode_destination(token: &Token) -> FontResult<String> {
    let Token::Hex(bytes) = token else {
        return Err(invalid_cmap("ToUnicode destination is not a hex string"));
    };
    if bytes.len() > MAX_UNICODE_BYTES {
        return Err(invalid_cmap(
            "ToUnicode destination exceeds the 1024-byte limit",
        ));
    }
    utf16be_to_string(bytes)
}

fn utf16be_to_string(bytes: &[u8]) -> FontResult<String> {
    if !bytes.len().is_multiple_of(2) {
        return Err(invalid_cmap(
            "ToUnicode destination has odd-length UTF-16BE",
        ));
    }
    let units = bytes
        .chunks(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units)
        .map_err(|_| invalid_cmap("ToUnicode destination has invalid UTF-16BE"))
}

fn insert_unicode(mappings: &mut BTreeMap<Vec<u8>, String>, source: Vec<u8>, unicode: String) {
    // The spec does not forbid overlapping mappings; a later entry wins.
    mappings.insert(source, unicode);
}

fn increment_source(source: &[u8], offset: usize) -> FontResult<Vec<u8>> {
    let mut result = source.to_vec();
    let mut carry = offset;
    for byte in result.iter_mut().rev() {
        let value = usize::from(*byte) + (carry & 0xFF);
        *byte = value as u8;
        carry = (carry >> 8) + (value >> 8);
    }
    if carry != 0 {
        return Err(invalid_cmap("ToUnicode source range overflows"));
    }
    Ok(result)
}

fn increment_destination(destination: &[u8], offset: usize) -> FontResult<Vec<u8>> {
    let mut result = destination.to_vec();
    let mut carry = offset;
    for byte in result.iter_mut().rev() {
        let value = usize::from(*byte) + (carry & 0xFF);
        *byte = value as u8;
        carry = (carry >> 8) + (value >> 8);
    }
    if carry != 0 {
        return Err(invalid_cmap("ToUnicode destination range overflows"));
    }
    Ok(result)
}

fn require_end_marker(tokens: &[Token], index: usize, expected: &str) -> FontResult<()> {
    if matches!(tokens.get(index), Some(Token::Word(marker)) if marker == expected) {
        Ok(())
    } else {
        Err(invalid_cmap("ToUnicode section is truncated or malformed"))
    }
}

fn parse_to_unicode_sections(data: &[u8]) -> FontResult<ParsedToUnicode> {
    if data.len() > MAX_CMAP_BYTES {
        return Err(invalid_cmap("ToUnicode stream exceeds the 16 MiB limit"));
    }
    let tokens = tokenize(data)?;
    let mut mappings = BTreeMap::new();
    let mut code_spaces = Vec::new();
    let mut base = None;
    let mut index = 0;
    while index < tokens.len() {
        let Token::Word(operator) = &tokens[index] else {
            index += 1;
            continue;
        };
        match operator.as_str() {
            "begincodespacerange" => {
                // Codespaces only clip inherited mappings; malformed entries
                // are ignored to keep text extraction lenient.
                if let Ok(count) = section_count(&tokens, index)
                    && let Ok(end) = section_end(&tokens, index, count, 2, "endcodespacerange")
                {
                    for entry in tokens[index + 1..end].chunks(2) {
                        let (Ok(low), Ok(high)) = (source_code(&entry[0]), source_code(&entry[1]))
                        else {
                            continue;
                        };
                        if low.0 == high.0 && low.1 <= high.1 {
                            code_spaces.push(CodeSpace {
                                length: low.0,
                                low: low.1,
                                high: high.1,
                            });
                        }
                    }
                    index = end + 1;
                } else {
                    index += 1;
                }
            }
            "beginbfchar" => {
                let count = section_count(&tokens, index)?;
                let mut cursor = index + 1;
                for _ in 0..count {
                    let source = tokens
                        .get(cursor)
                        .ok_or_else(|| invalid_cmap("truncated bfchar source"))?;
                    source_code(source)?;
                    let Token::Hex(source_bytes) = source else {
                        unreachable!();
                    };
                    let destination = tokens
                        .get(cursor + 1)
                        .ok_or_else(|| invalid_cmap("truncated bfchar destination"))?;
                    let unicode = unicode_destination(destination)?;
                    insert_unicode(&mut mappings, source_bytes.clone(), unicode);
                    cursor += 2;
                }
                require_end_marker(&tokens, cursor, "endbfchar")?;
                index = cursor + 1;
            }
            "beginbfrange" => {
                let count = section_count(&tokens, index)?;
                let mut cursor = index + 1;
                for _ in 0..count {
                    let (source_length, low) = source_code(
                        tokens
                            .get(cursor)
                            .ok_or_else(|| invalid_cmap("truncated bfrange source"))?,
                    )?;
                    let (high_length, high) = source_code(
                        tokens
                            .get(cursor + 1)
                            .ok_or_else(|| invalid_cmap("truncated bfrange range"))?,
                    )?;
                    if source_length != high_length || low > high {
                        return Err(invalid_cmap("invalid ToUnicode source range"));
                    }
                    let source_token = &tokens[cursor];
                    let Token::Hex(source_bytes) = source_token else {
                        unreachable!();
                    };
                    let span = high
                        .checked_sub(low)
                        .and_then(|span| span.checked_add(1))
                        .and_then(|span| usize::try_from(span).ok())
                        .ok_or_else(|| invalid_cmap("ToUnicode range length overflows"))?;
                    if span > MAX_CMAP_ENTRIES
                        || mappings.len().saturating_add(span) > MAX_CMAP_ENTRIES
                    {
                        return Err(invalid_cmap("ToUnicode range expands to too many entries"));
                    }
                    cursor += 2;
                    match tokens.get(cursor) {
                        Some(Token::Hex(first_destination)) => {
                            unicode_destination(&tokens[cursor])?;
                            for offset in 0..span {
                                let source = increment_source(source_bytes, offset)?;
                                let destination = increment_destination(first_destination, offset)?;
                                let unicode = utf16be_to_string(&destination)?;
                                insert_unicode(&mut mappings, source, unicode);
                            }
                            cursor += 1;
                        }
                        Some(Token::Word(marker)) if marker == "[" => {
                            cursor += 1;
                            for offset in 0..span {
                                let destination = tokens.get(cursor).ok_or_else(|| {
                                    invalid_cmap("truncated bfrange destination array")
                                })?;
                                let unicode = unicode_destination(destination)?;
                                let source = increment_source(source_bytes, offset)?;
                                insert_unicode(&mut mappings, source, unicode);
                                cursor += 1;
                            }
                            if !matches!(tokens.get(cursor), Some(Token::Word(marker)) if marker == "]")
                            {
                                return Err(invalid_cmap(
                                    "bfrange destination array has an invalid length",
                                ));
                            }
                            cursor += 1;
                        }
                        _ => {
                            return Err(invalid_cmap(
                                "bfrange destination is neither a hex string nor an array",
                            ));
                        }
                    }
                }
                require_end_marker(&tokens, cursor, "endbfrange")?;
                index = cursor + 1;
            }
            "usecmap" => {
                if base.is_some() {
                    return Err(invalid_cmap(
                        "ToUnicode CMap contains more than one usecmap",
                    ));
                }
                match (
                    index.checked_sub(2).and_then(|i| tokens.get(i)),
                    tokens.get(index.wrapping_sub(1)),
                ) {
                    (Some(Token::Word(slash)), Some(Token::Word(name)))
                        if slash == "/" && name != "/" =>
                    {
                        base = Some(name.clone());
                    }
                    _ => return Err(invalid_cmap("usecmap has no base CMap name")),
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    Ok(ParsedToUnicode {
        mappings,
        code_spaces,
        base,
    })
}
