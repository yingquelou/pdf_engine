//! Form XObject 流对象写出：`/BBox`、`/Matrix`、`/Group`、自有资源字典与
//! 打印机标记（14.11.3 Table 363）、陷印网络（14.11.6 Table 367）条目及
//! `/OPI` 代理字典的内联。

use crate::core::builder::PdfBuilder;
use crate::core::builder::types::text_string_value;
use crate::core::types::{
    Array, Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Value,
};
use crate::core::writer::WriteError;

use super::colorspace::color_space_value;
use super::opi;
use super::plan::{FormPlan, GroupPlan, MarkedPropertiesPlan};
use super::utils::{group_dict, real_array};

impl PdfBuilder {
    /// 写出实际被引用的 Form XObject 流；版本门禁先于条目校验，顺序与
    /// 拆分前一致。
    pub(super) fn assemble_form_streams(
        &mut self,
        doc: &mut Document,
        num: &mut i64,
        forms: &FormPlan,
        groups: &GroupPlan,
        marked_properties: &MarkedPropertiesPlan,
    ) -> Result<(), WriteError> {
        // Form XObject 流对象：BBox 必填，Matrix 仅在提供时写出。
        for &handle in &forms.used_forms {
            let form = &mut self.forms[handle];
            // Printer's mark 表单条目（14.11.3 Table 363）自 PDF 1.4 引入，
            // 版本门禁先于条目校验，保证低版本一律报 UnsupportedVersion。
            if form.has_printer_mark_entries() && self.version < PdfVersion::V1_4 {
                return Err(WriteError::UnsupportedVersion(format!(
                    "printer's mark form entries require PDF {}, got {}",
                    PdfVersion::V1_4.as_str(),
                    self.version.as_str()
                )));
            }
            // Trap 网络表单条目（14.11.6 Table 367）自 PDF 1.3 引入，同样
            // 先于条目校验，保证低版本一律报 UnsupportedVersion。
            if form.has_trap_network_entries() && self.version < PdfVersion::V1_3 {
                return Err(WriteError::UnsupportedVersion(format!(
                    "trap network form entries require PDF {}, got {}",
                    PdfVersion::V1_3.as_str(),
                    self.version.as_str()
                )));
            }
            form.validate()
                .map_err(|message| WriteError::InvalidObject(format!("form XObject: {message}")))?;
            let content_bytes = form.content.to_bytes();
            let len = content_bytes.len();
            let mut dict = Dict::new()
                .insert("Type", Value::Name("XObject".into()))
                .insert("Subtype", Value::Name("Form".into()))
                .insert("FormType", Value::Integer(1))
                .insert("BBox", real_array(&form.bbox))
                .insert("Length", Value::Integer(len as i64));
            if let Some(matrix) = form.matrix {
                dict = dict.insert("Matrix", real_array(&matrix));
            }
            // 表单级页面件字典（14.5 Table 95/318，PDF 1.3）。
            if let Some(piece_info) = &form.piece_info {
                if self.version < PdfVersion::V1_3 {
                    return Err(WriteError::UnsupportedVersion(format!(
                        "form XObject /PieceInfo requires PDF {}, got {}",
                        PdfVersion::V1_3.as_str(),
                        self.version.as_str()
                    )));
                }
                piece_info.validate()?;
                dict = dict.insert("PieceInfo", Value::Dict(piece_info.to_dict()));
            }
            if let Some(group) = &form.group {
                dict = dict.insert("Group", Value::Dict(group_dict(group, doc, num)?));
                doc.features.borrow_mut().insert(PdfFeature::Transparency);
            }
            // 自有资源字典：显式提供时总是写出（即使为空），从而替代而
            // 非合并继承自绘制页面的资源。
            if let Some(resources) = &form.resources {
                let mut form_resources = Dict::new();
                if !forms.form_font_nums[handle].is_empty() {
                    let mut font_dict = Dict::new();
                    for (k, fnum) in forms.form_font_nums[handle].iter().enumerate() {
                        font_dict = font_dict.insert(format!("F{}", k + 1), Value::Ref(*fnum, 0));
                    }
                    form_resources = form_resources.insert("Font", Value::Dict(font_dict));
                }
                if !forms.form_image_nums[handle].is_empty() {
                    let mut xobj_dict = Dict::new();
                    for (k, inum) in forms.form_image_nums[handle].iter().enumerate() {
                        xobj_dict = xobj_dict.insert(format!("Im{}", k + 1), Value::Ref(*inum, 0));
                    }
                    form_resources = form_resources.insert("XObject", Value::Dict(xobj_dict));
                }
                let mut properties = Dict::new();
                for (k, group) in resources.optional_content.iter().enumerate() {
                    properties = properties.insert(
                        format!("OC{}", k + 1),
                        Value::Ref(groups.ocg_nums[*group], 0),
                    );
                }
                // 表单内容里的标记内容属性表（14.6.2）：把本表单用到的
                // `MC<n>` 登记进自有 `/Properties`。
                for (name, object) in &marked_properties.form_properties[handle] {
                    properties = properties.insert(name.clone(), Value::Ref(*object, 0));
                }
                if !properties.0.is_empty() {
                    form_resources = form_resources.insert("Properties", Value::Dict(properties));
                }
                dict = dict.insert("Resources", Value::Dict(form_resources));
            }
            if let Some(group) = form.optional_content {
                dict = dict.insert("OC", Value::Ref(groups.ocg_nums[group], 0));
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::OptionalContent);
            }
            // Printer's mark 表单条目（14.11.3 Table 363，PDF 1.4）：
            // `/MarkStyle` 为文本串，`/Colorants` 为色料名 → Separation 空间。
            if form.has_printer_mark_entries() {
                if let Some(mark_style) = &form.mark_style {
                    dict = dict.insert("MarkStyle", text_string_value(mark_style));
                }
                if !form.colorants.is_empty() {
                    let mut colorants = Dict::new();
                    for (name, space) in &form.colorants {
                        colorants =
                            colorants.insert(name.as_str(), color_space_value(space, doc, num)?);
                    }
                    dict = dict.insert("Colorants", Value::Dict(colorants));
                }
                doc.features.borrow_mut().insert(PdfFeature::PrinterMarks);
            }
            // Trap 网络表单条目（14.11.6 Table 367，PDF 1.3）：`/PCM` 为
            // name，`/SeparationColorNames` 为 name 数组，`/TrapRegions` 为
            // 间接引用数组，`/TrapStyles` 为文本串，`/FontFauxing` 为字体
            // 字典数组（原样写出）。
            if form.has_trap_network_entries() {
                if let Some(pcm) = &form.pcm {
                    dict = dict.insert("PCM", Value::Name(pcm.name().into()));
                }
                if !form.separation_color_names.is_empty() {
                    let names = form
                        .separation_color_names
                        .iter()
                        .map(|name| Value::Name(name.clone()))
                        .collect();
                    dict = dict.insert("SeparationColorNames", Value::Array(Array(names)));
                }
                if !form.trap_regions.is_empty() {
                    let regions = form
                        .trap_regions
                        .iter()
                        .map(|(num, generation)| Value::Ref(*num, *generation))
                        .collect();
                    dict = dict.insert("TrapRegions", Value::Array(Array(regions)));
                }
                if let Some(styles) = &form.trap_styles {
                    dict = dict.insert("TrapStyles", text_string_value(styles));
                }
                if !form.font_fauxing.is_empty() {
                    let fonts = form.font_fauxing.iter().cloned().map(Value::Dict).collect();
                    dict = dict.insert("FontFauxing", Value::Array(Array(fonts)));
                }
                doc.features.borrow_mut().insert(PdfFeature::TrapNetworks);
            }
            // OPI 代理字典（14.11.7 Table 368，PDF 1.2）：内联版本字典，
            // 不分配额外对象；门禁与条目契约已在对象号分配前完成。
            dict = opi::insert_opi(doc, dict, &form.opi);
            doc.set_object(Object::new(
                forms.form_nums[handle],
                0,
                Value::Stream(dict, StreamData::Owned(content_bytes)),
            ));
        }
        Ok(())
    }
}
