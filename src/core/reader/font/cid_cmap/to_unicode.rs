//! ToUnicode CMap：码→Unicode 映射与 `usecmap` 基座继承。

use std::collections::BTreeMap;

use super::invalid_cmap;
use super::token::{Token, bytes_to_code, section_count, section_end, source_code, tokenize};
use super::{CodeSpace, FontResult, MAX_CMAP_BYTES, MAX_CMAP_ENTRIES, MAX_UNICODE_BYTES};

/// 内置恒等基座名：源码值即 Unicode 标量（仅 2 字节码）。
const IDENTITY_UCS_NAME: &str = "Adobe-Identity-UCS";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::core::reader::font) struct ToUnicodeMap {
    mappings: BTreeMap<Vec<u8>, String>,
    /// 基座为 `Adobe-Identity-UCS` 时，未显式映射的 2 字节码按码值解析。
    identity_base: bool,
    /// 本地声明的 codespace；非空时恒等基座只在这些区间内生效。
    identity_spaces: Vec<CodeSpace>,
}

impl ToUnicodeMap {
    /// Parse a standalone ToUnicode CMap, ignoring a declared `usecmap` base.
    #[cfg(test)]
    pub fn parse(data: &[u8]) -> FontResult<Self> {
        Ok(Self {
            mappings: parse_to_unicode_sections(data)?.mappings,
            identity_base: false,
            identity_spaces: Vec::new(),
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
                identity_base: false,
                identity_spaces: Vec::new(),
            },
        })
    }

    /// Return a mapping defined in this stream only.
    pub fn unicode_for(&self, source_code: &[u8]) -> Option<&str> {
        self.mappings.get(source_code).map(String::as_str)
    }

    /// Resolve a source code through local mappings, then the inherited
    /// identity base.
    pub fn resolve_unicode(&self, source_code: &[u8]) -> Option<String> {
        if let Some(text) = self.mappings.get(source_code) {
            return Some(text.clone());
        }
        self.identity_unicode(source_code)
    }

    fn identity_unicode(&self, source_code: &[u8]) -> Option<String> {
        if !self.identity_base {
            return None;
        }
        let [high, low] = source_code else {
            return None;
        };
        let value = u16::from_be_bytes([*high, *low]);
        if !self.identity_spaces.is_empty()
            && !self
                .identity_spaces
                .iter()
                .any(|space| space.contains(2, u32::from(value)))
        {
            return None;
        }
        char::from_u32(u32::from(value)).map(String::from)
    }
}

/// Resolve the built-in ToUnicode base roster.
///
/// 供 `parse_lenient` 与 `use_cmap`（Table 120 的 name 形态）共用同一份名册。
pub(super) fn builtin_base(name: &str) -> FontResult<ToUnicodeMap> {
    if name == IDENTITY_UCS_NAME {
        Ok(ToUnicodeMap {
            mappings: BTreeMap::new(),
            identity_base: true,
            identity_spaces: Vec::new(),
        })
    } else {
        Err(invalid_cmap("unknown ToUnicode base CMap"))
    }
}

fn merge_mappings(
    local: BTreeMap<Vec<u8>, String>,
    code_spaces: &[CodeSpace],
    base: ToUnicodeMap,
) -> ToUnicodeMap {
    let ToUnicodeMap {
        mappings: mut merged,
        identity_base,
        ..
    } = base;
    if !code_spaces.is_empty() {
        merged.retain(|source, _| {
            code_spaces
                .iter()
                .any(|space| space.contains(source.len(), bytes_to_code(source)))
        });
    }
    merged.extend(local);
    ToUnicodeMap {
        mappings: merged,
        identity_base,
        // 恒等基座没有可预先裁剪的映射表，改为按本地 codespace 过滤查询。
        identity_spaces: if identity_base {
            code_spaces.to_vec()
        } else {
            Vec::new()
        },
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
