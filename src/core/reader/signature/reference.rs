//! 变换引用（12.8.3 Table 256–257）与 `/TransformParams`（Table 253–255）。

use crate::core::reader::content::text_value;
use crate::core::types::{Dict, Value};

use super::entries::DigestAlgorithm;
use super::{Result, malformed};

/// 变换方法（Table 252 与 12.8.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformMethod {
    /// `/DocMDP`：文档级修改防护（12.8.4.3）。
    Docmdp,
    /// `/FieldMDP`：字段级修改防护（12.8.4.2）。
    Fieldmdp,
    /// `/UR`：用户权限（12.8.4.1）。
    Ur,
}

impl TransformMethod {
    /// 由 `/TransformMethod` 名称解析。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "DocMDP" => Some(Self::Docmdp),
            "FieldMDP" => Some(Self::Fieldmdp),
            "UR" => Some(Self::Ur),
            _ => None,
        }
    }

    /// 名称（不含前导 `/`）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Docmdp => "DocMDP",
            Self::Fieldmdp => "FieldMDP",
            Self::Ur => "UR",
        }
    }
}

/// 文档修改防护权限（Table 253 `/P`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocmdpPermission {
    /// 1：仅允许填表单与签名字段（最严格）。
    NoChanges,
    /// 2：在 1 之外还可创建/删除注解、填表单、签名。
    FillAndAnnotate,
    /// 3：在 2 之外还可添加/删除页面与页面注释。
    FillAnnotateAndForm,
}

impl DocmdpPermission {
    /// 由 `/P` 整数解析；越界返回 `None`。
    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            1 => Some(Self::NoChanges),
            2 => Some(Self::FillAndAnnotate),
            3 => Some(Self::FillAnnotateAndForm),
            _ => None,
        }
    }

    /// `/P` 数值。
    pub fn as_i64(&self) -> i64 {
        match self {
            Self::NoChanges => 1,
            Self::FillAndAnnotate => 2,
            Self::FillAnnotateAndForm => 3,
        }
    }
}

/// 字段修改防护范围（Table 254 `/P`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldMdPermission {
    /// `/All`：除 `/FieldNames` 列出的字段外全部锁定。
    All,
    /// `/Include`：仅 `/FieldNames` 列出的字段可修改。
    Include,
    /// `/Exclude`：除 `/FieldNames` 外全部可修改。
    Exclude,
}

/// 用户权限作业范围（Table 255 `/Scope`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrScope {
    /// 是否含 `/DocAttachment`。
    pub doc_attachment: bool,
    /// 是否含 `/Annotation`。
    pub annotation: bool,
    /// 是否含 `/Form`。
    pub form: bool,
}

/// `/TransformParams` 字典（按 `/TransformMethod` 区分，Table 253–255）。
#[derive(Debug, Clone, PartialEq)]
pub enum TransformParams {
    /// DocMDP：`/P` 权限级别。
    Docmdp { permission: DocmdpPermission },
    /// FieldMDP：`/P` 范围、`/Annots` 目标与原始参数键。
    Fieldmdp {
        permission: FieldMdPermission,
        annotations: Vec<u8>,
        dict: Dict,
    },
    /// UR：作业范围、版本与原因等；未建模条目保留在 `dict`。
    Ur {
        version: i64,
        scope: UrScope,
        needs_appearances: bool,
        reason: Option<String>,
        permission: Option<i64>,
        v2: Option<i64>,
        dict: Dict,
    },
}

/// 签名字典引用条目（12.8.3 Table 256–257，`/Reference` 数组元素）。
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureReference {
    /// 变换方法；缺失表示纯时间戳引用（本层不建模）。
    pub transform_method: Option<TransformMethod>,
    /// 已按方法校验的变换参数。
    pub params: Option<TransformParams>,
    /// `/Data` 引用的嵌入 PDF 流（原样保留，取流经 `data_reference`）。
    pub data: Option<Value>,
    /// `/DigestValue` 原始十六进制/字面串。
    pub digest_value: Option<Value>,
    /// `/DigestLength` 声明的摘要字节数。
    pub digest_length: Option<i64>,
    /// 完整引用字典；`/TransformParams` 的间接引用等经此访问。
    pub dict: Dict,
}

impl SignatureReference {
    /// `/Data` 若为间接引用则返回 `(num, gen)`。
    pub fn data_reference(&self) -> Option<(i64, i64)> {
        self.data.as_ref().and_then(|value| value.as_ref())
    }

    /// 声明的摘要长度与算法输出长度是否一致（算法未知返回 `None`）。
    pub fn matches_digest_length(&self, algorithm: DigestAlgorithm) -> Option<bool> {
        let declared = self.digest_length?;
        Some(declared == algorithm.output_len() as i64)
    }
}

/// 按 `/TransformMethod` 解析 `/TransformParams` 字典。
pub(super) fn parse_transform_params(
    method: TransformMethod,
    dict: &Dict,
) -> Result<TransformParams> {
    match method {
        TransformMethod::Docmdp => {
            let value = dict
                .get("P")
                .and_then(|item| item.as_integer())
                .ok_or_else(|| malformed("DocMDP /TransformParams lacks integer /P"))?;
            let permission = DocmdpPermission::from_i64(value)
                .ok_or_else(|| malformed(format!("DocMDP /P must be 1..3, got {value}")))?;
            Ok(TransformParams::Docmdp { permission })
        }
        TransformMethod::Fieldmdp => {
            let name = dict
                .get("P")
                .and_then(|item| item.as_name())
                .ok_or_else(|| malformed("FieldMDP /TransformParams lacks name /P"))?;
            let permission = match name {
                "All" => FieldMdPermission::All,
                "Include" => FieldMdPermission::Include,
                "Exclude" => FieldMdPermission::Exclude,
                other => {
                    return Err(malformed(format!(
                        "FieldMDP /P must be All/Include/Exclude, got /{other}"
                    )));
                }
            };
            let annotations = dict
                .get("Annots")
                .map(|item| {
                    super::bytes::pdf_string_bytes(item)
                        .ok_or_else(|| malformed("FieldMDP /Annots must be a bits string"))
                })
                .transpose()?
                .unwrap_or_default();
            Ok(TransformParams::Fieldmdp {
                permission,
                annotations,
                dict: dict.clone(),
            })
        }
        TransformMethod::Ur => {
            let version = match dict.get("V") {
                Some(item) => ur_version_value(item)
                    .ok_or_else(|| malformed("UR /V must be a version number"))?,
                None => return Err(malformed("UR /TransformParams lacks /V")),
            };
            let scope_names = match dict.get("Scope") {
                Some(item) => {
                    let array = item
                        .as_array()
                        .ok_or_else(|| malformed("UR /Scope must be an array of names"))?;
                    array
                        .0
                        .iter()
                        .map(|entry| {
                            entry
                                .as_name()
                                .ok_or_else(|| malformed("UR /Scope entries must be names"))
                        })
                        .collect::<Result<Vec<_>>>()?
                }
                None => return Err(malformed("UR /TransformParams lacks /Scope")),
            };
            let mut scope = UrScope {
                doc_attachment: false,
                annotation: false,
                form: false,
            };
            for entry in scope_names {
                match entry {
                    "DocAttachment" => scope.doc_attachment = true,
                    "Annotation" => scope.annotation = true,
                    "Form" => scope.form = true,
                    other => {
                        return Err(malformed(format!("unknown UR /Scope entry /{other}")));
                    }
                }
            }
            let needs_appearances = dict
                .get("NeedAppearances")
                .and_then(|item| match item {
                    Value::Bool(flag) => Some(*flag),
                    _ => None,
                })
                .unwrap_or(false);
            Ok(TransformParams::Ur {
                version,
                scope,
                needs_appearances,
                reason: dict.get("Reason").and_then(text_value),
                permission: dict.get("P").and_then(|item| item.as_integer()),
                v2: dict.get("V2").and_then(|item| item.as_integer()),
                dict: dict.clone(),
            })
        }
    }
}

fn ur_version_value(item: &Value) -> Option<i64> {
    match item {
        Value::Integer(number) => Some(*number),
        Value::Float(number) => (*number == 1.2_f64).then_some(12),
        Value::Name(name) => match name.as_str() {
            "1.2" => Some(12),
            "1.9" => Some(19),
            _ => None,
        },
        _ => None,
    }
}
