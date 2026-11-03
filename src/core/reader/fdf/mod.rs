//! 表单数据格式（Forms Data Format，FDF；ISO 32000-1 12.7.7，PDF 1.2）
//! 的只读类型化模型。
//!
//! FDF 是独立于 PDF 的文件格式：语法与 PDF 相同（7.5），但交叉引用表
//! 可选、对象只允许 generation 0（raw 抽取行 L24765–L24767）；文件
//! trailer 的 `/Root`（Table 241，L24835–L24838）指向 **FDF 目录字典**
//! （Table 242，L24839–L24851：可选 `/Version` 名字 + 必填 `/FDF` 字典），
//! 后者（Table 243，L24853–L24891 与其续表）承载字段、状态、编码等条目。
//! 本模块只做**读端**：[`Document::fdf`] 检测并解析该结构，
//! [`Document::fdf_field_values`] 提供带全限定名的字段值枚举。
//!
//! 类型化范围（对齐规范、不发明键）：
//! - FDF 字典 Table 243：`/F`、`/ID`、`/Fields`、`/Status`、`/Pages`、
//!   `/Encoding`、`/Annots`、`/Differences`、`/Target`、`/EmbeddedFDFs`、
//!   `/JavaScript`。除 `/Fields` 展开为 [`FdfField`] 树、`/Status`、
//!   `/Encoding`、`/Target`（L24927：浏览器框架名字符串）与
//!   `/JavaScript`（L24933：指向 Table 245 的脚本字典，raw
//!   L24951–L24981）解码外，其余按原值保留（文件规范与 `/ID` 数组
//!   属剩余缺口）；`/Pages` 与 `/Annots` 另有类型化只读视图（下一条）。
//! - PDF 1.3 的 `/Pages`（FDF 页字典数组；raw L24874–L24877）与
//!   `/Annots`（FDF 注解字典数组；raw L24885–L24889）由子模块 `pages`
//!   类型化：[`Document::fdf_pages`] 逐页展开全部四层（Table 248
//!   `/Templates`/`/Info` → Table 249 `/TRef`/`/Fields`/`/Rename` →
//!   Table 250 `/Name`/`/F`，模板 `/Fields` 复用 [`FdfField`] 字段树），
//!   [`Document::fdf_annots`] 逐项检查 Table 251 的必需 `/Page` 序数与
//!   Table 243 的注解类型白名单，[`Document::fdf_import_merge`] 给出
//!   Annex J（informative）口径的导入合并报告。三者的 shall 违例与降级
//!   一律走类型化诊断 [`FdfPagesIssue`]，绝不让整份 FDF 解析失败；
//!   与 `embedded` 同样以**方法**而非字段暴露，`pages`/`annots` 原值
//!   字段继续保留，两条通道互不替代。
//! - PDF 1.4 的 `/Differences`（stream；raw L24906–L24926）与
//!   `/EmbeddedFDFs`（file specification 数组；raw L24930–L24932）另有
//!   类型化只读视图：[`Document::fdf_differences`] 确证「它是流」并原值
//!   保留流字典条目（`/Length`、`/Filter` 等），[`Document::fdf_embedded_fdfs`]
//!   逐项按 7.11 文件规范解析（复用 `reader/embedded.rs`）、识别
//!   Table 244 的 `/EncryptionRevision` 并递归解析可解出的子 FDF 文档
//!   （递归受层数、单数组条目数与**跨层全局条目预算**三重上限约束，子文档
//!   自己的 `/Differences` 在同一趟递归里逐层类型化）。
//!   两者以**方法**而非 `FdfDictionary` 字段暴露（加字段需改本车道
//!   写集外的全字段字面构造点），见子模块 `embedded` 的模块文档；
//!   `differences`/`embedded_fdfs` 原值字段继续保留，两条通道互不替代。
//! - FDF 字段字典 Table 246（L24983 起）：`/Kids`（直接**或**间接子项）、
//!   `/T`（必填文本串）、`/V`、`/Ff`、`/SetFf`、`/ClrFf`、`/F`（整型
//!   widget 注解标志，与字段树 PDF 侧 `/F` 文件规范**不同名同义**）、
//!   `/SetF`、`/ClrF`、`/AP`、`/APRef`、`/IF`、`/Opt`、`/A`、`/AA`、
//!   `/RV` 建模；未建模条目经 [`FdfField::dict`] 原样取回，不丢弃。
//!
//! 版本门禁说明：`/Pages`/`/Annots`/`/APRef`/`/IF` 标注 PDF 1.3、
//! `/Differences`/`/Target`/`/EmbeddedFDFs`/`/JavaScript` 标注 PDF 1.4、
//! `/RV` 标注 PDF 1.5（Table 245 的 `/AfterPermsReady` 标注 PDF 1.6，
//! 由写端 `builder/navigation/fdf/mod.rs` 直接比较 `PdfVersion`
//! 常量把关）。
//! 新增 [`PdfFeature`](crate::core::types::PdfFeature)
//! 变体需要同时改动 `src/core/types/version/feature.rs` 与
//! `feature_display.rs` 的穷尽 `Display` match，既有 FDF 切片一律**不**
//! 新增变体、直接比较 `PdfVersion` 常量，本车道沿用该口径（读写两侧
//! 一致，避免为同一个特性开第二个门禁位）；
//! 故读端一律宽容读取、不做版本拒绝（保守行为，待核项：门禁位）；
//! 其中 PDF 1.4 的 `/Differences` 与 `/EmbeddedFDFs`、PDF 1.3 的
//! `/Pages` 与 `/Annots` 在声明版本低于对应档位
//! 时（优先序：FDF 目录 `/Version` → `%FDF-x.y` 文件头 →
//! [`Document::source_version`]）由类型化视图附带一条诊断记录，仍照常
//! 读取（见子模块 `embedded`）。

use std::fmt;

use crate::core::types::{Dict, Document, Value};

mod embedded;
mod pages;
mod parse;
#[cfg(test)]
mod tests;

pub use embedded::{FdfDifferences, FdfEmbeddedFdf, FdfEmbeddedFdfs};
pub(crate) use pages::MAX_FDF_PAGES_ARRAY_ENTRIES;
pub use pages::{
    FdfAnnotation, FdfAnnots, FdfFieldNameConflict, FdfImportMerge, FdfNamedPage, FdfPage,
    FdfPages, FdfPagesIssue, FdfTemplate, FdfTemplateImport, PROHIBITED_FDF_ANNOTATION_TYPES,
};

/// 字段树最大嵌套深度（库策略，与 `reader/form.rs` 的 `MAX_FIELD_DEPTH`
/// 同一口径，防护不可信文件）。
pub(crate) const MAX_FDF_FIELD_DEPTH: usize = 64;
/// 字段树节点总数上限（库策略，同上 `MAX_FORM_NODES`）。
pub(crate) const MAX_FDF_FIELD_NODES: usize = 4096;

/// FDF 读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum FdfError {
    /// 条目缺失、类型不符或间接引用无法解析。
    Malformed(String),
    /// 超出字段树深度或节点数量上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for FdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => f.write_str(message),
            Self::LimitExceeded(what) => write!(f, "FDF limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for FdfError {}

pub(crate) fn malformed(message: impl Into<String>) -> FdfError {
    FdfError::Malformed(message.into())
}

/// FDF 字段字典（Table 246）。
///
/// `/V`、`/Opt`、`/RV` 等按原值保留**未解引用**的 [`Value`]：FDF 字段值
/// 形态随字段类型而变（Table 246 引用 12.7.4 各字段类型），本切片不做
/// 类型相关的解释；未建模条目从 [`Self::dict`] 原样取回。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfField {
    /// `/T` 部分字段名（必填；Table 246）。
    pub partial_name: String,
    /// 全限定名：父链各级非空 `/T` 以 `.` 连接；空的 `/T` 不扩展父名。
    /// 连接规则与 `reader/form.rs` 的 AcroForm 字段树同一口径（12.7.3.2）。
    pub fully_qualified_name: String,
    /// `/V` 字段值原值（未解引用）。
    pub value: Option<Value>,
    /// `/Ff` 字段标志位（整型）。
    pub field_flags: Option<i64>,
    /// `/SetFf` 导入时置位的 `/Ff` 位（整型；`/Ff` 存在时被忽略）。
    pub set_field_flags: Option<i64>,
    /// `/ClrFf` 导入时清位的 `/Ff` 位（整型；`/Ff` 存在时被忽略）。
    pub clear_field_flags: Option<i64>,
    /// `/F` widget 注解标志位（整型；Table 246，非文件规范）。
    pub widget_flags: Option<i64>,
    /// `/SetF` 导入时置位的 `/F` 位（整型）。
    pub set_widget_flags: Option<i64>,
    /// `/ClrF` 导入时清位的 `/F` 位（整型）。
    pub clear_widget_flags: Option<i64>,
    /// `/Opt` 选项数组原值（选择字段必填；库策略不强制）。
    pub options: Option<Value>,
    /// `/AP` 外观字典原值（按钮字段；N/R/D 均为流）。
    pub appearance: Option<Value>,
    /// `/APRef` 外部页引用外观字典原值（PDF 1.3；`/AP` 存在时被忽略）。
    pub appearance_reference: Option<Value>,
    /// `/IF` 图标适配字典原值（PDF 1.3，仅按钮字段；Table 247）。
    pub icon_fit: Option<Value>,
    /// `/A` 激活动作字典原值（12.6）。
    pub action: Option<Value>,
    /// `/AA` 附加动作字典原值（12.6.3）。
    pub additional_actions: Option<Value>,
    /// `/RV` 富文本值原值（PDF 1.5；12.7.3.4）。
    pub rich_value: Option<Value>,
    /// `/Kids` 直接子字段（深度优先声明序；直接对象与间接对象混排合法）。
    pub kids: Vec<FdfField>,
    /// 原始字段字典（保留全部未建模条目）。
    pub dict: Dict,
}

/// FDF `/JavaScript` 文档级脚本字典（Table 245，raw L24951–L24981）的
/// 类型化读模型，由 [`FdfDictionary::javascript_scripts`] 承载。
///
/// Table 245 的四个条目都可取「文本串**或**文本流」两种形态（raw
/// L24953–L24981），本切片只就地确证文本串：字面串与十六进制串经
/// `text_value` 解码为 `Some`，流形态与间接引用保持 `None`，原值留在
/// [`Self::dict`] 与 [`FdfDictionary::javascript`] 里不丢弃（读端宽容，
/// 与 `/Target` 同一口径）。文本流需要独立间接对象承载字节，而
/// 12.7.7.2 的 FDF 写端只产出**一个**间接对象，故该形态属剩余缺口。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfJavaScript {
    /// `/Before` 导入前执行的脚本（文本串形态；raw L24953–L24956）。
    pub before: Option<String>,
    /// `/After` 导入后执行的脚本（文本串形态；raw L24957–L24960）。
    pub after: Option<String>,
    /// `/AfterPermsReady` 权限就绪后执行的脚本（文本串形态；
    /// Optional; PDF 1.6；raw L24961–L24969）。
    pub after_perms_ready: Option<String>,
    /// `/Doc` 附加全局脚本（raw L24970–L24981：数组按「名字 + 文本串
    /// 或文本流」成对排列）。只有**两端都可确证**的对进入本表；含流
    /// 形态的配对整体保留在 [`Self::dict`] 的 `/Doc` 原值中。
    pub doc_scripts: Vec<FdfJavaScriptScript>,
    /// 原始 JavaScript 字典（保留全部条目，含流形态与未建模键）。
    pub dict: Dict,
}

impl FdfJavaScript {
    /// 按脚本名查找 `/Doc` 中的首个匹配脚本正文。
    pub fn doc_script(&self, name: &str) -> Option<&str> {
        self.doc_scripts
            .iter()
            .find(|script| script.name == name)
            .map(|script| script.script.as_str())
    }
}

/// Table 245 `/Doc` 数组的一对：脚本名（name 对象）+ 脚本正文
/// （文本串形态的 text string）。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfJavaScriptScript {
    /// 配对第一项：脚本名（Table 245 raw L24974–L24975「The first
    /// element of each pair shall be a name」）。
    pub name: String,
    /// 配对第二项：脚本正文（仅文本串形态被解码；raw L24975–L24976
    /// 「the second shall be a text string or text stream」）。
    pub script: String,
}

/// FDF 字典（Table 243）。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfDictionary {
    /// `/F` 源/目标 PDF 文件的文件规范原值（字符串或文件规范字典）。
    pub source_file: Option<Value>,
    /// `/ID` 文件标识符数组原值（两个字节串，Table 243 引用 14.4）。
    pub identifier: Option<Value>,
    /// `/Fields` 根字段数组；缺失为 `None`（与「空数组」区分）。
    /// 与 `/Pages`、`/Status` 的互斥规则见 [`Self::fields_pages_conflict`]。
    pub fields: Option<Vec<FdfField>>,
    /// `/Status` 状态文本串（PDFDocEncoding 解码）。
    pub status: Option<String>,
    /// `/Pages` 页字典数组原值（PDF 1.3，Table 248）。逐项类型化的
    /// 页/模板（Table 249）/命名页引用（Table 250）视图见
    /// [`Document::fdf_pages`]（原值在此不丢弃，与 `/Differences` 同一
    /// 双通道口径）。
    pub pages: Option<Value>,
    /// `/Encoding` 名字（PDF 1.3；缺省语义为 PDFDocEncoding）。
    pub encoding: Option<String>,
    /// `/Annots` 注解字典数组原值（PDF 1.3）。逐项类型化（Table 251 的
    /// 必需 `/Page` 序数 + Table 243 的注解类型白名单）见
    /// [`Document::fdf_annots`]；与 `/Pages` 的导入合并交叉分析见
    /// [`Document::fdf_import_merge`]（原值在此不丢弃）。
    pub annots: Option<Value>,
    /// `/Differences` 增量更新流原值（PDF 1.4）。流形态识别与解引用后的
    /// 类型化视图见 [`Document::fdf_differences`]（原值在此不丢弃）。
    pub differences: Option<Value>,
    /// `/Target` 原值（保留全部形态，不丢弃；类型化解码见
    /// [`Self::target_frame`]）。
    pub target: Option<Value>,
    /// `/Target` 浏览器框架名（string；PDF 1.4；Table 243 续表，raw
    /// L24927–L24929「The name of a browser frame in which the
    /// underlying PDF document shall be opened」）。仅字面串/十六进制
    /// 串经 `text_value` 解码；非串形态（如 Adobe FDF 2.0 的字典式
    /// `/Target`，非 ISO 32000-1 基线，待核）保持 `None`，读端宽容，
    /// 原值不丢弃（写端装配前按 Table 243 拒绝非串，见
    /// `builder/navigation/fdf/mod.rs`）。
    pub target_frame: Option<String>,
    /// `/EmbeddedFDFs` 内嵌 FDF 文件规范数组原值（PDF 1.4）。逐项
    /// 文件规范类型化、Table 244 `/EncryptionRevision` 识别与递归子
    /// FDF 文档见 [`Document::fdf_embedded_fdfs`]（原值在此不丢弃）。
    pub embedded_fdfs: Option<Value>,
    /// `/JavaScript` 原值（保留全部形态，不丢弃；类型化解码见
    /// [`Self::javascript_scripts`]）。
    pub javascript: Option<Value>,
    /// `/JavaScript` 文档级脚本字典（Table 245）的类型化视图（PDF 1.4；
    /// Table 243 续表 raw L24933–L24934「JavaScript dictionary
    /// (Optional; PDF 1.4) A JavaScript dictionary (see Table 245)
    /// defining document-level JavaScript scripts」）。仅**字典形态**
    /// 给出 `Some`；字典内只有文本串形态的 `/Before`、`/After`、
    /// `/AfterPermsReady` 与「名字 + 文本串」的 `/Doc` 配对被解码，
    /// 流形态与间接引用保持原位（[`FdfJavaScript::dict`] 与
    /// [`Self::javascript`] 不丢弃）。非字典形态（如把 `/JavaScript`
    /// 直接写成串）读端宽容：本字段为 `None`，原值保留；写端装配前
    /// 按 Table 243 的 dictionary 类型拒绝。
    pub javascript_scripts: Option<FdfJavaScript>,
    /// 原始 FDF 字典（保留全部未建模条目）。
    pub dict: Dict,
}

impl FdfDictionary {
    /// 诊断：`/Fields` 与 `/Pages` 是否同时出现。Table 243 规定两者
    /// shall not both be present；读端宽容，仅暴露冲突供调用方决策。
    pub fn fields_pages_conflict(&self) -> bool {
        self.fields.is_some() && self.pages.is_some()
    }
}

/// FDF 文件目录（Table 242）与其 `/FDF` 字典（Table 243）的组合读模型。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfDocument {
    /// FDF 目录 `/Version` 名字原值（PDF 1.4，如 `1.4`；缺省按文件头版本）。
    pub catalog_version: Option<String>,
    /// FDF 目录字典原值（保留未建模条目；Table 242 只列 `/Version` 与 `/FDF`）。
    pub catalog: Dict,
    /// `/FDF` 指向的 FDF 字典。
    pub fdf: FdfDictionary,
}

impl FdfDocument {
    /// 深度优先展开全部字段（父字段先于其子字段），携带预先计算的
    /// 全限定名（Table 246 `/Kids` 树）。
    pub fn all_fields(&self) -> Vec<&FdfField> {
        let mut out = Vec::new();
        if let Some(fields) = &self.fdf.fields {
            collect_fields(fields, &mut out);
        }
        out
    }

    /// 按全限定名查找首个匹配字段（同名字段允许多重表示，12.7.3.2）。
    pub fn field(&self, fully_qualified_name: &str) -> Option<&FdfField> {
        self.all_fields()
            .into_iter()
            .find(|field| field.fully_qualified_name == fully_qualified_name)
    }
}

fn collect_fields<'a>(fields: &'a [FdfField], out: &mut Vec<&'a FdfField>) {
    for field in fields {
        out.push(field);
        collect_fields(&field.kids, out);
    }
}

impl Document {
    /// 解析本文件为 FDF 读模型（12.7.7.3）。
    ///
    /// 检测规则：trailer `/Root` 可解析为字典且含 `/FDF` 条目（Table 241/
    /// 242）时返回 [`FdfDocument`]；`/Root` 缺失、悬空、非字典或不含
    /// `/FDF` 一律返回 `Ok(None)`（普通 PDF 的正常路径，不报错）。
    /// 一旦确认为 FDF，其后的结构错误（`/FDF` 目标非字典、`/Fields` 非
    /// 数组、字段缺 `/T` 等）报 [`FdfError::Malformed`]。
    pub fn fdf(&self) -> Result<Option<FdfDocument>, FdfError> {
        parse::fdf_document(self)
    }

    /// 便捷枚举：深度优先返回每个字段的（全限定名, `/V` 原值）。
    ///
    /// `/V` 缺失的中间节点值为 `None`；非 FDF 文件返回空向量。
    /// 需要未建模条目或标志位时用 [`Self::fdf`] 取完整 [`FdfField`]。
    pub fn fdf_field_values(&self) -> Result<Vec<(String, Option<Value>)>, FdfError> {
        let Some(fdf) = self.fdf()? else {
            return Ok(Vec::new());
        };
        Ok(fdf
            .all_fields()
            .into_iter()
            .map(|field| (field.fully_qualified_name.clone(), field.value.clone()))
            .collect())
    }
}
