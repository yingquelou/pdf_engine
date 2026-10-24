use std::collections::BTreeMap;

use crate::core::reader::font::FontError;
use super::super::cmap::CmapFormat;
use super::invalid_subset;
use super::source::SourceFont;

impl SourceFont<'_> {
    pub(super) fn build_cmap(&self, mapping: &BTreeMap<u16, u16>) -> Result<Vec<u8>, FontError> {
        let cmap_data = self.source_table(b"cmap");
        let mut entries = Vec::new();
        for code in 0..=u32::from(u16::MAX) {
            let Some(glyph_id) = self.cmap.glyph_id(cmap_data, code, self.glyph_count)? else {
                continue;
            };
            // Missing characters and the reserved 0xFFFF sentinel are omitted.
            if glyph_id == 0 || code == 0xFFFF {
                continue;
            }
            if let Some(&new_glyph_id) = mapping.get(&glyph_id) {
                entries.push((code as u16, new_glyph_id));
            }
        }

        // Only formats 12 and 13 reach beyond the BMP.
        let mut supplementary = Vec::new();
        if matches!(
            self.cmap.format(),
            CmapFormat::Format12 | CmapFormat::Format13
        ) {
            for code in 0x1_0000..=0x10_FFFF {
                let Some(glyph_id) = self.cmap.glyph_id(cmap_data, code, self.glyph_count)? else {
                    continue;
                };
                if glyph_id == 0 {
                    continue;
                }
                if let Some(&new_glyph_id) = mapping.get(&glyph_id) {
                    supplementary.push((code, new_glyph_id));
                }
            }
        }

        if supplementary.is_empty() {
            return build_format4_cmap(&entries, self.symbolic_cmap);
        }

        // Format 12 is the preferred Unicode subtable, so it repeats the BMP
        // entries and adds the supplementary runs.
        let mut codes = Vec::with_capacity(entries.len() + supplementary.len());
        codes.extend(
            entries
                .iter()
                .map(|&(code, glyph_id)| (u32::from(code), glyph_id)),
        );
        codes.extend(supplementary);
        let groups = format12_groups(&codes);
        build_format12_cmap(&entries, &groups, self.symbolic_cmap)
    }
}

fn build_format4_cmap(entries: &[(u16, u16)], symbolic: bool) -> Result<Vec<u8>, FontError> {
    let subtable = format4_subtable(entries)?;
    let mut cmap = vec![0u8; 12 + subtable.len()];
    cmap[2..4].copy_from_slice(&1u16.to_be_bytes());
    cmap[4..6].copy_from_slice(&3u16.to_be_bytes());
    // Keep the Microsoft Symbol encoding so single-byte codes still resolve
    // through the 0xF000-based fallback used by PDF's built-in encoding.
    cmap[6..8].copy_from_slice(&(if symbolic { 0u16 } else { 1u16 }).to_be_bytes());
    cmap[8..12].copy_from_slice(&12u32.to_be_bytes());
    cmap[12..].copy_from_slice(&subtable);
    Ok(cmap)
}

/// Build a cmap with a format 4 subtable followed by a format 12 subtable.
///
/// The format 12 record uses the Unicode full-repertoire encoding, or
/// Microsoft Symbol when the source was symbolic, so readers prefer it over
/// the BMP-only format 4 record. Its groups therefore repeat the BMP entries
/// in addition to the supplementary-plane runs.
fn build_format12_cmap(
    entries: &[(u16, u16)],
    groups: &[(u32, u32, u32)],
    symbolic: bool,
) -> Result<Vec<u8>, FontError> {
    let format4 = format4_subtable(entries)?;
    let format12 = format12_subtable(groups)?;
    let format4_offset = 4 + 2 * 8;
    let format12_offset = format4_offset + format4.len();
    let mut cmap = vec![0u8; format12_offset + format12.len()];
    cmap[2..4].copy_from_slice(&2u16.to_be_bytes());

    cmap[4..6].copy_from_slice(&3u16.to_be_bytes());
    cmap[6..8].copy_from_slice(&(if symbolic { 0u16 } else { 1u16 }).to_be_bytes());
    cmap[8..12].copy_from_slice(&(format4_offset as u32).to_be_bytes());

    // Format 12 requires the full-repertoire encoding on Windows.
    let encoding = if symbolic { 0u16 } else { 10u16 };
    cmap[12..14].copy_from_slice(&3u16.to_be_bytes());
    cmap[14..16].copy_from_slice(&encoding.to_be_bytes());
    cmap[16..20].copy_from_slice(&(format12_offset as u32).to_be_bytes());

    cmap[format4_offset..format12_offset].copy_from_slice(&format4);
    cmap[format12_offset..].copy_from_slice(&format12);
    Ok(cmap)
}

fn format12_subtable(groups: &[(u32, u32, u32)]) -> Result<Vec<u8>, FontError> {
    let length = 16usize
        .checked_add(
            groups
                .len()
                .checked_mul(12)
                .ok_or_else(|| invalid_subset("cmap format 12 subtable length overflows"))?,
        )
        .ok_or_else(|| invalid_subset("cmap format 12 subtable length overflows"))?;
    let length_u32 = u32::try_from(length)
        .map_err(|_| invalid_subset("cmap format 12 subtable length exceeds u32"))?;
    let group_count = u32::try_from(groups.len())
        .map_err(|_| invalid_subset("cmap format 12 group count exceeds u32"))?;

    let mut subtable = vec![0u8; length];
    subtable[0..2].copy_from_slice(&12u16.to_be_bytes());
    // reserved and language stay zero
    subtable[4..8].copy_from_slice(&length_u32.to_be_bytes());
    subtable[12..16].copy_from_slice(&group_count.to_be_bytes());
    for (index, &(start, end, start_glyph)) in groups.iter().enumerate() {
        let group = 16 + index * 12;
        subtable[group..group + 4].copy_from_slice(&start.to_be_bytes());
        subtable[group + 4..group + 8].copy_from_slice(&end.to_be_bytes());
        subtable[group + 8..group + 12].copy_from_slice(&start_glyph.to_be_bytes());
    }
    Ok(subtable)
}

/// Group sorted `(code, new glyph ID)` pairs into format 12 runs.
fn format12_groups(codes: &[(u32, u16)]) -> Vec<(u32, u32, u32)> {
    let mut groups: Vec<(u32, u32, u32)> = Vec::new();
    for &(code, glyph_id) in codes {
        let glyph_id = u32::from(glyph_id);
        match groups.last_mut() {
            Some(group) if code == group.1 + 1 && glyph_id == group.2 + (group.1 - group.0) + 1 => {
                group.1 = code;
            }
            _ => groups.push((code, code, glyph_id)),
        }
    }
    groups
}

fn format4_subtable(entries: &[(u16, u16)]) -> Result<Vec<u8>, FontError> {
    let segment_count = entries.len() + 1;
    let length = segment_count
        .checked_mul(8)
        .and_then(|length| length.checked_add(16))
        .and_then(|length| length.checked_add(entries.len() * 2))
        .ok_or_else(|| invalid_subset("cmap format 4 subtable length overflows"))?;
    if length > usize::from(u16::MAX) {
        return Err(invalid_subset("cmap format 4 subtable exceeds 65535 bytes"));
    }

    let segment_count_x2 = u16::try_from(segment_count * 2)
        .map_err(|_| invalid_subset("cmap format 4 segment count exceeds u16"))?;
    let (search_range, entry_selector, range_shift) = cmap_search_parameters(segment_count)?;

    let mut subtable = vec![0u8; length];
    subtable[0..2].copy_from_slice(&4u16.to_be_bytes());
    subtable[2..4].copy_from_slice(&(length as u16).to_be_bytes());
    // language stays zero
    subtable[6..8].copy_from_slice(&segment_count_x2.to_be_bytes());
    subtable[8..10].copy_from_slice(&search_range.to_be_bytes());
    subtable[10..12].copy_from_slice(&entry_selector.to_be_bytes());
    subtable[12..14].copy_from_slice(&range_shift.to_be_bytes());

    let end_codes = 14;
    let start_codes = 16 + segment_count * 2;
    let deltas = 16 + segment_count * 4;
    let range_offsets = 16 + segment_count * 6;
    let glyph_array = 16 + segment_count * 8;
    // Every real segment has exactly one glyphIdArray entry, so the distance
    // from idRangeOffset[i] to its entry is 2 * segCount for every index.
    let range_offset = segment_count_x2;

    for (index, (code, glyph_id)) in entries.iter().enumerate() {
        subtable[end_codes + index * 2..end_codes + index * 2 + 2]
            .copy_from_slice(&code.to_be_bytes());
        subtable[start_codes + index * 2..start_codes + index * 2 + 2]
            .copy_from_slice(&code.to_be_bytes());
        subtable[range_offsets + index * 2..range_offsets + index * 2 + 2]
            .copy_from_slice(&range_offset.to_be_bytes());
        subtable[glyph_array + index * 2..glyph_array + index * 2 + 2]
            .copy_from_slice(&glyph_id.to_be_bytes());
    }

    let sentinel = entries.len();
    subtable[end_codes + sentinel * 2..end_codes + sentinel * 2 + 2]
        .copy_from_slice(&u16::MAX.to_be_bytes());
    subtable[start_codes + sentinel * 2..start_codes + sentinel * 2 + 2]
        .copy_from_slice(&u16::MAX.to_be_bytes());
    // idDelta = 1 maps the sentinel to glyph 0; idRangeOffset stays zero.
    subtable[deltas + sentinel * 2..deltas + sentinel * 2 + 2].copy_from_slice(&1u16.to_be_bytes());

    Ok(subtable)
}

fn cmap_search_parameters(segment_count: usize) -> Result<(u16, u16, u16), FontError> {
    let mut power = 1usize;
    let mut entry_selector = 0u16;
    while power * 2 <= segment_count {
        power *= 2;
        entry_selector += 1;
    }
    let search_range =
        u16::try_from(power * 2).map_err(|_| invalid_subset("cmap search range exceeds u16"))?;
    let range_shift = segment_count
        .checked_mul(2)
        .and_then(|value| value.checked_sub(usize::from(search_range)))
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| invalid_subset("cmap range shift exceeds u16"))?;
    Ok((search_range, entry_selector, range_shift))
}
