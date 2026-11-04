use super::{PdfBuilder, navigation, types};
use crate::core::reader::security::{StandardCryptFilter, StandardEncryptionRequest};
use crate::core::writer::{DocumentWriter, WriteError, validate_ascii85_payloads};
use std::io::Write;

/// 写端加密参数（ISO 32000-1 7.6.3 标准安全处理器）。
///
/// 由 `assemble::encryption` 的类型化入口在校验通过后构造，交给
/// [`PdfBuilder::emit`]；`None` 表示普通（不加密）输出。
pub(super) struct EncryptionSpec<'a> {
    /// 用户口令（Algorithm 4/5 的输入）。
    pub user_password: &'a [u8],
    /// 所有者口令（Algorithm 3 的输入）；`None` 时按 7.6.3.4.1 a 用用户口令代替。
    pub owner_password: Option<&'a [u8]>,
    /// 加密字典 `/P`（Table 22 权限位）。
    pub permissions: i32,
    /// 加密字典 `/V`（Table 20：`1` = RC4-40，`2` = RC4-128，`4` = crypt filter）。
    pub handler_version: u8,
    /// 密钥位宽（Table 20 `/Length`，`/V ≥ 2` 写盘）。
    pub key_bits: u8,
    /// `/V 4` 的 `StdCF` 筛选器（Table 25 `CFM`，7.6.3.1 限定 `V2`/`AESV2`）；
    /// `/V ≤ 2` 必须为 `None`（crypt filter 要求 `/V` 为 4，7.6.5）。
    pub crypt_filter: Option<StandardCryptFilter>,
}

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
        self.emit(w, None)
    }

    /// [`PdfBuilder::build`] 的实现主体，`encryption` 为 `Some` 时在写出
    /// 前按 7.6.3 加密整个文档（校验已由各类型化入口完成，此处只执行）。
    pub(super) fn emit<W: Write>(
        &mut self,
        w: &mut W,
        encryption: Option<EncryptionSpec<'_>>,
    ) -> Result<(), WriteError> {
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
        // ASCII85 串（`<~ ... ~>`）body 的装配期门禁（7.4.3 口径）：与
        // `writer::value` 的逐对象咽喉点同一实现、同一理由
        // （[`WriteError::InvalidObject`]），落在任何字节写出之前，保持本
        // 模块上方的零字节契约；它额外覆盖打包进对象流、只经无错 channel
        // 的 `write_value` 序列化的取值。
        validate_ascii85_payloads(&doc)?;
        if let Some(spec) = encryption {
            // trailer /ID 首元素是 Algorithm 2 e 的输入，必须在派生密钥前
            // 定型；缺省时以未加密字节确定性补出（14.4）。
            let file_id = self.ensure_standard_file_id(&mut doc)?;
            doc.encrypt_with_standard_handler(StandardEncryptionRequest {
                user_password: spec.user_password,
                owner_password: spec.owner_password,
                permissions: spec.permissions,
                handler_version: spec.handler_version,
                key_bits: spec.key_bits,
                crypt_filter: spec.crypt_filter,
                file_id: &file_id,
            })
            .map_err(WriteError::InvalidObject)?;
        }
        let mut writer = DocumentWriter::new().with_version(self.version);
        if let Some(compression) = self.compression.clone() {
            writer = writer.with_compression(compression);
        }
        writer.build(&doc, w)
    }
}
