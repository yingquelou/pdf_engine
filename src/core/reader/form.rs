//! 交互表单读取（12.7.2–12.7.4）：`/AcroForm` 字典与字段树。
//!
//! - 目录 `/AcroForm`（直接字典或间接引用）：`/Fields` 字段树、
//!   `/NeedAppearances` 布尔、`/DA`、`/XFA`（原样保留）；`/DR` 默认资源
//!   在 `form/acro_form.rs::DefaultResources` 中类型化（字体原样保留，
//!   `/ExtGState` `/ColorSpace` `/ProcSet` 解析为各自模型）。
//! - 字段属性按 12.7.3 继承：`/FT`、`/Ff`、`/V`、`/DV`、`/DA`、`/Q`、
//!   `/MaxLen` 取最近祖先提供的值，子节点可覆盖；`/Opt` 类型化为
//!   [`FormOption`]（Table 227 按钮字段可继承、Table 231 选择字段不继承）；
//!   全限定名由 `/T` 链以 `.` 连接。
//! - Widget 节点（`/Subtype /Widget`）建模 `/Rect`、`/AP`、`/AS` 与 `/P`；
//!   外观流内容求值归渲染，动作 `/A` 语义归 E3。
//! - 字段 `/AA` 附加动作（`/K` `/F` `/V` `/C`，Table 196）经动作模型
//!   类型化；`/CO` 计算顺序按引用解析为字段全限定名（Table 218）。
//! - 防护：字段树深度 64、节点总数 4096、间接引用环检测。

use std::collections::HashSet;
use std::fmt;

use crate::core::reader::action::Action;
use crate::core::reader::content::text_value;
use crate::core::types::{Dict, Document, Value};

mod acro_form;
mod flags;
mod options;

pub use acro_form::{AcroForm, DefaultResources};
pub use flags::FieldFlags;
pub use options::FormOption;

/// 字段树最大嵌套深度。
const MAX_FIELD_DEPTH: usize = 64;
/// 字段树节点总数上限。
pub const MAX_FORM_NODES: usize = 4096;

/// 表单读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum FormError {
    /// 文档目录缺失或无法解析为字典。
    CatalogMissing,
    /// `/AcroForm`、字段或 Widget 条目结构损坏。
    Malformed(String),
    /// 超出字段树深度或节点数量上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for FormError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogMissing => write!(f, "document catalog is missing or malformed"),
            Self::Malformed(message) => write!(f, "{message}"),
            Self::LimitExceeded(what) => write!(f, "form limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for FormError {}

type Result<T> = std::result::Result<T, FormError>;

fn malformed(message: impl Into<String>) -> FormError {
    FormError::Malformed(message.into())
}

/// 字段类型（Table 220 `/FT`）；未识别名称原样保留。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// `/Btn` 按钮（复选框/单选/按钮）。
    Button,
    /// `/Tx` 文本字段。
    Text,
    /// `/Ch` 选择字段（列表框/组合框）。
    Choice,
    /// `/Sig` 签名字段。
    Signature,
    /// 未识别的类型名。
    Unknown(String),
}

impl FieldType {
    fn from_name(name: &str) -> Self {
        match name {
            "Btn" => Self::Button,
            "Tx" => Self::Text,
            "Ch" => Self::Choice,
            "Sig" => Self::Signature,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// 字段类型名（`Unknown` 返回原始名）。
    pub fn name(&self) -> &str {
        match self {
            Self::Button => "Btn",
            Self::Text => "Tx",
            Self::Choice => "Ch",
            Self::Signature => "Sig",
            Self::Unknown(name) => name,
        }
    }
}

/// 字段附加动作（`/AA`，12.6.3 Table 196 触发事件）。
#[derive(Debug, Clone, PartialEq)]
pub struct FieldActions {
    /// `/K` 按键时触发。
    pub keystroke: Option<Action>,
    /// `/F` 格式化时触发。
    pub format: Option<Action>,
    /// `/V` 校验时触发。
    pub validate: Option<Action>,
    /// `/C` 计算时触发。
    pub calculate: Option<Action>,
    /// 其他触发事件（名称原样保留，动作值原样保留）。
    pub other: Vec<(String, Value)>,
}

impl FieldActions {
    /// 是否未声明任何触发事件。
    pub fn is_empty(&self) -> bool {
        self.keystroke.is_none()
            && self.format.is_none()
            && self.validate.is_none()
            && self.calculate.is_none()
            && self.other.is_empty()
    }
}

/// 字段树节点（含合并后的继承属性与 Widget 数据）。
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// 字段对象的间接引用；直接嵌入 `/Fields`/`/Kids` 时为 `None`。
    pub reference: Option<(i64, i64)>,
    /// `/T` 部分名（解码后）。
    pub partial_name: Option<String>,
    /// 全限定名（`/T` 链以 `.` 连接；无名的 Widget 继承父名）。
    pub fully_qualified_name: String,
    /// `/FT`（含继承）。
    pub field_type: Option<FieldType>,
    /// `/Ff`（含继承）。
    pub flags: FieldFlags,
    /// `/V` 字段值（原样保留：文本串、名称或数组）。
    pub value: Option<Value>,
    /// `/DV` 默认值。
    pub default_value: Option<Value>,
    /// `/DA` 默认外观字符串（含继承）。
    pub default_appearance: Option<Value>,
    /// `/Q` 对齐（含继承）：0 左对齐、1 居中、2 右对齐。
    pub quadding: Option<i64>,
    /// `/MaxLen` 文本最大长度（含继承）。
    pub max_len: Option<i64>,
    /// `/Opt` 选项数组：选择字段按 Table 231 类型化，按钮字段为
    /// Widget 开状态名称（Table 227，仅按钮字段可继承）。
    pub options: Option<Vec<FormOption>>,
    /// 该节点自身是否为 Widget 注解（`/Subtype /Widget`）。
    pub is_widget: bool,
    /// Widget `/Rect`。
    pub rect: Option<[f64; 4]>,
    /// Widget `/AP` 外观流（原样保留）。
    pub appearance: Option<Value>,
    /// Widget 常态外观状态子字典（`/AP /N` 为字典时）的状态名 → 外观值；
    /// `/N` 为单一流或缺失时为空。
    pub appearance_normal_states: Vec<(String, Value)>,
    /// Widget `/AS` 当前外观状态名。
    pub appearance_state: Option<String>,
    /// Widget `/P` 所在页面引用。
    pub page: Option<(i64, i64)>,
    /// `/AA` 附加动作；未声明时为 `None`。
    pub actions: Option<FieldActions>,
    /// 子字段/子 Widget。
    pub kids: Vec<Field>,
    /// 完整字段字典，未建模条目（`/DS`、`/RV`、`/I` 等）经此访问。
    pub dict: Dict,
}

/// 目录 `/AcroForm` 字典（12.7.2 Table 218）。
///
/// 沿字段树向下传递的继承属性（Table 222）。
#[derive(Clone, Default)]
struct Inherited {
    field_type: Option<FieldType>,
    flags: FieldFlags,
    value: Option<Value>,
    default_value: Option<Value>,
    default_appearance: Option<Value>,
    quadding: Option<i64>,
    max_len: Option<i64>,
    options: Option<Vec<FormOption>>,
}

/// 读取 `/Fields` 数组为字段树；`/AcroForm` 解析器经此进入字段层。
fn read_field_tree(document: &Document, entries: &[Value]) -> Result<Vec<Field>> {
    let mut reader = FieldReader {
        document,
        visited: HashSet::new(),
        nodes: 0,
    };
    reader.read_fields(entries, "", &Inherited::default(), 0)
}

/// 读取 `/AA` 字典：四个标准事件类型化，其余事件原样保留。
fn parse_actions(document: &Document, value: &Value) -> Result<FieldActions> {
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| malformed(err.to_string()))?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("form field /AA is not a dictionary"))?;
    let mut actions = FieldActions {
        keystroke: None,
        format: None,
        validate: None,
        calculate: None,
        other: Vec::new(),
    };
    for (key, value) in &dict.0 {
        let slot = match key.as_str() {
            "K" => &mut actions.keystroke,
            "F" => &mut actions.format,
            "V" => &mut actions.validate,
            "C" => &mut actions.calculate,
            _ => {
                actions.other.push((key.clone(), value.clone()));
                continue;
            }
        };
        *slot = Some(
            document
                .action(value)
                .map_err(|err| malformed(format!("form field /AA /{key}: {err}")))?,
        );
    }
    Ok(actions)
}

struct FieldReader<'a> {
    document: &'a Document,
    visited: HashSet<(i64, i64)>,
    nodes: usize,
}

impl FieldReader<'_> {
    fn read_fields(
        &mut self,
        entries: &[Value],
        parent_fqn: &str,
        inherited: &Inherited,
        depth: usize,
    ) -> Result<Vec<Field>> {
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            out.push(self.read_field(entry, parent_fqn, inherited, depth)?);
        }
        Ok(out)
    }

    fn read_field(
        &mut self,
        entry: &Value,
        parent_fqn: &str,
        inherited: &Inherited,
        depth: usize,
    ) -> Result<Field> {
        if depth > MAX_FIELD_DEPTH {
            return Err(FormError::LimitExceeded("form field depth"));
        }
        if self.nodes >= MAX_FORM_NODES {
            return Err(FormError::LimitExceeded("form field count"));
        }
        let reference = entry.as_ref();
        if let Some(key) = reference
            && !self.visited.insert(key)
        {
            return Err(malformed("form field tree contains a reference cycle"));
        }
        let node = crate::core::reader::navigation::resolve(self.document, entry)
            .map_err(|err| malformed(err.to_string()))?;
        let dict = node
            .as_dict()
            .ok_or_else(|| malformed("form field is not a dictionary"))?;
        self.nodes += 1;

        let partial_name = parse_partial_name(dict)?;
        let fully_qualified_name = match partial_name.as_deref() {
            Some(name) if !name.is_empty() => {
                if parent_fqn.is_empty() {
                    name.to_string()
                } else {
                    format!("{parent_fqn}.{name}")
                }
            }
            _ => parent_fqn.to_string(),
        };

        let field_type = match dict.get("FT") {
            Some(value) => Some(FieldType::from_name(
                value
                    .as_name()
                    .ok_or_else(|| malformed("form field /FT is not a name"))?,
            )),
            None => inherited.field_type.clone(),
        };
        let flags = match dict.get("Ff") {
            Some(value) => FieldFlags::from_bits(parse_bits(value, "form field /Ff")?),
            None => inherited.flags,
        };
        let value = dict.get("V").cloned().or_else(|| inherited.value.clone());
        let default_value = dict
            .get("DV")
            .cloned()
            .or_else(|| inherited.default_value.clone());
        let default_appearance = dict
            .get("DA")
            .cloned()
            .or_else(|| inherited.default_appearance.clone());
        let quadding = match dict.get("Q") {
            Some(value) => Some(parse_integer(value, "form field /Q")?),
            None => inherited.quadding,
        };
        let max_len = match dict.get("MaxLen") {
            Some(value) => {
                let len = parse_integer(value, "form field /MaxLen")?;
                if len < 0 {
                    return Err(malformed("form field /MaxLen must be nonnegative"));
                }
                Some(len)
            }
            None => inherited.max_len,
        };
        // `/Opt` 仅按钮字段可继承（Table 227）；选择字段的选项不随字段树
        // 下传（Table 231 未标记 inheritable）。
        let options = match dict.get("Opt") {
            Some(value) => Some(options::parse_options(
                self.document,
                value,
                field_type.as_ref(),
            )?),
            None if matches!(field_type, Some(FieldType::Button)) => inherited.options.clone(),
            None => None,
        };

        let is_widget = matches!(dict.get("Subtype").and_then(Value::as_name), Some("Widget"));
        let rect = if is_widget {
            Some(parse_rect(dict.get("Rect"))?)
        } else {
            None
        };
        let appearance = dict.get("AP").cloned();
        let appearance_normal_states = match &appearance {
            Some(value) => parse_appearance_states(self.document, value)?,
            None => Vec::new(),
        };
        let appearance_state = match dict.get("AS") {
            Some(value) => Some(
                value
                    .as_name()
                    .ok_or_else(|| malformed("widget annotation /AS is not a name"))?
                    .to_string(),
            ),
            None => None,
        };
        let page =
            match dict.get("P") {
                Some(value) => Some(value.as_ref().ok_or_else(|| {
                    malformed("widget annotation /P must be an indirect reference")
                })?),
                None => None,
            };
        let actions = match dict.get("AA") {
            Some(value) => Some(parse_actions(self.document, value)?),
            None => None,
        };

        let merged = Inherited {
            field_type: field_type.clone(),
            flags,
            value: value.clone(),
            default_value: default_value.clone(),
            default_appearance: default_appearance.clone(),
            quadding,
            max_len,
            options: options.clone(),
        };
        let kids = match dict.get("Kids") {
            Some(value) => {
                let resolved = crate::core::reader::navigation::resolve(self.document, value)
                    .map_err(|err| malformed(err.to_string()))?;
                let array = resolved
                    .as_array()
                    .ok_or_else(|| malformed("form field /Kids is not an array"))?;
                self.read_fields(&array.0, &fully_qualified_name, &merged, depth + 1)?
            }
            None => Vec::new(),
        };

        Ok(Field {
            reference,
            partial_name,
            fully_qualified_name,
            field_type,
            flags,
            value,
            default_value,
            default_appearance,
            quadding,
            max_len,
            options,
            is_widget,
            rect,
            appearance,
            appearance_normal_states,
            appearance_state,
            page,
            actions,
            kids,
            dict: dict.clone(),
        })
    }
}

fn parse_partial_name(dict: &Dict) -> Result<Option<String>> {
    match dict.get("T") {
        None => Ok(None),
        Some(value) => text_value(value)
            .map(Some)
            .ok_or_else(|| malformed("form field /T is not a text string")),
    }
}

fn parse_bits(value: &Value, context: &str) -> Result<u32> {
    match value {
        Value::Integer(bits) if *bits >= 0 && *bits <= u32::MAX as i64 => Ok(*bits as u32),
        _ => Err(malformed(format!("{context} is not a nonnegative integer"))),
    }
}

fn parse_appearance_states(
    document: &Document,
    appearance: &Value,
) -> Result<Vec<(String, Value)>> {
    // `/AP` 是可选条目：悬空引用不应让整个表单解析失败，原值仍可从
    // `Field::appearance` 取得。
    let Ok(resolved) = crate::core::reader::navigation::resolve(document, appearance) else {
        return Ok(Vec::new());
    };
    let Some(dict) = resolved.as_dict() else {
        return Ok(Vec::new());
    };
    let Some(normal) = dict.get("N") else {
        return Ok(Vec::new());
    };
    let Ok(normal) = crate::core::reader::navigation::resolve(document, normal) else {
        return Ok(Vec::new());
    };
    let Some(states) = normal.as_dict() else {
        return Ok(Vec::new());
    };
    Ok(states
        .0
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect())
}

fn parse_integer(value: &Value, context: &str) -> Result<i64> {
    match value {
        Value::Integer(number) => Ok(*number),
        _ => Err(malformed(format!("{context} is not an integer"))),
    }
}

fn parse_rect(value: Option<&Value>) -> Result<[f64; 4]> {
    let value = value.ok_or_else(|| malformed("widget annotation has no /Rect"))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("widget annotation /Rect is not an array"))?;
    if array.len() != 4 {
        return Err(malformed("widget annotation /Rect must have four numbers"));
    }
    let mut out = [0.0f64; 4];
    for (slot, item) in out.iter_mut().zip(&array.0) {
        let number = match item {
            Value::Integer(number) => *number as f64,
            Value::Float(number) => *number,
            _ => return Err(malformed("widget annotation /Rect must be numeric")),
        };
        if !number.is_finite() {
            return Err(malformed("widget annotation /Rect must be finite"));
        }
        *slot = number;
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
