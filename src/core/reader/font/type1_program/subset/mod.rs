//! Subsetting of embedded Type 1 font programs.
//!
//! [`subset_type1`] rebuilds a `/FontFile` program with only the requested
//! glyphs (plus `.notdef`). The cleartext header is copied byte for byte and
//! the Private dictionary keeps every entry except the removed `/CharStrings`
//! entries. CharString segments are encrypted individually inside the `eexec`
//! plaintext, so retained segments are copied verbatim and only the `eexec`
//! layer is encrypted again. All `/Subrs` are retained.

use std::collections::BTreeSet;
use std::ops::Range;

use super::super::encoding_data;
use super::{
    FontResult, Scanner, Token, Type1FontProgram, decrypt, eexec_section, integer, invalid_type1,
};

/// Number of `0` bytes that terminate the `eexec` section.
const ZERO_GUARD: usize = 512;

/// Bytes written after the zero guard.
const CLEARTOMARK: &[u8] = b"\ncleartomark\n";

/// Build a Type 1 font program containing `glyph_names` and `.notdef`.
///
/// The result keeps the cleartext header, the `/FontInfo` data, the complete
/// Private dictionary except for dropped `/CharStrings` entries, and every
/// `/Subrs` entry. `.notdef` is retained whenever the source contains it.
///
/// Retained `seac` composites are completed transitively: the base and accent
/// character codes are mapped through StandardEncoding and their CharStrings
/// are retained as well, so every retained composite stays interpretable.
/// Only retained glyphs are interpreted, so a dropped composite whose
/// components are missing does not affect the result.
///
/// A requested name absent from the source `/CharStrings` dictionary returns
/// `FontError::Invalid`, as do a source program with an empty `/CharStrings`
/// dictionary and input whose `eexec` section or `/CharStrings` entries cannot
/// be located safely. Subsetting also fails when a retained glyph cannot be
/// interpreted (the component closure cannot be proven), when a retained
/// `seac` character code has no StandardEncoding glyph name, and when a
/// referenced component is absent from `/CharStrings`. Duplicate names in
/// `glyph_names` are accepted.
pub fn subset_type1(data: &[u8], glyph_names: &[&str]) -> FontResult<Vec<u8>> {
    let source = Type1FontProgram::parse(data)?;
    if source.charstrings.is_empty() {
        return Err(invalid_type1("/CharStrings is empty"));
    }

    let layout = locate(data)?;
    let entries = charstring_entries(&layout.plaintext)?;
    let located: BTreeSet<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    let parsed: BTreeSet<&str> = source.charstrings.keys().map(String::as_str).collect();
    if located != parsed {
        return Err(invalid_type1(
            "/CharStrings entries could not be located safely",
        ));
    }

    let mut retained: BTreeSet<String> = BTreeSet::new();
    if source.charstrings.contains_key(".notdef") {
        retained.insert(".notdef".to_string());
    }
    for &name in glyph_names {
        if !source.charstrings.contains_key(name) {
            return Err(invalid_type1(format!(
                "requested glyph `{name}` is not in /CharStrings"
            )));
        }
        retained.insert(name.to_string());
    }
    retain_seac_components(&source, &mut retained)?;

    let spans = removal_spans(&entries, &retained, layout.plaintext.len())?;
    let plaintext = remove_spans(&layout.plaintext, &spans);

    // A four-byte prefix is skipped by the eexec decryption; zeros are a
    // deterministic stand-in for the usual random prefix.
    let mut prefixed = Vec::with_capacity(plaintext.len() + 4);
    prefixed.extend_from_slice(&[0u8; 4]);
    prefixed.extend_from_slice(&plaintext);
    let encrypted = encrypt(&prefixed, 55665);

    let mut output =
        Vec::with_capacity(layout.prefix.len() + encrypted.len() + ZERO_GUARD + CLEARTOMARK.len());
    output.extend_from_slice(layout.prefix);
    output.extend_from_slice(&encrypted);
    output.extend_from_slice(&[b'0'; ZERO_GUARD]);
    output.extend_from_slice(CLEARTOMARK);
    Ok(output)
}

/// Retain the StandardEncoding components of retained `seac` composites.
///
/// The retained set is iterated to a fixed point: each newly retained glyph is
/// interpreted, and the base and accent glyphs named by its `seac` operands are
/// added until no new names appear. Components are interpreted in turn, so
/// nested composites are closed as well. Only retained glyphs are interpreted.
fn retain_seac_components(
    source: &Type1FontProgram,
    retained: &mut BTreeSet<String>,
) -> FontResult<()> {
    let mut pending: Vec<String> = retained.iter().cloned().collect();
    while let Some(name) = pending.pop() {
        let glyph = source.glyph(&name).map_err(|error| {
            invalid_type1(format!(
                "retained glyph `{name}` cannot be interpreted to resolve `seac` components: \
                 {error}"
            ))
        })?;
        let Some(glyph) = glyph else {
            return Err(invalid_type1(format!(
                "retained glyph `{name}` is not in /CharStrings"
            )));
        };
        let Some(seac) = glyph.seac else {
            continue;
        };
        for (role, code) in [("base", seac.base_char), ("accent", seac.accent_char)] {
            let component = encoding_data::standard_glyph_name(code).ok_or_else(|| {
                invalid_type1(format!(
                    "`seac` {role} character code {code} of glyph `{name}` has no \
                     StandardEncoding glyph name"
                ))
            })?;
            if !source.charstrings.contains_key(&component) {
                return Err(invalid_type1(format!(
                    "`seac` {role} glyph `{component}` of glyph `{name}` is not in /CharStrings"
                )));
            }
            if retained.insert(component.clone()) {
                pending.push(component);
            }
        }
    }
    Ok(())
}

/// Byte ranges of the source program needed to rebuild it.
struct Layout<'a> {
    /// Cleartext bytes up to the start of the encrypted payload.
    prefix: &'a [u8],
    /// Decrypted `eexec` plaintext, including the Private dictionary.
    plaintext: Vec<u8>,
}

/// Locate the `eexec` section, requiring the 512-zero guard after it.
fn locate(data: &[u8]) -> FontResult<Layout<'_>> {
    let mut scanner = Scanner::new(data);
    loop {
        match scanner.next_token()? {
            None => return Err(invalid_type1("missing `eexec` section")),
            Some(Token::Keyword(keyword)) if keyword == "eexec" => break,
            Some(_) => {}
        }
    }
    scanner.skip_single_gap();
    let encrypted_start = scanner.position;
    let rest = data
        .get(encrypted_start..)
        .ok_or_else(|| invalid_type1("eexec section is truncated"))?;
    let encrypted = eexec_section(rest);
    if encrypted.len() == rest.len() {
        return Err(invalid_type1(
            "eexec section is not followed by the 512-zero guard",
        ));
    }
    if encrypted.len() < 4 {
        return Err(invalid_type1("eexec section is truncated"));
    }
    Ok(Layout {
        prefix: &data[..encrypted_start],
        plaintext: decrypt(encrypted, 55665, 4),
    })
}

/// One `/CharStrings` entry located in the decrypted Private dictionary.
struct CharStringEntry {
    name: String,
    /// Byte range from the glyph-name token through its terminator.
    span: Range<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Subrs,
    CharStrings,
}

enum OperandValue {
    Name(String),
    Number(f64),
}

struct Operand {
    value: OperandValue,
    start: usize,
}

enum SegmentKey {
    Name(String),
    /// A `/Subrs` segment key; its index is validated by the parser.
    Index,
}

/// Locate every `/CharStrings` entry in the decrypted Private dictionary.
///
/// This mirrors the token flow of the parser: `RD`/`-|` reads a binary
/// segment of the declared length, and the following keyword terminates the
/// entry. Segments in `/Subrs` are skipped over but never recorded.
fn charstring_entries(plaintext: &[u8]) -> FontResult<Vec<CharStringEntry>> {
    let mut scanner = Scanner::new(plaintext);
    let mut section = Section::None;
    let mut operands: Vec<Operand> = Vec::new();
    let mut entries = Vec::new();

    while let Some((token, start, _end)) = next_spanned(&mut scanner)? {
        match token {
            Token::Name(name) => {
                match name.as_str() {
                    "Subrs" => section = Section::Subrs,
                    "CharStrings" => section = Section::CharStrings,
                    _ => {}
                }
                operands.push(Operand {
                    value: OperandValue::Name(name),
                    start,
                });
            }
            Token::Number(value) => operands.push(Operand {
                value: OperandValue::Number(value),
                start,
            }),
            Token::Keyword(keyword) => {
                if keyword == "RD" || keyword == "-|" {
                    let (key, key_start, length) = segment_operands(&operands)?;
                    scanner.read_binary(length)?;
                    let end = terminator_end(&mut scanner)?;
                    if let (Section::CharStrings, SegmentKey::Name(name)) = (section, key) {
                        entries.push(CharStringEntry {
                            name,
                            span: key_start..end,
                        });
                    }
                    operands.clear();
                    continue;
                }
                if matches!(keyword.as_str(), "mark" | "cleartomark" | "closefile") {
                    break;
                }
                operands.clear();
            }
            _ => operands.clear(),
        }
    }
    Ok(entries)
}

/// Read a token together with its byte range, after leading whitespace.
fn next_spanned(scanner: &mut Scanner<'_>) -> FontResult<Option<(Token, usize, usize)>> {
    scanner.skip_ignorable();
    let start = scanner.position;
    let token = scanner.next_token()?;
    let end = scanner.position;
    Ok(token.map(|token| (token, start, end)))
}

/// The glyph name (or subroutine index) and length operands of an `RD`/`-|`.
fn segment_operands(operands: &[Operand]) -> FontResult<(SegmentKey, usize, usize)> {
    let length = operands
        .last()
        .ok_or_else(|| invalid_type1("RD segment is missing a length"))?;
    let OperandValue::Number(value) = &length.value else {
        return Err(invalid_type1("RD segment length is not a number"));
    };
    let length = usize::try_from(integer(*value, "RD segment length")?)
        .map_err(|_| invalid_type1("RD segment length is negative"))?;
    let key_index = operands
        .len()
        .checked_sub(2)
        .ok_or_else(|| invalid_type1("RD segment is missing a name or index"))?;
    let key = &operands[key_index];
    let key_start = key.start;
    let key = match &key.value {
        OperandValue::Name(name) => SegmentKey::Name(name.clone()),
        OperandValue::Number(_) => SegmentKey::Index,
    };
    Ok((key, key_start, length))
}

/// Consume and validate the token terminating a binary segment.
///
/// The byte range returned includes the terminator so that removing an entry
/// never leaves a dangling `def` or `put` behind.
fn terminator_end(scanner: &mut Scanner<'_>) -> FontResult<usize> {
    let Some((token, _, end)) = next_spanned(scanner)? else {
        return Err(invalid_type1("RD segment is missing its terminator"));
    };
    let Token::Keyword(keyword) = token else {
        return Err(invalid_type1("RD segment is missing its terminator"));
    };
    match keyword.as_str() {
        "ND" | "def" | "NP" => Ok(end),
        "noaccess" => {
            let Some((token, _, end)) = next_spanned(scanner)? else {
                return Err(invalid_type1("RD segment has an incomplete terminator"));
            };
            match token {
                Token::Keyword(suffix) if suffix == "def" || suffix == "put" => Ok(end),
                _ => Err(invalid_type1("RD segment has an unsupported terminator")),
            }
        }
        _ => Err(invalid_type1(format!(
            "RD segment terminator `{keyword}` is not supported"
        ))),
    }
}

/// Validate and sort the byte ranges of entries that must be removed.
fn removal_spans(
    entries: &[CharStringEntry],
    retained: &BTreeSet<String>,
    length: usize,
) -> FontResult<Vec<Range<usize>>> {
    let mut spans = Vec::new();
    for entry in entries {
        if retained.contains(entry.name.as_str()) {
            continue;
        }
        if entry.span.start >= entry.span.end || entry.span.end > length {
            return Err(invalid_type1(
                "located /CharStrings entry has an invalid byte range",
            ));
        }
        spans.push(entry.span.clone());
    }
    spans.sort_by_key(|span| span.start);
    if spans.windows(2).any(|pair| pair[0].end > pair[1].start) {
        return Err(invalid_type1("located /CharStrings entries overlap"));
    }
    Ok(spans)
}

/// Copy `data` while omitting validated, non-overlapping, sorted spans.
fn remove_spans(data: &[u8], spans: &[Range<usize>]) -> Vec<u8> {
    let mut output = Vec::with_capacity(data.len());
    let mut cursor = 0;
    for span in spans {
        output.extend_from_slice(&data[cursor..span.start]);
        cursor = span.end;
    }
    output.extend_from_slice(&data[cursor..]);
    output
}

/// Type 1 encryption: the high byte of the running key XORs the plaintext.
fn encrypt(plaintext: &[u8], key: u16) -> Vec<u8> {
    let mut running = key;
    let mut ciphertext = Vec::with_capacity(plaintext.len());
    for &plain in plaintext {
        let cipher = plain ^ (running >> 8) as u8;
        running = u16::from(cipher)
            .wrapping_add(running)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        ciphertext.push(cipher);
    }
    ciphertext
}

#[cfg(test)]
mod tests;
