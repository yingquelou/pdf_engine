//! 字体描述符度量（FontDescriptor）。

use crate::core::types::{Array, Dict, Value};

/// 字体描述符度量（FontDescriptor metrics）。
///
/// 对应 PDF 标准 9.8 节的字体描述符字典，包含字体的度量与属性信息。
/// 标准 14 字体通常可省略描述符，但嵌入字体或需要精确度量时必须提供。
/// CIDFont 描述符额外支持 `/FontName`、`/CharSet` 与 `/CIDSet`。
#[derive(Debug, Clone, PartialEq)]
pub struct FontDescriptor {
    /// `/FontName` 字体名称。
    pub font_name: Option<String>,
    /// `/FontFamily` 字体族名。
    pub font_family: Option<String>,
    /// `/FontStretch` 字体stretch 名称（如 Normal、Condensed）。
    pub font_stretch: Option<String>,
    /// `/FontWeight` 字体粗细（100-900）。
    pub font_weight: Option<f64>,
    /// `/Flags` 字体属性标志位。
    pub flags: Option<u32>,
    /// `/FontBBox` 字体边界框。
    pub font_bbox: Option<[f64; 4]>,
    /// `/ItalicAngle` 斜体角度（度，负值表示右倾）。
    pub italic_angle: Option<f64>,
    /// `/Ascent` 上升高度。
    pub ascent: Option<f64>,
    /// `/Descent` 下降深度（通常为负值）。
    pub descent: Option<f64>,
    /// `/Leading` 行间距。
    pub leading: Option<f64>,
    /// `/CapHeight` 大写字母高度。
    pub cap_height: Option<f64>,
    /// `/XHeight` 小写 x 高度。
    pub x_height: Option<f64>,
    /// `/StemV` 竖笔宽度。
    pub stem_v: Option<f64>,
    /// `/StemH` 横笔高度。
    pub stem_h: Option<f64>,
    /// `/AvgWidth` 平均宽度。
    pub avg_width: Option<f64>,
    /// `/MaxWidth` 最大宽度。
    pub max_width: Option<f64>,
    /// `/MissingWidth` 缺失字形宽度。
    pub missing_width: Option<f64>,
    /// `/CharSet` 字符集名称字符串（简单字体）。
    pub char_set: Option<String>,
    /// `/CIDSet` 位数组（MSB first），按 CID 索引标记哪些 CID 存在。
    /// 仅用于 CIDFont 描述符。
    pub cid_set: Option<Vec<u8>>,
}

impl Default for FontDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

impl FontDescriptor {
    /// 创建空的字体描述符。
    pub fn new() -> Self {
        Self {
            font_name: None,
            font_family: None,
            font_stretch: None,
            font_weight: None,
            flags: None,
            font_bbox: None,
            italic_angle: None,
            ascent: None,
            descent: None,
            leading: None,
            cap_height: None,
            x_height: None,
            stem_v: None,
            stem_h: None,
            avg_width: None,
            max_width: None,
            missing_width: None,
            char_set: None,
            cid_set: None,
        }
    }

    /// 设置字体名称。
    pub fn with_font_name(mut self, name: impl Into<String>) -> Self {
        self.font_name = Some(name.into());
        self
    }

    /// 设置字体族名。
    pub fn with_family(mut self, family: impl Into<String>) -> Self {
        self.font_family = Some(family.into());
        self
    }

    /// 设置字体stretch。
    pub fn with_stretch(mut self, stretch: impl Into<String>) -> Self {
        self.font_stretch = Some(stretch.into());
        self
    }

    /// 设置字体粗细。
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.font_weight = Some(weight);
        self
    }

    /// 设置属性标志位。
    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags = Some(flags);
        self
    }

    /// 设置字体边界框。
    pub fn with_bbox(mut self, bbox: [f64; 4]) -> Self {
        self.font_bbox = Some(bbox);
        self
    }

    /// 设置斜体角度。
    pub fn with_italic_angle(mut self, angle: f64) -> Self {
        self.italic_angle = Some(angle);
        self
    }

    /// 设置上升高度。
    pub fn with_ascent(mut self, ascent: f64) -> Self {
        self.ascent = Some(ascent);
        self
    }

    /// 设置下降深度。
    pub fn with_descent(mut self, descent: f64) -> Self {
        self.descent = Some(descent);
        self
    }

    /// 设置行间距。
    pub fn with_leading(mut self, leading: f64) -> Self {
        self.leading = Some(leading);
        self
    }

    /// 设置大写字母高度。
    pub fn with_cap_height(mut self, height: f64) -> Self {
        self.cap_height = Some(height);
        self
    }

    /// 设置小写 x 高度。
    pub fn with_x_height(mut self, height: f64) -> Self {
        self.x_height = Some(height);
        self
    }

    /// 设置竖笔宽度。
    pub fn with_stem_v(mut self, width: f64) -> Self {
        self.stem_v = Some(width);
        self
    }

    /// 设置横笔高度。
    pub fn with_stem_h(mut self, height: f64) -> Self {
        self.stem_h = Some(height);
        self
    }

    /// 设置平均宽度。
    pub fn with_avg_width(mut self, width: f64) -> Self {
        self.avg_width = Some(width);
        self
    }

    /// 设置最大宽度。
    pub fn with_max_width(mut self, width: f64) -> Self {
        self.max_width = Some(width);
        self
    }

    /// 设置缺失字形宽度。
    pub fn with_missing_width(mut self, width: f64) -> Self {
        self.missing_width = Some(width);
        self
    }

    /// 设置字符集名称（简单字体 `/CharSet`）。
    pub fn with_char_set(mut self, char_set: impl Into<String>) -> Self {
        self.char_set = Some(char_set.into());
        self
    }

    /// 设置 CIDSet 位数组（CIDFont `/CIDSet`）。
    ///
    /// 位数组按 MSB-first 编码，第 `cid` 位表示 CID `cid` 是否存在。
    pub fn with_cid_set(mut self, cid_set: Vec<u8>) -> Self {
        self.cid_set = Some(cid_set);
        self
    }

    /// 补齐 PDF 32000-1 Table 122 的必需项，供 assemble 阶段兜底。
    ///
    /// 仅填充缺失字段，用户显式提供的值保持不变。默认值：
    /// `FontName = font_name`（应为带子集标签的 `/BaseFont`）、
    /// `Flags = 4`（Symbolic）、`FontBBox = [0 -200 1000 900]`、
    /// `ItalicAngle = 0`、`Ascent = 800`、`Descent = -200`、
    /// `CapHeight = 700`、`StemV = 80`。
    pub(crate) fn fill_required_defaults(&mut self, font_name: &str) {
        self.font_name.get_or_insert_with(|| font_name.to_string());
        self.flags.get_or_insert(4);
        self.font_bbox.get_or_insert([0.0, -200.0, 1000.0, 900.0]);
        self.italic_angle.get_or_insert(0.0);
        self.ascent.get_or_insert(800.0);
        self.descent.get_or_insert(-200.0);
        self.cap_height.get_or_insert(700.0);
        self.stem_v.get_or_insert(80.0);
    }

    /// 转换为 PDF 字典值。
    pub(crate) fn to_value(&self) -> Value {
        let mut dict = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
        if let Some(name) = &self.font_name {
            dict = dict.insert("FontName", Value::Name(name.clone()));
        }
        if let Some(family) = &self.font_family {
            dict = dict.insert("FontFamily", Value::String(family.clone()));
        }
        if let Some(stretch) = &self.font_stretch {
            dict = dict.insert("FontStretch", Value::Name(stretch.clone()));
        }
        if let Some(weight) = self.font_weight {
            dict = dict.insert("FontWeight", Value::Float(weight));
        }
        if let Some(flags) = self.flags {
            dict = dict.insert("Flags", Value::Integer(flags as i64));
        }
        if let Some(bbox) = &self.font_bbox {
            dict = dict.insert(
                "FontBBox",
                Value::Array(
                    Array::new()
                        .push(Value::Float(bbox[0]))
                        .push(Value::Float(bbox[1]))
                        .push(Value::Float(bbox[2]))
                        .push(Value::Float(bbox[3])),
                ),
            );
        }
        if let Some(angle) = self.italic_angle {
            dict = dict.insert("ItalicAngle", Value::Float(angle));
        }
        if let Some(ascent) = self.ascent {
            dict = dict.insert("Ascent", Value::Float(ascent));
        }
        if let Some(descent) = self.descent {
            dict = dict.insert("Descent", Value::Float(descent));
        }
        if let Some(leading) = self.leading {
            dict = dict.insert("Leading", Value::Float(leading));
        }
        if let Some(cap_height) = self.cap_height {
            dict = dict.insert("CapHeight", Value::Float(cap_height));
        }
        if let Some(x_height) = self.x_height {
            dict = dict.insert("XHeight", Value::Float(x_height));
        }
        if let Some(stem_v) = self.stem_v {
            dict = dict.insert("StemV", Value::Float(stem_v));
        }
        if let Some(stem_h) = self.stem_h {
            dict = dict.insert("StemH", Value::Float(stem_h));
        }
        if let Some(avg_width) = self.avg_width {
            dict = dict.insert("AvgWidth", Value::Float(avg_width));
        }
        if let Some(max_width) = self.max_width {
            dict = dict.insert("MaxWidth", Value::Float(max_width));
        }
        if let Some(missing_width) = self.missing_width {
            dict = dict.insert("MissingWidth", Value::Float(missing_width));
        }
        if let Some(char_set) = &self.char_set {
            dict = dict.insert("CharSet", Value::String(char_set.clone()));
        }
        // /CIDSet 作为流对象写出（此处仅返回字典项，流由 assemble 单独处理）
        // 注意：CIDSet 是流，不能直接嵌入字典值，由 assemble.rs 单独构造
        Value::Dict(dict)
    }

    /// 返回 `/CIDSet` 数据（如果存在）。
    pub(crate) fn cid_set_data(&self) -> Option<&[u8]> {
        self.cid_set.as_deref()
    }
}
