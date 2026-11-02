//! JBIG2 段链遍历与段头解析（T.88 §7.2）：只读语法层，不解码像素。

use super::kinds::Jbig2SegmentKind;
use super::model::{Jbig2Error, Jbig2Finding, Jbig2PageInfo, Jbig2Segment, Jbig2Stream};

/// JBIG2 文件头签名（T.88 §6.2：`97 4A 42 32 0D 0A 1A 0A`）。
const FILE_SIGNATURE: [u8; 8] = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];

/// 页面信息段载荷的核心长度（4×u32 + BPC + BPIFlags = 18 字节）。
const PAGE_INFORMATION_MIN_LEN: usize = 18;

/// 页面信息段载荷的规范完整长度（再加 BPIExtension = 19 字节）。
const PAGE_INFORMATION_FULL_LEN: usize = 19;

/// 判断规范化的 `/Filter` 名列表中是否含 JBIG2Decode（含旧名 `JBIG2`，
/// 与 `types::features` 的映射同一口径）。
pub(crate) fn is_jbig2_filter(filters: &[String]) -> bool {
    filters
        .iter()
        .any(|name| name == "JBIG2Decode" || name == "JBIG2")
}

/// 遍历 JBIG2 段链，读取段头与页面信息段元数据（见模块文档的边界声明）。
///
/// 只解析语法层，不解码任何段载荷的内部结构；段头不完整返回类型化
/// [`Jbig2Error::TruncatedSegmentHeader`]（绝不 panic），其余畸形以
/// [`Jbig2Finding`] 记录后干净停止，已解析的段照常返回。
pub fn parse_jbig2_segments(data: &[u8]) -> Result<Jbig2Stream, Jbig2Error> {
    let mut segments = Vec::new();
    let mut findings = Vec::new();
    let mut page_information: Option<Jbig2PageInfo> = None;
    let mut position = 0usize;

    if data.len() >= 8 && data[..8] == FILE_SIGNATURE {
        // T.88 §6.2：旗标 bit0=0 为随机访问（无页数），bit1=0 时跟随
        // 4 字节页数。§7.4.7 禁止文件头进入 PDF：识别、跳过并记录。
        let flags = data.get(8).copied();
        let random_access = flags.map(|flags| flags & 1 == 0);
        let has_page_count = flags.is_some_and(|flags| flags & 2 == 0);
        let declared_pages = if has_page_count && data.len() >= 13 {
            Some(u32_at(data, 9))
        } else {
            None
        };
        findings.push(Jbig2Finding::FileHeaderPresent {
            position: 0,
            flags,
            random_access,
            declared_pages,
        });
        let header_len = if has_page_count { 13 } else { 9 };
        if data.len() < header_len {
            // 文件头自身截断：无从定位段起点，干净停止。
            return Ok(Jbig2Stream {
                segments,
                findings,
                page_information,
            });
        }
        position = header_len;
    }

    while position < data.len() {
        let (segment, advance, stop) =
            parse_one_segment(data, position, &mut findings, &mut page_information)?;
        segments.push(segment);
        if stop {
            break;
        }
        position += advance;
    }
    Ok(Jbig2Stream {
        segments,
        findings,
        page_information,
    })
}

/// 解析 `position` 处的单个段。返回（段, 应前进字节数, 是否干净停止）。
fn parse_one_segment(
    data: &[u8],
    position: usize,
    findings: &mut Vec<Jbig2Finding>,
    page_information: &mut Option<Jbig2PageInfo>,
) -> Result<(Jbig2Segment, usize, bool), Jbig2Error> {
    let remaining = data.len() - position;
    if remaining < 11 {
        // jbig2dec 同口径：11 字节是段头下界。
        return Err(Jbig2Error::TruncatedSegmentHeader {
            position,
            remaining,
            required: 11,
        });
    }
    let truncated = |required: usize| -> Result<(Jbig2Segment, usize, bool), Jbig2Error> {
        Err(Jbig2Error::TruncatedSegmentHeader {
            position,
            remaining,
            required,
        })
    };

    let number = u32_at(data, position);
    let flags = data[position + 4];
    let segment_type = flags & 0x3F;
    let kind = Jbig2SegmentKind::from_segment_type(segment_type);
    let deferred_non_retain = flags & 0x80 != 0;
    let page_association_long = flags & 0x40 != 0;
    let pa_len = if page_association_long { 4 } else { 1 };
    let rtscarf = data[position + 5];
    let mut offset = position + 6;

    let (referred_count, referred_count_extended, retained_bytes) = if rtscarf & 0xE0 == 0xE0 {
        // T.88 §7.2.4 长形式：32 位字段自本旗标字节起重叠读取（低 29 位
        // 为被引用段个数），随后 ceil(count/8) 字节保留位图。
        let count = u32_at(data, position + 5) & 0x1FFF_FFFF;
        offset = position + 9;
        let retained_len = (count as usize).div_ceil(8);
        let end = match offset.checked_add(retained_len) {
            Some(end) if end <= data.len() => end,
            _ => return truncated(offset - position + retained_len),
        };
        let bytes = data[offset..end].to_vec();
        offset = end;
        (count, true, bytes)
    } else {
        // 短形式：高 3 位直接是被引用段个数（0–6；TP=5/6 预留形式不
        // 支持，按个数处理，畸形输入会退化为后续截断发现）。
        ((rtscarf >> 5) as u32, false, Vec::new())
    };
    let retained_bits_indicator = if referred_count_extended {
        retained_bytes.first().copied().unwrap_or(0)
    } else {
        rtscarf & 0x1F
    };

    // 被引用段号字段宽度（T.88 §7.2.4，jbig2dec 同口径）：按当前段号。
    let field_size = if number <= 256 {
        1usize
    } else if number <= 65536 {
        2
    } else {
        4
    };
    let refs_len = match (referred_count as usize).checked_mul(field_size) {
        Some(len) => len,
        None => return truncated(remaining.saturating_add(1)),
    };
    let needed = (offset - position)
        .saturating_add(refs_len)
        .saturating_add(pa_len)
        .saturating_add(4);
    if needed > remaining {
        return truncated(needed);
    }
    let mut referred_to_segments = Vec::with_capacity(referred_count as usize);
    for _ in 0..referred_count {
        let value = match field_size {
            1 => u32::from(data[offset]),
            2 => u32::from(u16_at(data, offset)),
            _ => u32_at(data, offset),
        };
        offset += field_size;
        referred_to_segments.push(value);
    }
    let page_association = if page_association_long {
        u32_at(data, offset)
    } else {
        u32::from(data[offset])
    };
    offset += pa_len;
    let data_size = u32_at(data, offset);
    offset += 4;
    let header_len = offset - position;

    let mut payload = Vec::new();
    let mut advance = header_len;
    let mut stop = false;
    if data_size == u32::MAX {
        // §7.2.7 未指定长度：终止符搜索超出本层范围，记录并干净停止。
        findings.push(Jbig2Finding::SegmentLengthUnspecified {
            segment_number: number,
        });
        stop = true;
    } else {
        let declared = data_size as usize;
        let available = remaining - header_len;
        if declared > available {
            findings.push(Jbig2Finding::SegmentDataTruncated {
                segment_number: number,
                declared: data_size,
                available,
            });
            payload = data[offset..offset + available].to_vec();
            advance += available;
            stop = true;
        } else {
            payload = data[offset..offset + declared].to_vec();
            advance += declared;
        }
    }

    match kind {
        Jbig2SegmentKind::EndOfPage => findings.push(Jbig2Finding::EndOfPageSegmentPresent {
            segment_number: number,
        }),
        Jbig2SegmentKind::EndOfFile => findings.push(Jbig2Finding::EndOfFileSegmentPresent {
            segment_number: number,
        }),
        Jbig2SegmentKind::PageInformation => {
            if payload.len() < PAGE_INFORMATION_FULL_LEN {
                findings.push(Jbig2Finding::PageInformationSegmentTooShort {
                    segment_number: number,
                    available: payload.len(),
                });
            }
            if page_information.is_none() {
                *page_information = parse_page_information(&payload);
            }
        }
        _ => {}
    }

    Ok((
        Jbig2Segment {
            number,
            kind,
            segment_type,
            flags,
            deferred_non_retain,
            referred_to_segments,
            referred_count_extended,
            retained_bits_indicator,
            retained_bytes,
            page_association,
            page_association_long,
            data_size,
            data: payload,
            header_len,
        },
        advance,
        stop,
    ))
}

/// 页面信息段载荷（§7.4.8）：4×u32 几何 + BPC + BPIFlags (+ BPIExtension)。
fn parse_page_information(payload: &[u8]) -> Option<Jbig2PageInfo> {
    if payload.len() < PAGE_INFORMATION_MIN_LEN {
        return None;
    }
    let flags = payload[16];
    let bpi_flags = payload[17];
    let bpi_extension = (payload.len() >= PAGE_INFORMATION_FULL_LEN).then(|| payload[18]);
    let striped = bpi_flags & 0x80 != 0;
    let stripe_height = match (striped, bpi_extension) {
        (true, Some(extension)) => Some((u16::from(bpi_flags & 0x7F) << 8) | u16::from(extension)),
        _ => None,
    };
    Some(Jbig2PageInfo {
        width: u32_at(payload, 0),
        height: u32_at(payload, 4),
        x_resolution: u32_at(payload, 8),
        y_resolution: u32_at(payload, 12),
        flags,
        lossless: flags & 0x01 != 0,
        refinement: flags & 0x02 != 0,
        default_pixel: (flags >> 2) & 1,
        combination_operator: (flags >> 3) & 3,
        requires_buffer: flags & 0x20 != 0,
        combination_operator_override: flags & 0x40 != 0,
        colored: flags & 0x80 != 0,
        bpi_flags,
        bpi_extension,
        striped,
        stripe_height,
    })
}

fn u16_at(data: &[u8], offset: usize) -> u16 {
    let Some(bytes) = data.get(offset..offset + 2) else {
        return 0;
    };
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    let Some(bytes) = data.get(offset..offset + 4) else {
        return 0;
    };
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
