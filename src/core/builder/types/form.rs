//! Form XObject：可复用的自包含内容流。

use super::{ColorSpace, Font, ImageData};
use crate::core::content::ContentStream;

/// 组属性字典（ISO 32000-1 §8.10.3 Table 96、§11.6.6）。
///
/// 规范目前只定义 `/S /Transparency` 一种组子类型；`/CS` 省略时组使用
/// 绘制页面的默认颜色空间，`/I`、`/K` 省略时均为 false。
#[derive(Debug, Clone)]
pub struct GroupXObject {
    /// 组颜色空间，须为设备、CIE-Based 或 ICCBased 空间。
    pub color_space: Option<ColorSpace>,
    /// 隔离组（`/I`），省略即 false。
    pub isolated: bool,
    /// 挖空组（`/K`），省略即 false。
    pub knockout: bool,
}

impl GroupXObject {
    /// 创建透明组属性字典（不指定颜色空间、非隔离、非挖空）。
    pub fn transparency() -> Self {
        Self {
            color_space: None,
            isolated: false,
            knockout: false,
        }
    }

    /// 设置组颜色空间。
    pub fn with_color_space(mut self, color_space: ColorSpace) -> Self {
        self.color_space = Some(color_space);
        self
    }

    /// 设置是否隔离。
    pub fn with_isolated(mut self, isolated: bool) -> Self {
        self.isolated = isolated;
        self
    }

    /// 设置是否挖空。
    pub fn with_knockout(mut self, knockout: bool) -> Self {
        self.knockout = knockout;
        self
    }

    /// 校验组颜色空间；非法时返回可读原因。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(color_space) = &self.color_space {
            if matches!(color_space, ColorSpace::Indexed(_)) {
                return Err("group color space must not be Indexed".into());
            }
            if matches!(
                color_space,
                ColorSpace::Separation(_) | ColorSpace::DeviceN(_)
            ) {
                return Err("group color space must be a device or CIE-based color space".into());
            }
            color_space
                .validate()
                .map_err(|message| format!("group color space: {message}"))?;
        }
        Ok(())
    }
}

/// Form XObject 自有资源字典（ISO 32000-1 §8.10.1 Table 95 `/Resources`）。
///
/// 提供后表单内容不再继承绘制页面的资源字典（资源字典不合并）。资源名
/// 按声明顺序分配：字体 `/F1`、`/F2`…，图片 `/Im1`、`/Im2`…，可选内容
/// 属性 `/OC1`、`/OC2`…（`/Properties` 子字典中的 OCG 引用，供
/// `/OC /OC1 BDC` 使用）。可选内容组句柄来自
/// [`PdfBuilder::optional_content_group`](crate::core::builder::PdfBuilder::optional_content_group)，
/// 越界时构建返回 [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormResources {
    /// 表单自有字体（资源名 `/F<n>`）。
    pub fonts: Vec<Font>,
    /// 表单自有图片 XObject（资源名 `/Im<n>`）。
    pub images: Vec<ImageData>,
    /// 表单内容可选内容属性（资源名 `/OC<n>`），元素为文档级 OCG 句柄。
    pub optional_content: Vec<usize>,
}

impl FormResources {
    /// 创建空资源字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个表单自有字体。
    pub fn with_font(mut self, font: Font) -> Self {
        self.fonts.push(font);
        self
    }

    /// 追加一张表单自有图片。
    pub fn with_image(mut self, image: ImageData) -> Self {
        self.images.push(image);
        self
    }

    /// 追加一个可选内容属性（OCG 句柄）。
    pub fn with_optional_content(mut self, group: usize) -> Self {
        self.optional_content.push(group);
        self
    }
}

/// Form XObject（ISO 32000-1 §8.10.1，Table 95）。
///
/// 内容流由调用方提供。未提供 [`FormXObject::resources`] 时写端不追加
/// `/Resources`：按规范省略该条目时表单使用所在页面的资源字典，因此
/// 表单内容可以引用绘制页上的字体或图片资源名，也可以完全只含路径等
/// 无资源操作符。提供自有资源时写出 `/Resources`，表单内容引用其中按
/// [`FormResources`] 约定命名的资源。
#[derive(Debug, Clone)]
pub struct FormXObject {
    /// 表单坐标空间中的边界框 `[llx lly urx ury]`。
    pub bbox: [f64; 4],
    /// 表单矩阵 `[a b c d e f]`，省略时为单位矩阵。
    pub matrix: Option<[f64; 6]>,
    /// 组属性字典；提供时表单成为 Group XObject（PDF 1.4）。
    pub group: Option<GroupXObject>,
    /// 自有资源字典；提供时不再继承绘制页面的资源。
    pub resources: Option<FormResources>,
    /// 表单级可选内容组句柄（`/OC`，PDF 1.5）；整张表单在组隐藏时不绘制。
    pub optional_content: Option<usize>,
    /// Printer's mark 表单的 `/MarkStyle`（14.11.3 Table 363，PDF 1.4）。
    pub mark_style: Option<String>,
    /// Printer's mark 表单的 `/Colorants`（Table 363）：色料名 → Separation 空间。
    pub colorants: Vec<(String, ColorSpace)>,
    /// 表单内容流。
    pub content: ContentStream,
}

impl FormXObject {
    /// 以边界框与内容流创建表单。
    pub fn new(bbox: [f64; 4], content: ContentStream) -> Self {
        Self {
            bbox,
            matrix: None,
            group: None,
            resources: None,
            optional_content: None,
            mark_style: None,
            colorants: Vec::new(),
            content,
        }
    }

    /// 设置可选的表单矩阵。
    pub fn with_matrix(mut self, matrix: [f64; 6]) -> Self {
        self.matrix = Some(matrix);
        self
    }

    /// 附加组属性字典，使表单成为 Group XObject。
    pub fn with_group(mut self, group: GroupXObject) -> Self {
        self.group = Some(group);
        self
    }

    /// 设置表单自有资源字典（替代从绘制页面继承的资源）。
    pub fn with_resources(mut self, resources: FormResources) -> Self {
        self.resources = Some(resources);
        self
    }

    /// 设置表单级可选内容组（`/OC`，PDF 1.5）。
    pub fn with_optional_content(mut self, group: usize) -> Self {
        self.optional_content = Some(group);
        self
    }

    /// 设置 Printer's mark 表单描述 `/MarkStyle`（Table 363，PDF 1.4）。
    pub fn with_mark_style(mut self, mark_style: impl Into<String>) -> Self {
        self.mark_style = Some(mark_style.into());
        self
    }

    /// 追加一项 Printer's mark 色料定义 `/Colorants`（Table 363，PDF 1.4）。
    ///
    /// 键须与 `space` 中 Separation 颜色空间的色料名一致；校验在
    /// [`PdfBuilder::build`](crate::core::builder::PdfBuilder::build) 阶段进行。
    pub fn with_colorant(mut self, name: impl Into<String>, space: ColorSpace) -> Self {
        self.colorants.push((name.into(), space));
        self
    }

    /// 是否声明了 Table 363 的 Printer's mark 表单条目。
    pub(crate) fn has_printer_mark_entries(&self) -> bool {
        self.mark_style.is_some() || !self.colorants.is_empty()
    }

    /// 校验 BBox、Matrix 与组属性；非法时返回可读原因。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.bbox.iter().any(|v| !v.is_finite()) {
            return Err("BBox must contain 4 finite numbers".into());
        }
        if self.bbox[0] > self.bbox[2] || self.bbox[1] > self.bbox[3] {
            return Err("BBox minimum must not exceed maximum".into());
        }
        if let Some(matrix) = self.matrix
            && matrix.iter().any(|v| !v.is_finite())
        {
            return Err("Matrix must contain 6 finite numbers".into());
        }
        if let Some(group) = &self.group {
            group
                .validate()
                .map_err(|message| format!("group: {message}"))?;
        }
        // Table 363 `/Colorants`：键为合法 PDF name，值与 Separation 颜色
        // 空间的色料名一致、不得重复，并复用颜色空间自身的校验。
        let mut seen = std::collections::BTreeSet::new();
        for (name, space) in &self.colorants {
            super::annotation::validate_name_value(name, "printer's mark form colorant name")?;
            let ColorSpace::Separation(separation) = space else {
                return Err(format!(
                    "printer's mark form colorant {name} must be a Separation color space"
                ));
            };
            if separation.colorant != *name {
                return Err(format!(
                    "printer's mark form colorant {name} does not match its Separation colorant {}",
                    separation.colorant
                ));
            }
            if !seen.insert(name.as_str()) {
                return Err(format!(
                    "printer's mark form colorant {name} is registered twice"
                ));
            }
            space
                .validate()
                .map_err(|message| format!("printer's mark form colorant {name}: {message}"))?;
        }
        Ok(())
    }
}
