//! 目录 `/AcroForm`（12.7.2 Table 218）与 `/DR` 默认资源字典（Table 34）。

use std::collections::{BTreeMap, HashMap};

use super::{Field, Result, malformed, read_field_tree};
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::graphics_state::ExtGState;
use crate::core::types::{Dict, Document, Value};

/// 目录 `/AcroForm` 字典（12.7.2 Table 218）。
#[derive(Debug, Clone, PartialEq)]
pub struct AcroForm {
    /// `/NeedAppearances`：阅读器是否应为字段生成外观流。
    pub need_appearances: bool,
    /// `/DA` 文档级默认外观字符串。
    pub default_appearance: Option<Value>,
    /// `/DR` 默认资源字典。
    pub default_resources: Option<DefaultResources>,
    /// `/XFA` XFA 表单数据（原样保留；XFA 整体不实现）。
    pub xfa: Option<Value>,
    /// `/Fields` 字段树（顶层）。
    pub fields: Vec<Field>,
    /// `/CO` 计算顺序（按引用解析为字段全限定名）；缺失时为空。
    pub calculation_order: Vec<String>,
    /// 完整 `/AcroForm` 字典。
    pub dict: Dict,
}

/// `/DR` 默认资源字典（12.7.2 Table 218 / 资源字典 Table 34）的类型化视图。
///
/// 字体资源保留原值（间接引用不解引用），调用方可按需交给字体解析器；
/// 图形状态与颜色空间按各自条目模型类型化；`/ProcSet` 为名称数组。
#[derive(Debug, Clone, PartialEq)]
pub struct DefaultResources {
    /// `/Font`：资源名 → 字体字典值（原样保留）。
    pub fonts: BTreeMap<String, Value>,
    /// `/ExtGState`：类型化图形状态参数（Table 58）。
    pub ext_g_states: BTreeMap<String, ExtGState>,
    /// `/ColorSpace`：类型化颜色空间（8.6）。
    pub color_spaces: BTreeMap<String, ColorSpaceModel>,
    /// `/ProcSet`：过程集名称（Table 34）。
    pub proc_set: Vec<String>,
    /// 完整 `/DR` 字典（`/Pattern` `/Shading` `/XObject` 等未建模条目经此访问）。
    pub dict: Dict,
}

impl Document {
    /// 读取目录 `/AcroForm`（12.7.2）；文档没有交互表单时返回 `None`。
    pub fn acro_form(&self) -> Result<Option<AcroForm>> {
        let catalog = crate::core::reader::navigation::catalog(self)
            .map_err(|_| super::FormError::CatalogMissing)?;
        let Some(value) = catalog.get("AcroForm") else {
            return Ok(None);
        };
        let resolved = crate::core::reader::navigation::resolve(self, value)
            .map_err(|err| malformed(err.to_string()))?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| malformed("/AcroForm is not a dictionary"))?;

        let need_appearances = match dict.get("NeedAppearances") {
            None => false,
            Some(value) => {
                let resolved = crate::core::reader::navigation::resolve(self, value)
                    .map_err(|err| malformed(err.to_string()))?;
                match resolved {
                    Value::Bool(flag) => flag,
                    _ => return Err(malformed("/AcroForm /NeedAppearances is not a boolean")),
                }
            }
        };

        let fields = match dict.get("Fields") {
            None => Vec::new(),
            Some(value) => {
                let resolved = crate::core::reader::navigation::resolve(self, value)
                    .map_err(|err| malformed(err.to_string()))?;
                let array = resolved
                    .as_array()
                    .ok_or_else(|| malformed("/AcroForm /Fields is not an array"))?;
                read_field_tree(self, &array.0)?
            }
        };

        let default_resources = match dict.get("DR") {
            None => None,
            Some(value) => Some(parse_default_resources(self, value)?),
        };
        let calculation_order = parse_calculation_order(self, dict, &fields)?;

        Ok(Some(AcroForm {
            need_appearances,
            default_appearance: dict.get("DA").cloned(),
            default_resources,
            xfa: dict.get("XFA").cloned(),
            fields,
            calculation_order,
            dict: dict.clone(),
        }))
    }
}

fn parse_default_resources(document: &Document, value: &Value) -> Result<DefaultResources> {
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| malformed(err.to_string()))?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("/AcroForm /DR is not a dictionary"))?;

    let fonts = match dict.get("Font") {
        None => BTreeMap::new(),
        Some(value) => {
            let resolved = crate::core::reader::navigation::resolve(document, value)
                .map_err(|err| malformed(err.to_string()))?;
            let fonts = resolved
                .as_dict()
                .ok_or_else(|| malformed("/AcroForm /DR /Font is not a dictionary"))?;
            fonts
                .0
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect()
        }
    };

    let ext_g_states = document
        .ext_g_states(dict)
        .map_err(|err| malformed(format!("/AcroForm /DR /ExtGState: {err}")))?;

    let color_spaces = match dict.get("ColorSpace") {
        None => BTreeMap::new(),
        Some(value) => {
            let resolved = crate::core::reader::navigation::resolve(document, value)
                .map_err(|err| malformed(err.to_string()))?;
            let spaces = resolved
                .as_dict()
                .ok_or_else(|| malformed("/AcroForm /DR /ColorSpace is not a dictionary"))?;
            let mut out = BTreeMap::new();
            for (name, value) in &spaces.0 {
                let model = document.color_space(value).map_err(|err| {
                    malformed(format!("/AcroForm /DR /ColorSpace /{name}: {err}"))
                })?;
                out.insert(name.clone(), model);
            }
            out
        }
    };

    let proc_set = match dict.get("ProcSet") {
        None => Vec::new(),
        Some(value) => {
            let resolved = crate::core::reader::navigation::resolve(document, value)
                .map_err(|err| malformed(err.to_string()))?;
            let array = resolved
                .as_array()
                .ok_or_else(|| malformed("/AcroForm /DR /ProcSet is not an array"))?;
            array
                .0
                .iter()
                .map(|entry| {
                    entry
                        .as_name()
                        .map(str::to_string)
                        .ok_or_else(|| malformed("/AcroForm /DR /ProcSet entries must be names"))
                })
                .collect::<Result<Vec<_>>>()?
        }
    };

    Ok(DefaultResources {
        fonts,
        ext_g_states,
        color_spaces,
        proc_set,
        dict: dict.clone(),
    })
}

/// `/CO` 条目必须是指向字段树节点的间接引用，按引用解析为全限定名。
fn parse_calculation_order(
    document: &Document,
    dict: &Dict,
    fields: &[Field],
) -> Result<Vec<String>> {
    let Some(value) = dict.get("CO") else {
        return Ok(Vec::new());
    };
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| malformed(err.to_string()))?;
    let array = resolved
        .as_array()
        .ok_or_else(|| malformed("/AcroForm /CO is not an array"))?;
    let mut names = HashMap::new();
    collect_field_names(fields, &mut names);
    let mut out = Vec::with_capacity(array.0.len());
    for entry in &array.0 {
        let key = entry
            .as_ref()
            .ok_or_else(|| malformed("/AcroForm /CO entries must be indirect references"))?;
        let name = names.get(&key).ok_or_else(|| {
            malformed("/AcroForm /CO references an object that is not in the field tree")
        })?;
        out.push(name.clone());
    }
    Ok(out)
}

fn collect_field_names(fields: &[Field], out: &mut HashMap<(i64, i64), String>) {
    for field in fields {
        if let Some(key) = field.reference {
            out.insert(key, field.fully_qualified_name.clone());
        }
        collect_field_names(&field.kids, out);
    }
}
