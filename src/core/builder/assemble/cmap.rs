//! 嵌入编码 CMap 与 ToUnicode CMap 生成。

use crate::core::builder::CidEncoding;
use std::collections::BTreeMap;

/// 生成 Type 0 嵌入编码 CMap（PDF 32000-1 §9.7.5）。
///
/// 仅支持 2 字节字符码，codespace 固定 `<0000> <FFFF>`。字符码与 CID
/// 同时逐 1 递增的极大段写 `cidrange`，其余写 `cidchar`；每节至多 100 条
/// （与 §9.10.3 的分段约定一致）。CID 按 PDF CMap 语法写十进制整数。
pub(super) fn cid_encoding_cmap(encoding: &CidEncoding) -> Vec<u8> {
    /// cidrange / cidchar 单节条目上限。
    const SECTION_LIMIT: usize = 100;

    let entries: Vec<(u16, u16)> = encoding
        .mappings
        .iter()
        .map(|(&code, &cid)| (code, cid))
        .collect();

    // 找出极大连续段：字符码与 CID 同时逐 1 递增。以 u32 运算，
    // u16::MAX 之后无法再递增，天然避免回绕。
    let mut in_range = vec![false; entries.len()];
    let mut ranges: Vec<(u16, u16, u16)> = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let mut end = start;
        while end + 1 < entries.len() {
            let next_code = u32::from(entries[end].0) + 1;
            let next_cid = u32::from(entries[end].1) + 1;
            if next_code != u32::from(entries[end + 1].0)
                || next_cid != u32::from(entries[end + 1].1)
            {
                break;
            }
            end += 1;
        }
        if end > start {
            ranges.push((entries[start].0, entries[end].0, entries[start].1));
            in_range[start..=end].fill(true);
        }
        start = end + 1;
    }

    let wmode = encoding.wmode;
    let suffix = if wmode == 1 { "V" } else { "H" };
    let mut cmap = format!(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CMapName /Custom-{suffix} def\n\
         /CMapType 1 def\n\
         /WMode {wmode} def\n\
         1 begincodespacerange\n\
         <0000> <FFFF>\n\
         endcodespacerange\n"
    );
    for chunk in ranges.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} begincidrange\n", chunk.len()));
        for (start_code, end_code, start_cid) in chunk {
            cmap.push_str(&format!(
                "<{start_code:04X}> <{end_code:04X}> {start_cid}\n"
            ));
        }
        cmap.push_str("endcidrange\n");
    }

    let chars: Vec<(u16, u16)> = entries
        .iter()
        .zip(&in_range)
        .filter(|(_, covered)| !**covered)
        .map(|(&entry, _)| entry)
        .collect();
    for chunk in chars.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} begincidchar\n", chunk.len()));
        for (code, cid) in chunk {
            cmap.push_str(&format!("<{code:04X}> {cid}\n"));
        }
        cmap.push_str("endcidchar\n");
    }

    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap.into_bytes()
}

/// 生成标准 ToUnicode CMap：2 字节 codespace；CID 连续且目标为单个
/// UTF-16 码元、并逐 1 递增的极大段写 `bfrange`，其余（代理对、多码元
/// 串、目标跳变）写 `bfchar`；每节至多 100 条（PDF 32000-1 §9.10.3）。
pub(super) fn to_unicode_cmap(mappings: &BTreeMap<u16, String>) -> Vec<u8> {
    /// bfchar / bfrange 单节条目上限。
    const SECTION_LIMIT: usize = 100;

    // 仅「恰好一个 UTF-16 码元」的目标可参与 bfrange；
    // 代理对与多码元串只能逐条写 bfchar。
    let entries: Vec<(u16, &str, Option<u16>)> = mappings
        .iter()
        .map(|(cid, text)| {
            let mut units = text.encode_utf16();
            let first = units.next();
            let single = if units.next().is_none() { first } else { None };
            (*cid, text.as_str(), single)
        })
        .collect();

    // 找出极大连续段：CID 与目标码元同时逐 1 递增。以 u32 运算，
    // u16::MAX 之后无法再递增，天然避免回绕。
    let mut in_range = vec![false; entries.len()];
    let mut ranges: Vec<(u16, u16, u16)> = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let Some(first_unit) = entries[start].2 else {
            start += 1;
            continue;
        };
        let mut end = start;
        while end + 1 < entries.len() {
            let Some(next_unit) = entries[end + 1].2 else {
                break;
            };
            let offset = (end - start) as u32 + 1;
            if u32::from(entries[end].0) + 1 != u32::from(entries[end + 1].0)
                || u32::from(first_unit) + offset != u32::from(next_unit)
            {
                break;
            }
            end += 1;
        }
        if end > start {
            ranges.push((entries[start].0, entries[end].0, first_unit));
            in_range[start..=end].fill(true);
        }
        start = end + 1;
    }

    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n\
         <0000> <FFFF>\n\
         endcodespacerange\n",
    );
    for chunk in ranges.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} beginbfrange\n", chunk.len()));
        for (start_cid, end_cid, unit) in chunk {
            cmap.push_str(&format!("<{start_cid:04X}> <{end_cid:04X}> <{unit:04X}>\n"));
        }
        cmap.push_str("endbfrange\n");
    }

    let chars: Vec<(u16, &str)> = entries
        .iter()
        .zip(&in_range)
        .filter(|(_, covered)| !**covered)
        .map(|((cid, text, _), _)| (*cid, *text))
        .collect();
    for chunk in chars.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (cid, text) in chunk {
            cmap.push_str(&format!("<{cid:04X}> <"));
            for unit in text.encode_utf16() {
                cmap.push_str(&format!("{unit:04X}"));
            }
            cmap.push_str(">\n");
        }
        cmap.push_str("endbfchar\n");
    }

    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap.into_bytes()
}
