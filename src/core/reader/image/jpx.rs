//! JPXDecode（ISO 32000-1 §7.4.9 / JPEG 2000 ITU-T T.800）语法层解析。
//!
//! 本模块**只解析 JPX 字节流的头部语法结构**，不解码任何像素：
//! - JP2 boxed 文件（T.800 Annex A/L）：签名框 `jP  ` → 顶层框遍历，
//!   进入 `jp2h` 超框取首个 `ihdr`（几何、分量数、每分量位深）与
//!   `colr`（颜色空间说明），进入 `jp2c` 取连续 JPEG 2000 码流；
//! - 裸码流（T.800 §B.10）：SOC（0xFF4F）后按标记段扫描，跳过带长度的
//!   未知标记（如 CAP 0xFF50、COM 0xFF64），解析 SIZ（0xFF51）取图像
//!   区域、参考网格、瓦片与每分量采样/位深。
//!
//! **实现边界（刻意的设计约束，与 U3D/MIDI/CFF 同一立场）**：
//! - 不做像素解码：不解小波系数、不做熵解码（MQR/CoD），[`super::ImageModel::samples`]
//!   对 JPX 图像返回类型化 `Unsupported`；
//! - 不解析 ICC 剖面内容：`colr` 的 METH=2/3 只保留剖面字节长度
//!   （[`JpxColorSpace::IccProfile`]），不展开 ICC 头部或颜色变换；
//! - 不校验完整框树：未知框按长度跳过；框长度不一致即停止遍历，
//!   已得到的元数据照常返回。
//!
//! 读端遵循「宽进」：结构畸形到 [`parse_jpx_header`] 返回 `None`，
//! 绝不让文档解析失败；非 JPX 载荷同样返回 `None` 而不是错误。
//! 像素/小波/熵解码、完整框树校验、ICC 剖面解析、多码流拼接以及
//! `bpcc`/`cdef`/`pclr`/`resc`/`resx`/`resy` 等辅助框均为已知缺口，
//! 按需留待后续增量。

/// JP2 容器类型：boxed JP2 文件或裸 JPEG 2000 码流。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JpxContainer {
    /// JP2 boxed 文件（以 12 字节签名框 `jP  ` 起始）。
    Jp2File,
    /// 裸 JPEG 2000 码流（直接以 SOC 0xFF4F 起始）。
    RawCodestream,
}

/// 单个图像分量的采样属性（来自 SIZ 逐分量三元组或 `ihdr` 摘要）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JpxComponent {
    /// 每样本位数（T.800 `SSiz & 0x7F` 加 1；`ihdr` 的 BPC 同规则）。
    pub depth: u8,
    /// 样本是否有符号（`SSiz` / BPC 的 0x80 位）。
    pub signed: bool,
    /// 水平抽样步长（SIZ 的 `XRsiz`；`ihdr` 来源恒为 1）。
    pub x_subsampling: u8,
    /// 垂直抽样步长（SIZ 的 `YRsiz`；`ihdr` 来源恒为 1）。
    pub y_subsampling: u8,
}

/// `colr` 框承载的颜色空间说明（只建模头部，不解析 ICC 剖面字节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JpxColorSpace {
    /// 枚举颜色空间（METH=1）：`code` 为 EnumCS（12=CMYK、14=CIELab、
    /// 16=sRGB、17=Grayscale、18=sYCC、19=CIEJab、24=eYCC）。
    Enumerated {
        method: u8,
        precedence: u8,
        approximation: u8,
        code: u32,
    },
    /// ICC 剖面（METH=2 受限剖面 / METH=3 任意剖面）：只记录字节长度。
    IccProfile {
        method: u8,
        precedence: u8,
        approximation: u8,
        profile_len: usize,
    },
}

impl JpxColorSpace {
    /// 枚举颜色空间的标准名称；未知或非枚举来源返回 `None`。
    pub fn enumerated_name(&self) -> Option<&'static str> {
        let Self::Enumerated { code, .. } = self else {
            return None;
        };
        Some(match code {
            12 => "CMYK",
            14 => "CIELab",
            16 => "sRGB",
            17 => "Grayscale",
            18 => "sYCC",
            19 => "CIEJab",
            24 => "eYCC",
            _ => return None,
        })
    }
}

/// JPX 头部解析结果：几何 + 分量采样 + 颜色空间说明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JpxHeader {
    pub container: JpxContainer,
    /// 图像区域宽度（SIZ 的 `Xsiz − XOsiz`，或 `ihdr` 的 Width）。
    pub width: u32,
    /// 图像区域高度（SIZ 的 `Ysiz − YOsiz`，或 `ihdr` 的 Height）。
    pub height: u32,
    /// 参考网格偏移 `(XOsiz, YOsiz)`；仅 `ihdr` 来源时为 `None`。
    pub reference_offset: Option<(u32, u32)>,
    /// 参考瓦片宽度（SIZ 的 `XTsiz`）。
    pub tile_width: Option<u32>,
    /// 参考瓦片高度（SIZ 的 `YTsiz`）。
    pub tile_height: Option<u32>,
    /// 瓦片网格偏移 `(XTOsiz, YTOsiz)`。
    pub tile_grid_offset: Option<(u32, u32)>,
    /// 逐分量采样属性；来源不明（如 `ihdr` BPC=255 指向未解析的
    /// `bpcc` 框）时为空。
    pub components: Vec<JpxComponent>,
    /// `colr` 框的颜色空间说明；裸码流或缺框时为 `None`。
    pub color_space: Option<JpxColorSpace>,
}

impl JpxHeader {
    /// 分量个数。
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// 每分量最大位深；分量属性未知时为 `None`。
    pub fn bits_per_component(&self) -> Option<u8> {
        self.components
            .iter()
            .map(|component| component.depth)
            .max()
    }

    /// §7.4.9 的颜色空间提示：枚举 sRGB/sYCC→DeviceRGB、
    /// Grayscale→DeviceGray、CMYK→DeviceCMYK；其余按 Table 89 的回退
    /// 规则依分量数 1/3/4 → Gray/RGB/CMYK，其它分量数无提示。
    pub fn pdf_color_space_hint(&self) -> Option<&'static str> {
        if let Some(JpxColorSpace::Enumerated { code, .. }) = self.color_space {
            match code {
                12 => return Some("DeviceCMYK"),
                16 | 18 => return Some("DeviceRGB"),
                17 => return Some("DeviceGray"),
                _ => {}
            }
        }
        match self.components.len() {
            1 => Some("DeviceGray"),
            3 => Some("DeviceRGB"),
            4 => Some("DeviceCMYK"),
            _ => None,
        }
    }
}

/// 判断规范化的 `/Filter` 名列表中是否含 JPXDecode（含 PDF 1.3 前的
/// 旧名 `JPX`，与 `types::features` 的映射同一口径）。
pub(crate) fn is_jpx_filter(filters: &[String]) -> bool {
    filters
        .iter()
        .any(|name| name == "JPXDecode" || name == "JPX")
}

/// 解析 JPX 字节流的头部结构，取回几何与颜色空间元数据。
///
/// 只读语法层（见模块文档的边界声明）；任何结构畸形或非 JPX 载荷都
/// 返回 `None`，由调用方按「宽进」回退，不返回错误。
pub fn parse_jpx_header(data: &[u8]) -> Option<JpxHeader> {
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0x4F {
        // 裸码流：SOC 起始，无 JP2 框层，颜色空间无从得知。
        let info = parse_codestream(data)?;
        return Some(header_from_codestream(
            info,
            JpxContainer::RawCodestream,
            None,
        ));
    }
    parse_jp2(data)
}

fn header_from_codestream(
    info: CodestreamInfo,
    container: JpxContainer,
    color_space: Option<JpxColorSpace>,
) -> JpxHeader {
    JpxHeader {
        container,
        width: info.width,
        height: info.height,
        reference_offset: info.reference_offset,
        tile_width: info.tile_width,
        tile_height: info.tile_height,
        tile_grid_offset: info.tile_grid_offset,
        components: info.components,
        color_space,
    }
}

// ---------------------------------------------------------------------------
// JP2 boxed 文件遍历（T.800 Annex A / ISO 15444-1 Annex L）
// ---------------------------------------------------------------------------

/// JP2 遍历收集到的碎片：先收集、后合成，保证畸形框不影响已有元数据。
#[derive(Default)]
struct Jp2Fragments {
    ihdr: Option<(u32, u32, Vec<JpxComponent>)>,
    colr: Option<JpxColorSpace>,
    codestream: Option<CodestreamInfo>,
}

const JP2_SIGNATURE: [u8; 12] = [
    0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
];

const BOX_JP2H: [u8; 4] = *b"jp2h";
const BOX_IHDR: [u8; 4] = *b"ihdr";
const BOX_COLR: [u8; 4] = *b"colr";
const BOX_JP2C: [u8; 4] = *b"jp2c";
const BOX_FTY: [u8; 4] = *b"ftyp";

fn parse_jp2(data: &[u8]) -> Option<JpxHeader> {
    let start = if data.len() >= JP2_SIGNATURE.len() && data[..12] == JP2_SIGNATURE {
        12
    } else {
        // 宽松：无签名框时仍尝试框遍历，但必须看到已知 JP2 框才接受，
        // 避免把随机数据当 JP2。
        let first_type = if data.len() >= 8 {
            [data[4], data[5], data[6], data[7]]
        } else {
            return None;
        };
        if !matches!(first_type, BOX_JP2H | BOX_JP2C | BOX_FTY) {
            return None;
        }
        0
    };
    let mut fragments = Jp2Fragments::default();
    visit_boxes(&data[start..], &mut fragments, true);
    let CodestreamInfo {
        width,
        height,
        reference_offset,
        tile_width,
        tile_height,
        tile_grid_offset,
        components,
    } = fragments.codestream.or_else(|| {
        let (width, height, components) = fragments.ihdr?;
        Some(CodestreamInfo {
            width,
            height,
            reference_offset: None,
            tile_width: None,
            tile_height: None,
            tile_grid_offset: None,
            components,
        })
    })?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(JpxHeader {
        container: JpxContainer::Jp2File,
        width,
        height,
        reference_offset,
        tile_width,
        tile_height,
        tile_grid_offset,
        components,
        color_space: fragments.colr,
    })
}

/// 遍历一层次第框；`descend` 控制是否进入 `jp2h` 超框（仅一层）。
/// 框长度不一致即停止（保留已收集碎片）。
fn visit_boxes(data: &[u8], fragments: &mut Jp2Fragments, descend: bool) {
    let mut offset = 0usize;
    while offset + 8 <= data.len() {
        let len = u32_at(data, offset) as usize;
        let box_type = [
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ];
        let mut header_len = 8usize;
        let box_len = match len {
            0 => data.len() - offset,
            1 => {
                if offset + 16 > data.len() {
                    break;
                }
                let xl = u64_at(data, offset + 8);
                if xl < 16 {
                    break;
                }
                header_len = 16;
                xl as usize
            }
            other => {
                if other < header_len {
                    break;
                }
                other
            }
        };
        let content_start = offset + header_len;
        let content_end = offset.saturating_add(box_len).min(data.len());
        if content_start <= content_end {
            let content = &data[content_start..content_end];
            match box_type {
                BOX_JP2H if descend => visit_boxes(content, fragments, false),
                BOX_IHDR if fragments.ihdr.is_none() => {
                    if let Some(ihdr) = parse_ihdr(content) {
                        fragments.ihdr = Some(ihdr);
                    }
                }
                BOX_COLR if fragments.colr.is_none() => fragments.colr = parse_colr(content),
                BOX_JP2C if fragments.codestream.is_none() => {
                    fragments.codestream = parse_codestream(content);
                }
                _ => {}
            }
        }
        offset = match offset.checked_add(box_len) {
            Some(next) if next <= data.len() => next,
            _ => break,
        };
    }
}

/// `ihdr` 内容（≥14 字节）：Height/Width/NC/BPC/C/UnkC/IPR。
/// 分量数与位深直接按原值取（NC 是原始个数，不是个数减一）。
fn parse_ihdr(content: &[u8]) -> Option<(u32, u32, Vec<JpxComponent>)> {
    if content.len() < 14 {
        return None;
    }
    let height = u32_at(content, 0);
    let width = u32_at(content, 4);
    let count = u16_at(content, 8);
    let bpc = content[10];
    // C（压缩类型）规范要求为 7；「宽进」不校验，畸形仍按未解析降级。
    let components = if bpc == 255 || count == 0 || count > 4096 {
        // BPC=255 表示逐分量位深在未解析的 bpcc 框中。
        Vec::new()
    } else {
        let depth = (bpc & 0x7F) + 1;
        let signed = bpc & 0x80 != 0;
        let component = JpxComponent {
            depth,
            signed,
            x_subsampling: 1,
            y_subsampling: 1,
        };
        vec![component; count as usize]
    };
    Some((width, height, components))
}

/// `colr` 内容：METH(1) PRECEDENCE(1) APPROX(1)，随后 METH=1 为
/// EnumCS u32，METH=2/3 为 ICC 剖面字节（只记长度）。
fn parse_colr(content: &[u8]) -> Option<JpxColorSpace> {
    if content.len() < 4 {
        return None;
    }
    let method = content[0];
    let precedence = content[1];
    let approximation = content[2];
    if method == 1 {
        if content.len() < 7 {
            return None;
        }
        Some(JpxColorSpace::Enumerated {
            method,
            precedence,
            approximation,
            code: u32_at(content, 3),
        })
    } else {
        Some(JpxColorSpace::IccProfile {
            method,
            precedence,
            approximation,
            profile_len: content.len() - 3,
        })
    }
}

// ---------------------------------------------------------------------------
// JPEG 2000 码流标记段扫描（T.800 §B.10，只解析到 SIZ）
// ---------------------------------------------------------------------------

struct CodestreamInfo {
    width: u32,
    height: u32,
    reference_offset: Option<(u32, u32)>,
    tile_width: Option<u32>,
    tile_height: Option<u32>,
    tile_grid_offset: Option<(u32, u32)>,
    components: Vec<JpxComponent>,
}

fn parse_codestream(data: &[u8]) -> Option<CodestreamInfo> {
    if data.len() < 2 || data[0] != 0xFF || data[1] != 0x4F {
        return None;
    }
    let mut offset = 2usize;
    loop {
        if offset + 4 > data.len() {
            return None;
        }
        if data[offset] != 0xFF {
            return None;
        }
        let code = data[offset + 1];
        if code == 0x51 {
            return parse_siz(data, offset);
        }
        match code {
            // 无长度字段的标记：2 字节编码本身。
            0x00 | 0x30..=0x3F | 0x4F | 0x92 => offset += 2,
            // 图像数据已开始或码流结束却还没见到 SIZ：主头不完整。
            0x93 | 0xD9 | 0x90 => return None,
            // 带长度字段的未知标记段（CAP 0xFF50、COM 0xFF64 等）按
            // Lxxx 跳过；其余保留码无法界定长度，放弃。
            0x4E | 0x50..=0x9F => {
                let segment_len = u16_at(data, offset + 2) as usize;
                if segment_len < 2 {
                    return None;
                }
                offset = offset.checked_add(2 + segment_len)?;
            }
            _ => return None,
        }
    }
}

/// `SIZ`（0xFF51）标记段：Lsiz u16（含自身），内容 36 字节定长头 +
/// 每分量 3 字节（SSiz/XRsiz/YRsiz）。
fn parse_siz(data: &[u8], marker: usize) -> Option<CodestreamInfo> {
    let declared = u16_at(data, marker + 2) as usize;
    let content_start = marker + 4;
    if declared < 2 || content_start + 36 > data.len() {
        return None;
    }
    let available = data.len() - content_start;
    let content_len = (declared - 2).min(available);
    let content = &data[content_start..content_start + content_len];
    let xsiz = u32_at(content, 2);
    let ysiz = u32_at(content, 6);
    let xosiz = u32_at(content, 10);
    let yosiz = u32_at(content, 14);
    let xtsiz = u32_at(content, 18);
    let ytsiz = u32_at(content, 22);
    let xtosis = u32_at(content, 26);
    let ytosis = u32_at(content, 30);
    let count = u16_at(content, 34) as usize;
    let width = xsiz.checked_sub(xosiz)?;
    let height = ysiz.checked_sub(yosiz)?;
    if width == 0 || height == 0 {
        return None;
    }
    // 「宽进」：分量三元组不足声明个数时按可读取到的个数截断。
    let readable = (content_len.saturating_sub(36)) / 3;
    let components = (0..count.min(readable))
        .filter_map(|index| {
            let start = 36 + index * 3;
            if start + 3 > content_len {
                return None;
            }
            Some(JpxComponent {
                depth: (content[start] & 0x7F) + 1,
                signed: content[start] & 0x80 != 0,
                x_subsampling: content[start + 1],
                y_subsampling: content[start + 2],
            })
        })
        .collect();
    Some(CodestreamInfo {
        width,
        height,
        reference_offset: Some((xosiz, yosiz)),
        tile_width: Some(xtsiz),
        tile_height: Some(ytsiz),
        tile_grid_offset: Some((xtosis, ytosis)),
        components,
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

fn u64_at(data: &[u8], offset: usize) -> u64 {
    let Some(bytes) = data.get(offset..offset + 8) else {
        return 0;
    };
    u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

#[cfg(test)]
mod tests;
