//! Sound 动作写端（12.6.4.8 Table 208）。
//!
//! [`SoundAction`] 以 [`PdfBuilder::add_sound`](crate::core::builder::PdfBuilder::add_sound)
//! 返回的声音句柄构造；`build` 时 `/Sound` 写成声音流对象的间接引用。
//! `/Volume` 取值 -1.0..=1.0，`/Synchronous` `/Repeat` `/Mix` 为可选布尔，
//! 未显式声明时省略（不物化 Table 208 的缺省值）。
//!
//! 句柄越界与音量越界在任何对象写出前由 `action_requirements` 拒绝，
//! 保证零字节输出契约。

use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

/// Sound 动作的构建态。
#[derive(Debug, Clone, PartialEq)]
pub struct SoundAction {
    sound: usize,
    volume: Option<f64>,
    synchronous: Option<bool>,
    repeat: Option<bool>,
    mix: Option<bool>,
}

impl SoundAction {
    /// 播放指定声音对象；`sound_handle` 为注册声音的句柄。
    pub fn new(sound_handle: usize) -> Self {
        Self {
            sound: sound_handle,
            volume: None,
            synchronous: None,
            repeat: None,
            mix: None,
        }
    }

    /// 播放音量 `/Volume`（-1.0..=1.0；缺省 1.0 不写出）。
    pub fn with_volume(mut self, volume: f64) -> Self {
        self.volume = Some(volume);
        self
    }

    /// 是否同步播放 `/Synchronous`（缺省 false 不写出）。
    pub fn with_synchronous(mut self, synchronous: bool) -> Self {
        self.synchronous = Some(synchronous);
        self
    }

    /// 是否循环播放 `/Repeat`（缺省 false 不写出；存在时 `/Synchronous` 被忽略）。
    pub fn with_repeat(mut self, repeat: bool) -> Self {
        self.repeat = Some(repeat);
        self
    }

    /// 是否与其它声音混合 `/Mix`（缺省 false 不写出）。
    pub fn with_mix(mut self, mix: bool) -> Self {
        self.mix = Some(mix);
        self
    }

    /// 校验声音句柄上界与音量范围。
    pub(crate) fn validate(&self, sound_count: usize) -> Result<(), WriteError> {
        if self.sound >= sound_count {
            return Err(WriteError::InvalidObject(format!(
                "Sound action /Sound references unknown sound handle {}",
                self.sound
            )));
        }
        if let Some(volume) = self.volume
            && (!volume.is_finite() || !(-1.0..=1.0).contains(&volume))
        {
            return Err(WriteError::InvalidObject(format!(
                "Sound action /Volume must be within -1.0..=1.0, got {volume}"
            )));
        }
        Ok(())
    }

    /// 写出除 `/S` 外的动作条目；`sound_nums` 为声音句柄到对象号的映射。
    pub(crate) fn to_dict(&self, sound_nums: &[i64]) -> Result<Dict, WriteError> {
        self.validate(sound_nums.len())?;
        let number = sound_nums[self.sound];
        let mut dict = Dict::new().insert("Sound", Value::Ref(number, 0));
        if let Some(volume) = self.volume {
            dict = dict.insert("Volume", Value::Float(volume));
        }
        if let Some(synchronous) = self.synchronous {
            dict = dict.insert("Synchronous", Value::Bool(synchronous));
        }
        if let Some(repeat) = self.repeat {
            dict = dict.insert("Repeat", Value::Bool(repeat));
        }
        if let Some(mix) = self.mix {
            dict = dict.insert("Mix", Value::Bool(mix));
        }
        Ok(dict)
    }
}
