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
//!   `/JavaScript`。除 `/Fields` 展开为 [`FdfField`] 树、`/Status` 与
//!   `/Encoding` 解码外，其余按原值保留（文件规范、数组与字典不在本
//!   切片类型化，属剩余缺口）。
//! - FDF 字段字典 Table 246（L24983 起）：`/Kids`（直接**或**间接子项）、
//!   `/T`（必填文本串）、`/V`、`/Ff`、`/SetFf`、`/ClrFf`、`/F`（整型
//!   widget 注解标志，与字段树 PDF 侧 `/F` 文件规范**不同名同义**）、
//!   `/SetF`、`/ClrF`、`/AP`、`/APRef`、`/IF`、`/Opt`、`/A`、`/AA`、
//!   `/RV` 建模；未建模条目经 [`FdfField::dict`] 原样取回，不丢弃。
//!
//! 版本门禁说明：`/Pages`/`/Annots`/`/APRef`/`/IF` 标注 PDF 1.3、
//! `/Differences`/`/Target`/`/EmbeddedFDFs`/`/JavaScript` 标注 PDF 1.4、
//! `/RV` 标注 PDF 1.5。新增 [`PdfFeature`](crate::core::types::PdfFeature)
//! 变体需要改动 `src/core/types/version/feature.rs`，超出本车道写集，
//! 故读端一律宽容读取、不做版本拒绝（保守行为，待核项：门禁位）。

use std::fmt;

use crate::core::types::{Dict, Document, Value};

mod parse;
#[cfg(test)]
mod tests;

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
    /// `/Pages` 页字典数组原值（PDF 1.3，Table 248；本切片不类型化）。
    pub pages: Option<Value>,
    /// `/Encoding` 名字（PDF 1.3；缺省语义为 PDFDocEncoding）。
    pub encoding: Option<String>,
    /// `/Annots` 注解字典数组原值（PDF 1.3，Table 251 加 `/Page`；
    /// 本切片不类型化）。
    pub annots: Option<Value>,
    /// `/Differences` 增量更新流原值（PDF 1.4）。
    pub differences: Option<Value>,
    /// `/Target` 浏览器框架名文本串原值（PDF 1.4）。
    pub target: Option<Value>,
    /// `/EmbeddedFDFs` 内嵌 FDF 文件规范数组原值（PDF 1.4）。
    pub embedded_fdfs: Option<Value>,
    /// `/JavaScript` 文档级脚本字典原值（PDF 1.4，Table 245）。
    pub javascript: Option<Value>,
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
