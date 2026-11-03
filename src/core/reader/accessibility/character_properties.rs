//! 字体特征与字体选择器属性（ISO 32000-1 14.8.2.4.3「Font Characteristics」，
//! Table 331 与 Table 332；Table 331 随 Tagged PDF 自 PDF 1.4 起，Table 332
//! 的字体选择器属性自 PDF 1.5 起）。
//!
//! 规范原文（要点，pdf32000 第 579–581 页）：
//!
//! > These characteristics are not specified explicitly in the PDF file.
//! > Instead, the conforming reader derives the characteristics from the font
//! > descriptor for the font that is set in the text state at the time the
//! > character is shown. … Each of the characteristics shall be derived from
//! > information available in the font descriptor's Flags entry (see 9.8.2).
//!
//! Table 331 的四行推导（**shall**，全部来自 FontDescriptor `/Flags`，
//! 位序见 9.8.2 Table 123）：
//!
//! - `Serifed` ＝ Serif 标志（位 2）的值；
//! - `Proportional` ＝ FixedPitch 标志（位 1）的**取反**（complement）；
//! - `Italic` ＝ Italic 标志（位 7）的值；
//! - `Smallcap` ＝ SmallCap 标志（位 18）的值。
//!
//! 适用范围（**shall**）：这些特征只作用于内容流显示串（show strings）内的
//! 字符码；替代描述文本 `/Alt`、替换文本 `/ActualText`、缩写展开文本 `/E`
//! 上的文本**不存在**这些特征。因此 [`Document::page_font_character_properties`]
//! 只收集 `Tf` 算子选中的字体（字符显示时刻可能处于文本状态的字体），
//! 仅登记在 `/Resources` 而从未被 `Tf` 选中的字体不出现在结果里。
//!
//! 标准 14 Type 1 字体（9.6.2.2）允许缺省 FontDescriptor，规范用语是
//! "the well-known values for those fonts **shall** be used"——本库内置
//! 这 14 个字体名的众所周知值（Adobe Core 14 AFM 约定下的
//! FixedPitch/Serif/Italic/Symbolic 位、FamilyName 与 Weight 关键词，
//! 宽度表同源，见 `font/standard14_metrics`），见
//! [`super::character_properties_derive`]。
//!
//! Table 332（PDF 1.5）的字体选择器属性：`FontFamily` 取描述符
//! `/FontFamily`（Table 122）；`GenericFontFamily` 按 `/Flags` 的四个分支
//! 规则推导（**shall**，Serif 置位且 FixedPitch 与 Script 均清 → Serif；
//! 三者全清 → SansSerif；Script 置位且 FixedPitch 清 → Cursive；FixedPitch
//! 置位 → Monospace；Decorative 与 Symbol 两个取值规范明言**不可**推导，
//! 故不在枚举内）。`FontStyle` 与 `FontVariant` 是纯标志位的 shall 规则
//! （Italic/SmallCap 置位则 Italic/SmallCaps，否则 Normal）。
//! `FontWeight` 与 `FontStretch` 分别取描述符 `/FontWeight` `/FontStretch`；
//! 规范还补充 FontWeight「should not」用 ForceBold 标志或 StemV 字段设定，
//! 本库不从那两处取值。
//!
//! 当 `/FontFamily`、`/FontWeight`、`/FontStretch` 不在字体描述符中时，
//! 规范说 "these values shall be derived from the font name **in a manner of
//! the conforming reader's choosing**"——推导**方法本身是读者（即本库）
//! 的策略选择，不是规范内容**：本库按名字 token 扫描（族名剥离样式后缀、
//! 权重词映射 100–900 且未识别时按 400、拉伸词映射 Table 122 的九个名字
//! 且未识别时按 `Normal`），并把这类回退逐项记录在
//! [`CharacterProperties::name_derived`]，调用方据此区分规范值与库猜测，
//! 不把这些默认值当作 ISO 32000-1 的规范推导。
//!
//! Table 332 的 `FontSize`「a positive number specifying the height of the
//! typeface in points. Derived from the a, b, c, and d fields of the current
//! text matrix」是「字符显示时刻」的文本状态量而非字体的静态属性，故只由
//! 页面级入口 [`Document::page_font_character_properties`] 与
//! [`Document::page_font_character_property`] 给出：值取自既有文本抽取路径
//! （`reader::text` 的 crate 内入口 `page_font_effective_sizes`），等于
//! `Tf` 字号 × 文本矩阵 `Tm` 的纵向缩放 `hypot(c, d)`。
//!
//! 两处如实的取舍：① 规范只写「Tm 的 a/b/c/d」，但 9.4.4 把 `Tf` 字号作用在
//! **文本空间内部**的字形缩放上，写作端最常见的
//! `1 0 0 1 x y Tm /F1 12 Tf` 若严格只取 Tm 会得到 `FontSize = 1`，与「正数＝
//! 字面高度」矛盾，故本库取两因子之积；② 规范点名的是文本矩阵，**不含** CTM
//! （`cm`、`q`/`Q`、表单 XObject 的 `/Matrix`），本库遵此不折算 CTM 缩放；
//! 横向缩放 `Tz`、字距 `Tc`、行距 `TL` 与 `Ts` 亦不影响字面高度，故不参与推导。
//! 取值取模（负字号/镜像 `Tm` 只翻转不改变高度），非正或不可限时为 `None`——
//! 规范要求正数。字体级入口 [`Document::font_character_properties`] 没有内容流
//! 上下文，`font_size` 恒为 `None`。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::core::reader::content::Op;
use crate::core::types::{Document, PdfFeature, Value};

use super::character_properties_derive;

/// 由字体特征/字体选择器推导失败产生的类型化错误（不 panic）。
#[derive(Debug, Clone, PartialEq)]
pub enum CharacterPropertiesError {
    /// 字体字典、描述符或条目形态不合法（含非标准字体缺描述符等
    /// 违反 Table 111 的形态）。
    Invalid(String),
    /// 页面文本状态引用的字体资源名在页面 `/Resources /Font` 中不存在。
    MissingFontResource(String),
    /// 页面节点、内容流或资源字典无法读取。
    PageContent(String),
}

impl fmt::Display for CharacterPropertiesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => {
                write!(f, "cannot derive font characteristics: {message}")
            }
            Self::MissingFontResource(name) => {
                write!(
                    f,
                    "font resource /{name} is absent from page /Resources/Font"
                )
            }
            Self::PageContent(message) => {
                write!(
                    f,
                    "page content for characteristics cannot be read: {message}"
                )
            }
        }
    }
}

impl std::error::Error for CharacterPropertiesError {}

/// Table 332 `GenericFontFamily`：从 FontDescriptor `/Flags` 可推导的四个
/// 泛用分类。规范 NOTE 明言 Decorative 与 Symbol 两个取值不可推导，故
/// 本枚举只含四项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenericFontFamily {
    /// Serif 置位且 FixedPitch、Script 均未置位。
    Serif,
    /// FixedPitch、Script、Serif 三者均未置位。
    SansSerif,
    /// Script 置位且 FixedPitch 未置位。
    Cursive,
    /// FixedPitch 置位。
    Monospace,
}

impl fmt::Display for GenericFontFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Serif => "Serif",
            Self::SansSerif => "SansSerif",
            Self::Cursive => "Cursive",
            Self::Monospace => "Monospace",
        };
        write!(f, "{name}")
    }
}

/// Table 332 `FontStyle`：斜体化取值。规范为纯 shall 规则——FontDescriptor
/// `Flags` 的 Italic 位（位 7）置位则 Italic，否则 Normal。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontStyle {
    /// Italic 标志未置位。
    Normal,
    /// Italic 标志置位。
    Italic,
}

impl fmt::Display for FontStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::Italic => write!(f, "Italic"),
        }
    }
}

/// Table 332 `FontVariant`：小型大写取值。规范为纯 shall 规则——
/// FontDescriptor `Flags` 的 SmallCap 位（位 18）置位则 SmallCaps，
/// 否则 Normal。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontVariant {
    /// SmallCap 标志未置位。
    Normal,
    /// SmallCap 标志置位。
    SmallCaps,
}

impl fmt::Display for FontVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::SmallCaps => write!(f, "SmallCaps"),
        }
    }
}

/// Table 332 中「描述符缺失时读者可从字体名自行推导」的三个属性。
/// 出现在 [`CharacterProperties::name_derived`] 里即表示该值是**本库的
/// 名字扫描策略**（规范只说 in a manner of the conforming reader's
/// choosing），不是文件内容或规范强制的推导。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NameDerivedAttribute {
    /// `/FontFamily` 缺失，族名来自字体名扫描。
    FontFamily,
    /// `/FontWeight` 缺失，权重来自字体名扫描。
    FontWeight,
    /// `/FontStretch` 缺失，拉伸度来自字体名扫描。
    FontStretch,
}

/// 一个字体在 Tagged PDF 文本提取时刻应携带的推导特征集
/// （Table 331 + Table 332）。
///
/// 所有条目都是「读者推导值」而非文件值：Table 331 四个布尔与
/// Table 332 的标志位规则（Generic/Style/Variant）完全取决于
/// FontDescriptor `/Flags`；描述符与 `Flags` 都不可得（非标准字体缺
/// 描述符、Type 3 无描述符、TrueType 未给出描述符等）时对应字段为
/// `None`，**不臆造默认值**。标准 14 字体在描述符缺失时改用众所周知值
/// （`from_standard14_well_known` 标记）；`/FontFamily` `/FontWeight`
/// `/FontStretch` 缺失时的名字回退逐项记录在 `name_derived`。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterProperties {
    /// Table 331 `Serifed`：`Flags` Serif 标志（位 2）的值。
    pub serifed: Option<bool>,
    /// Table 331 `Proportional`：`Flags` FixedPitch 标志（位 1）的**取反**。
    pub proportional: Option<bool>,
    /// Table 331 `Italic`：`Flags` Italic 标志（位 7）的值。
    pub italic: Option<bool>,
    /// Table 331 `Smallcap`：`Flags` SmallCap 标志（位 18）的值。
    pub smallcap: Option<bool>,
    /// Table 332 `FontFamily`：描述符 `/FontFamily`；缺失时为标准 14
    /// 众所周知族名或名字扫描值（后者记入 `name_derived`）。
    pub font_family: Option<String>,
    /// Table 332 `GenericFontFamily`：按 `/Flags` 的分支规则推导；规范语义
    /// 是「FontFamily 未找到时使用」，本库在标志位可得时总是同时给出，
    /// 由调用方按 needs 取用。
    pub generic_font_family: Option<GenericFontFamily>,
    /// Table 332 `FontSize`：字符显示时刻的字面高度（正数），
    /// `Tf` 字号 × 文本矩阵 `Tm` 纵向缩放 `hypot(c, d)`。
    ///
    /// 只有页面级入口（[`Document::page_font_character_properties`] 与
    /// [`Document::page_font_character_property`]）会填此值，且只在该字体
    /// **确实显示过显示串**时给出：字体级入口 [`Document::font_character_properties`]
    /// 没有内容流上下文，被 `Tf` 选中却从未显示的字体、以及文本被
    /// `/ActualText` 替换的区段（14.8.2.4.3：特征不存在于 `/Alt`
    /// `/ActualText` `/E` 文本）都是 `None`，不臆造。同一字体在一页内有多个
    /// 字号时取**最大**观测值（本模型按资源名键控，无法逐次表达）；CTM
    /// （`cm`、表单 `/Matrix`）按规范措辞不折算，见模块文档。
    pub font_size: Option<f64>,
    /// Table 332 `FontWeight`：描述符 `/FontWeight`（Table 122 的 100–900）；
    /// 缺失时为名字扫描值（记入 `name_derived`）。规范「should not」条款：
    /// 不用 ForceBold 标志或 StemV 推导，本库遵从此建议。
    pub font_weight: Option<f64>,
    /// Table 332 `FontStretch`：描述符 `/FontStretch` 名字（Table 122 的九个
    /// 取值之一，原样保留）；缺失时为名字扫描值（记入 `name_derived`）。
    pub font_stretch: Option<String>,
    /// Table 332 `FontStyle`：Italic 标志置位则 `Italic`，否则 `Normal`
    /// （shall）；标志不可得为 `None`。
    pub font_style: Option<FontStyle>,
    /// Table 332 `FontVariant`：SmallCap 标志置位则 `SmallCaps`，否则
    /// `Normal`（shall）；标志不可得为 `None`。
    pub font_variant: Option<FontVariant>,
    /// true 表示描述符整体缺失、Table 331/332 的标志位与族名/权重/拉伸
    /// 值来自标准 14 Type 1 字体的众所周知值表。
    pub from_standard14_well_known: bool,
    /// 因「描述符未含该字段而从字体名推导」而落入本库名字策略的属性集
    /// （规范把推导方式交给读者，故逐属性显式标记）。
    pub name_derived: BTreeSet<NameDerivedAttribute>,
}

impl Document {
    /// 推导单个字体字典（直接对象或间接引用）的特征集（Table 331/332）。
    ///
    /// 支持 Type 1、MMType1、TrueType、Type 0（经第一个 DescendantFont 的
    /// 描述符）与 Type 3。Type 3 在 ISO 32000-1 中没有描述符入口：除通过
    /// [`Document::font_character_properties`] 读到（非 1.7 规范的）
    /// `/FontDescriptor` 外，标志位规则项为 `None`，只保留名字回退的
    /// 族名/权重/拉伸。
    ///
    /// 非标准 Type 1/MMType1 缺描述符（违反 Table 111 的 Required 条款）、
    /// 未知 `/Subtype`、描述符条目形态错误等都返回
    /// [`CharacterPropertiesError::Invalid`]，绝不 panic。成功推导时向
    /// 特性画像登记 [`PdfFeature::FontCharacteristics`]（1.4 门槛）。
    pub fn font_character_properties(
        &self,
        font: &Value,
    ) -> Result<CharacterProperties, CharacterPropertiesError> {
        let properties = character_properties_derive::derive(self, font)?;
        self.features
            .borrow_mut()
            .insert(PdfFeature::FontCharacteristics);
        Ok(properties)
    }

    /// 推导一页文本状态可能用到的所有字体的特征集（按资源名键控）。
    ///
    /// 只收集内容流里 `Tf` 算子选中的字体名——这正是 14.8.2.4.3 的
    /// 「the font that is set in the text state at the time the character
    /// is shown」，也即特征只作用于显示串、不作用于 `/Alt` `/ActualText`
    /// `/E` 文本的建模方式：未被 `Tf` 选中的资源、以及属性表文本引用的
    /// 字体都不参与。表单 XObject 的内容流不被递归（沿用
    /// [`Document::page_content_ops`] 的行为）。
    ///
    /// Table 332 `FontSize` 由既有文本抽取路径的显示时刻文本矩阵给出（见
    /// [`CharacterProperties::font_size`]）；该次抽取失败（内容超预算等）时
    /// 只缺字号，其余条目仍按字体字典推导，不把整次查询降级为错误。
    ///
    /// `Tf` 引用的名字不在页面 `/Resources /Font` 中时返回
    /// [`CharacterPropertiesError::MissingFontResource`]。
    pub fn page_font_character_properties(
        &self,
        page_index: usize,
    ) -> Result<BTreeMap<String, CharacterProperties>, CharacterPropertiesError> {
        let ops = self
            .page_content_ops(page_index)
            .map_err(|error| CharacterPropertiesError::PageContent(error.to_string()))?;
        let mut selected = BTreeSet::new();
        for op in &ops {
            if let Op::SetFont(name, _) = op {
                selected.insert(name.clone());
            }
        }
        let sizes = self
            .page_font_effective_sizes(page_index)
            .unwrap_or_default();
        let mut properties = BTreeMap::new();
        for name in selected {
            let mut property = self.page_font_property_from_resource(page_index, &name)?;
            property.font_size = sizes.get(&name).copied();
            properties.insert(name, property);
        }
        Ok(properties)
    }

    /// 按页面字体资源名（`Tf` 操作数，`#xx` 转义已按 7.3.5 处理）推导
    /// 单个字体的特征集。
    ///
    /// 名字不存在于页面 `/Resources /Font` 时返回
    /// [`CharacterPropertiesError::MissingFontResource`]；字体字典形态
    /// 错误按 [`CharacterPropertiesError::Invalid`] 返回。
    ///
    /// Table 332 `FontSize` 同样按显示时刻取值，故该名字在本页未被显示串
    /// 使用时为 `None`；与 [`Document::page_font_character_properties`] 中
    /// 同名条目语义一致。
    pub fn page_font_character_property(
        &self,
        page_index: usize,
        name: &str,
    ) -> Result<CharacterProperties, CharacterPropertiesError> {
        let mut properties = self.page_font_property_from_resource(page_index, name)?;
        let sizes = self
            .page_font_effective_sizes(page_index)
            .unwrap_or_default();
        properties.font_size = sizes.get(name).copied();
        Ok(properties)
    }

    /// 页面资源名 → 字体字典 → Table 331/332 的字体级推导（不含显示时刻的
    /// `FontSize`，由上面两个页面级入口各自补填）。
    fn page_font_property_from_resource(
        &self,
        page_index: usize,
        name: &str,
    ) -> Result<CharacterProperties, CharacterPropertiesError> {
        let value = self
            .page_resource(page_index, "Font", name)
            .map_err(|error| CharacterPropertiesError::PageContent(error.to_string()))?
            .ok_or_else(|| CharacterPropertiesError::MissingFontResource(name.to_owned()))?;
        self.font_character_properties(&value)
    }
}
