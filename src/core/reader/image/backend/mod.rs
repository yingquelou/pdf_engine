//! JBIG2 / JPEG 2000 的可替换图像解码后端**契约**（阶段 D3 第一环）。
//!
//! 本模块只定义「后端接入位」：trait、请求/输出形状、注册与分发通路、
//! 契约层版本门禁与损坏输入的错误形态。按 ROADMAP 边界**不引入任何第三方
//! 依赖、不含任何真实 JBIG2/JPEG 2000 解码**：默认注册表为空
//! （[`ImageBackends::NONE`]），`ImageModel::samples` 的 `Unsupported`
//! 行为原样保留。
//!
//! # 与 `StreamFilter` 的接缝位置
//!
//! 后端契约位于过滤链**之后**、图像模型**之侧**：`StreamFilter::decode`
//! 只返回 `Vec<u8>` 流字节，无法携带通道数/位深等像素元信息，且图像读端
//! 已用 `filters::decode_stream_except` 把 JBIG2/JPX 步骤原样透传（语法层
//! 解析依赖原始码流）。因此真实后端以 [`Jbig2Backend`]/[`JpxBackend`]
//! 适配器接入、由 [`ImageModel::decode_image_with`] 分发，而不是伪装成
//! 一个 `StreamFilter`；这沿用 `flate`/`jpeg` 模块内部后端适配器
//! （`InflateBackend`/`JpegBackend`）「trait 只暴露 PDF 语义、不外泄
//! 第三方类型」的先例。后续若需要，可在 `filters` 侧包一个把
//! `DecodedBitmap` 展平成 `Vec<u8>` 的 `StreamFilter` 薄适配器，契约不变。
//!
//! # 分发语义（对齐 `decode_stream_with` 风格）
//!
//! 后端集合由调用方**显式传入**（无全局可变状态、无单例、无 unsafe）；
//! 集合中第一个成功产出位图的后端生效；返回 `ImageError::Unsupported`
//! 的后端视为「能力不覆盖」，分发层继续尝试下一个；返回
//! `ImageError::Filter` 视为码流损坏，立即透传、不再回退。全部后端
//! 缺失或不覆盖时返回类型化 `Unsupported`（与现有 `samples()` 口径一致）。
//!
//! # 版本门禁（ISO 32000-1 依据，`pdftotext -raw docs/PDF32000_2008.pdf -` 行号）
//!
//! - Table 9（标准过滤器一览）：raw 行 1236 `JBIG2Decode yes (PDF 1.4)`、
//!   行 1243 `JPXDecode no (PDF 1.5)`；
//! - §7.4.7 JBIG2Decode：raw 行 1607 「The JBIG2Decode filter (PDF 1.4)
//!   decodes monochrome (1 bit per pixel) image data…」；
//! - §7.4.9 JPXDecode：raw 行 1810 「The JPXDecode filter (PDF 1.5)
//!   decodes data that has been encoded using the JPEG2000 compression…」。
//!
//! 门禁常量直接复用既有 `PdfFeature::JBIG2Decode`（min 1.4）与
//! `PdfFeature::JPXDecode`（min 1.5），经 `PdfVersion::supports` 判定，
//! 不在本环新增特性常量。
//!
//! # 后端选型结论（D3 前置 spike）
//!
//! - **候选一：纯 Rust crate**（JPX 侧如 `jpeg2k`，JBIG2 侧仅有实验性
//!   实现；具体 crate 版本、许可证与维护活跃度需主控在使用时核实）。
//!   优点：与 `flate2`/`zune-jpeg` 经适配器接入的先例一致、进程内可测
//!   （无 fixture 也能用单元测验证契约与错误形态）、无运行期外部依赖。
//! - **候选二：动态加载 C 库**（OpenJPEG＝BSD-2-Clause 的 JPEG 2000
//!   参考实现；jbig2dec＝GPL-2.0-or-later，许可证兼容性须先评估）。
//!   优点：格式语义成熟（JBIG2 符号字典/区域合成、JP2 框树与 ICC 剖面
//!   都已被实战解码器覆盖）；缺点：分发面引入运行期库查找与「缺库降级」
//!   路径，无 CI 环境时不可测。
//! - **候选三：自研最小解码器**。否决：JBIG2 符号字典 + 细化近乎
//!   Turing-complete，JPEG 2000 小波/MQR 熵解码工作量大且易错，与本库
//!   「不解析第三方二进制」的立场直接冲突。
//! - **「不解析第三方二进制」边界如何兼容**：该边界约束的是*本库核心*
//!   不实现格式解码；默认构建下 [`ImageBackends::NONE`] 为空、行为与
//!   现状逐字一致，真实解码只发生在调用方显式注册的适配器内，trait 签名
//!   不出现任何第三方类型（`DecodeError`/`ImageError` 均本库所有）。
//! - **推荐**：JPX 优先纯 Rust 后端（经 `JpxBackend` 适配器、feature
//!   门控、接入前核实活跃度），OpenJPEG 动态加载作降级备选；JBIG2 因
//!   纯 Rust 生态不成熟、语义复杂，推荐 jbig2dec（或许可证不允许 GPL
//!   时的商业实现）动态加载，同样只经 `Jbig2Backend` 适配器接入。两者
//!   均不默认链接、不进 `Cargo.toml`，注册入口即本模块契约。

use crate::core::types::{PdfFeature, PdfVersion};

use super::error::{ImageError, Result, invalid, unsupported};
use super::jbig2::{Jbig2PageInfo, Jbig2Stream, is_jbig2_filter};
use super::jpx::{JpxHeader, is_jpx_filter};
use super::model::ImageModel;

/// 契约覆盖的像素解码域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageCodec {
    /// `/JBIG2Decode`（§7.4.7，PDF 1.4）。
    Jbig2,
    /// `/JPXDecode`（§7.4.9，PDF 1.5）。
    Jpx,
}

impl ImageCodec {
    /// 规范过滤器全名（`JBIG2Decode` / `JPXDecode`）。
    pub fn filter_name(self) -> &'static str {
        match self {
            Self::Jbig2 => "JBIG2Decode",
            Self::Jpx => "JPXDecode",
        }
    }

    /// 对应特性常量：min_version 即规范版本门（1.4 / 1.5，依据见模块文档）。
    pub fn feature(self) -> PdfFeature {
        match self {
            Self::Jbig2 => PdfFeature::JBIG2Decode,
            Self::Jpx => PdfFeature::JPXDecode,
        }
    }

    /// 该解码域的最低 PDF 版本（经 `PdfFeature::min_version` 唯一事实源）。
    pub fn min_pdf_version(self) -> PdfVersion {
        self.feature().min_version()
    }
}

/// 后端输出的像素位图契约：行优先、每行按位紧排（与
/// `ColorSpaceModel::decode_samples` 的行规则一致）。
///
/// `samples` 长度必须等于 `row_bytes() × height`；分发层在进入调用方前
/// 经 [`DecodedBitmap::contract_violation`] 校验，违约归一化为
/// `ImageError::Invalid`，后端交回的脏数据不会以「成功」形态扩散。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedBitmap {
    pub width: u32,
    pub height: u32,
    /// 每像素分量数（JBIG2 恒 1；JPX 按码流分量数）。
    pub components: u8,
    /// 每分量位数（1/2/4/8/16）。
    pub bits_per_component: u8,
    pub samples: Vec<u8>,
}

impl DecodedBitmap {
    /// 每行字节数：`ceil(width × components × bpc / 8)`。
    pub fn row_bytes(&self) -> usize {
        (self.width as usize)
            .saturating_mul(self.components as usize)
            .saturating_mul(self.bits_per_component.max(1) as usize)
            .div_ceil(8)
    }

    /// 契约违约说明；`None` 表示形状合法。
    pub fn contract_violation(&self) -> Option<String> {
        if self.width == 0 || self.height == 0 {
            return Some(format!("zero dimension {}x{}", self.width, self.height));
        }
        if self.components == 0 || !matches!(self.bits_per_component, 1 | 2 | 4 | 8 | 16) {
            return Some(format!(
                "invalid layout {} component(s) x {} bits per component",
                self.components, self.bits_per_component
            ));
        }
        let row_bytes = self.row_bytes();
        let expected = row_bytes.checked_mul(self.height as usize);
        match expected {
            None => Some("bitmap size overflows usize".to_owned()),
            Some(expected) if self.samples.len() != expected => Some(format!(
                "sample length {} does not match expected {expected} bytes for \
                 {}x{} at {} component(s) x {} bits",
                self.samples.len(),
                self.width,
                self.height,
                self.components,
                self.bits_per_component
            )),
            Some(_) => None,
        }
    }
}

/// JBIG2 后端输入：`/Filter` 链其余步骤解码后、JBIG2 步骤原样透传的
/// 段流字节，加上语法层模型能给出的最小必要信息。
#[derive(Debug)]
pub struct Jbig2DecodeRequest<'a> {
    /// 图像流内的 JBIG2 段链原始字节（§7.4.7：PDF 中不应含文件头段）。
    pub codestream: &'a [u8],
    /// `/JBIG2Globals` 全局段流的已解析段链（缺失为 `None`）。
    pub globals: Option<&'a Jbig2Stream>,
    /// 图像流页面信息段（T.88 §7.4.8）几何；未解析到时为 `None`，
    /// 后端须自行从码流读取。
    pub page_information: Option<&'a Jbig2PageInfo>,
    /// 调用方注入的文档声明版本；`None` 表示门禁交由上层判断。
    pub pdf_version: Option<PdfVersion>,
}

/// JPX 后端输入：过滤链透传的 JP2 码流字节 + 语法层头部解析结果。
#[derive(Debug)]
pub struct JpxDecodeRequest<'a> {
    /// JP2 boxed 文件或裸码流的原始字节。
    pub codestream: &'a [u8],
    /// [`super::parse_jpx_header`] 的头部结果（几何/分量/颜色空间提示）；
    /// 载荷畸形时为 `None`，后端自行解析。
    pub header: Option<&'a JpxHeader>,
    /// 调用方注入的文档声明版本；`None` 表示门禁交由上层判断。
    pub pdf_version: Option<PdfVersion>,
}

/// JBIG2 像素解码后端。
///
/// 错误形态契约（与本库既有语义一一对应，不新增错误枚举）：
/// - 码流损坏/截断无法恢复 → `ImageError::Filter`（分发层立即透传）；
/// - 后端能力不覆盖（如彩色 JBIG2 页、符号字典细化组合）→
///   `ImageError::Unsupported`（分发层尝试下一个后端）；
/// - 输出形状必须满足 [`DecodedBitmap`] 契约，违约由分发层归一化为
///   `ImageError::Invalid`。
pub trait Jbig2Backend: Send + Sync {
    /// 后端名（诊断用，如 `jbig2dec-dynamic`）。
    fn name(&self) -> &'static str;

    /// 把 JBIG2 段链解码为位图；绝不 panic，畸形输入按上述契约报错。
    fn decode(&self, request: &Jbig2DecodeRequest<'_>) -> Result<DecodedBitmap>;
}

/// JPEG 2000（JPX）像素解码后端；错误形态契约同 [`Jbig2Backend`]。
pub trait JpxBackend: Send + Sync {
    /// 后端名（诊断用，如 `jpeg2k-rust`）。
    fn name(&self) -> &'static str;

    /// 把 JP2 码流解码为位图；绝不 panic，畸形输入按 [`Jbig2Backend`]
    /// 的错误契约报错。
    fn decode(&self, request: &JpxDecodeRequest<'_>) -> Result<DecodedBitmap>;
}

/// 显式传入的后端集合（对齐 `builtin_filters()`/`decode_stream_with()`
/// 的「调用方持有集合」风格；本库不维护全局注册状态）。
#[derive(Clone, Copy)]
pub struct ImageBackends<'a> {
    jbig2: &'a [&'a dyn Jbig2Backend],
    jpx: &'a [&'a dyn JpxBackend],
}

impl ImageBackends<'_> {
    /// 空注册表：本库默认形态。任何图像都不会被送入后端，
    /// `Unsupported` 行为与现状逐字一致。
    pub const NONE: Self = Self {
        jbig2: &[],
        jpx: &[],
    };
}

impl<'a> ImageBackends<'a> {
    /// 用显式集合构造。集合内第一个成功产出位图的后端生效。
    pub const fn new(jbig2: &'a [&'a dyn Jbig2Backend], jpx: &'a [&'a dyn JpxBackend]) -> Self {
        Self { jbig2, jpx }
    }

    pub fn jbig2(&self) -> &'a [&'a dyn Jbig2Backend] {
        self.jbig2
    }

    pub fn jpx(&self) -> &'a [&'a dyn JpxBackend] {
        self.jpx
    }
}

/// 内建图像解码后端集合。
///
/// 与 [`crate::core::filters::builtin_filters`] 对称的注册入口，但内建
/// 集合恒为 [`ImageBackends::NONE`]：本库按既定边界不解析第三方二进制。
/// 调用方经 [`ImageBackends::new`] 显式注册后端即可覆盖该默认。
pub fn builtin_image_backends() -> &'static ImageBackends<'static> {
    &ImageBackends::NONE
}

/// 契约层版本门禁：声明版本低于规范门（JBIG2 1.4 / JPX 1.5）时拒绝。
///
/// 未注入版本（`None`）时放行——`ImageModel` 不携带文档版本，判定权
/// 留给持有 `Document` 的调用层。
pub fn check_codec_version(codec: ImageCodec, version: Option<PdfVersion>) -> Result<()> {
    if let Some(version) = version
        && !version.supports(codec.feature())
    {
        return Err(unsupported(format!(
            "/{} requires PDF {} but the declared document version is {}",
            codec.filter_name(),
            codec.min_pdf_version().as_str(),
            version.as_str()
        )));
    }
    Ok(())
}

/// 用指定 JBIG2 后端集合解码。分发语义见模块文档。
pub fn decode_jbig2_with(
    backends: &[&dyn Jbig2Backend],
    request: &Jbig2DecodeRequest<'_>,
) -> Result<DecodedBitmap> {
    check_codec_version(ImageCodec::Jbig2, request.pdf_version)?;
    let mut last_unsupported: Option<ImageError> = None;
    for backend in backends {
        match backend.decode(request) {
            Ok(bitmap) => return accept_output(backend.name(), bitmap),
            Err(error @ ImageError::Unsupported(_)) => last_unsupported = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_unsupported.unwrap_or_else(|| {
        unsupported(
            "no JBIG2Decode pixel backend registered; this crate parses the JBIG2 segment \
             syntax only and does not decode pixels",
        )
    }))
}

/// 用指定 JPX 后端集合解码。分发语义见模块文档。
pub fn decode_jpx_with(
    backends: &[&dyn JpxBackend],
    request: &JpxDecodeRequest<'_>,
) -> Result<DecodedBitmap> {
    check_codec_version(ImageCodec::Jpx, request.pdf_version)?;
    let mut last_unsupported: Option<ImageError> = None;
    for backend in backends {
        match backend.decode(request) {
            Ok(bitmap) => return accept_output(backend.name(), bitmap),
            Err(error @ ImageError::Unsupported(_)) => last_unsupported = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_unsupported.unwrap_or_else(|| {
        unsupported(
            "no JPXDecode pixel backend registered; this crate parses the JPX header syntax \
             only and does not decode pixels",
        )
    }))
}

fn accept_output(name: &str, bitmap: DecodedBitmap) -> Result<DecodedBitmap> {
    match bitmap.contract_violation() {
        Some(message) => Err(invalid(format!(
            "backend `{name}` output violates the DecodedBitmap contract: {message}"
        ))),
        None => Ok(bitmap),
    }
}

impl ImageModel {
    /// 把 JBIG2/JPX 图像数据分发到显式注册的后端集合（本环打通「谁能
    /// 被调用」，不含真实解码）。
    ///
    /// - 未注册任何后端（[`ImageBackends::NONE`]）：返回类型化
    ///   `Unsupported`，口径与 [`ImageModel::samples`] 的既有行为一致；
    /// - 非 JBIG2/JPX 图像：返回 `Unsupported` 并提示改用 `samples()`；
    /// - [`ImageModel::samples`] 本身**不受影响**，现有行为零变化。
    ///
    /// 便捷路径不注入文档版本（`pdf_version: None`）；需要版本门禁的
    /// 调用方改用
    /// [`Document::decode_image_with`](crate::core::types::Document::decode_image_with)
    /// （由 `Document` 注入 `source_version()`），或自行构造请求并调用
    /// [`decode_jbig2_with`]/[`decode_jpx_with`]。
    /// 同时含两种过滤器的病态字典按 JPX 优先分发。
    pub fn decode_image_with(&self, backends: &ImageBackends<'_>) -> Result<DecodedBitmap> {
        self.decode_image_at(backends, None)
    }

    /// 带文档声明版本的内部分发：与 [`ImageModel::decode_image_with`]
    /// 逐路径一致，仅把 `pdf_version` 注入请求供版本门禁判定。便捷路径
    /// 恒传 `None`（行为不变），
    /// [`Document::decode_image_with`](crate::core::types::Document::decode_image_with)
    /// 传入 `self.source_version()` 使 [`check_codec_version`] 端到端生效。
    pub(super) fn decode_image_at(
        &self,
        backends: &ImageBackends<'_>,
        pdf_version: Option<PdfVersion>,
    ) -> Result<DecodedBitmap> {
        if is_jpx_filter(&self.filters) {
            let request = JpxDecodeRequest {
                codestream: &self.data,
                header: self.jpx.as_ref(),
                pdf_version,
            };
            decode_jpx_with(backends.jpx(), &request)
        } else if is_jbig2_filter(&self.filters) {
            let request = Jbig2DecodeRequest {
                codestream: &self.data,
                globals: self.jbig2.as_ref().and_then(|info| info.globals.as_ref()),
                page_information: self
                    .jbig2
                    .as_ref()
                    .and_then(|info| info.page.page_information.as_ref()),
                pdf_version,
            };
            decode_jbig2_with(backends.jbig2(), &request)
        } else {
            Err(unsupported(
                "image does not use JBIG2Decode/JPXDecode; read its samples via \
                 ImageModel::samples",
            ))
        }
    }
}

mod document_entry;
#[cfg(test)]
mod document_entry_tests;

#[cfg(test)]
mod tests;
