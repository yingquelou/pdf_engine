//! 文档需求读端（ISO 32000-1 12.10：12.10.1 General 与 Table 264、
//! 12.10.2 Requirement Handlers 与 Table 265；目录条目见 Table 3
//! `/Requirements`「(Optional; PDF 1.7) An array of requirement
//! dictionaries」）。
//!
//! 规范要点（本模块的 shall 依据，均取自 `docs/PDF32000_2008.pdf`
//! `pdftotext -raw` 的 L26093–L26146；多列表格必须用 `-raw`，`-layout`
//! 会错位）：
//!
//! - 12.10.1：自 PDF 1.7 起，文档可声明「符合要求的阅读器必须具备」的需求；
//!   目录 `/Requirements` 是**需求字典数组**。
//! - Table 264：`Type` 可选，存在时 shall 为 `Requirement`；`S` 必需
//!   （Required），值 shall 为 `EnableJavaScripts`（12.10.1「In PDF 1.7, the
//!   only defined requirement type shall be EnableJavaScripts」）；`RH` 可选，
//!   是 Table 265 的 requirement handler 字典数组，列出「阅读器已能自行校验
//!   `S` 所指需求时应当被禁用（不执行）」的处理器。
//! - 12.10.1 收尾（L26123–L26125）：`EnableJavaScripts` 存在时用 `RH` 指定
//!   JavaScript 段毫无意义——「The RH entry shall not be used in PDF 1.7」。
//!   故 `S == EnableJavaScripts` 且带 `RH` 是 shall 违例，本模块类型化报出，
//!   不静默接受。
//! - 12.10.1 向后兼容（L26100–L26101）：阅读器校验与 JavaScript 段校验
//!   「but not both」二者取一。本读端只建模「`RH` 列出了哪些处理器」，
//!   不替阅读器决定是否执行（无执行引擎，属库边界）。
//! - Table 265：`Type` 可选，存在时 shall 为 `ReqHandler`；`S` 必需，合法值
//!   shall 为 `JS` 或 `NoOp`（`NoOp` 不给字典添加任何条目，让老阅读器忽略
//!   无法识别的需求）；`Script` 可选且「valid only if the `S` entry has a
//!   value of `JS`」，是存放在**文档名称字典**（7.7.4）里的文档级 JavaScript
//!   动作的名称。
//!
//! 读端策略（与同目录 `extensions.rs`、`articles.rs` 一类「目录级字典 +
//! 类型化 + 违例诊断」切片同形）：
//!
//! - 目录无 `/Requirements` ⇒ `Ok(None)`；除此之外**结构违例一律不失败**，
//!   而是记进 [`DocumentRequirements::issues`]（数组级）或
//!   [`RequirementDictionary::issues`]（该条及其处理器级），读取路径不
//!   panic。返回 `Err` 的唯一情况是目录缺失/不可解析（继承
//!   [`NavigationError`]）。
//! - 不认识的 `S` 名字**不丢整条**：以 [`RequirementType::Unknown`] /
//!   [`HandlerType::Unknown`] 保留原始名字，并同时记一条 shall 违例。
//! - 数组元素是间接引用时先解引用（规范要求直接对象，读端宽容，同
//!   `extensions.rs` 的库策略）；解引用失败或解出来不是字典的元素以
//!   [`RequirementsIssue::EntryNotDictionary`] /
//!   [`RequirementsIssue::EntryUnresolvable`] 保留其**数组下标**，其余元素
//!   照常建模。
//! - `/Script` 只做文本串解码与「仅 `S == JS` 合法」的判定，**不**回查名称
//!   字典 `/JavaScript` 里是否真有该动作（本轮范围外）。解码失败时保留原始
//!   串内容（库策略，同 `templates.rs` 的 `decode_name`）。
//! - 写端已落地（12.10）：类型化写端在 `builder/types/requirements.rs`
//!   （唯一序列化路径），目录写出与构建期校验在
//!   `builder/navigation/requirements.rs`，入口是
//!   `PdfBuilder::add_document_requirement`。requirement / handler 字典一律按
//!   **直接对象**嵌套写进目录 `/Requirements`，不分配任何间接对象号。
//! - PDF 1.7 版本门禁走 `PdfFeature::DocumentRequirements` 能力模型
//!   （`min_version()` 即 Table 28 raw L3692 的 1.7）：低于 1.7 仍携带
//!   `/Requirements` → `WriteError::UnsupportedVersion`，零字节。任何非空
//!   `/RH` 由构建期校验拒为 `WriteError::InvalidObject`——raw L26125「The RH
//!   entry shall not be used in PDF 1.7」与 1.7 门槛互斥，故本库没有能写出
//!   `/RH` 的版本。写出时同步将 `DocumentRequirements` 登记进文档特征画像
//!   （`builder/navigation/mod.rs`），与读端 `features/inspect.rs` 扫到目录
//!   `/Requirements` 时的登记同口径；两端都**不**回查名称字典，因此上一条
//!   「不回查 `/JavaScript`」是读写一致的库边界，而非读端单侧的宽容。

mod issues;
mod parse;
#[cfg(test)]
mod requirements_tests;

pub use issues::RequirementsIssue;

use crate::core::types::{Dict, Document, Value};

use super::{NavigationError, catalog, resolve};

/// 需求类型（Table 264 `/S`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementType {
    /// `EnableJavaScripts`：12.10.1 中 PDF 1.7 唯一定义的需求类型，表示文档
    /// 要求阅读器启用 JavaScript 执行。
    EnableJavaScripts,
    /// 其它需求类型名字，原样保留（PDF 1.7 未定义，同时记 shall 违例）。
    Unknown(String),
    /// `/S` 缺失（Table 264 标注 Required）。
    Missing,
    /// `/S` 存在但不是 name 对象，保留值的类型名。
    NotName(&'static str),
}

/// requirement handler 类型（12.10.2 Table 265 `/S`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandlerType {
    /// `JS`：JavaScript 需求处理器。
    JavaScript,
    /// `NoOp`：不给字典添加任何条目，让老阅读器忽略无法识别的需求。
    NoOp,
    /// 其它处理器类型名字，原样保留（Table 265 shall 只允许 `JS`/`NoOp`）。
    Unknown(String),
    /// `/S` 缺失（Table 265 标注 Required）。
    Missing,
    /// `/S` 存在但不是 name 对象，保留值的类型名。
    NotName(&'static str),
}

/// 一个 requirement handler 字典（12.10.2 Table 265）。
///
/// 处理器自身的违例记在**所属需求字典**的
/// [`RequirementDictionary::issues`] 里（变体带 `handler` 下标），避免同一
/// 违例在两处重复。
#[derive(Debug, Clone, PartialEq)]
pub struct RequirementHandler {
    /// `/Type` 原始名字（存在且为 name 时保留，即便值不是 `ReqHandler`）。
    pub type_name: Option<String>,
    /// `/S` 处理器类型，缺失或类型错误以类型化变体保留。
    pub subtype: HandlerType,
    /// `/Script` 解码后的文本串：文档名称字典（7.7.4）里的文档级
    /// JavaScript 动作名。
    pub script: Option<String>,
    /// 原始字典，保留 Table 265 之外的未建模条目。
    pub dict: Dict,
}

/// 一个需求字典（12.10.1 Table 264）。
#[derive(Debug, Clone, PartialEq)]
pub struct RequirementDictionary {
    /// `/Type` 原始名字（存在且为 name 时保留，即便值不是 `Requirement`）。
    pub type_name: Option<String>,
    /// `/S` 需求类型。
    pub subtype: RequirementType,
    /// `/RH` 数组中可建模的处理器；不可建模的元素以 [`Self::issues`]
    /// 的类型化违例保留下标。
    pub handlers: Vec<RequirementHandler>,
    /// 该条需求自身（含其处理器）的 12.10 shall 违例。
    pub issues: Vec<RequirementsIssue>,
    /// 原始字典，保留 Table 264 之外的未建模条目。
    pub dict: Dict,
}

impl RequirementDictionary {
    /// 该条需求（含其处理器）是否没有任何 12.10 shall 违例。
    pub fn is_conforming(&self) -> bool {
        self.issues.is_empty()
    }
}

/// 目录 `/Requirements` 的类型化视图（12.10）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentRequirements {
    /// 数组里可建模的需求字典；不可建模的元素以 [`Self::issues`] 保留下标，
    /// 故本列表长度可以小于数组长度，但不会静默丢任何条目信息。
    pub requirements: Vec<RequirementDictionary>,
    /// 数组级（目录 `/Requirements` 自身与元素形态）的 shall 违例。
    pub issues: Vec<RequirementsIssue>,
}

impl DocumentRequirements {
    /// 是否完全符合 12.10 的 shall 集合（数组级与每条需求的违例都为空）。
    pub fn is_conforming(&self) -> bool {
        self.issues.is_empty()
            && self
                .requirements
                .iter()
                .all(|entry| entry.issues.is_empty())
    }

    /// 按「数组级在前、逐条在后」汇总全部违例。
    pub fn all_issues(&self) -> Vec<&RequirementsIssue> {
        let mut issues: Vec<&RequirementsIssue> = self.issues.iter().collect();
        for entry in &self.requirements {
            issues.extend(&entry.issues);
        }
        issues
    }
}

impl Document {
    /// 读取目录 `/Requirements` 需求字典数组（12.10、Table 264/265）。
    ///
    /// 目录无该条目返回 `Ok(None)`。除目录本身缺失/不可解析（返回
    /// [`NavigationError::CatalogMissing`] 等）外，本方法**不因文件损坏而
    /// 失败**：`/Requirements` 不是数组、元素不可解析、`Type`/`S`/`RH`/
    /// `Script` 的任何 shall 违例都以 [`RequirementsIssue`] 类型化保留，
    /// 见 [`DocumentRequirements`]。读取路径不 panic。
    pub fn requirements(&self) -> Result<Option<DocumentRequirements>, NavigationError> {
        let catalog = catalog(self)?;
        let Some(value) = catalog.get("Requirements") else {
            return Ok(None);
        };
        let value = resolve(self, value)?;
        Ok(Some(match value {
            Value::Array(array) => parse::read_requirements_array(self, &array),
            other => DocumentRequirements {
                requirements: Vec::new(),
                issues: vec![RequirementsIssue::CatalogEntryNotArray {
                    found: other.type_name(),
                }],
            },
        }))
    }

    /// 只读视图：目录 `/Requirements` 的全部 shall 违例（数组级在前）。
    ///
    /// 条目缺失时返回空列表；错误语义与 [`Self::requirements`] 一致。
    pub fn requirements_issues(&self) -> Result<Vec<RequirementsIssue>, NavigationError> {
        Ok(match self.requirements()? {
            None => Vec::new(),
            Some(model) => model.all_issues().into_iter().cloned().collect(),
        })
    }

    /// 文档是否要求阅读器启用 JavaScript 执行（12.10.1 的
    /// `EnableJavaScripts`，PDF 1.7 唯一定义的需求类型）。
    ///
    /// 目录无 `/Requirements`、数组为空或没有任何 `S == EnableJavaScripts`
    /// 的条目时为 `false`；违例不影响判定（未识别的类型不会被丢）。
    pub fn requires_javascript(&self) -> Result<bool, NavigationError> {
        Ok(self.requirements()?.is_some_and(|model| {
            model
                .requirements
                .iter()
                .any(|entry| entry.subtype == RequirementType::EnableJavaScripts)
        }))
    }
}
