use std::collections::BTreeMap;

use super::{FontError, FontResult};

const MAX_CMAP_BYTES: usize = 16 * 1024 * 1024;
const MAX_CMAP_ENTRIES: usize = 1_000_000;
const MAX_UNICODE_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeSpace {
    length: usize,
    low: u32,
    high: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CidRange {
    length: usize,
    low: u32,
    high: u32,
    first_cid: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CidCMap {
    code_spaces: Vec<CodeSpace>,
    ranges: Vec<CidRange>,
    writing_mode: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ToUnicodeMap {
    mappings: BTreeMap<Vec<u8>, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Integer(i64),
    Hex(Vec<u8>),
    Word(String),
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
            return Err(FontError::Invalid(format!(
                "invalid CID CMap: usecmap base /{base} cannot be resolved without a CMap resolver"
            )));
        }
        parsed.finish()
    }

    /// Parse an embedded CMap that may inherit a base CMap through
    /// `/BaseName usecmap`. Mappings defined here take precedence over
    /// inherited ones; codespace ranges defined here replace the inherited
    /// codespace and clip inherited mappings to their bounds.
    pub fn parse_inheriting(
        data: &[u8],
        resolve: impl FnOnce(&str) -> FontResult<CidCMap>,
    ) -> FontResult<Self> {
        let parsed = parse_sections(data)?;
        match parsed.base.clone() {
            Some(name) => parsed.merge_into(resolve(&name)?),
            None => parsed.finish(),
        }
    }

    pub fn writing_mode(&self) -> u8 {
        self.writing_mode
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
                    .any(|space| space.length == length && (space.low..=space.high).contains(&code))
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
                    range.first_cid + u16::try_from(code - range.low).expect("validated CID range")
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
    for (index, shorter) in code_spaces.iter().enumerate() {
        for longer in code_spaces.iter().skip(index + 1) {
            if shorter.length >= longer.length {
                continue;
            }
            let shift = (longer.length - shorter.length) * 8;
            let prefix_low = longer.low >> shift;
            let prefix_high = longer.high >> shift;
            if prefix_low <= shorter.high && prefix_high >= shorter.low {
                return Err(invalid_cmap("codespace ranges have ambiguous prefixes"));
            }
        }
    }
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
                first_cid: range.first_cid + (low - range.low) as u16,
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
        });
    }
    if cut.high < existing.high {
        pieces.push(CidRange {
            length: existing.length,
            low: cut.high + 1,
            high: existing.high,
            first_cid: existing.first_cid + (cut.high + 1 - existing.low) as u16,
        });
    }
    pieces
}

impl ToUnicodeMap {
    /// Parse a standalone ToUnicode CMap.
    #[cfg(test)]
    pub fn parse(data: &[u8]) -> FontResult<Self> {
        Ok(Self {
            mappings: parse_to_unicode_sections(data)?,
        })
    }

    /// Parse a ToUnicode CMap leniently for text extraction.
    ///
    /// Unsupported constructs such as a `/usecmap` base that cannot be
    /// resolved are ignored: mappings defined in this stream are still used.
    /// Returns `None` when the stream itself is malformed.
    pub fn parse_lenient(data: &[u8]) -> Option<Self> {
        Some(Self {
            mappings: parse_to_unicode_sections(data).ok()?,
        })
    }

    pub fn unicode_for(&self, source_code: &[u8]) -> Option<&str> {
        self.mappings.get(source_code).map(String::as_str)
    }
}

pub(super) fn identity_cmap(writing_mode: u8) -> CidCMap {
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
        }],
        writing_mode,
    }
}

fn require_end_marker(tokens: &[Token], index: usize, expected: &str) -> FontResult<()> {
    if matches!(tokens.get(index), Some(Token::Word(marker)) if marker == expected) {
        Ok(())
    } else {
        Err(invalid_cmap("ToUnicode section is truncated or malformed"))
    }
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

fn section_count(tokens: &[Token], operator_index: usize) -> FontResult<usize> {
    let Some(Token::Integer(count)) = operator_index
        .checked_sub(1)
        .and_then(|index| tokens.get(index))
    else {
        return Err(invalid_cmap("CMap section has no entry count"));
    };
    let count =
        usize::try_from(*count).map_err(|_| invalid_cmap("negative section entry count"))?;
    if count > MAX_CMAP_ENTRIES {
        return Err(invalid_cmap("CMap section contains too many entries"));
    }
    Ok(count)
}

fn section_end(
    tokens: &[Token],
    start: usize,
    count: usize,
    width: usize,
    end_marker: &str,
) -> FontResult<usize> {
    let entries_length = count
        .checked_mul(width)
        .and_then(|length| start.checked_add(1 + length))
        .ok_or_else(|| invalid_cmap("CMap section length overflows"))?;
    if !matches!(tokens.get(entries_length), Some(Token::Word(marker)) if marker == end_marker) {
        return Err(invalid_cmap("CMap section is truncated or malformed"));
    }
    Ok(entries_length)
}

fn source_code(token: &Token) -> FontResult<(usize, u32)> {
    let Token::Hex(bytes) = token else {
        return Err(invalid_cmap("CMap source code is not a hex string"));
    };
    if bytes.is_empty() || bytes.len() > 4 {
        return Err(invalid_cmap("CMap source code must contain 1 to 4 bytes"));
    }
    Ok((bytes.len(), bytes_to_code(bytes)))
}

fn cid_number(token: &Token) -> FontResult<u16> {
    match token {
        Token::Integer(cid) => {
            u16::try_from(*cid).map_err(|_| invalid_cmap("CID is outside 0..=65535"))
        }
        _ => Err(invalid_cmap("CID mapping target is not an integer")),
    }
}

fn bytes_to_code(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
}

fn tokenize(data: &[u8]) -> FontResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < data.len() {
        match data[index] {
            byte if is_whitespace(byte) => index += 1,
            b'%' => {
                while index < data.len() && data[index] != b'\n' && data[index] != b'\r' {
                    index += 1;
                }
            }
            b'<' if data.get(index + 1) == Some(&b'<') => {
                tokens.push(Token::Word("<<".into()));
                index += 2;
            }
            b'>' if data.get(index + 1) == Some(&b'>') => {
                tokens.push(Token::Word(">>".into()));
                index += 2;
            }
            b'<' => {
                index += 1;
                let mut digits = Vec::new();
                loop {
                    let Some(&byte) = data.get(index) else {
                        return Err(invalid_cmap("unterminated hex string"));
                    };
                    index += 1;
                    if byte == b'>' {
                        break;
                    }
                    if is_whitespace(byte) {
                        continue;
                    }
                    if !byte.is_ascii_hexdigit() {
                        return Err(invalid_cmap("invalid hex string"));
                    }
                    digits.push(byte);
                    if digits.len() > MAX_UNICODE_BYTES * 2 {
                        return Err(invalid_cmap("hex string exceeds the supported limit"));
                    }
                }
                if digits.len() % 2 != 0 {
                    digits.push(b'0');
                }
                let bytes = digits
                    .chunks(2)
                    .map(|pair| {
                        let high = hex_value(pair[0]);
                        let low = hex_value(pair[1]);
                        (high << 4) | low
                    })
                    .collect();
                tokens.push(Token::Hex(bytes));
            }
            b'(' => {
                index += 1;
                let mut depth = 1usize;
                while index < data.len() && depth > 0 {
                    match data[index] {
                        b'\\' => index = (index + 2).min(data.len()),
                        b'(' => {
                            depth += 1;
                            index += 1;
                        }
                        b')' => {
                            depth -= 1;
                            index += 1;
                        }
                        _ => index += 1,
                    }
                }
                if depth != 0 {
                    return Err(invalid_cmap("unterminated literal string"));
                }
            }
            byte if is_delimiter(byte) => {
                tokens.push(Token::Word(char::from(byte).to_string()));
                index += 1;
            }
            _ => {
                let start = index;
                while index < data.len()
                    && !is_whitespace(data[index])
                    && !is_delimiter(data[index])
                {
                    index += 1;
                }
                let word = std::str::from_utf8(&data[start..index])
                    .map_err(|_| invalid_cmap("CMap token is not ASCII"))?;
                if let Ok(integer) = word.parse::<i64>() {
                    tokens.push(Token::Integer(integer));
                } else {
                    tokens.push(Token::Word(word.into()));
                }
            }
        }
        if tokens.len() > MAX_CMAP_ENTRIES * 4 {
            return Err(invalid_cmap("CMap contains too many tokens"));
        }
    }
    Ok(tokens)
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => unreachable!(),
    }
}

fn parse_to_unicode_sections(data: &[u8]) -> FontResult<BTreeMap<Vec<u8>, String>> {
    if data.len() > MAX_CMAP_BYTES {
        return Err(invalid_cmap("ToUnicode stream exceeds the 16 MiB limit"));
    }
    let tokens = tokenize(data)?;
    let mut mappings = BTreeMap::new();
    let mut saw_base = false;
    let mut index = 0;
    while index < tokens.len() {
        let Token::Word(operator) = &tokens[index] else {
            index += 1;
            continue;
        };
        match operator.as_str() {
            "beginbfchar" => {
                let count = section_count(&tokens, index)?;
                if count > MAX_CMAP_ENTRIES {
                    return Err(invalid_cmap("ToUnicode section contains too many entries"));
                }
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
                if count > MAX_CMAP_ENTRIES {
                    return Err(invalid_cmap("ToUnicode section contains too many entries"));
                }
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
                if saw_base {
                    return Err(invalid_cmap(
                        "ToUnicode CMap contains more than one usecmap",
                    ));
                }
                // Only the syntax is validated; the base CMap cannot be
                // resolved from the font dictionary, so its mappings are not
                // inherited during text extraction.
                match (
                    index.checked_sub(2).and_then(|i| tokens.get(i)),
                    tokens.get(index.wrapping_sub(1)),
                ) {
                    (Some(Token::Word(slash)), Some(Token::Word(name)))
                        if slash == "/" && name != "/" => {}
                    _ => return Err(invalid_cmap("usecmap has no base CMap name")),
                }
                saw_base = true;
                index += 1;
            }
            _ => index += 1,
        }
    }
    Ok(mappings)
}

fn invalid_cmap(message: &str) -> FontError {
    FontError::Invalid(format!("invalid CID CMap: {message}"))
}

#[cfg(test)]
mod tests {
    use super::{CidCMap, ToUnicodeMap, identity_cmap, invalid_cmap};

    #[test]
    fn parses_codespaces_cid_chars_ranges_and_wmode() {
        let source = b"
            /WMode 1 def
            2 begincodespacerange
              <00> <7F>
              <8100> <81FF>
            endcodespacerange
            2 begincidchar
              <41> 5
              <8101> 99
            endcidchar
            1 begincidrange
              <50> <52> 20
            endcidrange";
        let cmap = CidCMap::parse(source).unwrap();
        assert_eq!(cmap.writing_mode(), 1);
        assert_eq!(
            cmap.decode(&[0x41, 0x50, 0x51, 0x81, 0x01]).unwrap(),
            [
                (vec![0x41], 5),
                (vec![0x50], 20),
                (vec![0x51], 21),
                (vec![0x81, 1], 99)
            ]
        );
        assert_eq!(cmap.decode(&[0x42]).unwrap(), [(vec![0x42], 0)]);
    }

    #[test]
    fn identity_cmaps_decode_two_byte_codes_in_both_modes() {
        for mode in 0..=1 {
            let cmap = identity_cmap(mode);
            assert_eq!(cmap.writing_mode(), mode);
            assert_eq!(
                cmap.decode(&[0x00, 0x41, 0x20, 0xAC]).unwrap(),
                [(vec![0, 0x41], 0x41), (vec![0x20, 0xAC], 0x20AC)]
            );
        }
    }

    #[test]
    fn rejects_invalid_cmap_sections_ranges_and_inputs() {
        assert!(CidCMap::parse(b"").is_err());
        assert!(
            CidCMap::parse(b"1 begincodespacerange <00> <FF> endcodespacerange usecmap").is_err()
        );
        assert!(
            CidCMap::parse(b"/WMode 2 def 1 begincodespacerange <00> <FF> endcodespacerange")
                .is_err()
        );
        assert!(
            CidCMap::parse(b"2 begincodespacerange <00> <FF> <0000> <FFFF> endcodespacerange")
                .is_err()
        );
        assert!(
            CidCMap::parse(
                b"1 begincodespacerange <00> <FF> endcodespacerange 1 begincidrange <01> <02> 65535 endcidrange"
            )
            .is_err()
        );
        let cmap = CidCMap::parse(b"1 begincodespacerange <00> <FF> endcodespacerange").unwrap();
        assert!(cmap.decode(&[0, 1, 2]).is_ok());
        let one_byte = CidCMap::parse(
            b"1 begincodespacerange <00> <FF> endcodespacerange 1 begincidchar <00> 2 endcidchar",
        )
        .unwrap();
        assert_eq!(
            one_byte.decode(&[0x01, 0x00]).unwrap(),
            [(vec![0x01], 0), (vec![0x00], 2)]
        );
    }

    #[test]
    fn usecmap_inherits_identity_base_and_overrides_mappings() {
        let source = b"
            /Identity-H usecmap
            1 begincodespacerange <0000> <FFFF> endcodespacerange
            1 begincidrange <0041> <0042> 100 endcidrange";
        let cmap = CidCMap::parse_inheriting(source, |name| match name {
            "Identity-H" => Ok(identity_cmap(0)),
            other => panic!("unexpected base {other}"),
        })
        .unwrap();
        assert_eq!(
            cmap.decode(&[0, 0x41, 0, 0x43]).unwrap(),
            [(vec![0, 0x41], 100), (vec![0, 0x43], 0x43)]
        );
    }

    #[test]
    fn usecmap_inherits_writing_mode_and_rejects_unknown_bases() {
        let source = b"/Identity-V usecmap 1 begincodespacerange <0000> <FFFF> endcodespacerange";
        let cmap = CidCMap::parse_inheriting(source, |_| Ok(identity_cmap(1))).unwrap();
        assert_eq!(cmap.writing_mode(), 1);
        assert!(CidCMap::parse_inheriting(source, |_| Err(invalid_cmap("unknown base"))).is_err());
        assert!(CidCMap::parse(source).is_err());
        assert!(
            CidCMap::parse_inheriting(
                b"usecmap 1 begincodespacerange <00> <FF> endcodespacerange",
                |_| Ok(identity_cmap(0)),
            )
            .is_err()
        );
    }

    #[test]
    fn usecmap_clips_inherited_mappings_to_defined_codespace() {
        let source = b"/Identity-H usecmap 1 begincodespacerange <00> <FF> endcodespacerange";
        let cmap = CidCMap::parse_inheriting(source, |_| Ok(identity_cmap(0))).unwrap();
        assert_eq!(cmap.decode(&[0x41]).unwrap(), [(vec![0x41], 0)]);
        assert_eq!(cmap.decode(&[0x00, 0x41]).unwrap().len(), 2);
    }

    #[test]
    fn parses_tounicode_bfchar_and_both_bfrange_forms() {
        let source = b"
            2 beginbfchar
              <01> <0041>
              <02> <D83DDE00>
            endbfchar
            2 beginbfrange
              <10> <12> <0061>
              <20> <21> [<4E2D> <6587>]
            endbfrange";
        let map = ToUnicodeMap::parse(source).unwrap();
        assert_eq!(map.unicode_for(&[0x01]), Some("A"));
        assert_eq!(map.unicode_for(&[0x02]), Some("😀"));
        assert_eq!(map.unicode_for(&[0x10]), Some("a"));
        assert_eq!(map.unicode_for(&[0x11]), Some("b"));
        assert_eq!(map.unicode_for(&[0x12]), Some("c"));
        assert_eq!(map.unicode_for(&[0x20]), Some("中"));
        assert_eq!(map.unicode_for(&[0x21]), Some("文"));
        assert_eq!(map.unicode_for(&[0x22]), None);
    }

    #[test]
    fn rejects_malformed_tounicode_mappings() {
        assert!(ToUnicodeMap::parse(b"1 beginbfchar <01> <D800> endbfchar").is_err());
        assert!(ToUnicodeMap::parse(b"1 beginbfchar <01> <0041> <02> <0042> endbfchar").is_err());
        assert!(ToUnicodeMap::parse(b"1 beginbfrange <FE> <FF> <FFFF> endbfrange").is_err());
        assert!(ToUnicodeMap::parse(b"1 beginbfrange <01> <02> [<0041>] endbfrange").is_err());
        // usecmap 的基名必须写成带斜杠的名称对象。
        assert!(ToUnicodeMap::parse(b"Base usecmap 1 beginbfchar <01> <0041> endbfchar").is_err());
    }

    #[test]
    fn tounicode_duplicate_source_uses_last_mapping() {
        let map = ToUnicodeMap::parse(b"2 beginbfchar <01> <0041> <01> <0042> endbfchar").unwrap();
        assert_eq!(map.unicode_for(&[0x01]), Some("B"));
    }

    #[test]
    fn tounicode_usecmap_keeps_local_mappings_without_inheriting_base() {
        let source = b"/Base usecmap 1 beginbfchar <02> <0058> endbfchar";
        let map = ToUnicodeMap::parse(source).expect("本地映射应可用");
        assert_eq!(map.unicode_for(&[0x02]), Some("X"));
    }

    #[test]
    fn tounicode_lenient_parse_keeps_local_mappings_when_base_is_unknown() {
        let source = b"/Unknown usecmap 1 beginbfchar <01> <0041> endbfchar";
        let map = ToUnicodeMap::parse_lenient(source).expect("本地映射应可用");
        assert_eq!(map.unicode_for(&[0x01]), Some("A"));
    }
}
