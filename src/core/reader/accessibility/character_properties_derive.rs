//! Table 331/Table 332 推导的内部实现：字体字典与描述符视图解析、
//! 标准 14 众所周知值表、以及「字体名回退」的本库策略（规范只说
//! in a manner of the conforming reader's choosing，见
//! [`super::character_properties`] 模块文档）。

use crate::core::reader::font::strip_subset_tag;
use crate::core::types::{Dict, Document, PdfTextString, Value};

use super::character_properties::{
    CharacterProperties, CharacterPropertiesError, FontStyle, FontVariant, GenericFontFamily,
    NameDerivedAttribute,
};

fn invalid(message: impl Into<String>) -> CharacterPropertiesError {
    CharacterPropertiesError::Invalid(message.into())
}

/// FontDescriptor 中与 14.8.2.4.3 推导相关的五个条目（Table 122）。
/// 其余条目（FontBBox、StemV 等）与本节无关，不校验也不读取。
pub(super) struct DescriptorView {
    pub(super) font_family: Option<String>,
    pub(super) font_stretch: Option<String>,
    pub(super) font_weight: Option<f64>,
    pub(super) flags: Option<u32>,
}

/// 标准 14 Type 1 字体的众所周知值（14.8.2.4.3：描述符缺失时
/// "the well-known values for those fonts shall be used"）。
///
/// 标志位取 Adobe Core 14 AFM 约定的位语义（IsFixedPitch → FixedPitch、
/// Serif/Italic/Symbolic），族名取 AFM `FamilyName`，粗细取 AFM `Weight`
/// 关键词（Medium → Table 122 的 400、Bold → 700），拉伸全部 `Normal`。
/// 与 `font/standard14_metrics` 的宽度表同源。
#[derive(Debug, Clone, Copy)]
pub(super) struct Standard14WellKnown {
    pub(super) family: &'static str,
    pub(super) flags: u32,
    pub(super) weight: f64,
    pub(super) stretch: &'static str,
}

/// FontDescriptor 标志位（9.8.2 Table 123；位序从 1 起）。
const FLAG_FIXED_PITCH: u32 = 1 << 0;
const FLAG_SERIF: u32 = 1 << 1;
const FLAG_SYMBOLIC: u32 = 1 << 2;
const FLAG_SCRIPT: u32 = 1 << 3;
const FLAG_ITALIC: u32 = 1 << 6;
const FLAG_SMALL_CAP: u32 = 1 << 17;

pub(super) fn standard14_well_known(base_font: &str) -> Option<Standard14WellKnown> {
    let (family, flags, weight) = match strip_subset_tag(base_font) {
        "Courier" => ("Courier", FLAG_FIXED_PITCH, 400.0),
        "Courier-Bold" => ("Courier", FLAG_FIXED_PITCH, 700.0),
        "Courier-Oblique" => ("Courier", FLAG_FIXED_PITCH | FLAG_ITALIC, 400.0),
        "Courier-BoldOblique" => ("Courier", FLAG_FIXED_PITCH | FLAG_ITALIC, 700.0),
        "Helvetica" => ("Helvetica", 0, 400.0),
        "Helvetica-Bold" => ("Helvetica", 0, 700.0),
        "Helvetica-Oblique" => ("Helvetica", FLAG_ITALIC, 400.0),
        "Helvetica-BoldOblique" => ("Helvetica", FLAG_ITALIC, 700.0),
        "Times-Roman" => ("Times", FLAG_SERIF, 400.0),
        "Times-Bold" => ("Times", FLAG_SERIF, 700.0),
        "Times-Italic" => ("Times", FLAG_SERIF | FLAG_ITALIC, 400.0),
        "Times-BoldItalic" => ("Times", FLAG_SERIF | FLAG_ITALIC, 700.0),
        "Symbol" => ("Symbol", FLAG_SYMBOLIC, 400.0),
        "ZapfDingbats" => ("ZapfDingbats", FLAG_SYMBOLIC, 400.0),
        _ => return None,
    };
    Some(Standard14WellKnown {
        family,
        flags,
        weight,
        stretch: "Normal",
    })
}

/// Table 331 四行 + Table 332 中纯标志位规则（Generic/Style/Variant）。
/// `Proportional` 是 FixedPitch 的取反——规范用词 complement，非独立标志。
fn apply_descriptor_flags(properties: &mut CharacterProperties, flags: u32) {
    let fixed_pitch = flags & FLAG_FIXED_PITCH != 0;
    let serif = flags & FLAG_SERIF != 0;
    let script = flags & FLAG_SCRIPT != 0;
    let italic = flags & FLAG_ITALIC != 0;
    let small_cap = flags & FLAG_SMALL_CAP != 0;
    properties.serifed = Some(serif);
    properties.proportional = Some(!fixed_pitch);
    properties.italic = Some(italic);
    properties.smallcap = Some(small_cap);
    // Table 332 GenericFontFamily 分支：Monospace 覆盖一切 FixedPitch；
    // Cursive 要求 Script 置位且 FixedPitch 清（即使 Serif 同置也归 Cursive）；
    // Serif 行自带「FixedPitch 与 Script 未置位」前提；其余为 SansSerif。
    properties.generic_font_family = Some(if fixed_pitch {
        GenericFontFamily::Monospace
    } else if script {
        GenericFontFamily::Cursive
    } else if serif {
        GenericFontFamily::Serif
    } else {
        GenericFontFamily::SansSerif
    });
    properties.font_style = Some(if italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    });
    properties.font_variant = Some(if small_cap {
        FontVariant::SmallCaps
    } else {
        FontVariant::Normal
    });
}

/// 把值解引用一层（字体字典与描述符按 Table 111 应为间接引用，
/// 直接字典同样接受——读端宽容，见模块文档）。
fn resolve_value(document: &Document, value: &Value) -> Result<Value, CharacterPropertiesError> {
    match value {
        Value::Ref(num, generation) => document
            .get_object(*num)
            .map_err(|error| {
                invalid(format!(
                    "font object reference {num} {generation} R cannot be read: {error}"
                ))
            })?
            .map(|object| object.value)
            .ok_or_else(|| {
                invalid(format!(
                    "font object reference {num} {generation} R does not exist"
                ))
            }),
        other => Ok(other.clone()),
    }
}

fn name_entry(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<String, CharacterPropertiesError> {
    match resolve_value(document, value)? {
        Value::Name(name) => Ok(name.clone()),
        other => Err(invalid(format!(
            "{context} is not a name ({})",
            other.type_name()
        ))),
    }
}

fn optional_name(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<Option<String>, CharacterPropertiesError> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => name_entry(document, value, &format!("/FontDescriptor /{key}")).map(Some),
    }
}

fn optional_text(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<Option<String>, CharacterPropertiesError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    match resolve_value(document, value)? {
        Value::String(text) => Ok(Some(text.clone())),
        Value::HexString(text) => {
            let message = format!("/FontDescriptor /{key} is not a decodable text string");
            let bytes = hex_bytes(&text).ok_or_else(|| invalid(message.clone()))?;
            PdfTextString::decode(&bytes)
                .map(|decoded| Some(decoded.as_str().to_owned()))
                .map_err(|_| invalid(message))
        }
        other => Err(invalid(format!(
            "/FontDescriptor /{key} is not a text string ({})",
            other.type_name()
        ))),
    }
}

fn optional_number(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<Option<f64>, CharacterPropertiesError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    match resolve_value(document, value)? {
        Value::Integer(number) => Ok(Some(number as f64)),
        Value::Float(number) if number.is_finite() => Ok(Some(number)),
        other => Err(invalid(format!(
            "/FontDescriptor /{key} is not a number ({})",
            other.type_name()
        ))),
    }
}

fn optional_flags(
    document: &Document,
    dict: &Dict,
) -> Result<Option<u32>, CharacterPropertiesError> {
    let Some(value) = dict.get("Flags") else {
        return Ok(None);
    };
    match resolve_value(document, value)? {
        Value::Integer(number) => u32::try_from(number)
            .ok()
            .map(Some)
            .ok_or_else(|| invalid("invalid /FontDescriptor /Flags".to_string())),
        other => Err(invalid(format!(
            "/FontDescriptor /Flags is not an integer ({})",
            other.type_name()
        ))),
    }
}

fn hex_bytes(value: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for index in (0..digits.len()).step_by(2) {
        let high = nibble(digits[index])? << 4;
        let low = nibble(digits[index + 1])?;
        bytes.push(high | low);
    }
    Some(bytes)
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn descriptor_view(
    document: &Document,
    font_dict: &Dict,
) -> Result<Option<DescriptorView>, CharacterPropertiesError> {
    let Some(value) = font_dict.get("FontDescriptor") else {
        return Ok(None);
    };
    let resolved = resolve_value(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| invalid("/FontDescriptor is not a dictionary".to_string()))?;
    if dict.get("Type").and_then(Value::as_name) != Some("FontDescriptor") {
        return Err(invalid(
            "/FontDescriptor has a missing or invalid /Type".to_string(),
        ));
    }
    Ok(Some(DescriptorView {
        font_family: optional_text(document, dict, "FontFamily")?,
        font_stretch: optional_name(document, dict, "FontStretch")?,
        font_weight: optional_number(document, dict, "FontWeight")?,
        flags: optional_flags(document, dict)?,
    }))
}

/// Type 0 字体经第一个 DescendantFont 的描述符推导（9.7.4.1：描述符挂在
/// CIDFont 上而非 Type 0 父字典）。
fn descendant_descriptor_view(
    document: &Document,
    type0_dict: &Dict,
) -> Result<Option<DescriptorView>, CharacterPropertiesError> {
    let value = type0_dict
        .get("DescendantFonts")
        .ok_or_else(|| invalid("Type 0 font is missing /DescendantFonts".to_string()))?;
    let resolved = resolve_value(document, value)?;
    let list = resolved
        .as_array()
        .ok_or_else(|| invalid("Type 0 /DescendantFonts is not an array".to_string()))?;
    let first = list
        .0
        .first()
        .ok_or_else(|| invalid("Type 0 /DescendantFonts array is empty".to_string()))?;
    let resolved = resolve_value(document, first)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| invalid("Type 0 descendant font is not a dictionary".to_string()))?;
    descriptor_view(document, dict)
}

/// 本库的名字扫描策略（规范把方式交给读者）：剥离样式后缀取族名。
pub(super) fn derived_font_family(base_font: &str) -> String {
    const STYLE_SUFFIXES: &[&str] = &[
        "bolditalic",
        "boldoblique",
        "italic",
        "oblique",
        "bold",
        "roman",
        "regular",
    ];
    let stripped = strip_subset_tag(base_font);
    let mut parts: Vec<&str> = stripped.split(['-', '_']).collect();
    while let Some(last) = parts.last() {
        let lower = last.to_ascii_lowercase();
        if STYLE_SUFFIXES.contains(&lower.as_str()) {
            parts.pop();
        } else {
            break;
        }
    }
    let mut family = if parts.is_empty() {
        stripped.to_owned()
    } else {
        parts.join("-")
    };
    // 连写后缀（如 "ArialBoldItalic"）：剥离尾部样式词，保留非空主干。
    let lower = family.to_ascii_lowercase();
    for suffix in STYLE_SUFFIXES {
        if lower.len() > suffix.len() && lower.ends_with(suffix) {
            family.truncate(lower.len() - suffix.len());
            break;
        }
    }
    family
}

/// 名字里的权重词映射到 Table 122 的 100–900；未识别时按 400（本库默认，
/// 记入 `name_derived` 即表明这是读者策略而非规范值）。顺序保证
/// semi/demi/extra/ultra 复合词先于简单词匹配。
pub(super) fn derived_font_weight(base_font: &str) -> f64 {
    const WEIGHT_TOKENS: &[(&str, f64)] = &[
        ("black", 900.0),
        ("heavy", 900.0),
        ("extrabold", 800.0),
        ("ultrabold", 800.0),
        ("semibold", 600.0),
        ("demibold", 600.0),
        ("extralight", 200.0),
        ("ultralight", 200.0),
        ("thin", 100.0),
        ("bold", 700.0),
        ("medium", 500.0),
        ("light", 300.0),
    ];
    let lower = strip_subset_tag(base_font).to_ascii_lowercase();
    WEIGHT_TOKENS
        .iter()
        .find(|(token, _)| lower.contains(token))
        .map_or(400.0, |(_, weight)| *weight)
}

/// 名字里的拉伸词映射到 Table 122 的九个 `/FontStretch` 名字之一；
/// 未识别时按 `Normal`（本库默认，同上标记）。
pub(super) fn derived_font_stretch(base_font: &str) -> String {
    const STRETCH_TOKENS: &[(&str, &str)] = &[
        ("ultracondensed", "UltraCondensed"),
        ("extracondensed", "ExtraCondensed"),
        ("semicondensed", "SemiCondensed"),
        ("ultraexpanded", "UltraExpanded"),
        ("extraexpanded", "ExtraExpanded"),
        ("semiexpanded", "SemiExpanded"),
        ("condensed", "Condensed"),
        ("narrow", "Condensed"),
        ("compressed", "Condensed"),
        ("expanded", "Expanded"),
        ("wide", "Expanded"),
    ];
    let lower = strip_subset_tag(base_font).to_ascii_lowercase();
    STRETCH_TOKENS
        .iter()
        .find(|(token, _)| lower.contains(token))
        .map_or("Normal", |(_, name)| *name)
        .to_owned()
}

/// 从字体资源值（字典或间接引用）推导 [`CharacterProperties`]。
pub(super) fn derive(
    document: &Document,
    font: &Value,
) -> Result<CharacterProperties, CharacterPropertiesError> {
    let resolved = resolve_value(document, font)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| invalid("font resource is not a dictionary".to_string()))?;
    let subtype = dict
        .get("Subtype")
        .and_then(Value::as_name)
        .ok_or_else(|| invalid("font dictionary has no /Subtype name".to_string()))?;
    if !matches!(
        subtype,
        "Type1" | "MMType1" | "TrueType" | "Type3" | "Type0"
    ) {
        return Err(invalid(format!("unsupported font subtype /{subtype}")));
    }
    let base_font = match dict.get("BaseFont") {
        Some(value) => Some(name_entry(document, value, "font dictionary /BaseFont")?),
        // Type 3 字体字典没有 /BaseFont（9.6.5）；其余子类型缺失即畸形。
        None if subtype == "Type3" => None,
        None => {
            return Err(invalid(format!(
                "{subtype} font dictionary is missing /BaseFont"
            )));
        }
    };
    let descriptor = if subtype == "Type0" {
        descendant_descriptor_view(document, dict)?
    } else {
        descriptor_view(document, dict)?
    };
    if descriptor.is_none()
        && matches!(subtype, "Type1" | "MMType1")
        && base_font
            .as_deref()
            .is_none_or(|name| standard14_well_known(name).is_none())
    {
        // Table 111：除标准 14 外 FontDescriptor 为 Required——缺失即畸形字体。
        return Err(invalid(format!(
            "{subtype} font requires a /FontDescriptor unless it is one of the standard 14 fonts"
        )));
    }
    let well_known = if descriptor.is_none() && subtype == "Type1" {
        base_font.as_deref().and_then(standard14_well_known)
    } else {
        None
    };

    let mut properties = CharacterProperties::default();
    if let Some(descriptor) = &descriptor {
        if let Some(flags) = descriptor.flags {
            apply_descriptor_flags(&mut properties, flags);
        }
    } else if let Some(well_known) = well_known {
        apply_descriptor_flags(&mut properties, well_known.flags);
        properties.from_standard14_well_known = true;
    }

    // Table 332 三个描述符条目；「不在描述符中」的按规范从字体名回退，
    // 方式为本库策略（in a manner of the conforming reader's choosing）。
    properties.font_family = descriptor.as_ref().and_then(|d| d.font_family.clone());
    properties.font_weight = descriptor.as_ref().and_then(|d| d.font_weight);
    properties.font_stretch = descriptor.as_ref().and_then(|d| d.font_stretch.clone());
    if properties.font_family.is_none() {
        if let Some(well_known) = well_known {
            properties.font_family = Some(well_known.family.to_owned());
        } else if let Some(name) = &base_font {
            properties.font_family = Some(derived_font_family(name));
            properties
                .name_derived
                .insert(NameDerivedAttribute::FontFamily);
        }
    }
    if properties.font_weight.is_none() {
        if let Some(well_known) = well_known {
            properties.font_weight = Some(well_known.weight);
        } else if let Some(name) = &base_font {
            properties.font_weight = Some(derived_font_weight(name));
            properties
                .name_derived
                .insert(NameDerivedAttribute::FontWeight);
        }
    }
    if properties.font_stretch.is_none() {
        if let Some(well_known) = well_known {
            properties.font_stretch = Some(well_known.stretch.to_owned());
        } else if let Some(name) = &base_font {
            properties.font_stretch = Some(derived_font_stretch(name));
            properties
                .name_derived
                .insert(NameDerivedAttribute::FontStretch);
        }
    }
    Ok(properties)
}
