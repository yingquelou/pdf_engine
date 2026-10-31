//! Trap 网络注解专有条目读取（14.11.6，Table 366/367）。
//!
//! 陷印网络（trap network）是 PDF 1.3 引入的 Form XObject，由页面至多一个
//! `TrapNet` 注解承载，且该注解必须位于页面 `/Annots` 数组最后一项。
//! 其 `/AP /N` 既可以是单个 Form XObject 流，也可以是「状态名 → 表单」
//! 的子字典，此时由 `/AS` 选择网络；`/F` 恒要求 Print（4）与 ReadOnly（64）
//! 置位（写端恒写 68）。
//!
//! Table 366 的跟踪条目只有两种互斥形式：`/LastModified`（PDF 1.4 日期），
//! 或 `/Version`（页面描述对象引用数组）与 `/AnnotStates`（与页面其余注解
//! 一一对应的 name/null 数组）。
//!
//! Table 367 的条目挂在选中的 Form XObject 上：`/PCM` 必填，其余
//! `/SeparationColorNames`、`/TrapRegions`、`/TrapStyles`、`/FontFauxing`
//! 可选。本模块只做结构与类型校验，不解释陷印几何。

use crate::core::types::{Dict, Document, PdfDate, Value};

use super::{AnnotationSubtype, Result, malformed};

/// Trap 网络注解（Table 366）：跟踪条目与选定的外观表单。
#[derive(Debug, Clone, PartialEq)]
pub struct TrapNetwork {
    /// `/LastModified` 修改日期；与 `/Version` + `/AnnotStates` 互斥。
    pub last_modified: Option<PdfDate>,
    /// `/Version` 页面描述对象引用数组（原样保留；每项须为间接引用）。
    pub version: Option<Vec<Value>>,
    /// `/AnnotStates` 与页面其余注解顺序对应的状态数组（Name → `Some`，
    /// Null → `None`）。
    pub annot_states: Option<Vec<Option<String>>>,
    /// 选中的陷阱网络外观表单及其 Table 367 条目。
    pub form: TrapNetworkForm,
}

/// 陷阱网络的 Form XObject（Table 367）。
#[derive(Debug, Clone, PartialEq)]
pub struct TrapNetworkForm {
    /// 选中外观流的间接引用；直接嵌入的流为 `None`。
    pub reference: Option<(i64, i64)>,
    /// `/PCM` 过程颜色模型（必填）。
    pub pcm: ProcessColorModel,
    /// `/SeparationColorNames` 创建网络时假定的色料名。
    pub separation_color_names: Vec<String>,
    /// `/TrapRegions` TrapRegion 对象引用。
    pub trap_regions: Vec<(i64, i64)>,
    /// `/TrapStyles` 人类可读的陷印样式串。
    pub trap_styles: Option<String>,
    /// `/FontFauxing` 字体字典数组（通常为空）。
    pub font_fauxing: Vec<Dict>,
    /// 选中外观流的字典（含 `/Subtype /Form` 等条目）。
    pub dict: Dict,
}

/// `/PCM` 过程颜色模型（Table 367）；未知名称原样保留。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessColorModel {
    DeviceGray,
    DeviceRGB,
    DeviceCMYK,
    DeviceCMY,
    DeviceRGBK,
    DeviceN,
    /// 规范之外的名称，原样保留（读端不拒绝）。
    Other(String),
}

impl ProcessColorModel {
    /// 从 `/PCM` 名称映射；未知名称返回 [`Self::Other`]。
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "DeviceGray" => Self::DeviceGray,
            "DeviceRGB" => Self::DeviceRGB,
            "DeviceCMYK" => Self::DeviceCMYK,
            "DeviceCMY" => Self::DeviceCMY,
            "DeviceRGBK" => Self::DeviceRGBK,
            "DeviceN" => Self::DeviceN,
            other => Self::Other(other.to_string()),
        }
    }

    /// `/PCM` 名称。
    pub fn name(&self) -> &str {
        match self {
            Self::DeviceGray => "DeviceGray",
            Self::DeviceRGB => "DeviceRGB",
            Self::DeviceCMYK => "DeviceCMYK",
            Self::DeviceCMY => "DeviceCMY",
            Self::DeviceRGBK => "DeviceRGBK",
            Self::DeviceN => "DeviceN",
            Self::Other(name) => name,
        }
    }
}

/// 解析 Trap 网络注解条目；非 `TrapNet` 子类型返回 `None`。
pub(super) fn parse_trap_network(
    document: &Document,
    dict: &Dict,
    subtype: &AnnotationSubtype,
) -> Result<Option<TrapNetwork>> {
    if *subtype != AnnotationSubtype::TrapNet {
        // Table 366 的跟踪条目只属于 TrapNet 注解；其他子类型出现即报错。
        for key in ["LastModified", "Version", "AnnotStates"] {
            if dict.get(key).is_some() {
                return Err(malformed(format!(
                    "annotation /{key} is specific to TrapNet annotations"
                )));
            }
        }
        return Ok(None);
    }
    if dict.get("F").is_none() {
        return Err(malformed("TrapNet annotation is missing /F"));
    }
    let form = form_entries(document, dict)?;
    let (last_modified, version, annot_states) = tracking_entries(dict)?;
    Ok(Some(TrapNetwork {
        last_modified,
        version,
        annot_states,
        form,
    }))
}

/// 按 `/AP /N` 与 `/AS` 选出外观表单并读取 Table 367 条目。
fn form_entries(document: &Document, dict: &Dict) -> Result<TrapNetworkForm> {
    let ap = dict
        .get("AP")
        .ok_or_else(|| malformed("TrapNet annotation is missing /AP"))?;
    let ap = resolve(document, ap)?;
    let ap = ap
        .as_dict()
        .ok_or_else(|| malformed("TrapNet annotation /AP must be a dictionary"))?;
    let normal = ap
        .get("N")
        .ok_or_else(|| malformed("TrapNet annotation /AP is missing /N"))?;
    let selected = select_appearance(document, dict, normal)?;
    let reference = selected.as_ref();
    let stream = resolve(document, &selected)?;
    let Value::Stream(stream_dict, _) = stream else {
        return Err(malformed(
            "TrapNet annotation appearance must be a form XObject stream",
        ));
    };
    if stream_dict.get("Subtype").and_then(Value::as_name) != Some("Form") {
        return Err(malformed(
            "TrapNet annotation appearance /Subtype must be Form",
        ));
    }
    Ok(TrapNetworkForm {
        reference,
        pcm: parse_pcm(&stream_dict)?,
        separation_color_names: parse_separation_color_names(&stream_dict)?,
        trap_regions: parse_trap_regions(&stream_dict)?,
        trap_styles: parse_trap_styles(&stream_dict)?,
        font_fauxing: parse_font_fauxing(document, &stream_dict)?,
        dict: stream_dict,
    })
}

/// 从 `/AP /N` 选出外观流值：直接流，或按 `/AS` 选择的状态子字典项。
fn select_appearance(document: &Document, dict: &Dict, normal: &Value) -> Result<Value> {
    let resolved = resolve(document, normal)?;
    let states = match &resolved {
        Value::Stream(_, _) => {
            if dict.get("AS").is_some() {
                return Err(malformed(
                    "TrapNet annotation /AS is not permitted when /AP /N is a stream",
                ));
            }
            return Ok(normal.clone());
        }
        Value::Dict(states) => states,
        _ => {
            return Err(malformed(
                "TrapNet annotation /AP /N must be a stream or a state dictionary",
            ));
        }
    };
    if states.0.is_empty() {
        return Err(malformed(
            "TrapNet annotation /AP /N state dictionary is empty",
        ));
    }
    if let Some(state) = dict.get("AS") {
        let name = state
            .as_name()
            .ok_or_else(|| malformed("TrapNet annotation /AS must be a name"))?;
        return states.get(name).cloned().ok_or_else(|| {
            malformed(format!(
                "TrapNet annotation /AS names unknown appearance state {name}"
            ))
        });
    }
    if states.0.len() > 1 {
        return Err(malformed(
            "TrapNet annotation /AP /N has multiple appearances but no /AS",
        ));
    }
    states
        .0
        .values()
        .next()
        .cloned()
        .ok_or_else(|| malformed("TrapNet annotation /AP /N state dictionary is empty"))
}

/// Table 366 跟踪条目：`/LastModified` 与 `/Version` + `/AnnotStates` 互斥。
type TrackingEntries = (
    Option<PdfDate>,
    Option<Vec<Value>>,
    Option<Vec<Option<String>>>,
);

/// 读取 Table 366 的两种互斥跟踪形式。
fn tracking_entries(dict: &Dict) -> Result<TrackingEntries> {
    let has_version = dict.get("Version").is_some();
    let has_states = dict.get("AnnotStates").is_some();
    match dict.get("LastModified") {
        Some(value) => {
            if has_version || has_states {
                return Err(malformed(
                    "TrapNet annotation /Version and /AnnotStates shall be absent when /LastModified is present",
                ));
            }
            let text = crate::core::reader::content::text_value(value).ok_or_else(|| {
                malformed("TrapNet annotation /LastModified must be a text string")
            })?;
            let date = PdfDate::parse(&text).map_err(|error| {
                malformed(format!(
                    "TrapNet annotation /LastModified is not a valid PDF date: {error}"
                ))
            })?;
            Ok((Some(date), None, None))
        }
        None => {
            if !has_version || !has_states {
                return Err(malformed(
                    "TrapNet annotation requires /Version and /AnnotStates when /LastModified is absent",
                ));
            }
            let version = parse_version(dict)?;
            let states = parse_annot_states(dict)?;
            Ok((None, Some(version), Some(states)))
        }
    }
}

/// `/Version`：每个条目都必须是间接引用（原样保留）。
fn parse_version(dict: &Dict) -> Result<Vec<Value>> {
    let value = dict
        .get("Version")
        .ok_or_else(|| malformed("TrapNet annotation is missing /Version"))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("TrapNet annotation /Version must be an array"))?;
    if array.iter().any(|entry| entry.as_ref().is_none()) {
        return Err(malformed(
            "TrapNet annotation /Version entries must be indirect references",
        ));
    }
    Ok(array.0.clone())
}

/// `/AnnotStates`：name → `Some(name)`，null → `None`。
fn parse_annot_states(dict: &Dict) -> Result<Vec<Option<String>>> {
    let value = dict
        .get("AnnotStates")
        .ok_or_else(|| malformed("TrapNet annotation is missing /AnnotStates"))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("TrapNet annotation /AnnotStates must be an array"))?;
    array
        .iter()
        .map(|entry| match entry {
            Value::Name(name) => Ok(Some(name.clone())),
            Value::Null => Ok(None),
            _ => Err(malformed(
                "TrapNet annotation /AnnotStates entries must be names or null",
            )),
        })
        .collect()
}

/// Table 367 `/PCM`：必填 name。
fn parse_pcm(form: &Dict) -> Result<ProcessColorModel> {
    let value = form
        .get("PCM")
        .ok_or_else(|| malformed("TrapNet form is missing /PCM"))?;
    let name = value
        .as_name()
        .ok_or_else(|| malformed("TrapNet form /PCM must be a name"))?;
    Ok(ProcessColorModel::from_name(name))
}

/// Table 367 `/SeparationColorNames`：可选 name 数组。
fn parse_separation_color_names(form: &Dict) -> Result<Vec<String>> {
    let Some(value) = form.get("SeparationColorNames") else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| malformed("TrapNet form /SeparationColorNames must be an array of names"))?;
    array
        .iter()
        .map(|entry| {
            entry.as_name().map(str::to_string).ok_or_else(|| {
                malformed("TrapNet form /SeparationColorNames must be an array of names")
            })
        })
        .collect()
}

/// Table 367 `/TrapRegions`：可选间接引用数组。
fn parse_trap_regions(form: &Dict) -> Result<Vec<(i64, i64)>> {
    let Some(value) = form.get("TrapRegions") else {
        return Ok(Vec::new());
    };
    let array = value.as_array().ok_or_else(|| {
        malformed("TrapNet form /TrapRegions entries must be indirect references")
    })?;
    array
        .iter()
        .map(|entry| {
            entry.as_ref().ok_or_else(|| {
                malformed("TrapNet form /TrapRegions entries must be indirect references")
            })
        })
        .collect()
}

/// Table 367 `/TrapStyles`：可选文本串。
fn parse_trap_styles(form: &Dict) -> Result<Option<String>> {
    match form.get("TrapStyles") {
        None => Ok(None),
        Some(value) => crate::core::reader::content::text_value(value)
            .map(Some)
            .ok_or_else(|| malformed("TrapNet form /TrapStyles must be a text string")),
    }
}

/// Table 367 `/FontFauxing`：可选字体字典数组（元素可经间接引用解析）。
fn parse_font_fauxing(document: &Document, form: &Dict) -> Result<Vec<Dict>> {
    let Some(value) = form.get("FontFauxing") else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| malformed("TrapNet form /FontFauxing must be an array"))?;
    array
        .iter()
        .map(|entry| {
            let resolved = resolve(document, entry)?;
            resolved.as_dict().cloned().ok_or_else(|| {
                malformed("TrapNet form /FontFauxing entries must be font dictionaries")
            })
        })
        .collect()
}

fn resolve(document: &Document, value: &Value) -> Result<Value> {
    crate::core::reader::navigation::resolve(document, value)
        .map_err(|error| super::AnnotationError::Malformed(error.to_string()))
}
