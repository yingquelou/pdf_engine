//! 图像字典条目解析助手：`/Mask`、`/Filter`、维度与整数/实数数组读取、
//! 间接引用解引用，以及 §7.4.7 JBIG2 段链的图像级建模。

use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::types::{Dict, Document, Value};

use super::error::{Result, invalid, unsupported};
use super::jbig2::{self, Jbig2Error, Jbig2Finding, Jbig2ImageInfo, Jbig2Stream};
use super::model::ImageMaskModel;

impl Document {
    /// §7.4.7：解析图像流的 JBIG2 段链，并解析 `/JBIG2Globals` 全局段流
    /// （挂在 JBIG2 步骤对应下标的 `/DecodeParms` 上，数组早位可为
    /// `null`），产出类型化一致性发现。只读诊断，不改变解析成败。
    pub(super) fn jbig2_image_info(
        &self,
        dict: &Dict,
        data: &[u8],
        filters: &[String],
    ) -> Jbig2ImageInfo {
        let mut findings = Vec::new();
        let page = jbig2::parse_jbig2_segments(data).unwrap_or_else(|error| {
            findings.push(Jbig2Finding::UnparsableSegmentChain(error));
            Jbig2Stream {
                segments: Vec::new(),
                findings: Vec::new(),
                page_information: None,
            }
        });
        // 图像流规则：段页关联应为 1；页关联 0 的全局段应另置于
        // `/JBIG2Globals` 流（Table 12）。
        for segment in &page.segments {
            if segment.page_association == 0 {
                findings.push(Jbig2Finding::GlobalSegmentInImageStream {
                    segment_number: segment.number,
                });
            } else if segment.page_association > 1 {
                findings.push(Jbig2Finding::UnexpectedPageAssociation {
                    segment_number: segment.number,
                    page_association: segment.page_association,
                });
            }
        }
        let step = filters
            .iter()
            .position(|name| name == "JBIG2Decode" || name == "JBIG2")
            .unwrap_or(0);
        let params = crate::core::filters::decode_params(dict, step);
        let globals_value = params
            .as_ref()
            .and_then(|params| params.get("JBIG2Globals"));
        let mut globals = None;
        let mut globals_reference = None;
        match globals_value {
            Some(value) => {
                if let Value::Ref(num, generation) = value {
                    globals_reference = Some((*num, *generation));
                }
                match self.resolve_image_value(value) {
                    Ok(resolved) => match &resolved {
                        Value::Stream(_, _) => {
                            let parsed = self
                                .decode_stream_bytes(&resolved)
                                .map_err(Jbig2Error::GlobalsStreamDecode)
                                .and_then(|bytes| jbig2::parse_jbig2_segments(&bytes));
                            match parsed {
                                Ok(stream) => globals = Some(stream),
                                Err(error) => {
                                    findings.push(Jbig2Finding::Jbig2GlobalsUnparsable(error))
                                }
                            }
                        }
                        other => findings.push(Jbig2Finding::Jbig2GlobalsNotAStream {
                            value_type: other.type_name().to_string(),
                        }),
                    },
                    Err(_) => findings.push(Jbig2Finding::Jbig2GlobalsNotAStream {
                        value_type: "unresolved reference".to_string(),
                    }),
                }
            }
            None => {
                // 没有 /JBIG2Globals，而图像流段引用了本流不存在的段号：
                // 全局段应置于单独的全局段流。
                let present: Vec<u32> = page.segments.iter().map(|s| s.number).collect();
                let dangling = page
                    .segments
                    .iter()
                    .flat_map(|s| s.referred_to_segments.iter())
                    .filter(|number| !present.contains(number))
                    .count();
                if dangling > 0 {
                    findings.push(Jbig2Finding::Jbig2GlobalsMissing {
                        dangling_reference_count: dangling,
                    });
                }
            }
        }
        Jbig2ImageInfo {
            page,
            globals,
            globals_reference,
            findings,
        }
    }

    pub(super) fn parse_image_mask(
        &self,
        value: &Value,
        color_space: Option<&ColorSpaceModel>,
        bits: u8,
        depth: usize,
    ) -> Result<ImageMaskModel> {
        let resolved = self.resolve_image_value(value)?;
        if let Value::Array(array) = &resolved {
            let ranges = array
                .0
                .iter()
                .map(|item| self.image_integer(item, "Mask"))
                .collect::<Result<Vec<_>>>()?;
            let components = color_space
                .and_then(|space| space.component_count())
                .ok_or_else(|| unsupported("color key mask on an image without color space"))?;
            if ranges.len() != components * 2 {
                return Err(invalid(format!(
                    "color key /Mask must contain {} integers for a {components}-component \
                     image, got {}",
                    components * 2,
                    ranges.len()
                )));
            }
            let limit = (1i64 << bits) - 1;
            for pair in ranges.as_chunks::<2>().0 {
                if pair[0] < 0 || pair[0] > pair[1] || pair[1] > limit {
                    return Err(invalid(format!(
                        "color key /Mask ranges must satisfy 0 <= min <= max <= {limit}"
                    )));
                }
            }
            return Ok(ImageMaskModel::ColorKey(ranges));
        }
        let mask = self.image_depth(&resolved, depth + 1)?;
        if !mask.image_mask {
            return Err(invalid(
                "/Mask entry must reference an image mask (/ImageMask true)",
            ));
        }
        Ok(ImageMaskModel::Explicit(Box::new(mask)))
    }

    pub(super) fn image_dimension(&self, dict: &Dict, key: &str) -> Result<u32> {
        let Some(value) = dict.get(key) else {
            return Err(invalid(format!("missing required /{key}")));
        };
        let value = self.image_integer(value, key)?;
        if !(1..=u32::MAX as i64).contains(&value) {
            return Err(invalid(format!(
                "/{key} must be positive and fit in 32 bits, got {value}"
            )));
        }
        Ok(value as u32)
    }

    pub(super) fn image_integer(&self, value: &Value, key: &str) -> Result<i64> {
        match self.resolve_image_value(value)? {
            Value::Integer(number) => Ok(number),
            Value::Float(number) if number.fract() == 0.0 => Ok(number as i64),
            other => Err(invalid(format!(
                "/{key} must be an integer, got {}",
                other.type_name()
            ))),
        }
    }

    pub(super) fn image_real_array(&self, value: &Value, key: &str) -> Result<Vec<f64>> {
        let resolved = self.resolve_image_value(value)?;
        let array = resolved
            .as_array()
            .ok_or_else(|| invalid(format!("/{key} must be an array")))?;
        array
            .0
            .iter()
            .map(|item| match item {
                Value::Integer(number) => Ok(*number as f64),
                Value::Float(number) => Ok(*number),
                other => Err(invalid(format!(
                    "/{key} has a non-numeric element ({})",
                    other.type_name()
                ))),
            })
            .collect()
    }

    /// 7.4.1 / Table 5：图像流字典 `/Filter` 的规范名列表。
    ///
    /// 复用 [`crate::core::filters::normalize_filter_entries`] 共享规范化器
    /// （与 `Document::decode_stream_bytes` 同一入口逻辑）：`/Filter` 的值、
    /// 以及 Name 数组的**元素**写成间接引用（7.3.10）时都经同一懒加载通道
    /// （[`Document::get_object`]）解引用，跳数上限与环检测由规范化器内部
    /// 完成；解引用失败保留原值并产出 `7.4.1` 诊断，随后按真正畸形的形态
    /// 报 `invalid`——绝不静默按「无筛选器」误解码。
    pub(super) fn image_filters(&self, dict: &Dict) -> Result<Vec<String>> {
        if dict.get("Filter").is_none() {
            return Ok(Vec::new());
        }
        let resolve = |value: &Value| -> std::result::Result<Option<Value>, String> {
            let Value::Ref(num, _) = value else {
                return Err(format!(
                    "expected an indirect reference, got {}",
                    value.type_name()
                ));
            };
            // 按对象号取最新修订，忽略引用中的代号（与 document 侧同口径）。
            self.get_object(*num)
                .map(|object| object.map(|object| object.value))
                .map_err(|error| error.to_string())
        };
        let (normalized, issues) = crate::core::filters::normalize_filter_entries(dict, &resolve);
        for issue in issues {
            self.push_warning(format!("image stream filter dictionary (7.4.1): {issue}"));
        }
        let names = match normalized.get("Filter") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Name(name)) => vec![name.clone()],
            Some(Value::Array(array)) => array
                .0
                .iter()
                .map(|item| match item {
                    Value::Name(name) => Ok(name.clone()),
                    other => Err(invalid(format!(
                        "/Filter array has a non-name element ({})",
                        other.type_name()
                    ))),
                })
                .collect::<Result<Vec<_>>>()?,
            Some(other) => {
                return Err(invalid(format!(
                    "/Filter must be a name or array, got {}",
                    other.type_name()
                )));
            }
        };
        Ok(names
            .iter()
            .map(|name| crate::core::filters::canonical_filter(name).to_string())
            .collect())
    }

    pub(super) fn resolve_image_value(&self, value: &Value) -> Result<Value> {
        match value {
            Value::Ref(num, generation) => {
                let object = self.get_object(*num)?.ok_or_else(|| {
                    invalid(format!("unresolved object reference {num} {generation} R"))
                })?;
                Ok(object.value)
            }
            other => Ok(other.clone()),
        }
    }
}
