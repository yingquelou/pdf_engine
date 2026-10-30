//! 输出意图写出（ISO 32000-1 14.11.5 Table 365）。
//!
//! 目录 `/OutputIntents`（PDF 1.4）数组的每个条目写出 `/Type
//! /OutputIntent`、`/S`、按 Table 365 声明的文本条目与
//! `/DestOutputProfile` 间接流引用。校验在分配任何对象号之前完成：
//! 版本门禁、扩展子类型名称语法、`/N` 分量范围、标识非空，以及
//! `Custom` 标识的 `/Info` + `/DestOutputProfile` 条件性要求。

use crate::core::builder::types::{OutputIntent, text_string_value, validate_name_value};
use crate::core::reader::output_intent::OutputIntentSubtype;
use crate::core::types::{Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Value};
use crate::core::writer::WriteError;

/// 注册校验：Table 365 条目契约与 PDF 1.4 版本门禁，在分配任何对象号之前调用。
pub(super) fn validate_registration(
    intents: &[OutputIntent],
    version: PdfVersion,
) -> Result<(), WriteError> {
    if intents.is_empty() {
        return Ok(());
    }
    if !version.supports(PdfFeature::OutputIntents) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::OutputIntents,
            PdfFeature::OutputIntents.min_version().as_str(),
            version.as_str()
        )));
    }
    for (index, intent) in intents.iter().enumerate() {
        if let OutputIntentSubtype::Other(name) = &intent.subtype {
            validate_name_value(name, &format!("output intent {index} /S"))
                .map_err(WriteError::InvalidObject)?;
        }
        if intent.identifier.is_empty() {
            return Err(WriteError::InvalidObject(format!(
                "output intent {index} /OutputConditionIdentifier must not be empty"
            )));
        }
        if let Some(profile) = &intent.profile
            && !(1..=4).contains(&profile.components)
        {
            return Err(WriteError::InvalidObject(format!(
                "output intent {index} /DestOutputProfile /N must be 1 to 4 components, got {}",
                profile.components
            )));
        }
        // Table 365：OutputConditionIdentifier 未指定标准生产条件时
        // （规范以 `Custom` 标记），/Info 与 /DestOutputProfile 为必需条目。
        if intent.identifier == "Custom" {
            if intent.info.is_none() {
                return Err(WriteError::InvalidObject(format!(
                    "output intent {index} /Info is required when /OutputConditionIdentifier is Custom (Table 365)"
                )));
            }
            if intent.profile.is_none() {
                return Err(WriteError::InvalidObject(format!(
                    "output intent {index} /DestOutputProfile is required when /OutputConditionIdentifier is Custom (Table 365)"
                )));
            }
        }
    }
    Ok(())
}

/// 写出各意图的 `/DestOutputProfile` 流对象，返回目录数组元素。
///
/// 配置文件流在 PDF 语法中必须间接，故每一项先由共享游标分配对象号；
/// 文本条目一律按 PDF 文本串写成十六进制字符串。
pub(super) fn write_objects(
    doc: &mut Document,
    intents: &[OutputIntent],
    num: &mut i64,
) -> Vec<Value> {
    let mut values = Vec::with_capacity(intents.len());
    for intent in intents {
        let profile = intent.profile.as_ref().map(|profile| {
            let number = *num;
            *num += 1;
            let dict = Dict::new()
                .insert("N", Value::Integer(i64::from(profile.components)))
                .insert("Length", Value::Integer(profile.data.len() as i64));
            doc.set_object(Object::new(
                number,
                0,
                Value::Stream(dict, StreamData::Owned(profile.data.clone())),
            ));
            Value::Ref(number, 0)
        });

        let mut dict = Dict::new()
            .insert("Type", Value::Name("OutputIntent".into()))
            .insert("S", Value::Name(intent.subtype.name().to_owned()));
        if let Some(condition) = &intent.output_condition {
            dict = dict.insert("OutputCondition", text_string_value(condition));
        }
        dict = dict.insert(
            "OutputConditionIdentifier",
            text_string_value(&intent.identifier),
        );
        if let Some(registry) = &intent.registry_name {
            dict = dict.insert("RegistryName", text_string_value(registry));
        }
        if let Some(info) = &intent.info {
            dict = dict.insert("Info", text_string_value(info));
        }
        if let Some(profile) = profile {
            dict = dict.insert("DestOutputProfile", profile);
        }
        values.push(Value::Dict(dict));
    }
    values
}
