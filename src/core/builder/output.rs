use super::{PdfBuilder, navigation, types};
use crate::core::writer::{DocumentWriter, WriteError};
use std::io::Write;

impl PdfBuilder {
    /// 每个 Form XObject 句柄在哪些页上被绘制（页资源 `/Fm<n>` 登记表）；
    /// 供结构树 OBJR 的 Form 目标复核 `/Pg` 是否谎报渲染页（14.7.4.3）。
    fn form_render_pages(&self) -> Vec<Vec<usize>> {
        (0..self.forms.len())
            .map(|handle| {
                self.pages
                    .iter()
                    .enumerate()
                    .filter(|(_, page)| page.forms.contains(&handle))
                    .map(|(page, _)| page)
                    .collect()
            })
            .collect()
    }

    /// 序列化为 PDF 字节流。
    ///
    /// 内部按固定顺序分配对象号、构造底层 `Document`，再交给
    /// [`DocumentWriter`] 输出。xref/trailer 由 writer 自动生成。
    pub fn build<W: Write>(&mut self, w: &mut W) -> Result<(), WriteError> {
        if let Some(length) = self.pages.iter().find_map(|page| page.invalid_code_length) {
            return Err(WriteError::InvalidObject(format!(
                "invalid CID character code length {length} in text_code_bytes (expected 1 to 4 bytes)"
            )));
        }
        // 自动/显式 MCID 撞号（14.7.4.4）：begin_structure_content 复用了
        // begin_auto_structure_content 已分配的标识符时，在 assemble_document
        // 分配对象号之前拒绝，保持零字节契约。
        if let Some(message) = self.navigation.structure_mcids_mut().take_conflict() {
            return Err(WriteError::InvalidObject(format!(
                "structure marked content: {message}"
            )));
        }
        let mut doc = self.assemble_document()?;
        // OBJR 内容项（14.7.4.3 Table 325）的目标解析表：三档句柄都在
        // 对象号分配之后才有对象号，故随导航阶段一并交给结构树写出。
        let form_render_pages = self.form_render_pages();
        navigation::apply(
            &mut doc,
            self.version,
            self.pages.len(),
            &self.navigation,
            types::ActionRefs {
                annotation_nums: &self.annotation_nums,
                ocg_nums: &self.ocg_nums,
                sound_nums: &self.sound_nums,
                form_nums: &self.form_nums,
                embedded_nums: &self.embedded_nums,
                rendition_nums: &self.rendition_nums,
                rendition_cursor: Some(&self.rendition_cursor),
            },
            navigation::structure::StructureTargets {
                annotation_nums: &self.annotation_nums,
                annotation_pages: &self.annotation_pages,
                form_nums: &self.form_nums,
                form_pages: &form_render_pages,
                image_nums: &self.image_nums,
            },
        )?;
        // 内联 section 的消费游标必须覆盖全部首级目标：遗漏说明注解/大纲
        // 的写出顺序与分配顺序不一致，此时在任何字节写出前拒绝。
        if self.rendition_cursor.get() != self.rendition_nums.len() {
            return Err(WriteError::InvalidObject(format!(
                "media clip section inline /D wrote {} of {} allocated roots",
                self.rendition_cursor.get(),
                self.rendition_nums.len()
            )));
        }
        let mut writer = DocumentWriter::new().with_version(self.version);
        if let Some(compression) = self.compression.clone() {
            writer = writer.with_compression(compression);
        }
        writer.build(&doc, w)
    }
}
