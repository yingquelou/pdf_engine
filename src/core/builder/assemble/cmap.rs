//! 嵌入编码 CMap 与 ToUnicode CMap 生成。

use crate::core::builder::CidEncoding;
use crate::core::writer::WriteError;
use std::collections::BTreeMap;

/// 字符码按（字节长度, 大端码值）排序，同一长度内码值升序。
#[derive(Debug, Clone, Copy)]
struct CodeEntry {
    length: usize,
    value: u32,
    cid: u16,
}

/// 生成 Type 0 嵌入编码 CMap（PDF 32000-1 §9.7.5）。
///
/// 支持 1..=4 字节字符码：字符码按实际长度写成大端十六进制。仅含 2 字节
/// 映射时保持传统 codespace `<0000> <FFFF>`；存在其他码长时按实际连续码
/// 区间分组写 codespace，避免与较短码产生前缀歧义（歧义时返回
/// [`WriteError::InvalidObject`]）。字符码与 CID 同时逐 1 递增的极大段写
/// `cidrange`，其余写 `cidchar`；每节至多 100 条（与 §9.10.3 一致）。
/// 可选 `usecmap` 基座在 codespace 之前写出，仅声明继承关系。
pub(super) fn cid_encoding_cmap(encoding: &CidEncoding) -> Result<Vec<u8>, WriteError> {
    /// cidrange / cidchar 单节条目上限。
    const SECTION_LIMIT: usize = 100;

    let mut entries = Vec::with_capacity(encoding.mappings.len() + encoding.byte_mappings.len());
    for (&code, &cid) in &encoding.mappings {
        entries.push(CodeEntry {
            length: 2,
            value: u32::from(code),
            cid,
        });
    }
    for (code, &cid) in &encoding.byte_mappings {
        if !(1..=4).contains(&code.len()) {
            return Err(WriteError::InvalidObject(format!(
                "CidEncoding character code must be 1 to 4 bytes, got {} bytes",
                code.len()
            )));
        }
        entries.push(CodeEntry {
            length: code.len(),
            value: bytes_to_code(code),
            cid,
        });
    }
    entries.sort_by_key(|entry| (entry.length, entry.value));

    // codespace：仅 2 字节映射时沿用覆盖全部 BMP 的固定区间；有自定义码长
    // 时改为按实际连续码区间分组，避免与短码 JIS 前缀产生歧义。
    let spaces = if entries.iter().all(|entry| entry.length == 2) {
        vec![(2usize, 0u32, u32::from(u16::MAX))]
    } else {
        contiguous_runs(entries.iter().map(|entry| (entry.length, entry.value)))
    };
    validate_codespaces(&spaces)?;

    let wmode = encoding.wmode;
    let name = cmap_name(encoding);
    let mut cmap = format!(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CMapName /{name} def\n\
         /CMapType 1 def\n\
         /WMode {wmode} def\n"
    );
    if let Some(base) = &encoding.usecmap {
        // 基座只声明继承；本 CMap 仍写完整的 codespace 与映射。
        cmap.push_str(&format!("/{base} usecmap\n"));
    }
    cmap.push_str(&format!("{} begincodespacerange\n", spaces.len()));
    for (length, low, high) in &spaces {
        cmap.push_str(&format!(
            "{} {}\n",
            hex_code(*low, *length),
            hex_code(*high, *length)
        ));
    }
    cmap.push_str("endcodespacerange\n");

    // 最大连续段：字符码与 CID 同时逐 1 递增，且长度一致。
    let mut in_range = vec![false; entries.len()];
    let mut ranges: Vec<(CodeEntry, CodeEntry)> = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let mut end = start;
        while end + 1 < entries.len() {
            let current = entries[end];
            let next = entries[end + 1];
            if current.length != next.length
                || u64::from(current.value) + 1 != u64::from(next.value)
                || u32::from(current.cid) + 1 != u32::from(next.cid)
            {
                break;
            }
            end += 1;
        }
        if end > start {
            ranges.push((entries[start], entries[end]));
            in_range[start..=end].fill(true);
        }
        start = end + 1;
    }

    for chunk in ranges.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} begincidrange\n", chunk.len()));
        for (first, last) in chunk {
            cmap.push_str(&format!(
                "{} {} {}\n",
                hex_code(first.value, first.length),
                hex_code(last.value, last.length),
                first.cid
            ));
        }
        cmap.push_str("endcidrange\n");
    }

    let chars: Vec<CodeEntry> = entries
        .iter()
        .zip(&in_range)
        .filter(|(_, covered)| !**covered)
        .map(|(entry, _)| *entry)
        .collect();
    for chunk in chars.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} begincidchar\n", chunk.len()));
        for entry in chunk {
            cmap.push_str(&format!(
                "{} {}\n",
                hex_code(entry.value, entry.length),
                entry.cid
            ));
        }
        cmap.push_str("endcidchar\n");
    }

    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    Ok(cmap.into_bytes())
}

/// 返回实际写出的 `/CMapName`：未设置时按书写模式取 `/Custom-H` 或
/// `/Custom-V`。
pub(super) fn cmap_name(encoding: &CidEncoding) -> String {
    encoding.cmap_name.clone().unwrap_or_else(|| {
        if encoding.wmode == 1 {
            "Custom-V".to_string()
        } else {
            "Custom-H".to_string()
        }
    })
}

/// ToUnicode 条目：源字符码字节序列 + 首个 UTF-16 码元（仅单码元可入
/// `bfrange`）。
struct UnicodeEntry<'a> {
    code: Vec<u8>,
    value: u32,
    text: &'a str,
    single_unit: Option<u16>,
}

/// 生成标准 ToUnicode CMap：codespace 与 bfchar/bfrange 键按字符码的实际
/// 字节长度写出（仅 2 字节码时保持 `<0000> <FFFF>`）。CID 连续、目标为单个
/// UTF-16 码元且逐 1 递增的极大段写 `bfrange`，其余（代理对、多码元串、
/// 目标跳变）写 `bfchar`；每节至多 100 条（PDF 32000-1 §9.10.3）。
pub(super) fn to_unicode_cmap(
    two_byte: &BTreeMap<u16, String>,
    byte_keys: &BTreeMap<Vec<u8>, String>,
) -> Vec<u8> {
    /// bfchar / bfrange 单节条目上限。
    const SECTION_LIMIT: usize = 100;

    let mut entries: Vec<UnicodeEntry<'_>> = Vec::with_capacity(two_byte.len() + byte_keys.len());
    for (code, text) in two_byte {
        entries.push(UnicodeEntry {
            code: code.to_be_bytes().to_vec(),
            value: u32::from(*code),
            text,
            single_unit: single_utf16_unit(text),
        });
    }
    for (code, text) in byte_keys {
        // 长度合法性由构建校验保证；此处防御性截断到可表示范围。
        let length = code.len().min(4);
        entries.push(UnicodeEntry {
            code: code[..length].to_vec(),
            value: bytes_to_code(&code[..length]),
            text,
            single_unit: single_utf16_unit(text),
        });
    }
    entries.sort_by_key(|entry| (entry.code.len(), entry.value));

    let spaces = if entries.iter().all(|entry| entry.code.len() == 2) {
        vec![(2usize, 0u32, u32::from(u16::MAX))]
    } else {
        contiguous_runs(entries.iter().map(|entry| (entry.code.len(), entry.value)))
    };

    // 最大连续段：源码与目标码元同时逐 1 递增，且源码长度一致。
    let mut in_range = vec![false; entries.len()];
    let mut ranges: Vec<(Vec<u8>, Vec<u8>, u16)> = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let Some(first_unit) = entries[start].single_unit else {
            start += 1;
            continue;
        };
        let mut end = start;
        while end + 1 < entries.len() {
            let current = &entries[end];
            let next = &entries[end + 1];
            let Some(next_unit) = next.single_unit else {
                break;
            };
            let offset = (end - start) as u32 + 1;
            if current.code.len() != next.code.len()
                || u64::from(current.value) + 1 != u64::from(next.value)
                || u32::from(first_unit) + offset != u32::from(next_unit)
            {
                break;
            }
            end += 1;
        }
        if end > start {
            ranges.push((
                entries[start].code.clone(),
                entries[end].code.clone(),
                first_unit,
            ));
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
         /CMapType 2 def\n",
    );
    cmap.push_str(&format!("{} begincodespacerange\n", spaces.len()));
    for (length, low, high) in &spaces {
        cmap.push_str(&format!(
            "{} {}\n",
            hex_code(*low, *length),
            hex_code(*high, *length)
        ));
    }
    cmap.push_str("endcodespacerange\n");

    for chunk in ranges.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} beginbfrange\n", chunk.len()));
        for (first, last, unit) in chunk {
            cmap.push_str(&format!(
                "{} {} <{unit:04X}>\n",
                hex_bytes(first),
                hex_bytes(last)
            ));
        }
        cmap.push_str("endbfrange\n");
    }

    let chars: Vec<&UnicodeEntry<'_>> = entries
        .iter()
        .zip(&in_range)
        .filter(|(_, covered)| !**covered)
        .map(|(entry, _)| entry)
        .collect();
    for chunk in chars.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for entry in chunk {
            cmap.push_str(&format!("{} <", hex_bytes(&entry.code)));
            for unit in entry.text.encode_utf16() {
                cmap.push_str(&format!("{unit:04X}"));
            }
            cmap.push_str(">\n");
        }
        cmap.push_str("endbfchar\n");
    }

    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap.into_bytes()
}

/// 按相同长度合并连续码值区间，返回 `(长度, 起始码, 结束码)`。
///
/// 输入须按（长度, 码值）升序且同一长度内无重复；用迭代方式一趟完成，
/// 不递归。
fn contiguous_runs(entries: impl Iterator<Item = (usize, u32)>) -> Vec<(usize, u32, u32)> {
    let mut runs = Vec::new();
    let mut current: Option<(usize, u32, u32)> = None;
    for (length, value) in entries {
        match &mut current {
            Some((run_length, _, high))
                if *run_length == length && u64::from(*high) + 1 == u64::from(value) =>
            {
                *high = value;
            }
            _ => {
                if let Some(run) = current.take() {
                    runs.push(run);
                }
                current = Some((length, value, value));
            }
        }
    }
    if let Some(run) = current {
        runs.push(run);
    }
    runs
}

/// 校验 codespace 分组：同长度不重叠，不同长度前缀不歧义（与读端
/// `finalize_cmap` 的约束一致）。
fn validate_codespaces(spaces: &[(usize, u32, u32)]) -> Result<(), WriteError> {
    for (index, first) in spaces.iter().enumerate() {
        for second in spaces.iter().skip(index + 1) {
            if first.0 == second.0 {
                if first.2 >= second.1 {
                    return Err(WriteError::InvalidObject(
                        "CidEncoding codespace ranges overlap".into(),
                    ));
                }
                continue;
            }
            let (short, long) = if first.0 < second.0 {
                (first, second)
            } else {
                (second, first)
            };
            let shift = (long.0 - short.0) * 8;
            let prefix_low = long.1 >> shift;
            let prefix_high = long.2 >> shift;
            if prefix_low <= short.2 && prefix_high >= short.1 {
                return Err(WriteError::InvalidObject(format!(
                    "CidEncoding codespace has ambiguous {}-byte prefix for {}-byte codes",
                    short.0, long.0
                )));
            }
        }
    }
    Ok(())
}

/// 以大端字节序列求码值。
fn bytes_to_code(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
}

/// 按指定字节长度把码值写成 `<...>` 十六进制串。
fn hex_code(value: u32, length: usize) -> String {
    let bytes = value.to_be_bytes();
    hex_bytes(&bytes[4 - length..])
}

/// 把字节序列写成 `<...>` 十六进制串。
fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push('<');
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
    out.push('>');
    out
}

/// 目标恰为单个 UTF-16 码元时返回该码元，否则返回 `None`。
fn single_utf16_unit(text: &str) -> Option<u16> {
    let mut units = text.encode_utf16();
    let first = units.next();
    if units.next().is_none() { first } else { None }
}
