//! [`Document`] 读端入口 API：颜色空间值、页面 `/Resources` 命名与默认
//! 颜色空间，以及间接引用解析与字典数值数组访问器。

use crate::core::types::{Dict, Document, Value};

use super::error::{Result, malformed};
use super::model::{ColorSpaceModel, DefaultColorSpace};

impl Document {
    /// 从颜色空间值（名字或数组，可为间接引用）构造类型化模型。
    pub fn color_space(&self, value: &Value) -> Result<ColorSpaceModel> {
        self.color_space_depth(value, 0)
    }

    /// 解析页面 `/Resources` 中命名颜色空间资源；资源不存在时返回 `None`。
    pub fn page_color_space(
        &self,
        page_index: usize,
        name: &str,
    ) -> Result<Option<ColorSpaceModel>> {
        match self
            .page_resource(page_index, "ColorSpace", name)
            .map_err(|error| malformed(error.to_string()))?
        {
            Some(value) => self.color_space(&value).map(Some),
            None => Ok(None),
        }
    }

    /// 解析页面资源字典的 `/DefaultGray`、`/DefaultRGB` 或 `/DefaultCMYK`。
    ///
    /// 页面、资源或默认项不存在时返回 `None`（默认项是可选条目）。
    pub fn page_default_color_space(
        &self,
        page_index: usize,
        kind: DefaultColorSpace,
    ) -> Result<Option<ColorSpaceModel>> {
        let resources = match self.locate_page_resources(page_index) {
            Ok(Some(resources)) => resources,
            Ok(None) => return Ok(None),
            Err(error) => return Err(malformed(error.to_string())),
        };
        let Some(value) = resources.get(kind.resource_key()) else {
            return Ok(None);
        };
        self.color_space(value).map(Some)
    }

    /// 解一层间接引用；按对象号取最新修订，引用对象缺失报类型化错误。
    pub(super) fn resolve_one(&self, value: &Value) -> Result<Value> {
        match value {
            Value::Ref(num, generation) => self
                .get_object(*num)
                .map_err(|error| malformed(error.to_string()))?
                .map(|object| object.value)
                .ok_or_else(|| {
                    malformed(format!("referenced object {num} {generation} R missing"))
                }),
            other => Ok(other.clone()),
        }
    }

    pub(super) fn dict_item(&self, items: &[Value], index: usize, what: &str) -> Result<Dict> {
        let Some(value) = items.get(index) else {
            return Err(malformed(format!(
                "{what} color space is missing its dictionary"
            )));
        };
        self.resolve_one(value)?
            .as_dict()
            .cloned()
            .ok_or_else(|| malformed(format!("{what} color space parameter is not a dictionary")))
    }

    pub(super) fn real_array(
        &self,
        dict: &Dict,
        key: &str,
        len: usize,
        default: Option<&[f64]>,
    ) -> Result<Vec<f64>> {
        let Some(value) = dict.get(key) else {
            return match default {
                Some(values) => Ok(values.to_vec()),
                None => Err(malformed(format!("missing required key /{key}"))),
            };
        };
        let resolved = self.resolve_one(value)?;
        let array = resolved
            .as_array()
            .ok_or_else(|| malformed(format!("/{key} must be an array")))?;
        let mut out = Vec::with_capacity(array.0.len());
        for item in &array.0 {
            let number = match item {
                Value::Integer(i) => Some(*i as f64),
                Value::Float(f) => Some(*f),
                _ => None,
            }
            .ok_or_else(|| malformed(format!("/{key} has a non-numeric element")))?;
            out.push(number);
        }
        if out.len() != len {
            return Err(malformed(format!(
                "/{key} must contain {len} numbers, got {}",
                out.len()
            )));
        }
        Ok(out)
    }

    pub(super) fn ordered_pairs(
        &self,
        dict: &Dict,
        key: &str,
        len: usize,
        default: &[f64],
    ) -> Result<Vec<(f64, f64)>> {
        let numbers = self.real_array(dict, key, len, Some(default))?;
        Ok(numbers
            .chunks(2)
            .map(|pair| {
                let (lo, hi) = (pair[0], pair[1]);
                if lo <= hi { (lo, hi) } else { (hi, lo) }
            })
            .collect())
    }
}
