//! 位图后端输出展平为 `Vec<u8>` 的薄适配器（§7.4.7 / §7.4.9 的过滤器层接缝）。
//!
//! 后端契约模块（`reader::image::backend`）文档指定的接缝落地点：把
//! [`DecodedBitmap`](crate::core::reader::image::DecodedBitmap) 展平成
//! `Vec<u8>`，契约不变。本模块只做三件事：
//!
//! 1. **请求构造**：`/Filter` 链中位图步骤（`JBIG2Decode`/`JPXDecode`，
//!    含旧名 `JBIG2`/`JPX`）之前的其余步骤照常用内建过滤器解码，位图
//!    步骤原样透传（与图像读端 `decode_stream_except` 同一口径）；
//!    字典本身给不出的语法上下文——文档声明版本、已解析的
//!    `/JBIG2Globals` 段链——经 [`BitmapStreamContext`] 由调用方注入，
//!    页面信息段与 JPX 头部则由本层对码流就地做语法解析兜底；
//! 2. **调既有分发层**：[`decode_jbig2_with`](crate::core::reader::image::decode_jbig2_with) /
//!    [`decode_jpx_with`](crate::core::reader::image::decode_jpx_with)，
//!    版本门禁（`check_codec_version`，拦在任何后端被调用之前）与输出
//!    契约校验（`accept_output` 归一化违约为 `Invalid`）都在其中；
//! 3. **展平**：成功位图的 `samples`（行优先、每行按位紧排，长度已由
//!    契约校验为 `row_bytes() × height`）原样作为 `Vec<u8>` 返回。
//!
//! **契约边界**：[`super::decode_stream`] 与 [`super::decode_stream_with`]
//! 的签名与行为零变化；本适配器是兄弟入口，不是注册进过滤链的
//! `StreamFilter`——后者的步式签名（`data + params`）既携带不了后端集合
//! 与文档版本，返回值 `DecodeError(String)` 也会丢掉后端契约的类型化
//! 错误形态。错误形状沿用 [`ImageError`](crate::core::reader::image::ImageError)，
//! 不新增任何错误枚举变体：`Unsupported` = 版本门禁或未注册后端（默认
//! `ImageBackends::NONE` 即此，本库不内建真实像素解码），`Invalid` =
//! 后端输出违约（归一化、不 panic），`Filter` = 码流损坏或前序过滤器
//! 失败。同时含两种位图过滤器的病态字典按 JPX 优先分发（与
//! [`ImageModel::decode_image_with`](crate::core::reader::image::ImageModel::decode_image_with)
//! 同口径）。

use crate::core::reader::image::{
    ImageBackends, ImageError, Jbig2DecodeRequest, JpxDecodeRequest, decode_jbig2_with,
    decode_jpx_with, parse_jbig2_segments, parse_jpx_header,
};
use crate::core::types::{Dict, PdfVersion, Value};

use super::{canonical_filter, decode_stream_except, filter_names};

/// 过滤链中需要原样透传给位图后端的步骤（含 PDF 1.4/1.5 之前的旧名）。
fn is_bitmap_codec(name: &str) -> bool {
    matches!(name, "JBIG2Decode" | "JBIG2" | "JPXDecode" | "JPX")
}

/// 位图解码需要、但流字典本身给不出的语法上下文（均为可选注入）。
#[derive(Debug, Default, Clone, Copy)]
pub struct BitmapStreamContext<'a> {
    /// 文档声明版本：`Some` 时由分发层 `check_codec_version` 在任何后端
    /// 被调用之前拦截（类型化 `Unsupported`）；`None` 放行，判定权交给
    /// 持有 `Document` 的调用层（与 `ImageModel::decode_image_with` 的
    /// 便捷路径同口径）。
    pub pdf_version: Option<PdfVersion>,
    /// 已解码的 `/JBIG2Globals` 全局段链**原始字节**。该流是间接引用、需
    /// `Document` 解引用才能取到，故由持有它的调用方注入；缺省 `None`
    /// 表示后端须自行从码流读取（后端契约口径）。
    pub jbig2_globals: Option<&'a [u8]>,
}

/// 经显式注册的位图后端解码 `/JBIG2Decode`/`/JPXDecode` 流并展平为
/// `Vec<u8>`（便捷入口：不注入文档版本与全局段链）。
///
/// 需要版本门禁或 `/JBIG2Globals` 的调用方用
/// [`decode_bitmap_stream_with`]；未注册后端时两者都返回类型化
/// `Unsupported`（与 [`ImageBackends::NONE`] 的默认行为一致）。
pub fn decode_bitmap_stream(
    dict: &Dict,
    data: &[u8],
    backends: &ImageBackends<'_>,
) -> std::result::Result<Vec<u8>, ImageError> {
    decode_bitmap_stream_with(dict, data, backends, &BitmapStreamContext::default())
}

/// 用显式上下文经位图后端把位图流解码并展平为 `Vec<u8>`。
///
/// 只做「请求构造 → 调既有分发层 → 展平」，错误原样透成后端契约的
/// 既有形态；`/Filter` 非 JBIG2/JPX 时返回 `Unsupported` 并提示改用
/// [`super::decode_stream`]。
pub fn decode_bitmap_stream_with(
    dict: &Dict,
    data: &[u8],
    backends: &ImageBackends<'_>,
    context: &BitmapStreamContext<'_>,
) -> std::result::Result<Vec<u8>, ImageError> {
    let names = filter_names(dict).map_err(|error| ImageError::Invalid(error.to_string()))?;
    let has_jpx = names
        .iter()
        .any(|name| matches!(canonical_filter(name), "JPXDecode" | "JPX"));
    let has_jbig2 = names
        .iter()
        .any(|name| matches!(canonical_filter(name), "JBIG2Decode" | "JBIG2"));
    if !has_jpx && !has_jbig2 {
        return Err(ImageError::Unsupported(
            "stream does not use /JBIG2Decode or /JPXDecode; decode it with filters::decode_stream"
                .to_owned(),
        ));
    }
    // 位图步骤之外的其余过滤器步骤照常解码，位图步骤原样透传成码流。
    let codestream = decode_stream_except(dict, data, &is_bitmap_codec)
        .map_err(|error| ImageError::Filter(error.to_string()))?;
    let bitmap = if has_jpx {
        let header = parse_jpx_header(&codestream);
        // `/SMaskInData` 属于图像字典本身；非整数形态按缺省 0 处理（忽略
        // 码流内不透明通道），与图像读端同口径。
        let smask_in_data = match dict.get("SMaskInData") {
            Some(Value::Integer(value)) => u8::try_from(*value).ok(),
            _ => None,
        };
        let request = JpxDecodeRequest {
            codestream: &codestream,
            header: header.as_ref(),
            smask_in_data,
            pdf_version: context.pdf_version,
        };
        decode_jpx_with(backends.jpx(), &request)?
    } else {
        let segments = parse_jbig2_segments(&codestream).ok();
        let request = Jbig2DecodeRequest {
            codestream: &codestream,
            globals_raw: context.jbig2_globals,
            page_information: segments
                .as_ref()
                .and_then(|stream| stream.page_information.as_ref()),
            pdf_version: context.pdf_version,
        };
        decode_jbig2_with(backends.jbig2(), &request)?
    };
    // samples 已由分发层 accept_output 按 DecodedBitmap 契约校验过形状；
    // 展平字节即行优先、每行按位紧排的像素行，逐行拼接。
    Ok(bitmap.samples)
}

#[cfg(test)]
mod tests;
