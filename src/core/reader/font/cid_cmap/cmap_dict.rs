//! CMap 流字典（9.7.5.2，Table 120）的类型化读端，含流内声明的对照数据。
//!
//! Table 120（raw 抽取 L14927–14948）在标准流条目（Table 5）之外只列出五个键：
//!
//! | 键 | 类型 | 必填性 | 原文行号 |
//! |---|---|---|---|
//! | `/Type` | name | Required，shall be `CMap` | L14929–14930 |
//! | `/CMapName` | name | Required，shall 与 CMap 文件内 `CMapName` 相同 | L14931–14932 |
//! | `/CIDSystemInfo` | dictionary | Required，shall 与 CMap 文件内相同（Identity-H/V 除外） | L14933–14938 |
//! | `/WMode` | integer | Optional，shall be 0/1，默认 0，shall 与文件内相同 | L14939–14943 |
//! | `/UseCMap` | name or stream | Optional | L14944–14948 |
//!
//! `/CMapType` 与 `/UID` **不在** ISO 32000-1 Table 120 中：前者只作为 CMap
//! 文件内部的 `def` 声明出现（L14996 `/CMapType 1 def`、L15772 `/CMapType 2 def`，
//! 取值语义属 Adobe TN #5014），后者在本规范全文 0 命中（仅 L14997 的
//! `/UIDOffset`）。两者按本库惯例宽容保留原值形态，不参与任何 shall 判定，
//! 供诊断侧作「形态不符」的 should 级线索。
//!
//! 本模块只读：缺失、形态非法、引用悬空、解码或词法失败一律降级成类型化
//! 条目（[`CMapEntry::Missing`] / [`CMapEntry::Malformed`] / `None`），**绝不**
//! 返回错误，也**绝不**改变 `super::cid` 与 `super::to_unicode` 的既有解析结果。

use crate::core::types::{Dict, Document, Value};

use super::MAX_CMAP_BYTES;
use super::token::{Token, tokenize};
use super::use_cmap::{UseCMap, entry as use_cmap_entry};
use crate::core::reader::font::CidSystemInfo;
use crate::core::reader::font::cid::parse_cid_system_info;

/// Table 120 单个条目的读取结果，区分「缺失」「形态非法」与取到的值。
///
/// 缺省条目不物化：`Missing` 表示键根本不存在（`/WMode` 的缺省语义由调用方
/// 按 Table 120 的默认值 0 处理）；`Malformed` 表示键存在但形态不符（类型
/// 不对、引用悬空或子字典不可解析），原值一律不猜测替代。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CMapEntry<T> {
    /// 条目不存在。
    Missing,
    /// 条目存在但形态不符；`found` 是实际看到的 PDF 对象类型名或形态描述。
    Malformed {
        /// 实际形态，如 `"integer"` 或 `"unresolved reference"`。
        found: String,
    },
    /// 条目存在且按规范形态取到值。
    Present(T),
}

impl<T> CMapEntry<T> {
    /// 取到值时返回其引用，其余形态返回 `None`。
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Present(value) => Some(value),
            _ => None,
        }
    }

    /// 是否为「条目不存在」。
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }

    /// 是否为「存在但形态非法」。
    pub fn is_malformed(&self) -> bool {
        matches!(self, Self::Malformed { .. })
    }

    /// 形态描述，用于诊断条目：缺失为 `"missing"`。
    pub fn found(&self) -> &str {
        match self {
            Self::Missing => "missing",
            Self::Malformed { found } => found,
            Self::Present(_) => "present",
        }
    }
}

/// `/WMode`（Table 120：integer，0 水平 / 1 垂直，默认 0）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CMapWritingMode {
    /// 规范取值：`0` 水平、`1` 垂直。
    Mode(u8),
    /// 是整数但不在 `0..=1`（Table 120「The value shall be 0 for horizontal or
    /// 1 for vertical」，L14940–14941）。
    OutOfRange {
        /// 声明的原始整数，一律保留不改写。
        value: i64,
    },
}

/// Table 120 – `/UseCMap` 的两种取值形态（`name or stream`，L14944–14948）。
///
/// 解析本身仍由 [`super::use_cmap`] 承担（#237），这里只把它纳入类型化模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CMapBaseReference {
    /// 预定义 CMap 的名字。
    Name(String),
    /// 含 CMap 的流；`number` 为间接对象号（流是文档内直接对象时为 `None`）。
    Stream {
        /// 基座流的对象号。
        number: Option<i64>,
    },
}

/// CMap 文件（流内容）里以 PostScript `def` 形式重复声明的条目。
///
/// Table 120 明确要求 `/CMapName`（L14931–14932）、`/WMode`（L14942–14943）等
/// 字典条目「shall be the same as the value ... in the CMap file」，因此对照
/// 需要读回流内声明。9.7.5.4 约束 (b)（L14953）还要求 `usefont` 的字体号恰为
/// 0，一并在这里采集。
///
/// 词法失败或超长时 [`parsed`](Self::parsed) 为 `false`，此时不产出任何对照
/// 发现（宁可漏报，不误报）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CMapFileDeclarations {
    /// 词法可读：tokenize 未超限、未报错。
    pub parsed: bool,
    /// `/CMapName /X def` 声明的名字。
    pub cmap_name: Option<String>,
    /// `/CMapType n def` 声明的类型（TN #5014，非 Table 120 条目）。
    pub cmap_type: Option<i64>,
    /// `/WMode n def` 声明的书写模式。
    pub writing_mode: Option<i64>,
    /// 首个非零 `usefont` 字体号（约束 (b) 的违背证据）。
    pub nonzero_use_font_number: Option<i64>,
}

/// Table 120 – CMap 流字典的类型化模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CMapDictionary {
    /// `/Type`：Required，shall be `CMap`。
    pub object_type: CMapEntry<String>,
    /// `/CMapName`：Required，name。
    pub cmap_name: CMapEntry<String>,
    /// `/CIDSystemInfo`：Required，dictionary（复用字体侧的 Table 116 解析）。
    pub system_info: CMapEntry<CidSystemInfo>,
    /// `/WMode`：Optional，integer 0/1，缺省不物化。
    pub writing_mode: CMapEntry<CMapWritingMode>,
    /// `/UseCMap`：Optional，`name or stream`；缺失为 `None`。
    pub use_cmap: Option<CMapBaseReference>,
    /// `/CMapType`：非 Table 120 条目，宽容保留（TN #5014）。
    pub cmap_type: CMapEntry<i64>,
    /// `/UID`：非 Table 120 条目，宽容保留（ISO 32000-1 全文无此键，待核）。
    pub uid: CMapEntry<i64>,
    /// 流内容里的同名声明，用于「字典须与 CMap 文件一致」对照。
    pub declarations: CMapFileDeclarations,
}

impl CMapDictionary {
    /// 该 CMap 是否**就叫** Identity-H / Identity-V（流内声明优先于字典条目）。
    ///
    /// 规范原文按名字约束：「The Identity-H and Identity-V CMaps shall not be
    /// used with a non-embedded font.」（9.7.5.2，L14912–14913），故判定只看名字。
    pub fn is_identity_cmap(&self) -> bool {
        let name = self
            .declarations
            .cmap_name
            .as_deref()
            .or(self.cmap_name.value().map(String::as_str));
        matches!(name, Some("Identity-H") | Some("Identity-V"))
    }

    /// 该 CMap 是否按 Identity 口径运行：预定义名，或声明 `Adobe-Identity`
    /// 字符集（自定义的 Identity 型编码，等价于 Table 120 的 CIDSystemInfo
    /// 豁免，L14937–14938；Table 119 亦记 Identity-H/V 可用于任意
    /// Registry/Ordering/Supplement 的 CIDFont，L14849–14851）。
    ///
    /// 只用于**放宽**字体侧配对，不用于任何 shall 判定，避免把「声明
    /// Adobe-Identity 的自定义 CMap」误当成预定义 Identity-H。
    pub fn is_identity_encoding(&self) -> bool {
        self.is_identity_cmap()
            || self
                .system_info
                .value()
                .is_some_and(|info| info.registry == "Adobe" && info.ordering == "Identity")
    }
}

impl Document {
    /// 把一个 CMap 流（Table 120）读成类型化字典模型。
    ///
    /// 解析间接引用后不是流时返回 `None`；其余任何形态问题都落在
    /// [`CMapEntry`] 里，本调用不会失败、也不会改变既有 CMap 解析结果。
    pub fn cmap_dictionary(&self, value: &Value) -> Option<CMapDictionary> {
        let value = self.resolve_value(value).ok()?;
        let dict = match &value {
            Value::Stream(dict, _) => dict,
            _ => return None,
        };
        Some(CMapDictionary {
            object_type: name_entry(dict, "Type"),
            cmap_name: name_entry(dict, "CMapName"),
            system_info: system_info_entry(self, dict),
            writing_mode: writing_mode_entry(dict),
            use_cmap: use_cmap_entry(self, dict).map(base_reference),
            cmap_type: integer_entry(dict, "CMapType"),
            uid: integer_entry(dict, "UID"),
            declarations: file_declarations(self, &value),
        })
    }
}

/// `/Type`、`/CMapName` 这类 name 条目的读取：缺 `Type` 键名与值分离的形态。
fn name_entry(dict: &Dict, key: &str) -> CMapEntry<String> {
    let Some(raw) = dict.get(key) else {
        return CMapEntry::Missing;
    };
    match raw {
        Value::Name(name) => CMapEntry::Present(name.clone()),
        other => CMapEntry::Malformed {
            found: other.type_name().into(),
        },
    }
}

/// integer 条目（`/WMode`、`/CMapType`、`/UID`）的宽容读取。
fn integer_entry(dict: &Dict, key: &str) -> CMapEntry<i64> {
    let Some(raw) = dict.get(key) else {
        return CMapEntry::Missing;
    };
    match raw {
        Value::Integer(value) => CMapEntry::Present(*value),
        Value::Float(_) => CMapEntry::Malformed {
            found: "real number".into(),
        },
        other => CMapEntry::Malformed {
            found: other.type_name().into(),
        },
    }
}

fn writing_mode_entry(dict: &Dict) -> CMapEntry<CMapWritingMode> {
    match integer_entry(dict, "WMode") {
        CMapEntry::Missing => CMapEntry::Missing,
        CMapEntry::Malformed { found } => CMapEntry::Malformed { found },
        CMapEntry::Present(value) => match u8::try_from(value) {
            Ok(mode @ (0 | 1)) => CMapEntry::Present(CMapWritingMode::Mode(mode)),
            _ => CMapEntry::Present(CMapWritingMode::OutOfRange { value }),
        },
    }
}

/// 复用字体侧的 Table 116 解析器读取 `/CIDSystemInfo`。
fn system_info_entry(document: &Document, dict: &Dict) -> CMapEntry<CidSystemInfo> {
    let Some(raw) = dict.get("CIDSystemInfo") else {
        return CMapEntry::Missing;
    };
    if matches!(raw, Value::Ref(_, _) if document.resolve_value(raw).is_err()) {
        return CMapEntry::Malformed {
            found: "unresolved reference".into(),
        };
    }
    match parse_cid_system_info(document, dict) {
        Ok(info) => CMapEntry::Present(info),
        Err(_) => CMapEntry::Malformed {
            found: "unparsable CIDSystemInfo dictionary".into(),
        },
    }
}

/// #237 的 `/UseCMap` 解析结果映射进公开模型。
fn base_reference(base: UseCMap) -> CMapBaseReference {
    match base {
        UseCMap::Name(name) => CMapBaseReference::Name(name),
        UseCMap::Stream { number, .. } => CMapBaseReference::Stream { number },
    }
}

/// 采集 CMap 文件内的 `def` 声明与 `usefont` 字体号。
fn file_declarations(document: &Document, stream: &Value) -> CMapFileDeclarations {
    let Ok(bytes) = document.decode_stream_bytes(stream) else {
        return CMapFileDeclarations::default();
    };
    if bytes.len() > MAX_CMAP_BYTES {
        return CMapFileDeclarations::default();
    }
    let Ok(tokens) = tokenize(&bytes) else {
        return CMapFileDeclarations::default();
    };
    let mut found = CMapFileDeclarations {
        parsed: true,
        ..CMapFileDeclarations::default()
    };
    for index in 0..tokens.len() {
        // `/Name value… def`：值段形态严格匹配，避免误认其他 `def`。
        if let Some((name, values)) = declaration(&tokens, index) {
            match name {
                "CMapName" => {
                    if found.cmap_name.is_none()
                        && let Some(Token::Word(declared)) = literal_name(values)
                    {
                        found.cmap_name = Some(declared.clone());
                    }
                }
                "CMapType" => {
                    if found.cmap_type.is_none()
                        && let Some(value) = single_integer(values)
                    {
                        found.cmap_type = Some(value);
                    }
                }
                "WMode" => {
                    if found.writing_mode.is_none()
                        && let Some(value) = single_integer(values)
                    {
                        found.writing_mode = Some(value);
                    }
                }
                _ => {}
            }
        }
        // `n /usefont`：9.7.5.4 约束 (b) 要求字体号恰为 0。
        if found.nonzero_use_font_number.is_none()
            && matches!(tokens.get(index), Some(Token::Word(word)) if word == "usefont")
            && let Some(Token::Integer(value)) = tokens.get(index.saturating_sub(1))
            && *value != 0
        {
            found.nonzero_use_font_number = Some(*value);
        }
    }
    found
}

/// 识别 `/Name values… def` 形态，返回 `Name` 与其值段。
///
/// 值段限 1～4 个 token：CMap 文件里的条目声明都是单值 `def`，更长的值段
/// （例如 `/CIDSystemInfo 3 dict dup begin … end def`）一律不匹配，宁缺勿误。
fn declaration(tokens: &[Token], index: usize) -> Option<(&str, &[Token])> {
    if !matches!(tokens.get(index), Some(Token::Word(slash)) if slash == "/") {
        return None;
    }
    let name = match tokens.get(index + 1) {
        Some(Token::Word(name)) if name != "/" => name.as_str(),
        _ => return None,
    };
    for offset in 1..=4usize {
        let end = index.checked_add(2)?.checked_add(offset)?;
        if matches!(tokens.get(end), Some(Token::Word(def)) if def == "def") {
            return Some((name, tokens.get(index + 2..end)?));
        }
    }
    None
}

/// 值段恰为一个 PostScript 名字（`/ X`）时取回 `X`。
fn literal_name(values: &[Token]) -> Option<&Token> {
    match values {
        [Token::Word(slash), name] if slash == "/" => Some(name),
        _ => None,
    }
}

/// 值段恰为一个整数时取回它。
fn single_integer(values: &[Token]) -> Option<i64> {
    match values {
        [Token::Integer(value)] => Some(*value),
        _ => None,
    }
}
