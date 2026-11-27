//! 写端流压缩（D4：`FlateDecode` 编码路径）。
//!
//! 与读端 `filters::flate::InflateBackend` 对称，压缩实现经
//! [`DeflateBackend`] trait 隔离，默认 [`Flate2DeflateBackend`]（纯 Rust
//! miniz_oxide）始终可用。压缩在序列化咽喉点（`write_object`）应用，
//! 内存中的 `StreamData::Owned` **恒为明文**——「宽进严出」不变量不受影响。
//!
//! **失败单一契约**：调用方注入的后端失败时报类型化
//! [`WriteError::Compression`]；[`DocumentWriter`](super::DocumentWriter)
//! 先缓冲全部字节、成功后才写出，因此失败时输出端**零字节**，
//! 绝不静默回退明文。

use std::collections::HashMap;
use std::fmt;
use std::io::Write as _;
use std::sync::Arc;

use crate::core::types::{Dict, Value};

use super::WriteError;

/// 写端主动编码的 PDF 流过滤器（ISO 32000-1 7.4）。
///
/// Flate 经 [`DeflateBackend`] 注入，其余四种为纯 Rust 本地编码；
/// ASCIIHex/ASCII85/RunLength/LZW 均自 PDF 1.0 起可用，Flate 自 1.2 起。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamFilter {
    /// `FlateDecode`（RFC 1950 zlib）。
    Flate,
    /// `ASCIIHexDecode`（`>` EOD）。
    AsciiHex,
    /// `ASCII85Decode`（`<~ … ~>`，Adobe 约定）。
    Ascii85,
    /// `RunLengthDecode`（128 字节字面量 / 重复块）。
    RunLength,
    /// `LZWDecode`（9–12 位，EarlyChange=1）。
    Lzw,
}

impl StreamFilter {
    /// 过滤器名（写进 `/Filter` 的 PDF 名字）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Flate => "FlateDecode",
            Self::AsciiHex => "ASCIIHexDecode",
            Self::Ascii85 => "ASCII85Decode",
            Self::RunLength => "RunLengthDecode",
            Self::Lzw => "LZWDecode",
        }
    }

    /// 引入该过滤器的最低 PDF 版本。
    pub(crate) fn min_version(self) -> crate::core::types::PdfVersion {
        match self {
            Self::Flate => crate::core::types::PdfVersion::V1_2,
            _ => crate::core::types::PdfVersion::V1_0,
        }
    }
}

/// DEFLATE 压缩后端（第三方实现的隔离层）。
///
/// PDF 只规定 `FlateDecode` 的字节格式（RFC 1950 zlib），不规定实现。
/// 写端只依赖本 trait；默认实现 [`Flate2DeflateBackend`] 把 `flate2`
/// 封装在唯一适配器中，可替换为其他纯 Rust/平台后端而不影响写出流程。
pub trait DeflateBackend: Send + Sync {
    /// 把数据压缩为 zlib（RFC 1950）字节串；失败返回人类可读原因。
    fn deflate(&self, data: &[u8]) -> Result<Vec<u8>, String>;
}

/// 默认压缩后端：`flate2`（纯 Rust miniz_oxide），始终可用。
#[derive(Debug, Clone, Copy)]
pub struct Flate2DeflateBackend;

impl DeflateBackend for Flate2DeflateBackend {
    fn deflate(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(data)
            .map_err(|e| format!("FlateEncode failed: {e}"))?;
        enc.finish().map_err(|e| format!("FlateEncode failed: {e}"))
    }
}

/// 图像预测器选择（ISO 32000-1 7.4.4.4，Table 10）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePredictor {
    /// Predictor 1：不使用预测器（默认），纯 Flate。
    None,
    /// Predictor 2：TIFF 逐样本左差分（要求 BitsPerComponent 为 8 的倍数）。
    Tiff,
    /// Predictor 10–14：固定 PNG 过滤函数（无逐行过滤字节）。
    PngFixed(PngFilter),
    /// Predictor 15（旧名 PngUp，保持兼容）：每行前置过滤字节 `2`（Up），
    /// 对 8-bit 设备色图像样本做逐行差分。要求目标版本 ≥1.3。
    PngUp,
    /// Predictor 15：逐行自适应选择最优 PNG 过滤函数，每行前置选中的标记。
    PngOptimum,
}

/// PNG 过滤函数（Predictor 10–14 的固定标记与 Predictor 15 的逐行标记）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PngFilter {
    /// 0：None。
    None,
    /// 1：Sub（左邻样本差分）。
    Sub,
    /// 2：Up（上行同列差分）。
    Up,
    /// 3：Average（左/上均值差分）。
    Average,
    /// 4：Paeth（左/上/左上最优预测差分）。
    Paeth,
}

impl PngFilter {
    fn tag(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Sub => 1,
            Self::Up => 2,
            Self::Average => 3,
            Self::Paeth => 4,
        }
    }

    /// 对一行做过滤编码；`bpp` 为每像素字节数（左邻跨距），`prev` 为上一行。
    fn encode_row(self, cur: &[u8], prev: &[u8], bpp: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(cur.len());
        for (i, &byte) in cur.iter().enumerate() {
            let left = if i >= bpp { cur[i - bpp] } else { 0 };
            let above = prev.get(i).copied().unwrap_or(0);
            let upper_left = if i >= bpp {
                prev.get(i - bpp).copied().unwrap_or(0)
            } else {
                0
            };
            let predictor = match self {
                Self::None => 0,
                Self::Sub => left,
                Self::Up => above,
                Self::Average => ((left as u16 + above as u16) / 2) as u8,
                Self::Paeth => paeth(left, above, upper_left),
            };
            out.push(byte.wrapping_sub(predictor));
        }
        out
    }

    /// 一行的字节代价（用于 Predictor 15 的自适应选择；含行首标记）。
    fn row_cost(self, cur: &[u8], prev: &[u8], bpp: usize) -> usize {
        self.encode_row(cur, prev, bpp)
            .iter()
            .map(|&v| (v as i16).unsigned_abs() as usize)
            .sum::<usize>()
            + 1
    }
}

fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
    let (a, b, c) = (left as i16, above as i16, upper_left as i16);
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        left
    } else if pb <= pc {
        above
    } else {
        upper_left
    }
}

/// 写端流压缩策略。
///
/// 生效范围：序列化时所有**未声明 `/Filter`** 的流（Owned 明文流；含页面
/// 内容流、图像、字体程序与 writer 生成的对象流/XRef 流）。已声明过滤器的
/// 流（如 `DCTDecode`/`JPXDecode` 透传流）原样保留，不二次编码。
/// 空流不压缩。
pub struct StreamCompression {
    backend: Arc<dyn DeflateBackend>,
    predictor: ImagePredictor,
    filter: StreamFilter,
    /// 逐对象号过滤器覆盖（7.8 逐流粒度 opt-in）：`Some(Some(f))` 用 f 编码，
    /// `Some(None)` 跳过压缩（保持明文），缺省走全局 `filter`。
    overrides: HashMap<i64, Option<StreamFilter>>,
}

impl Clone for StreamCompression {
    fn clone(&self) -> Self {
        Self {
            backend: Arc::clone(&self.backend),
            predictor: self.predictor,
            filter: self.filter,
            overrides: self.overrides.clone(),
        }
    }
}

impl fmt::Debug for StreamCompression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamCompression")
            .field("filter", &self.filter)
            .field("predictor", &self.predictor)
            .field("backend", &"<dyn DeflateBackend>")
            .finish()
    }
}

impl StreamCompression {
    /// 默认 FlateDecode 压缩：`flate2` 后端、无预测器。
    pub fn flate() -> Self {
        Self {
            backend: Arc::new(Flate2DeflateBackend),
            predictor: ImagePredictor::None,
            filter: StreamFilter::Flate,
            overrides: HashMap::new(),
        }
    }

    /// ASCIIHexDecode 编码（PDF 1.0），纯 Rust 本地实现。
    pub fn ascii_hex() -> Self {
        Self {
            filter: StreamFilter::AsciiHex,
            ..Self::flate()
        }
    }

    /// ASCII85Decode 编码（PDF 1.0），Adobe `<~ … ~>` 约定。
    pub fn ascii85() -> Self {
        Self {
            filter: StreamFilter::Ascii85,
            ..Self::flate()
        }
    }

    /// RunLengthDecode 编码（PDF 1.0），纯 Rust 本地实现。
    pub fn run_length() -> Self {
        Self {
            filter: StreamFilter::RunLength,
            ..Self::flate()
        }
    }

    /// LZWDecode 编码（PDF 1.0，EarlyChange=1），纯 Rust 本地实现。
    pub fn lzw() -> Self {
        Self {
            filter: StreamFilter::Lzw,
            ..Self::flate()
        }
    }

    /// 使用调用方注入的压缩后端（替代默认 `flate2`，仍为 Flate 编码）。
    pub fn with_backend(backend: Arc<dyn DeflateBackend>) -> Self {
        Self {
            backend,
            predictor: ImagePredictor::None,
            filter: StreamFilter::Flate,
            overrides: HashMap::new(),
        }
    }

    /// 逐对象号设置过滤器（7.8 逐流粒度 opt-in）：`Some(filter)` 让该流用
    /// 指定过滤器编码（忽略全局策略），`None` 让该流保持明文不压缩。
    /// 后设覆盖先设；对象号在 `Object::num`，构建器写出前可查。
    pub fn with_object_filter(mut self, num: i64, filter: Option<StreamFilter>) -> Self {
        self.overrides.insert(num, filter);
        self
    }

    /// 该对象号是否被设为不压缩。
    pub(crate) fn object_skipped(&self, num: i64) -> bool {
        matches!(self.overrides.get(&num), Some(None))
    }

    /// 本策略使用的过滤器。
    pub fn filter(&self) -> StreamFilter {
        self.filter
    }

    /// 该对象号实际使用的过滤器（含逐流覆盖）。
    pub(crate) fn filter_for(&self, num: i64) -> StreamFilter {
        self.overrides
            .get(&num)
            .copied()
            .flatten()
            .unwrap_or(self.filter)
    }

    /// 该过滤器引入的最低 PDF 版本。
    pub(crate) fn min_version(&self) -> crate::core::types::PdfVersion {
        self.filter.min_version()
    }

    /// 设定图像预测器。
    pub fn with_image_predictor(mut self, predictor: ImagePredictor) -> Self {
        self.predictor = predictor;
        self
    }

    /// 是否用到了需要更高版本门控的选项（PNG 预测器，PDF 1.3+）。
    pub(crate) fn uses_image_predictor(&self) -> bool {
        !matches!(self.predictor, ImagePredictor::None | ImagePredictor::Tiff)
    }

    /// 把「未声明 Filter 的明文流」压缩为（新字典，压缩字节）。
    ///
    /// 返回 `Ok(None)` 表示保持原样（已有 `/Filter` 或 `/F`、空字节、
    /// 或压缩后反而更大）。后端失败报 [`WriteError::Compression`]。
    pub(crate) fn apply(
        &self,
        num: i64,
        dict: &Dict,
        plain: &[u8],
    ) -> Result<Option<(Dict, Vec<u8>)>, WriteError> {
        if plain.is_empty() {
            return Ok(None);
        }
        if self.object_skipped(num) {
            // 逐流 opt-out：该对象号保持明文。
            return Ok(None);
        }
        if dict.get("Filter").or_else(|| dict.get("F")).is_some() {
            // 已声明过滤器（JPX/DCT/BZip2 等透传流）：不二次编码。
            return Ok(None);
        }
        let filter = self.filter_for(num);
        let mut d = dict.clone();
        let payload: Vec<u8>;
        let mut source = plain;
        if let Some((parms, filtered)) = self.predictor_image_parms(dict, plain) {
            d.0.insert("DecodeParms".into(), Value::Dict(parms));
            payload = filtered;
            source = &payload;
        }
        let encoded = match filter {
            StreamFilter::Flate => self
                .backend
                .deflate(source)
                .map_err(|e| WriteError::Compression(format!("{e} (object dict: {d:?})")))?,
            StreamFilter::AsciiHex => ascii_hex_encode(source),
            StreamFilter::Ascii85 => ascii85_encode(source),
            StreamFilter::RunLength => run_length_encode(source),
            StreamFilter::Lzw => lzw_encode(source),
        };
        if matches!(filter, StreamFilter::Flate) && encoded.len() >= source.len() {
            // Flate 压缩不减体积（随机数据）：保持明文，避免无收益膨胀。
            // 无损/ASCII 过滤器不做此判断——它们的编码目标是可解码表示，
            // 即使字节更多也必须写出（否则 /Filter 与载荷语义不符）。
            return Ok(None);
        }
        d.0.insert("Filter".into(), Value::Name(filter.name().into()));
        d.0.insert("Length".into(), Value::Integer(encoded.len() as i64));
        Ok(Some((d, encoded)))
    }

    /// 图像预测器前置：`/Subtype /Image` 且几何可判定（8-bit、设备色
    /// 命名空间、样本长度与行几何一致）时做 PNG Up 逐行差分（每行前缀
    /// 过滤字节 2），并给出与之配套的 `/DecodeParms`。
    fn predictor_image_parms(&self, dict: &Dict, plain: &[u8]) -> Option<(Dict, Vec<u8>)> {
        if self.predictor == ImagePredictor::None {
            return None;
        }
        if dict.get("Subtype").and_then(Value::as_name) != Some("Image") {
            return None;
        }
        if dict
            .get("ImageMask")
            .is_some_and(|v| matches!(v, Value::Bool(true)))
        {
            return None;
        }
        let bits = dict.get("BitsPerComponent").and_then(Value::as_integer)?;
        let tiff = self.predictor == ImagePredictor::Tiff;
        if !tiff && bits != 8 {
            return None;
        }
        if tiff && (bits <= 0 || bits % 8 != 0) {
            return None;
        }
        let colors = match dict.get("ColorSpace").and_then(Value::as_name) {
            Some("DeviceGray") => 1usize,
            Some("DeviceRGB") => 3,
            Some("DeviceCMYK") => 4,
            _ => return None,
        };
        let width = dict.get("Width").and_then(Value::as_integer)?;
        if width <= 0 {
            return None;
        }
        let row_bytes = (width as usize * colors * bits as usize).div_ceil(8);
        if row_bytes == 0 || !plain.len().is_multiple_of(row_bytes) {
            return None;
        }
        let predictor_value = match self.predictor {
            ImagePredictor::None => 1,
            ImagePredictor::Tiff => 2,
            ImagePredictor::PngFixed(filter) => 10 + filter.tag() as i64,
            ImagePredictor::PngUp | ImagePredictor::PngOptimum => 15,
        };
        let parms = Dict::new()
            .insert("Predictor", Value::Integer(predictor_value))
            .insert("Colors", Value::Integer(colors as i64))
            .insert("BitsPerComponent", Value::Integer(bits))
            .insert("Columns", Value::Integer(width));
        let encoded = match self.predictor {
            ImagePredictor::Tiff => tiff_encode(plain, row_bytes, colors * bits as usize / 8),
            ImagePredictor::PngFixed(filter) => png_fixed_encode(plain, row_bytes, filter, colors),
            ImagePredictor::PngUp => png_up_encode(plain, row_bytes),
            ImagePredictor::PngOptimum => png_optimum_encode(plain, row_bytes, colors),
            ImagePredictor::None => unreachable!(),
        };
        Some((parms, encoded))
    }
}

/// ASCIIHexDecode 编码（7.4.2）：每字节两个小写十六进制数字，末尾 `>` EOD。
fn ascii_hex_encode(data: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(data.len() * 2 + 1);
    for &byte in data {
        out.push(HEX[(byte >> 4) as usize]);
        out.push(HEX[(byte & 0x0f) as usize]);
    }
    out.push(b'>');
    out
}

/// ASCII85Decode 编码（7.4.3，Adobe `<~ … ~>` 约定）：4 字节一组 → 5 个
/// base-85 字符；全零组写 `z`；末组不足 4 字节按 0 补齐后截去多余字符。
fn ascii85_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 5 / 4 + 2);
    out.extend_from_slice(b"<~");
    for chunk in data.chunks(4) {
        if chunk.iter().all(|&b| b == 0) {
            out.push(b'z');
            continue;
        }
        let mut value = 0u32;
        for (i, &byte) in chunk.iter().enumerate() {
            value |= (byte as u32) << (24 - 8 * i);
        }
        let mut digits = [0u8; 5];
        for digit in &mut digits {
            *digit = b'!' + (value % 85) as u8;
            value /= 85;
        }
        digits.reverse(); // 高位在前：`digits[0]` 是最显著的 base-85 位。
        let keep = chunk.len() + 1;
        out.extend_from_slice(&digits[..keep]);
    }
    out.extend_from_slice(b"~>");
    out
}

/// RunLengthDecode 编码（7.4.5）：重复块 `(257-n, byte)`（2≤n≤128）；
/// 字面量块 `(n-1, 数据…)`（1≤n≤128）；末尾 `128` EOD。
fn run_length_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 8 + 1);
    let mut i = 0;
    let mut literals: Vec<u8> = Vec::with_capacity(128);
    let flush = |out: &mut Vec<u8>, literals: &mut Vec<u8>| {
        while !literals.is_empty() {
            let n = literals.len().min(128);
            out.push((n - 1) as u8);
            out.extend_from_slice(&literals[..n]);
            literals.drain(..n);
        }
    };
    while i < data.len() {
        let mut run = 1usize;
        while i + run < data.len() && data[i + run] == data[i] && run < 128 {
            run += 1;
        }
        if run >= 2 {
            flush(&mut out, &mut literals);
            out.push((257 - run) as u8);
            out.push(data[i]);
            i += run;
        } else {
            literals.push(data[i]);
            i += 1;
            if literals.len() == 128 {
                flush(&mut out, &mut literals);
            }
        }
    }
    flush(&mut out, &mut literals);
    out.push(128);
    out
}

/// 位写出器（MSB-first，按 `width` 位追加，末字节补零）。
struct BitWriter {
    bytes: Vec<u8>,
    acc: u32,
    bits: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            acc: 0,
            bits: 0,
        }
    }

    fn push(&mut self, value: u16, width: u8) {
        self.acc = (self.acc << width) | value as u32;
        self.bits += width;
        while self.bits >= 8 {
            self.bits -= 8;
            self.bytes.push((self.acc >> self.bits) as u8);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            self.bytes.push((self.acc << (8 - self.bits)) as u8);
        }
        self.bytes
    }
}

/// LZWDecode 编码（7.4.4，EarlyChange=1，9–12 位，Clear=256，EOD=257）。
fn lzw_encode(data: &[u8]) -> Vec<u8> {
    use std::collections::HashMap;

    const CLEAR: u16 = 256;
    const EOD: u16 = 257;
    let mut bits = BitWriter::new();
    let mut table: HashMap<Vec<u8>, u16> = HashMap::new();
    let reset = |table: &mut HashMap<Vec<u8>, u16>| {
        table.clear();
        for byte in 0..=255u8 {
            table.insert(vec![byte], byte as u16);
        }
    };
    reset(&mut table);
    let mut next = 258u16;
    let mut width = 9u8;
    bits.push(CLEAR, width);
    let mut w: Vec<u8> = Vec::new();
    for &byte in data {
        w.push(byte);
        if table.contains_key(&w) {
            continue;
        }
        let code = table[&w[..w.len() - 1]];
        bits.push(code, width);
        if next < 4096 {
            table.insert(w.clone(), next);
            next += 1;
            // EarlyChange=1：下一个待分配码 ≥ 2^width + 1 时升宽。
            if next + 1 > (1u16 << width) && width < 12 {
                width += 1;
            }
        } else {
            bits.push(CLEAR, width);
            reset(&mut table);
            next = 258;
            width = 9;
        }
        w.clear();
        w.push(byte);
    }
    if !w.is_empty() {
        let code = table[&w];
        bits.push(code, width);
    }
    bits.push(EOD, width);
    bits.finish()
}

/// PNG Up 过滤编码：每行前置过滤字节 `2`，行内 `enc[i] = raw[i] - prev[i]`
/// （首行 prev 视为全 0），按字节模 256 回绕。与读端 Predictor 15 解码
/// （`filters::predictor::png_predictor`）互逆。
fn png_up_encode(raw: &[u8], row_bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / row_bytes);
    let zero_row = vec![0u8; row_bytes];
    let mut prev: &[u8] = &zero_row;
    for cur in raw.chunks(row_bytes) {
        out.push(2u8);
        for (c, p) in cur.iter().zip(prev) {
            out.push(c.wrapping_sub(*p));
        }
        prev = cur;
    }
    out
}

/// TIFF Predictor 2：每采样值减去其左侧相邻采样值（`bpp` 为样本字节数，
/// 即 `Colors × BitsPerComponent / 8`），按字节模 256 回绕。
fn tiff_encode(raw: &[u8], row_bytes: usize, bpp: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    for cur in raw.chunks(row_bytes) {
        for (i, &byte) in cur.iter().enumerate() {
            let left = if i >= bpp { cur[i - bpp] } else { 0 };
            out.push(byte.wrapping_sub(left));
        }
    }
    out
}

/// 固定 PNG 过滤（Predictor 10–14）：无逐行过滤字节，全行用同一过滤器。
fn png_fixed_encode(raw: &[u8], row_bytes: usize, filter: PngFilter, colors: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let zero_row = vec![0u8; row_bytes];
    let mut prev: &[u8] = &zero_row;
    for cur in raw.chunks(row_bytes) {
        out.extend_from_slice(&filter.encode_row(cur, prev, colors));
        prev = cur;
    }
    out
}

/// 自适应 PNG 过滤（Predictor 15）：每行挑字节代价最小的过滤器并写其标记。
fn png_optimum_encode(raw: &[u8], row_bytes: usize, colors: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / row_bytes);
    let zero_row = vec![0u8; row_bytes];
    let mut prev: &[u8] = &zero_row;
    for cur in raw.chunks(row_bytes) {
        let mut best = PngFilter::None;
        let mut best_cost = usize::MAX;
        for filter in [
            PngFilter::None,
            PngFilter::Sub,
            PngFilter::Up,
            PngFilter::Average,
            PngFilter::Paeth,
        ] {
            let cost = filter.row_cost(cur, prev, colors);
            if cost < best_cost {
                best_cost = cost;
                best = filter;
            }
        }
        out.push(best.tag());
        out.extend_from_slice(&best.encode_row(cur, prev, colors));
        prev = cur;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::filters::decode_stream;
    use crate::core::types::PdfVersion;
    use crate::core::writer::WriteError;

    fn dict() -> Dict {
        Dict::new()
    }

    fn image_dict(width: i64, colors: &str) -> Dict {
        Dict::new()
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(width))
            .insert("Height", Value::Integer(2))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("ColorSpace", Value::Name(colors.into()))
    }

    /// 默认后端压缩输出可被读端 `FlateDecode` 过滤器解回原字节。
    #[test]
    fn default_backend_output_decodes_through_reader_filter() {
        let plain: Vec<u8> = b"aaaaaaaaaa".repeat(40);
        let comp = StreamCompression::flate();
        let (d, bytes) = comp
            .apply(1, &dict(), &plain)
            .expect("apply")
            .expect("should compress");
        assert_eq!(
            d.get("Filter").and_then(Value::as_name),
            Some("FlateDecode")
        );
        assert_eq!(
            d.get("Length").and_then(Value::as_integer),
            Some(bytes.len() as i64)
        );
        assert_eq!(decode_stream(&d, &bytes).expect("decode"), plain);
    }

    /// 空流与已声明 `/Filter` 的流（如 JPX 透传）保持原样。
    #[test]
    fn empty_and_filtered_streams_are_left_untouched() {
        let comp = StreamCompression::flate();
        assert!(
            comp.apply(1, &dict(), b"").expect("apply").is_none(),
            "empty stream must stay plain"
        );
        let jpx = dict().insert("Filter", Value::Name("JPXDecode".into()));
        assert!(
            comp.apply(1, &jpx, b"abc").expect("apply").is_none(),
            "declared filter must not be recompressed"
        );
    }

    /// 压缩不减体积时保持明文（无收益不膨胀），且不残留 /DecodeParms。
    #[test]
    fn inflating_backend_result_keeps_stream_plain() {
        struct AlwaysBigger;
        impl DeflateBackend for AlwaysBigger {
            fn deflate(&self, data: &[u8]) -> Result<Vec<u8>, String> {
                let mut out = data.to_vec();
                out.push(0);
                Ok(out)
            }
        }
        let comp = StreamCompression::with_backend(Arc::new(AlwaysBigger))
            .with_image_predictor(ImagePredictor::PngUp);
        // 图像几何满足预测器条件，但压缩不减体积 → 整体保持明文。
        let samples: Vec<u8> = (0..12).collect();
        assert!(
            comp.apply(1, &image_dict(2, "DeviceRGB"), &samples)
                .expect("apply")
                .is_none(),
            "non-shrinking compression must keep the stream plain"
        );
    }

    /// PNG Up（Predictor 15）图像预测器编码可被读端过滤器解回原样本：
    /// 行间差分使平滑图像显著缩小。
    #[test]
    fn png_up_image_predictor_roundtrips_through_reader() {
        let width = 16i64;
        let row_bytes = width as usize * 3;
        let samples: Vec<u8> = (0..4u8)
            .flat_map(|row| std::iter::repeat_n(row.wrapping_mul(10), row_bytes))
            .collect();
        let comp = StreamCompression::flate().with_image_predictor(ImagePredictor::PngUp);
        let (d, bytes) = comp
            .apply(1, &image_dict(width, "DeviceRGB"), &samples)
            .expect("apply")
            .expect("should compress");
        let parms = d
            .get("DecodeParms")
            .and_then(Value::as_dict)
            .expect("parms");
        assert_eq!(parms.get("Predictor").and_then(Value::as_integer), Some(15));
        assert_eq!(parms.get("Colors").and_then(Value::as_integer), Some(3));
        assert_eq!(
            parms.get("Columns").and_then(Value::as_integer),
            Some(width)
        );
        assert_eq!(decode_stream(&d, &bytes).expect("decode"), samples);
    }

    /// 颜色空间不可判定（数组形式，如 Indexed）时跳过预测器、只做纯 Flate。
    #[test]
    fn predictor_skipped_when_colorspace_not_named_device() {
        let dict = Dict::new()
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(2))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert(
                "ColorSpace",
                Value::Array(crate::core::types::Array(vec![
                    Value::Name("Indexed".into()),
                    Value::Integer(1),
                ])),
            );
        let plain: Vec<u8> = b"0123456789".repeat(40);
        let comp = StreamCompression::flate().with_image_predictor(ImagePredictor::PngUp);
        let (d, bytes) = comp.apply(1, &dict, &plain).expect("apply").expect("flate");
        assert!(d.get("DecodeParms").is_none(), "no predictor for Indexed");
        assert_eq!(decode_stream(&d, &bytes).expect("decode"), plain);
    }

    /// 注入后端失败时报类型化 `WriteError::Compression`。
    #[test]
    fn failing_backend_reports_compression_write_error() {
        struct Broken;
        impl DeflateBackend for Broken {
            fn deflate(&self, _data: &[u8]) -> Result<Vec<u8>, String> {
                Err("backend exploded".into())
            }
        }
        let comp = StreamCompression::with_backend(Arc::new(Broken));
        let error = comp
            .apply(1, &dict(), b"hello hello hello")
            .expect_err("backend failure must surface");
        assert!(
            matches!(&error, WriteError::Compression(_)),
            "expected Compression: {error}"
        );
        assert!(error.to_string().contains("backend exploded"), "{error}");
    }

    /// 四种本地过滤器写出后能由读端 `decode_stream` 无损还原。
    #[test]
    fn local_filter_encodings_roundtrip_through_reader() {
        let samples: Vec<Vec<u8>> = vec![
            "ASCIIHex 编码的明文流，含二进制 \x00\x01\u{00ff} 字节"
                .as_bytes()
                .to_vec(),
            (0..=255u8).collect(),
            b"aaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbc short mixed runs and literals".to_vec(),
            Vec::new(),
        ];
        for comp in [
            StreamCompression::ascii_hex(),
            StreamCompression::ascii85(),
            StreamCompression::run_length(),
            StreamCompression::lzw(),
        ] {
            for plain in &samples {
                let result = comp.apply(1, &dict(), plain).expect("apply");
                if plain.is_empty() {
                    assert!(result.is_none(), "empty streams stay plain");
                    continue;
                }
                let (d, encoded) = result.expect("encoded");
                assert_eq!(
                    d.get("Filter").and_then(Value::as_name),
                    Some(comp.filter().name())
                );
                assert_eq!(
                    decode_stream(&d, &encoded).expect("decode"),
                    *plain,
                    "{} roundtrip",
                    comp.filter().name()
                );
            }
        }
    }

    /// TIFF（Predictor 2）与四种固定 PNG 过滤（10–14）经读端无损还原。
    #[test]
    fn tiff_and_fixed_png_predictors_roundtrip() {
        let width = 16i64;
        let samples: Vec<u8> = (0..40u8)
            .flat_map(|row| (0..width as u8).map(move |col| row.wrapping_mul(20).wrapping_add(col)))
            .collect();
        let cases = [
            (ImagePredictor::Tiff, 2),
            (ImagePredictor::PngFixed(PngFilter::None), 10),
            (ImagePredictor::PngFixed(PngFilter::Sub), 11),
            (ImagePredictor::PngFixed(PngFilter::Up), 12),
            (ImagePredictor::PngFixed(PngFilter::Average), 13),
            (ImagePredictor::PngFixed(PngFilter::Paeth), 14),
        ];
        for (predictor, expected) in cases {
            let comp = StreamCompression::flate().with_image_predictor(predictor);
            let (d, bytes) = comp
                .apply(1, &image_dict(width, "DeviceGray"), &samples)
                .expect("apply")
                .expect("encoded");
            assert_eq!(
                d.get("DecodeParms")
                    .and_then(Value::as_dict)
                    .and_then(|p| p.get("Predictor"))
                    .and_then(Value::as_integer),
                Some(expected),
                "predictor {predictor:?}"
            );
            assert_eq!(
                decode_stream(&d, &bytes).expect("decode"),
                samples,
                "predictor {predictor:?} roundtrip"
            );
        }
    }

    /// Predictor 15 自适应逐行过滤经读端无损还原。
    #[test]
    fn png_optimum_predictor_roundtrips() {
        let width = 18i64;
        let row_bytes = width as usize * 3; // DeviceRGB
        let samples: Vec<u8> = (0..40u8)
            .flat_map(|row| {
                (0..row_bytes)
                    .map(move |col| (row as u16 * 7 + col as u16 * 31).wrapping_mul(3) as u8)
            })
            .collect();
        let comp = StreamCompression::flate().with_image_predictor(ImagePredictor::PngOptimum);
        let (d, bytes) = comp
            .apply(1, &image_dict(width, "DeviceRGB"), &samples)
            .expect("apply")
            .expect("encoded");
        assert_eq!(
            d.get("DecodeParms")
                .and_then(Value::as_dict)
                .and_then(|p| p.get("Predictor"))
                .and_then(Value::as_integer),
            Some(15)
        );
        assert_eq!(decode_stream(&d, &bytes).expect("decode"), samples);
    }

    /// 各过滤器引入版本：Flate 1.2，其余 1.0。
    #[test]
    fn stream_filter_min_versions_match_spec() {
        assert_eq!(StreamCompression::flate().min_version(), PdfVersion::V1_2);
        assert_eq!(
            StreamCompression::ascii_hex().min_version(),
            PdfVersion::V1_0
        );
        assert_eq!(StreamCompression::ascii85().min_version(), PdfVersion::V1_0);
        assert_eq!(
            StreamCompression::run_length().min_version(),
            PdfVersion::V1_0
        );
        assert_eq!(StreamCompression::lzw().min_version(), PdfVersion::V1_0);
    }
}
