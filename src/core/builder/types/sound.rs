//! 声音对象写端（ISO 32000-1 13.3 Table 294）。
//!
//! [`Sound`] 是链式构建的声音对象：采样数据、采样率、声道数与每样本
//! 位数，可选编码 `/E` 与压缩格式 `/CO`。经
//! [`PdfBuilder::add_sound`](crate::core::builder::PdfBuilder::add_sound)
//! 注册为句柄，`build` 时写成独立的间接流对象（流对象在 PDF 语法中
//! 必须间接），供 Sound 动作（Table 208）与 Sound 注解（Table 185）
//! 以 `/Sound` 间接引用。
//!
//! 写出只包含 `/Type /Sound`、必填 `/R`、与缺省不同的 `/C` `/B`、
//! 显式声明的 `/E` `/CO` 及 `/Length`；采样数据原样写入，不附加
//! `/Filter`（`/CO` 是独立的采样压缩格式，不在此建模）。
//!
//! 校验：采样率须为正有限数；声道数 ≥1；每样本位数限 8 或 16
//! （13.3 的便携集合）；编码限四个标准名（非标准名不可写出）；`/CO`
//! 须为合法 PDF 名字；数据不得为空（13.3：无 `/F` 时声音对象自身
//! 必须包含采样数据；外部文件声音不在本模型内）。13.3 关于 `Raw`/
//! `Signed` 采样率与 `muLaw` 声道/位数的额外约束未在此强制，调用方
//! 应自行遵守以保证可移植性。

use super::validate_name_value;
use crate::core::types::{Dict, StreamData, Value};

pub use crate::core::reader::sound::SoundEncoding;

/// 声音对象的构建态（尚未分配对象号）。
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    data: Vec<u8>,
    sample_rate: f64,
    channels: u32,
    bits_per_sample: u32,
    encoding: SoundEncoding,
    compression: Option<String>,
}

impl Sound {
    /// 新建声音对象；`data` 为采样数据字节，`sample_rate` 为每秒样本数，
    /// `channels` 为声道数，`bits_per_sample` 为每声道每样本位数。
    pub fn new(
        data: impl Into<Vec<u8>>,
        sample_rate: f64,
        channels: u32,
        bits_per_sample: u32,
    ) -> Self {
        Self {
            data: data.into(),
            sample_rate,
            channels,
            bits_per_sample,
            encoding: SoundEncoding::Raw,
            compression: None,
        }
    }

    /// 采样编码格式 `/E`；缺省 `Raw`（写出时省略）。
    pub fn with_encoding(mut self, encoding: SoundEncoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// 采样压缩格式 `/CO`（PDF 未定义标准值）；未声明时省略。
    pub fn with_compression(mut self, compression: impl Into<String>) -> Self {
        self.compression = Some(compression.into());
        self
    }

    /// 校验取值；错误串直接用于 [`WriteError`](crate::core::writer::WriteError)。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.sample_rate.is_finite() || self.sample_rate <= 0.0 {
            return Err("sound /R (sample rate) must be a positive finite number".into());
        }
        if self.channels == 0 {
            return Err("sound /C (channel count) must be at least 1".into());
        }
        if !matches!(self.bits_per_sample, 8 | 16) {
            return Err(format!(
                "sound /B (bits per sample) must be 8 or 16, got {}",
                self.bits_per_sample
            ));
        }
        if self.data.is_empty() {
            return Err(
                "sound stream data must not be empty (external /F sounds are not modeled)".into(),
            );
        }
        match &self.encoding {
            SoundEncoding::Raw
            | SoundEncoding::Signed
            | SoundEncoding::MuLaw
            | SoundEncoding::ALaw => {}
            SoundEncoding::Other(name) => {
                return Err(format!(
                    "sound /E encoding {name:?} is not one of Raw, Signed, muLaw, ALaw"
                ));
            }
        }
        if let Some(compression) = &self.compression {
            validate_name_value(compression, "sound /CO compression")?;
        }
        Ok(())
    }

    /// 写成流对象值：`/Type /Sound` + 必填 `/R` + 非缺省 `/C` `/B` +
    /// 显式 `/E` `/CO` + `/Length`，数据原样。
    pub(crate) fn to_stream_value(&self) -> Value {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("Sound".into()))
            .insert("R", Value::Float(self.sample_rate));
        if self.channels != 1 {
            dict = dict.insert("C", Value::Integer(i64::from(self.channels)));
        }
        if self.bits_per_sample != 8 {
            dict = dict.insert("B", Value::Integer(i64::from(self.bits_per_sample)));
        }
        if self.encoding != SoundEncoding::Raw {
            dict = dict.insert("E", Value::Name(self.encoding.name().into()));
        }
        if let Some(compression) = &self.compression {
            dict = dict.insert("CO", Value::Name(compression.clone()));
        }
        let length = self.data.len() as i64;
        Value::Stream(
            dict.insert("Length", Value::Integer(length)),
            StreamData::Owned(self.data.clone()),
        )
    }
}
