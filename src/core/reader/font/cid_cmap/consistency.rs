//! CMap ↔ CIDFont 一致性诊断出口（9.7.3 / 9.7.5.2 / 9.7.5.3，Table 116–120）。
//!
//! 本模块**只报告、不改变行为**：[`Document::cmap_consistency_issues`] 按需调用，
//! 读取阶段不因诊断报错，既有 CMap 解析与文本抽取结果一律不变。风格照
//! `reader/structure` 的 `content_model_violations()` ＋ `is_mandatory()`（区分
//! shall/should）与 `linearization` 的 `shared_object_index_issues()`。
//!
//! 已确证的规范依据（`pdftotext -raw docs/PDF32000_2008.pdf` 的 raw 行号）：
//! - 9.7.3：「CIDFont and CMap dictionaries **shall** contain a CIDSystemInfo
//!   entry …」（L14519–14521）；「In order to be compatible, the Registry and
//!   Ordering values must be the same.」（L14522–14523）；「For proper behaviour,
//!   the CIDSystemInfo entry of a CMap **shall** be compatible with that of the
//!   CIDFont or CIDFonts with which it is used.」（L14529–14530）。
//! - Table 116 Supplement：「This value **shall not** be used in determining
//!   compatibility between character collections.」（L14560–14561）→ Supplement
//!   不等只作 should 线索，绝不判 shall。
//! - Table 120：条目必填性与「shall be the same as the value … in the CMap
//!   file」（L14929–14943）；Identity-H/V 的 CIDSystemInfo 豁免（L14937–14938）
//!   与 Table 119「may be used with CIDFonts using any Registry, Ordering, and
//!   Supplement values」（L14849–14851）。
//! - 9.7.5.2：「The Identity-H and Identity-V CMaps **shall not** be used with a
//!   non-embedded font.」（L14912–14913）。
//! - 9.7.5.4 约束 (b)：「The usefont operator, if present, **shall** specify a
//!   font number of 0.」（L14953）。
//!
//! **未采**：`/CMapType` 与 `/UID` 不是 Table 120 条目（理由见 `cmap_dict` 模块
//! 文档）；ISO 32000-1 也没有在 `/CMapType` 与后代 CIDFont 的 `CIDFontType`
//! （Type 0 CIDFont 基于 CFF、Type 2 基于 TrueType，L14544 与 L14546）之间建立
//! 任何对应关系，因此这里只对该取值判「范围/与流内一致」的 should 级线索，
//! `/UID` 只读取入模型、不判定。

use crate::core::types::{Dict, Document, Value};

use super::cmap_dict::{CMapDictionary, CMapEntry, CMapWritingMode};
use crate::core::reader::font::CidSystemInfo;
use crate::core::reader::font::cid::parse_cid_system_info;

/// 发现所属的 CMap 位置（Type 0 字体可以有两处 CMap 流）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CMapPosition {
    /// `/Encoding` CMap：决定字符码到 CID 的映射。
    Encoding,
    /// `/ToUnicode` CMap：只用于反查 Unicode。
    ToUnicode,
}

/// 一条 CMap 一致性发现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CMapConsistencyIssue {
    /// 发现落在哪一处 CMap 上。
    pub position: CMapPosition,
    /// 违反的规则。
    pub rule: CMapConsistencyRule,
}

/// 违反的 CMap 一致性规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CMapConsistencyRule {
    /// Table 120 的 Required 条目整体缺失（`key` 不含前导 `/`）。
    RequiredEntryMissing {
        /// 缺失的条目名，如 `"CMapName"`。
        key: String,
    },
    /// Table 120 的条目存在但形态不符（类型不对、引用悬空、子字典不可解析）。
    EntryMalformed {
        /// 条目名。
        key: String,
        /// 实际形态描述。
        found: String,
    },
    /// `/Type` 不是 `CMap`（Table 120：shall be CMap，L14929–14930）。
    TypeNotCMap {
        /// 实际 `/Type` 值。
        found: String,
    },
    /// 字典 `/CMapName` 与 CMap 文件内 `CMapName` 声明不一致
    /// （Table 120，L14931–14932）。
    CMapNameDiffersFromFile {
        /// 字典声明的名字。
        dict: String,
        /// 流内 `def` 声明的名字。
        file: String,
    },
    /// 字典 `/WMode` 与流内 `/WMode` 声明不一致（Table 120，L14942–14943）。
    WModeDiffersFromFile {
        /// 字典声明的取值。
        dict: i64,
        /// 流内声明的取值。
        file: i64,
    },
    /// `/WMode` 是整数但不在 `0..=1`（Table 120，L14940–14941）。
    WModeOutOfRange {
        /// 声明的原始取值，一律原样保留。
        value: i64,
    },
    /// CMap 与 CIDFont 的 `/CIDSystemInfo` 字符集不兼容：Registry 或 Ordering
    /// 不同（9.7.3，L14522–14523 加 L14529–14530）。
    CidSystemInfoIncompatible {
        /// CMap 侧的 `Registry/Ordering`。
        cmap: String,
        /// CIDFont 侧的 `Registry/Ordering`。
        font: String,
    },
    /// CMap 与 CIDFont 的 Supplement 不同。
    ///
    /// Table 116 明确该值「shall not be used in determining compatibility」
    /// （L14560–14561），故只作 should 线索（另见 NOTE 4，L14914–14916：写端
    /// 遇到更高 supplement 时应改嵌更高 supplement 的 CMap）。
    CidSystemInfoSupplementDiffers {
        /// CMap 侧 Supplement。
        cmap: i64,
        /// CIDFont 侧 Supplement。
        font: i64,
    },
    /// `/Encoding` 是 Identity-H/V 而后代 CIDFont 未内嵌字体程序
    /// （9.7.5.2，L14912–14913）。
    IdentityEncodingWithoutEmbeddedFont,
    /// `usefont` 指定了非 0 字体号（9.7.5.4 约束 (b)，L14953）。
    NonZeroUseFontNumber {
        /// 声明的字体号。
        value: i64,
    },
    /// 字典 `/CMapType` 不在 `1..=2`（非 Table 120 条目，取值语义属
    /// Adobe TN #5014；库策略线索）。
    CMapTypeUnsupported {
        /// 声明的取值。
        value: i64,
    },
    /// 字典 `/CMapType` 与流内 `/CMapType` 声明不一致（同上，库策略线索）。
    CMapTypeDiffersFromFile {
        /// 字典声明的取值。
        dict: i64,
        /// 流内声明的取值。
        file: i64,
    },
}

impl CMapConsistencyRule {
    /// 该规则是否为规范强制（shall）；否则为规范建议（should）或库策略线索。
    pub fn is_mandatory(&self) -> bool {
        !matches!(
            self,
            Self::CidSystemInfoSupplementDiffers { .. }
                | Self::CMapTypeUnsupported { .. }
                | Self::CMapTypeDiffersFromFile { .. }
        )
    }
}

/// Type 0 字体两处 CMap 流字典（Table 120）的类型化读取结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Type0CMaps {
    /// `/Encoding` CMap 流字典；预定义名引用（如 `Identity-H`）或缺失时为 `None`。
    pub encoding: Option<CMapDictionary>,
    /// `/ToUnicode` CMap 流字典；缺失或非流时为 `None`。
    pub to_unicode: Option<CMapDictionary>,
}

/// 诊断上下文：两处 CMap 取值与后代 CIDFont 字典。
struct Type0Context<'a> {
    /// `/Encoding` 原始取值（名字或流）。
    encoding_raw: Option<&'a Value>,
    /// `/Encoding` 的流形态取值。
    encoding: Option<&'a Value>,
    to_unicode: Option<&'a Value>,
    descendant: Option<Dict>,
}

impl Document {
    /// 读取 Type 0 字体两处 CMap 流（Table 120）的类型化字典模型。
    ///
    /// 非 Type 0 字体返回两个 `None`；只读，不影响任何既有解析。
    pub fn type0_cmap_dictionaries(&self, font: &Value) -> Type0CMaps {
        let Ok(value) = self.resolve_value(font) else {
            return Type0CMaps::default();
        };
        let context = match value.as_dict().and_then(|dict| self.type0_context(dict)) {
            Some(context) => context,
            None => return Type0CMaps::default(),
        };
        Type0CMaps {
            encoding: context
                .encoding
                .and_then(|value| self.cmap_dictionary(value)),
            to_unicode: context
                .to_unicode
                .and_then(|value| self.cmap_dictionary(value)),
        }
    }

    /// 只读诊断：Type 0 字体的 CMap 流字典（9.7.5.2，Table 120）自身形态，以及
    /// CMap 与后代 CIDFont 之间的一致性发现。
    ///
    /// 覆盖：`/Type`、`/CMapName`、`/CIDSystemInfo`、`/WMode` 四类的缺失与非法
    /// 形态；字典条目与流内 `def` 声明的互相对照；`/UseCMap`（#237 的解析结果
    /// 已纳入 [`CMapDictionary`]，此处不重复解析）；CMap 与字体两侧
    /// `/CIDSystemInfo` 的 Registry/Ordering 兼容性（shall）与 Supplement 差异
    /// （should）；Identity 编码搭配非内嵌字体（shall）；`usefont` 非 0 字体号
    /// （shall）；以及 `/CMapType` 的取值范围与流内一致性（should/策略线索）。
    ///
    /// 非 Type 0 字体、`/Encoding` 非流、结构不可解析时返回空列表而**不报错**；
    /// 调用本方法不改变任何解析或文本抽取结果。既有惯例（含 ISO 自身的
    /// `/ToUnicode` 例子 L15760–15772 只写 `/Length`）不填 Required 条目，故
    /// 缺失类发现只对 `/Encoding` CMap 提出；`/ToUnicode` 侧只在条目**存在**时
    /// 判形态与对照，避免把惯例当违规。
    pub fn cmap_consistency_issues(&self, font: &Value) -> Vec<CMapConsistencyIssue> {
        let mut issues = Vec::new();
        let Ok(value) = self.resolve_value(font) else {
            return issues;
        };
        let context = match value.as_dict().and_then(|dict| self.type0_context(dict)) {
            Some(context) => context,
            None => return issues,
        };
        let encoding = context
            .encoding
            .and_then(|value| self.cmap_dictionary(value));
        if let Some(dictionary) = &encoding {
            self.collect_cmap_issues(
                CMapPosition::Encoding,
                dictionary,
                true,
                self.cid_system_info_of(context.descendant.as_ref()),
                &mut issues,
            );
        }
        // Identity 编码（预定义名或内嵌 CMap 自报 Identity）不得搭配非内嵌字体
        // （9.7.5.2，L14912–14913）。规范按**名字**约束，故只认 Identity-H/V，
        // 自定义 CMap 声明 Adobe-Identity 字符集不触发本条。
        let identity = match &encoding {
            Some(dictionary) => dictionary.is_identity_cmap(),
            None => context.is_identity_name_encoding(self),
        };
        if identity && !context.has_embedded_program(self) {
            issues.push(CMapConsistencyIssue {
                position: CMapPosition::Encoding,
                rule: CMapConsistencyRule::IdentityEncodingWithoutEmbeddedFont,
            });
        }
        if let Some(dictionary) = context
            .to_unicode
            .and_then(|value| self.cmap_dictionary(value))
        {
            self.collect_cmap_issues(
                CMapPosition::ToUnicode,
                &dictionary,
                false,
                None,
                &mut issues,
            );
        }
        issues
    }

    /// 单处 CMap 流字典的发现收集。`required` 决定是否按 Table 120 的 Required
    /// 条目判缺失；`font_info` 为 `Some` 时才做字体侧配对。
    fn collect_cmap_issues(
        &self,
        position: CMapPosition,
        cmap: &CMapDictionary,
        required: bool,
        font_info: Option<CidSystemInfo>,
        issues: &mut Vec<CMapConsistencyIssue>,
    ) {
        let mut push = |rule: CMapConsistencyRule| {
            issues.push(CMapConsistencyIssue { position, rule });
        };

        match &cmap.object_type {
            CMapEntry::Missing => {
                if required {
                    push(CMapConsistencyRule::RequiredEntryMissing { key: "Type".into() });
                }
            }
            CMapEntry::Malformed { found } => push(CMapConsistencyRule::EntryMalformed {
                key: "Type".into(),
                found: found.clone(),
            }),
            CMapEntry::Present(name) if name != "CMap" => {
                push(CMapConsistencyRule::TypeNotCMap {
                    found: name.clone(),
                });
            }
            _ => {}
        }

        match &cmap.cmap_name {
            CMapEntry::Missing => {
                if required {
                    push(CMapConsistencyRule::RequiredEntryMissing {
                        key: "CMapName".into(),
                    });
                }
            }
            CMapEntry::Malformed { found } => push(CMapConsistencyRule::EntryMalformed {
                key: "CMapName".into(),
                found: found.clone(),
            }),
            CMapEntry::Present(dict_name) => {
                if let Some(file) = &cmap.declarations.cmap_name
                    && file != dict_name
                {
                    push(CMapConsistencyRule::CMapNameDiffersFromFile {
                        dict: dict_name.clone(),
                        file: file.clone(),
                    });
                }
            }
        }

        match &cmap.system_info {
            CMapEntry::Missing => {
                if required {
                    push(CMapConsistencyRule::RequiredEntryMissing {
                        key: "CIDSystemInfo".into(),
                    });
                }
            }
            CMapEntry::Malformed { found } => push(CMapConsistencyRule::EntryMalformed {
                key: "CIDSystemInfo".into(),
                found: found.clone(),
            }),
            CMapEntry::Present(info) => {
                if let Some(font) = &font_info
                    && !cmap.is_identity_encoding()
                {
                    if info.registry != font.registry || info.ordering != font.ordering {
                        push(CMapConsistencyRule::CidSystemInfoIncompatible {
                            cmap: collection_name(info),
                            font: collection_name(font),
                        });
                    } else if info.supplement != font.supplement {
                        push(CMapConsistencyRule::CidSystemInfoSupplementDiffers {
                            cmap: info.supplement,
                            font: font.supplement,
                        });
                    }
                }
            }
        }

        match &cmap.writing_mode {
            CMapEntry::Missing => {}
            CMapEntry::Malformed { found } => push(CMapConsistencyRule::EntryMalformed {
                key: "WMode".into(),
                found: found.clone(),
            }),
            CMapEntry::Present(CMapWritingMode::OutOfRange { value }) => {
                push(CMapConsistencyRule::WModeOutOfRange { value: *value });
            }
            CMapEntry::Present(CMapWritingMode::Mode(mode)) => {
                if let Some(file) = cmap.declarations.writing_mode
                    && i64::from(*mode) != file
                {
                    push(CMapConsistencyRule::WModeDiffersFromFile {
                        dict: i64::from(*mode),
                        file,
                    });
                }
            }
        }

        if let Some(value) = cmap.declarations.nonzero_use_font_number {
            push(CMapConsistencyRule::NonZeroUseFontNumber { value });
        }

        // /CMapType 与 /UID 非 Table 120 条目：只判 should／策略级形态线索。
        if let Some(value) = cmap.cmap_type.value() {
            if !matches!(*value, 1 | 2) {
                push(CMapConsistencyRule::CMapTypeUnsupported { value: *value });
            } else if let Some(file) = cmap.declarations.cmap_type
                && *value != file
            {
                push(CMapConsistencyRule::CMapTypeDiffersFromFile { dict: *value, file });
            }
        }
    }

    /// 校验 Type 0 字体字典并摘出两处 CMap 取值与后代 CIDFont 字典；非 Type 0
    /// 字体返回 `None`。
    fn type0_context<'a>(&self, dict: &'a Dict) -> Option<Type0Context<'a>> {
        if dict.get("Subtype").and_then(Value::as_name) != Some("Type0") {
            return None;
        }
        let mut context = Type0Context {
            encoding_raw: dict.get("Encoding"),
            encoding: dict
                .get("Encoding")
                .filter(|value| matches!(self.resolve_value(value).ok(), Some(Value::Stream(..)))),
            to_unicode: dict
                .get("ToUnicode")
                .filter(|value| matches!(self.resolve_value(value).ok(), Some(Value::Stream(..)))),
            descendant: None,
        };
        context.descendant = dict
            .get("DescendantFonts")
            .and_then(|value| self.resolve_value(value).ok())
            .and_then(|value| {
                let array = value.as_array()?;
                let first = array.0.first()?;
                self.resolve_value(first)
                    .ok()
                    .and_then(|value| value.as_dict().cloned())
            });
        Some(context)
    }

    /// 宽容读取后代 CIDFont 字典的 `/CIDSystemInfo`（Table 117 Required）。
    fn cid_system_info_of(&self, descendant: Option<&Dict>) -> Option<CidSystemInfo> {
        parse_cid_system_info(self, descendant?).ok()
    }
}

impl Type0Context<'_> {
    /// `/Encoding` 是否以预定义名 `/Identity-H`、`/Identity-V` 引用。
    fn is_identity_name_encoding(&self, document: &Document) -> bool {
        self.encoding_raw.is_some_and(|value| {
            document.resolve_value(value).ok().is_some_and(|value| {
                matches!(value, Value::Name(name)
                        if matches!(name.as_str(), "Identity-H" | "Identity-V"))
            })
        })
    }

    /// 后代 CIDFont 是否内嵌了字体程序（Table 126 的三种 `/FontFile*`）。
    ///
    /// 只看条目是否存在（含悬空引用也算「声称内嵌」），宁可漏报不误报。
    fn has_embedded_program(&self, document: &Document) -> bool {
        let Some(descendant) = self.descendant.as_ref() else {
            return false;
        };
        descendant
            .get("FontDescriptor")
            .and_then(|value| document.resolve_value(value).ok())
            .and_then(|value| value.as_dict().cloned())
            .is_some_and(|descriptor| {
                ["FontFile", "FontFile2", "FontFile3"]
                    .iter()
                    .any(|key| descriptor.get(key).is_some())
            })
    }
}

/// `Registry/Ordering` 形式的字符集标识，用于诊断条目展示。
fn collection_name(info: &CidSystemInfo) -> String {
    format!("{}/{}", info.registry, info.ordering)
}
