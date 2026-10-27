//! 半色调字典（ISO 32000-1 §10.5.4 Table 168–170）。
//!
//! 建模类型 1（`/XFrequency` + `/SpotFunction`）与类型 2
//! （`/Frequency`/`/Width`，可 `/Uncompensated`）的加网半色调字典，以及
//! 半色调包装字典（`/DefaultHalftoneType` + 按色料条目）。`/SpotFunction`
//! 与 `/TransferFunction` 以 [`PdfBuilder::function`] 注册后的句柄引用，
//! 组装阶段解析为函数对象值（同一句柄只写一次）。
//!
//! [`PdfBuilder::function`]: super::super::PdfBuilder::function

use std::collections::BTreeMap;

use crate::core::types::{Dict, Value};

/// 类型 1 半色调字典（Table 169）。
#[derive(Debug, Clone, PartialEq)]
pub struct Type1Halftone {
    /// `/XFrequency` 网点频率（cells/inch，> 0）。
    pub x_frequency: f64,
    /// `/Angle` 网点角度（度）。
    pub angle: f64,
    /// `/SpotFunction` 点增益函数句柄（须单输出）。
    pub spot_function: usize,
    /// `/TransferFunction` 传递函数句柄（可选，须单输出）。
    pub transfer_function: Option<usize>,
}

/// 类型 2 半色调字典（Table 170）。
#[derive(Debug, Clone, PartialEq)]
pub struct Type2Halftone {
    /// `/Frequency` 网点频率（cells/inch，> 0）。
    pub frequency: f64,
    /// `/Angle` 网点角度（度）。
    pub angle: f64,
    /// `/Width` 网点直径相对单元宽度的比例（≥ 0）。
    pub width: f64,
    /// `/Uncompensated` 是否不做网点增大补偿；false 时省略条目。
    pub uncompensated: bool,
    /// `/SpotFunction` 点增益函数句柄（须单输出）。
    pub spot_function: usize,
    /// `/TransferFunction` 传递函数句柄（可选，须单输出）。
    pub transfer_function: Option<usize>,
}

/// 已建模的单个半色调字典。
#[derive(Debug, Clone, PartialEq)]
pub enum Halftone {
    /// 类型 1。
    Type1(Type1Halftone),
    /// 类型 2。
    Type2(Type2Halftone),
}

/// 半色调包装字典（Table 168）：默认类型 + 按色料覆盖。
#[derive(Debug, Clone, PartialEq)]
pub struct HalftoneDictionary {
    /// `/DefaultHalftoneType`（当前仅支持 1 或 2）。
    pub default_type: u8,
    /// 色料名 → 半色调字典覆盖。
    pub colorants: BTreeMap<String, Halftone>,
}

impl Halftone {
    /// 引用的全部函数句柄（spot 在前、transfer 在后）。
    pub(crate) fn function_handles(&self) -> [Option<usize>; 2] {
        match self {
            Self::Type1(t1) => [Some(t1.spot_function), t1.transfer_function],
            Self::Type2(t2) => [Some(t2.spot_function), t2.transfer_function],
        }
    }

    /// 类型号（1 或 2）。
    pub fn halftone_type(&self) -> u8 {
        match self {
            Self::Type1(_) => 1,
            Self::Type2(_) => 2,
        }
    }

    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Type1(t1) => {
                positive("type 1 /XFrequency", t1.x_frequency)?;
                finite("type 1 /Angle", t1.angle)?;
            }
            Self::Type2(t2) => {
                positive("type 2 /Frequency", t2.frequency)?;
                finite("type 2 /Angle", t2.angle)?;
                non_negative("type 2 /Width", t2.width)?;
            }
        }
        Ok(())
    }

    /// 写成半色调字典；函数句柄经 `resolve` 解析为已写出的值。
    pub(crate) fn to_value(
        &self,
        resolve: &mut impl FnMut(usize) -> Option<Value>,
    ) -> Result<Value, String> {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("Halftone".into()))
            .insert("HalftoneType", Value::Integer(self.halftone_type().into()));
        match self {
            Self::Type1(t1) => {
                dict = dict
                    .insert("XFrequency", Value::Float(t1.x_frequency))
                    .insert("Angle", Value::Float(t1.angle));
                dict = insert_function(dict, "SpotFunction", Some(t1.spot_function), resolve)?;
                dict = insert_function(dict, "TransferFunction", t1.transfer_function, resolve)?;
            }
            Self::Type2(t2) => {
                dict = dict
                    .insert("Frequency", Value::Float(t2.frequency))
                    .insert("Angle", Value::Float(t2.angle))
                    .insert("Width", Value::Float(t2.width));
                if t2.uncompensated {
                    dict = dict.insert("Uncompensated", Value::Bool(true));
                }
                dict = insert_function(dict, "SpotFunction", Some(t2.spot_function), resolve)?;
                dict = insert_function(dict, "TransferFunction", t2.transfer_function, resolve)?;
            }
        }
        Ok(Value::Dict(dict))
    }
}

impl HalftoneDictionary {
    /// 创建包装字典，默认半色调类型为 `default_type`（1 或 2）。
    pub fn new(default_type: u8) -> Self {
        Self {
            default_type,
            colorants: BTreeMap::new(),
        }
    }

    /// 为色料 `name`（如 `"C"`、`"Magenta"`）设置覆盖半色调。
    pub fn with_colorant(mut self, name: impl Into<String>, halftone: Halftone) -> Self {
        self.colorants.insert(name.into(), halftone);
        self
    }

    /// 引用的全部函数句柄（去重由组装阶段的缓存完成）。
    pub(crate) fn function_handles(&self) -> impl Iterator<Item = usize> + '_ {
        self.colorants
            .values()
            .flat_map(|halftone| halftone.function_handles().into_iter().flatten())
    }

    /// 校验默认类型、色料名与半色调参数；非法时返回可读原因。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !matches!(self.default_type, 1 | 2) {
            return Err(format!(
                "/DefaultHalftoneType {} is not modeled (only 1 and 2)",
                self.default_type
            ));
        }
        for (name, halftone) in &self.colorants {
            if !is_valid_pdf_name(name) {
                return Err(format!(
                    "halftone colorant name {name:?} is not a valid PDF name"
                ));
            }
            halftone
                .validate()
                .map_err(|message| format!("colorant {name}: {message}"))?;
        }
        Ok(())
    }

    /// 写成 `/HT` 包装字典值。
    pub(crate) fn to_value(
        &self,
        resolve: &mut impl FnMut(usize) -> Option<Value>,
    ) -> Result<Value, String> {
        let mut dict = Dict::new().insert(
            "DefaultHalftoneType",
            Value::Integer(self.default_type.into()),
        );
        for (name, halftone) in &self.colorants {
            dict = dict.insert(name.clone(), halftone.to_value(resolve)?);
        }
        Ok(Value::Dict(dict))
    }
}

fn insert_function(
    mut dict: Dict,
    key: &str,
    handle: Option<usize>,
    resolve: &mut impl FnMut(usize) -> Option<Value>,
) -> Result<Dict, String> {
    let Some(handle) = handle else {
        return Ok(dict);
    };
    let value = resolve(handle)
        .ok_or_else(|| format!("/{key} references unknown function handle {handle}"))?;
    dict = dict.insert(key.to_string(), value);
    Ok(dict)
}

fn positive(what: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{what} must be a positive finite number"));
    }
    Ok(())
}

fn non_negative(what: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{what} must be a non-negative finite number"));
    }
    Ok(())
}

fn finite(what: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("{what} must be finite"));
    }
    Ok(())
}

fn is_valid_pdf_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            (0x21..=0x7E).contains(&byte)
                && !matches!(
                    byte,
                    b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' | b'#'
                )
        })
}
