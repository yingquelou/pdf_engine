//! 嵌入 CMap：码空间、码→CID 映射与 `usecmap` 基座合并。

use std::collections::BTreeMap;

use super::token::{
    Token, bytes_to_code, cid_number, section_count, section_end, source_code, tokenize,
};
use super::{CodeSpace, FontResult, MAX_CMAP_BYTES, MAX_CMAP_ENTRIES, invalid_cmap};

#[derive(Debug, Clone, PartialEq, Eq)]
struct CidRange {
    length: usize,
    low: u32,
    high: u32,
    first_cid: u16,
    /// `cidrange`/`cidchar` 的 CID 随码递增；`notdefrange` 整段使用同一个 CID。
    increments: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::core::reader::font) struct CidCMap {
    code_spaces: Vec<CodeSpace>,
    ranges: Vec<CidRange>,
    writing_mode: u8,
}

impl CidCMap {
    /// Create a CidCMap from predefined CMap data.
    pub fn from_predefined(
        codespace_ranges: &[(u32, u32, usize)],
        cid_mappings: &BTreeMap<u32, u16>,
        writing_mode: u8,
    ) -> Self {
        let code_spaces = codespace_ranges
            .iter()
            .map(|&(low, high, length)| CodeSpace { length, low, high })
            .collect();

        let mut ranges = Vec::new();
        let mut sorted_codes: Vec<_> = cid_mappings.iter().collect();
        sorted_codes.sort_by_key(|entry| entry.0);

        // Code width comes from the declared codespace; codes outside every
        // codespace fall back to the smallest width that can hold them.
        let code_length = |code: u32| -> usize {
            codespace_ranges
                .iter()
                .find(|&&(low, high, _)| (low..=high).contains(&code))
                .map(|&(_, _, length)| length)
                .unwrap_or(match code {
                    0..=0xFF => 1,
                    0x100..=0xFFFF => 2,
                    0x1_0000..=0xFF_FFFF => 3,
                    _ => 4,
                })
        };

        let mut i = 0;
        while i < sorted_codes.len() {
            let start_code = *sorted_codes[i].0;
            let start_cid = *sorted_codes[i].1;
            let length = code_length(start_code);
            let mut j = i + 1;

            while j < sorted_codes.len() {
                let expected_code = start_code + (j - i) as u32;
                let expected_cid = start_cid + (j - i) as u16;
                if *sorted_codes[j].0 == expected_code
                    && *sorted_codes[j].1 == expected_cid
                    && code_length(*sorted_codes[j].0) == length
                {
                    j += 1;
                } else {
                    break;
                }
            }

            let end_code = *sorted_codes[j - 1].0;

            ranges.push(CidRange {
                length,
                low: start_code,
                high: end_code,
                first_cid: start_cid,
                increments: true,
            });

            i = j;
        }

        Self {
            code_spaces,
            ranges,
            writing_mode,
        }
    }

    /// Parse a standalone CMap; a `/usecmap` base is rejected because there is
    /// no resolver available. Production paths use [`Self::parse_inheriting`].
    #[cfg(test)]
    pub fn parse(data: &[u8]) -> FontResult<Self> {
        let parsed = parse_sections(data)?;
        if let Some(base) = &parsed.base {
            return Err(invalid_cmap(&format!(
                "usecmap base /{base} cannot be resolved without a CMap resolver"
            )));
        }
        parsed.finish()
    }

    /// Parse an embedded CMap that inherits a base CMap.
    ///
    /// The base comes either from `dict_base` — the stream dictionary's
    /// `/UseCMap` entry (Table 120: "The name of a predefined CMap, or a
    /// stream containing a CMap"), resolved by the caller — or from an
    /// in-stream `/BaseName usecmap` operator resolved through `resolve`.
    /// Rule (a) of 9.7.5.3 requires both to identify the same CMap; when both
    /// are available the stream dictionary entry is used, since it is the only
    /// form that can carry an embedded base (library policy).
    ///
    /// Mappings defined here take precedence over inherited ones; codespace
    /// ranges defined here replace the inherited codespace and clip inherited
    /// mappings to their bounds. An in-stream base that `resolve` cannot
    /// resolve is an error, as before; a missing or unresolvable `/UseCMap`
    /// base is never replaced by synthesized mappings.
    pub(in crate::core::reader::font) fn parse_with_base(
        data: &[u8],
        dict_base: Option<Self>,
        resolve: impl FnOnce(&str) -> FontResult<CidCMap>,
    ) -> FontResult<Self> {
        let parsed = parse_sections(data)?;
        let inherited = match dict_base {
            Some(base) => Some(base),
            None => match &parsed.base {
                Some(name) => Some(resolve(name)?),
                None => None,
            },
        };
        match inherited {
            Some(base) => parsed.merge_into(base),
            None => parsed.finish(),
        }
    }

    pub fn writing_mode(&self) -> u8 {
        self.writing_mode
    }

    /// 声明的源字符码空间（供 ToUnicode 预定义基座按码长/码值过滤）。
    pub(in crate::core::reader::font) fn code_spaces(&self) -> &[CodeSpace] {
        &self.code_spaces
    }

    /// Whether `code` falls inside a codespace range of the given byte length.
    pub(in crate::core::reader::font) fn codespace_contains(
        &self,
        length: usize,
        code: u32,
    ) -> bool {
        self.code_spaces
            .iter()
            .any(|space| space.contains(length, code))
    }

    pub fn decode(&self, input: &[u8]) -> FontResult<Vec<(Vec<u8>, u16)>> {
        let mut output = Vec::new();
        let mut offset = 0;
        while offset < input.len() {
            let remaining = &input[offset..];
            let mut matched = None;
            for length in (1..=4).rev() {
                let Some(code_bytes) = remaining.get(..length) else {
                    continue;
                };
                let code = bytes_to_code(code_bytes);
                if self
                    .code_spaces
                    .iter()
                    .any(|space| space.contains(length, code))
                {
                    matched = Some((length, code));
                    break;
                }
            }
            let (length, code) = matched
                .ok_or_else(|| invalid_cmap("input contains a code outside the codespace"))?;
            let cid = self
                .ranges
                .iter()
                .find(|range| range.length == length && (range.low..=range.high).contains(&code))
                .map_or(0, |range| {
                    if range.increments {
                        range.first_cid
                            + u16::try_from(code - range.low).expect("validated CID range")
                    } else {
                        range.first_cid
                    }
                });
            output.push((remaining[..length].to_vec(), cid));
            offset += length;
        }
        Ok(output)
    }
}

/// Raw sections of an embedded CMap before a possible `usecmap` merge.
struct ParsedCidCMap {
    code_spaces: Vec<CodeSpace>,
    ranges: Vec<CidRange>,
    writing_mode: Option<u8>,
    base: Option<String>,
}

impl ParsedCidCMap {
    fn finish(self) -> FontResult<CidCMap> {
        finalize_cmap(
            self.code_spaces,
            self.ranges,
            self.writing_mode.unwrap_or(0),
        )
    }

    fn merge_into(self, base: CidCMap) -> FontResult<CidCMap> {
        let writing_mode = self.writing_mode.unwrap_or(base.writing_mode);
        let (base_ranges, code_spaces) = if self.code_spaces.is_empty() {
            (base.ranges, base.code_spaces)
        } else {
            let clipped = base
                .ranges
                .iter()
                .flat_map(|range| clip_to_codespaces(range, &self.code_spaces))
                .collect();
            (clipped, self.code_spaces)
        };
        // Later mappings take precedence: cut the overridden codes out of the
        // inherited ranges, then append the overrides.
        let mut ranges = Vec::with_capacity(base_ranges.len() + self.ranges.len());
        for base_range in base_ranges {
            let mut pieces = vec![base_range];
            for range in &self.ranges {
                pieces = pieces
                    .iter()
                    .flat_map(|piece| subtract_range(piece, range))
                    .collect();
            }
            ranges.extend(pieces);
        }
        let ranges = ranges.into_iter().chain(self.ranges).collect();
        finalize_cmap(code_spaces, ranges, writing_mode)
    }
}

fn parse_sections(data: &[u8]) -> FontResult<ParsedCidCMap> {
    if data.len() > MAX_CMAP_BYTES {
        return Err(invalid_cmap("CMap stream exceeds the 16 MiB limit"));
    }
    let tokens = tokenize(data)?;
    let mut code_spaces = Vec::new();
    let mut ranges = Vec::new();
    let mut writing_mode = None;
    let mut base = None;
    let mut index = 0;

    while index < tokens.len() {
        let Token::Word(operator) = &tokens[index] else {
            index += 1;
            continue;
        };
        if operator == "usecmap" {
            if base.is_some() {
                return Err(invalid_cmap("CMap contains more than one usecmap"));
            }
            let is_name_reference =
                index >= 2 && matches!(&tokens[index - 2], Token::Word(slash) if slash == "/");
            let name = match index.checked_sub(1).and_then(|i| tokens.get(i)) {
                Some(Token::Word(name)) if is_name_reference && name != "/" => name.clone(),
                _ => return Err(invalid_cmap("usecmap has no base CMap name")),
            };
            base = Some(name);
            index += 1;
            continue;
        }
        if operator == "begincodespacerange" {
            let count = section_count(&tokens, index)?;
            let end = section_end(&tokens, index, count, 2, "endcodespacerange")?;
            for entry in tokens[index + 1..end].chunks(2) {
                let low = source_code(&entry[0])?;
                let high = source_code(&entry[1])?;
                if low.0 != high.0 || low.1 > high.1 {
                    return Err(invalid_cmap("invalid codespace range"));
                }
                code_spaces.push(CodeSpace {
                    length: low.0,
                    low: low.1,
                    high: high.1,
                });
            }
            index = end + 1;
            continue;
        }
        if operator == "begincidchar" {
            let count = section_count(&tokens, index)?;
            let end = section_end(&tokens, index, count, 2, "endcidchar")?;
            for entry in tokens[index + 1..end].chunks(2) {
                let (length, code) = source_code(&entry[0])?;
                let cid = cid_number(&entry[1])?;
                ranges.push(CidRange {
                    length,
                    low: code,
                    high: code,
                    first_cid: cid,
                    increments: true,
                });
            }
            index = end + 1;
            continue;
        }
        if operator == "begincidrange" {
            let count = section_count(&tokens, index)?;
            let end = section_end(&tokens, index, count, 3, "endcidrange")?;
            for entry in tokens[index + 1..end].chunks(3) {
                let (low_length, low) = source_code(&entry[0])?;
                let (high_length, high) = source_code(&entry[1])?;
                let first_cid = cid_number(&entry[2])?;
                if low_length != high_length || low > high {
                    return Err(invalid_cmap("invalid CID range"));
                }
                if u32::from(first_cid) + high - low > u32::from(u16::MAX) {
                    return Err(invalid_cmap("CID range exceeds 65535"));
                }
                ranges.push(CidRange {
                    length: low_length,
                    low,
                    high,
                    first_cid,
                    increments: true,
                });
            }
            index = end + 1;
            continue;
        }
        if operator == "beginnotdefrange" {
            let count = section_count(&tokens, index)?;
            let end = section_end(&tokens, index, count, 3, "endnotdefrange")?;
            for entry in tokens[index + 1..end].chunks(3) {
                let (low_length, low) = source_code(&entry[0])?;
                let (high_length, high) = source_code(&entry[1])?;
                let cid = cid_number(&entry[2])?;
                if low_length != high_length || low > high {
                    return Err(invalid_cmap("invalid notdef range"));
                }
                // `notdefrange` 把整段码映射到同一个 CID（通常为 0，Adobe 数据
                // 也有 1/7716 这类取值），不随码递增。
                ranges.push(CidRange {
                    length: low_length,
                    low,
                    high,
                    first_cid: cid,
                    increments: false,
                });
            }
            index = end + 1;
            continue;
        }
        if operator == "def"
            && index >= 2
            && matches!(&tokens[index - 2], Token::Word(name) if name == "WMode")
            && let Token::Integer(mode) = tokens[index - 1]
        {
            writing_mode = Some(
                u8::try_from(mode)
                    .ok()
                    .filter(|mode| *mode <= 1)
                    .ok_or_else(|| invalid_cmap("WMode must be 0 or 1"))?,
            );
        }
        index += 1;
    }

    Ok(ParsedCidCMap {
        code_spaces,
        ranges,
        writing_mode,
        base,
    })
}

fn finalize_cmap(
    mut code_spaces: Vec<CodeSpace>,
    mut ranges: Vec<CidRange>,
    writing_mode: u8,
) -> FontResult<CidCMap> {
    if code_spaces.is_empty() {
        return Err(invalid_cmap("CMap has no codespace range"));
    }
    if code_spaces.len() > MAX_CMAP_ENTRIES || ranges.len() > MAX_CMAP_ENTRIES {
        return Err(invalid_cmap("CMap contains too many mapping ranges"));
    }
    code_spaces.sort_by_key(|space| (space.length, space.low));
    for pair in code_spaces.windows(2) {
        if pair[0].length == pair[1].length && pair[0].high >= pair[1].low {
            return Err(invalid_cmap("codespace ranges overlap"));
        }
    }
    // 不同码长区间之间的数值前缀重叠不再视为错误：解码按声明的最长码长
    // 优先匹配，失败再逐级回退（如 GBK2K 的 4 字节 `<81308130>`–`<FE39FE39>`
    // 与 2 字节 `<8140>`–`<FEFE>`）。同长度区间之间的重叠仍在上方拒绝。
    ranges.sort_by_key(|range| (range.length, range.low));
    for pair in ranges.windows(2) {
        if pair[0].length == pair[1].length && pair[0].high >= pair[1].low {
            return Err(invalid_cmap("CID mappings overlap"));
        }
    }
    if ranges.iter().any(|range| {
        !code_spaces.iter().any(|space| {
            space.length == range.length && space.low <= range.low && range.high <= space.high
        })
    }) {
        return Err(invalid_cmap("CID mapping is outside the codespace"));
    }

    Ok(CidCMap {
        code_spaces,
        ranges,
        writing_mode,
    })
}

/// Intersect a mapping range with each same-length codespace, preserving the
/// code-to-CID offset in the clipped pieces.
fn clip_to_codespaces(range: &CidRange, code_spaces: &[CodeSpace]) -> Vec<CidRange> {
    code_spaces
        .iter()
        .filter_map(|space| {
            if space.length != range.length || range.high < space.low || space.high < range.low {
                return None;
            }
            let low = range.low.max(space.low);
            let high = range.high.min(space.high);
            Some(CidRange {
                length: range.length,
                low,
                high,
                first_cid: if range.increments {
                    range.first_cid + (low - range.low) as u16
                } else {
                    range.first_cid
                },
                increments: range.increments,
            })
        })
        .collect()
}

/// Remove `cut` from `existing`, returning the remaining pieces with their
/// code-to-CID offsets preserved.
fn subtract_range(existing: &CidRange, cut: &CidRange) -> Vec<CidRange> {
    if existing.length != cut.length || existing.high < cut.low || cut.high < existing.low {
        return vec![existing.clone()];
    }
    let mut pieces = Vec::new();
    if existing.low < cut.low {
        pieces.push(CidRange {
            length: existing.length,
            low: existing.low,
            high: cut.low - 1,
            first_cid: existing.first_cid,
            increments: existing.increments,
        });
    }
    if cut.high < existing.high {
        pieces.push(CidRange {
            length: existing.length,
            low: cut.high + 1,
            high: existing.high,
            first_cid: if existing.increments {
                existing.first_cid + (cut.high + 1 - existing.low) as u16
            } else {
                existing.first_cid
            },
            increments: existing.increments,
        });
    }
    pieces
}

pub(in crate::core::reader::font) fn identity_cmap(writing_mode: u8) -> CidCMap {
    CidCMap {
        code_spaces: vec![CodeSpace {
            length: 2,
            low: 0,
            high: u16::MAX.into(),
        }],
        ranges: vec![CidRange {
            length: 2,
            low: 0,
            high: u16::MAX.into(),
            first_cid: 0,
            increments: true,
        }],
        writing_mode,
    }
}
