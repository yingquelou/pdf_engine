//! 媒体播放器字典写端（13.2.7 Table 290–293，PDF 1.5）。
//!
//! `/PL` 同时用于 media clip data（Table 274）与 media play parameters
//! （Table 279）。只写显式声明的条目：`/MU` `/A` `/NU` 空数组等价于
//! 缺失而不写出，`/L` `/LI` `/H` `/HI` `/OS` 缺省值不物化。`/MH` `/BE`
//! 无定义条目，不于写端暴露。校验在对象号分配前触发，非法取值返回
//! `InvalidObject` 并保持零字节输出。

use super::super::text_string_value;
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

/// 媒体播放器字典（Table 290）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaPlayers {
    must_use: Vec<MediaPlayerInfo>,
    any: Vec<MediaPlayerInfo>,
    not_used: Vec<MediaPlayerInfo>,
}

impl MediaPlayers {
    /// 空播放器字典；三个数组均可省略。
    pub fn new() -> Self {
        Self::default()
    }

    /// `/MU` 必须使用其中之一的播放器集合。
    pub fn with_must_use(mut self, players: Vec<MediaPlayerInfo>) -> Self {
        self.must_use = players;
        self
    }

    /// `/A` 任意可用的播放器集合。
    pub fn with_any(mut self, players: Vec<MediaPlayerInfo>) -> Self {
        self.any = players;
        self
    }

    /// `/NU` 不得使用的播放器集合。
    pub fn with_not_used(mut self, players: Vec<MediaPlayerInfo>) -> Self {
        self.not_used = players;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), WriteError> {
        for player in self.must_use.iter().chain(&self.any).chain(&self.not_used) {
            player.validate()?;
        }
        Ok(())
    }

    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert("Type", Value::Name("MediaPlayers".into()));
        if !self.must_use.is_empty() {
            dict = dict.insert("MU", player_array(&self.must_use));
        }
        if !self.any.is_empty() {
            dict = dict.insert("A", player_array(&self.any));
        }
        if !self.not_used.is_empty() {
            dict = dict.insert("NU", player_array(&self.not_used));
        }
        dict
    }
}

fn player_array(players: &[MediaPlayerInfo]) -> Value {
    Value::Array(Array(
        players
            .iter()
            .map(|player| Value::Dict(player.to_dict()))
            .collect(),
    ))
}

/// 媒体播放器信息字典（Table 291）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlayerInfo {
    software: SoftwareIdentifier,
}

impl MediaPlayerInfo {
    /// 以 `/PID` 软件标识字典构造；`/MH` `/BE` 无定义条目不写出。
    pub fn new(software: SoftwareIdentifier) -> Self {
        Self { software }
    }

    fn validate(&self) -> Result<(), WriteError> {
        self.software.validate()
    }

    fn to_dict(&self) -> Dict {
        Dict::new()
            .insert("Type", Value::Name("MediaPlayerInfo".into()))
            .insert("PID", Value::Dict(self.software.to_dict()))
    }
}

/// 软件标识字典（Table 292）。
#[derive(Debug, Clone, PartialEq)]
pub struct SoftwareIdentifier {
    uri: String,
    lower: Option<Vec<i64>>,
    lower_inclusive: Option<bool>,
    upper: Option<Vec<i64>>,
    upper_inclusive: Option<bool>,
    operating_systems: Option<Vec<Vec<u8>>>,
}

impl SoftwareIdentifier {
    /// 以 `/U` 软件 URI 构造；未知 scheme 合法（视为不匹配）。
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            lower: None,
            lower_inclusive: None,
            upper: None,
            upper_inclusive: None,
            operating_systems: None,
        }
    }

    /// `/L` 版本下界；元素非负，空数组表示无穷大。
    pub fn with_lower(mut self, lower: Vec<i64>) -> Self {
        self.lower = Some(lower);
        self
    }

    /// `/LI` 下界是否含入。
    pub fn with_lower_inclusive(mut self, inclusive: bool) -> Self {
        self.lower_inclusive = Some(inclusive);
        self
    }

    /// `/H` 版本上界；元素非负，空数组表示无穷大。
    pub fn with_upper(mut self, upper: Vec<i64>) -> Self {
        self.upper = Some(upper);
        self
    }

    /// `/HI` 上界是否含入。
    pub fn with_upper_inclusive(mut self, inclusive: bool) -> Self {
        self.upper_inclusive = Some(inclusive);
        self
    }

    /// `/OS` 操作系统标识字节串；空数组表示全部系统，条目须唯一。
    pub fn with_operating_systems(mut self, systems: Vec<Vec<u8>>) -> Self {
        self.operating_systems = Some(systems);
        self
    }

    fn validate(&self) -> Result<(), WriteError> {
        if self.uri.is_empty() || !self.uri.is_ascii() {
            return Err(WriteError::InvalidObject(
                "software identifier /U must be non-empty ASCII".into(),
            ));
        }
        for (key, versions) in [("L", &self.lower), ("H", &self.upper)] {
            if let Some(versions) = versions
                && versions.iter().any(|version| *version < 0)
            {
                return Err(WriteError::InvalidObject(format!(
                    "software identifier /{key} entries must not be negative"
                )));
            }
        }
        if let (Some(lower), Some(upper)) = (&self.lower, &self.upper)
            && compare_versions(lower, upper).is_gt()
        {
            return Err(WriteError::InvalidObject(
                "software identifier /L must not exceed /H".into(),
            ));
        }
        if let Some(systems) = &self.operating_systems {
            let mut seen = std::collections::HashSet::with_capacity(systems.len());
            if systems.iter().any(|system| !seen.insert(system)) {
                return Err(WriteError::InvalidObject(
                    "software identifier /OS entries must be unique".into(),
                ));
            }
        }
        Ok(())
    }

    fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("SoftwareIdentifier".into()))
            .insert("U", text_string_value(&self.uri));
        if let Some(lower) = &self.lower {
            dict = dict.insert("L", version_array(lower));
        }
        if let Some(inclusive) = self.lower_inclusive {
            dict = dict.insert("LI", Value::Bool(inclusive));
        }
        if let Some(upper) = &self.upper {
            dict = dict.insert("H", version_array(upper));
        }
        if let Some(inclusive) = self.upper_inclusive {
            dict = dict.insert("HI", Value::Bool(inclusive));
        }
        if let Some(systems) = &self.operating_systems {
            let entries = systems.iter().map(|system| byte_string(system)).collect();
            dict = dict.insert("OS", Value::Array(Array(entries)));
        }
        dict
    }
}

/// 按 13.2.7.4.3 比较版本数组：空数组为无穷大，短数组补零。
fn compare_versions(lower: &[i64], upper: &[i64]) -> std::cmp::Ordering {
    match (lower.is_empty(), upper.is_empty()) {
        (true, true) => return std::cmp::Ordering::Equal,
        (true, false) => return std::cmp::Ordering::Greater,
        (false, true) => return std::cmp::Ordering::Less,
        (false, false) => {}
    }
    let length = lower.len().max(upper.len());
    for index in 0..length {
        let left = lower.get(index).copied().unwrap_or(0);
        let right = upper.get(index).copied().unwrap_or(0);
        match left.cmp(&right) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

fn version_array(versions: &[i64]) -> Value {
    Value::Array(Array(
        versions
            .iter()
            .map(|version| Value::Integer(*version))
            .collect(),
    ))
}

fn byte_string(bytes: &[u8]) -> Value {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02X}");
    }
    Value::HexString(hex)
}
