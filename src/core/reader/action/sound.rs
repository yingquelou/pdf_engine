//! Sound 动作读取（12.6.4.8 Table 208）。
//!
//! `/Sound` 是声音流对象的间接引用（流对象在 PDF 语法中总是间接的），
//! 经 [`crate::core::reader::sound::Document::sound`] 解析为类型化声音模型。
//! `/Volume`、`/Synchronous`、`/Repeat`、`/Mix` 均为可选条目，读取时保留
//! 原始缺省语义（不物化 Table 208 的缺省值）；`/Volume` 校验落在 -1.0..=1.0。

use crate::core::reader::navigation::resolve;
use crate::core::reader::sound::Sound;
use crate::core::types::{Dict, Document, Value};

use super::{ActionError, malformed};

/// Sound 动作（Table 208）。
#[derive(Debug, Clone, PartialEq)]
pub struct SoundAction {
    /// `/Sound` 声音对象（必填）。
    pub sound: Sound,
    /// `/Volume` 播放音量（-1.0..=1.0）；缺失表示查看器缺省（1.0）。
    pub volume: Option<f64>,
    /// `/Synchronous` 是否同步播放；缺失表示缺省 `false`。
    pub synchronous: Option<bool>,
    /// `/Repeat` 是否循环播放；缺失表示缺省 `false`。
    pub repeat: Option<bool>,
    /// `/Mix` 是否与其它声音混合；缺失表示缺省 `false`。
    pub mix: Option<bool>,
}

/// 解析 Sound 动作字典；`/S` 已由调用方识别。
pub(super) fn parse_sound_action(
    document: &Document,
    dict: &Dict,
) -> Result<SoundAction, ActionError> {
    let sound_value = dict
        .get("Sound")
        .ok_or_else(|| malformed("Sound action is missing /Sound"))?;
    if !matches!(sound_value, Value::Ref(..)) {
        return Err(malformed(
            "Sound action /Sound must be an indirect reference to a sound stream",
        ));
    }
    let sound = document
        .sound(sound_value)
        .map_err(|error| malformed(format!("Sound action: {error}")))?;
    let volume = match dict.get("Volume") {
        None => None,
        Some(value) => {
            let number = match resolve(document, value)? {
                Value::Integer(number) => number as f64,
                Value::Float(number) => number,
                _ => return Err(malformed("Sound action /Volume is not a number")),
            };
            if !number.is_finite() || !(-1.0..=1.0).contains(&number) {
                return Err(malformed(
                    "Sound action /Volume must be in the range -1.0 to 1.0",
                ));
            }
            Some(number)
        }
    };
    Ok(SoundAction {
        sound,
        volume,
        synchronous: parse_optional_bool(document, dict, "Synchronous")?,
        repeat: parse_optional_bool(document, dict, "Repeat")?,
        mix: parse_optional_bool(document, dict, "Mix")?,
    })
}

/// 可选布尔条目；缺失返回 `None`，非布尔报错。
fn parse_optional_bool(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<Option<bool>, ActionError> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => match resolve(document, value)? {
            Value::Bool(flag) => Ok(Some(flag)),
            _ => Err(malformed(format!("Sound action /{key} is not a boolean"))),
        },
    }
}
