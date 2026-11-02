//! Pattern 资源值与页面 `/Pattern` 资源的解析入口，及 PatternType 1 平铺
//! 图案与 PatternType 2 着色图案的字典/流解析。

use crate::core::types::{Dict, Document, Value};

use super::error::{MAX_PATTERN_CONTENT, Result, from_color_space, malformed, unsupported};
use super::fields::number;
use super::model::{PaintType, PatternModel, ShadingPattern, TilingPattern, TilingType};

impl Document {
    /// 从 Pattern 资源值构造类型化模型。
    ///
    /// PatternType 2 的 `/Shading` 若为名称，需要资源字典上下文——请用
    /// [`Document::page_pattern`]，否则报 [`ShadingError::Malformed`]。
    pub fn pattern(&self, value: &Value) -> Result<PatternModel> {
        self.pattern_model(value, None)
    }

    /// 解析页面 `/Resources/Pattern` 中的命名图案；资源不存在返回 `None`。
    ///
    /// 页面资源字典同时用于解析 PatternType 2 中名称形式的 `/Shading`。
    pub fn page_pattern(&self, page_index: usize, name: &str) -> Result<Option<PatternModel>> {
        let value = self
            .page_resource(page_index, "Pattern", name)
            .map_err(|error| malformed(error.to_string()))?;
        match value {
            Some(value) => {
                let resources = self
                    .locate_page_resources(page_index)
                    .map_err(|error| malformed(error.to_string()))?;
                self.pattern_model(&value, resources.as_ref()).map(Some)
            }
            None => Ok(None),
        }
    }
}

impl Document {
    fn pattern_model(&self, value: &Value, resources: Option<&Dict>) -> Result<PatternModel> {
        let resolved = self.resolve(value)?;
        let pattern_type = match &resolved {
            Value::Dict(_) | Value::Stream(_, _) => resolved
                .as_dict()
                .and_then(|dict| dict.get("PatternType"))
                .and_then(Value::as_integer)
                .unwrap_or(1),
            other => {
                return Err(malformed(format!(
                    "pattern must be a dictionary or stream, got {}",
                    other.type_name()
                )));
            }
        };
        match pattern_type {
            1 => self.tiling_pattern(&resolved).map(PatternModel::Tiling),
            2 => self
                .shading_pattern_dict(&resolved, resources)
                .map(PatternModel::Shading),
            other => Err(unsupported(format!("unsupported /PatternType {other}"))),
        }
    }

    fn tiling_pattern(&self, value: &Value) -> Result<TilingPattern> {
        let Value::Stream(_, _) = value else {
            return Err(malformed("tiling pattern (PatternType 1) must be a stream"));
        };
        let dict = value.as_dict().expect("stream dict");
        let paint_type = match dict.get("PaintType").and_then(Value::as_integer) {
            Some(1) => PaintType::Colored,
            Some(2) => PaintType::Uncolored,
            Some(other) => return Err(malformed(format!("invalid /PaintType {other}"))),
            None => return Err(malformed("missing required key /PaintType")),
        };
        let tiling_type = match dict.get("TilingType").and_then(Value::as_integer) {
            Some(1) => TilingType::ConstantSpacing,
            Some(2) => TilingType::NonAdaptiveVariableSpacing,
            Some(3) => TilingType::AdaptiveVariableSpacing,
            Some(other) => return Err(malformed(format!("invalid /TilingType {other}"))),
            None => return Err(malformed("missing required key /TilingType")),
        };
        let bbox_values = self.required_real_array(dict, "BBox", 4)?;
        let bbox = [
            bbox_values[0],
            bbox_values[1],
            bbox_values[2],
            bbox_values[3],
        ];
        let x_step = number(dict, "XStep")?;
        let y_step = number(dict, "YStep")?;
        if !x_step.is_finite() || x_step == 0.0 || !y_step.is_finite() || y_step == 0.0 {
            return Err(malformed("/XStep and /YStep must be finite and non-zero"));
        }
        let matrix = self.optional_matrix(dict)?;
        let resources = match dict.get("Resources") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                self.resolve(value)?
                    .as_dict()
                    .cloned()
                    .ok_or_else(|| malformed("/Resources must be a dictionary"))?,
            ),
        };
        let color_space = match dict.get("ColorSpace") {
            None | Some(Value::Null) => None,
            Some(value) => Some(self.color_space(value).map_err(from_color_space)?),
        };
        if paint_type == PaintType::Uncolored && color_space.is_none() {
            return Err(malformed(
                "uncolored tiling pattern (PaintType 2) requires /ColorSpace",
            ));
        }
        let content = self
            .decode_stream_bytes(value)
            .map_err(|error| malformed(format!("tiling pattern stream: {error}")))?;
        if content.len() > MAX_PATTERN_CONTENT {
            return Err(malformed("tiling pattern content exceeds the size limit"));
        }
        Ok(TilingPattern {
            paint_type,
            tiling_type,
            bbox,
            x_step,
            y_step,
            matrix,
            resources,
            color_space,
            content,
        })
    }

    fn shading_pattern_dict(
        &self,
        value: &Value,
        resources: Option<&Dict>,
    ) -> Result<ShadingPattern> {
        let dict = value
            .as_dict()
            .ok_or_else(|| malformed("shading pattern (PatternType 2) must be a dictionary"))?;
        let matrix = self.optional_matrix(dict)?;
        let shading_value = dict
            .get("Shading")
            .ok_or_else(|| malformed("missing required key /Shading"))?;
        let shading = match shading_value.as_name() {
            Some(name) => {
                let category = resources
                    .and_then(|resources| resources.get("Shading"))
                    .and_then(|value| match value {
                        Value::Dict(dict) => Some(dict),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        malformed("named /Shading requires a shading resource dictionary")
                    })?;
                let target = category
                    .get(name)
                    .ok_or_else(|| malformed(format!("shading resource /{name} missing")))?;
                self.shading_model(target)?
            }
            None => self.shading_model(shading_value)?,
        };
        Ok(ShadingPattern { matrix, shading })
    }
}
