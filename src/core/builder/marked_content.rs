use super::{
    AnnotationEntry, ArtifactProperties, GraphicsState, PdfAnnotation, PdfBuilder, Sound,
    SpanProperties, ThreeDArtwork, same_font,
};
use crate::core::content::MarkedContentProperties;
use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

impl PdfBuilder {
    /// 把一个注解（12.5）绑定到 0 基页号，返回文档级句柄。
    ///
    /// 句柄供另一注解的 [`PdfAnnotation::with_popup`] 引用。注册的全部
    /// 注解在 [`PdfBuilder::build`] 时写成独立间接对象并登记进页面
    /// `/Annots`；页号越界或字典非法（[`PdfAnnotation::validate`] 语义）
    /// 返回 [`WriteError::InvalidObject`]，子类型版本不足返回
    /// [`WriteError::UnsupportedVersion`]。
    pub fn add_annotation(&mut self, page: usize, annot: PdfAnnotation) -> usize {
        let idx = self.annotations.len();
        self.annotations.push(AnnotationEntry { page, annot });
        idx
    }

    /// 注册一个声音对象（13.3 Table 294），返回文档级句柄。
    ///
    /// 句柄供 [`PdfAnnotation::with_sound`]（Sound 注解，12.5.6.16）与
    /// [`SoundAction`]（Sound 动作，12.6.4.8）引用。每个注册的声音在
    /// [`PdfBuilder::build`] 时写成独立间接流对象；采样率、声道数、
    /// 每样本位数与数据在构建阶段校验，非法输入返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.2 返回
    /// [`WriteError::UnsupportedVersion`]，均不写出任何字节。
    pub fn add_sound(&mut self, sound: Sound) -> usize {
        let handle = self.sounds.len();
        self.sounds.push(sound);
        handle
    }

    /// 注册一个 3D artwork 流（13.6 Table 300），返回文档级句柄。
    ///
    /// 句柄供 [`PdfAnnotation::with_three_d`]（3D 注解 `/3DD`）引用。
    /// 每个注册项在 [`PdfBuilder::build`] 时写成独立间接流对象；启用
    /// [`ThreeDArtwork::with_reference_dictionary`] 时另写 Table 303 引用
    /// 字典，引用同一字典的多个注解共享运行实例；声明
    /// [`ThreeDArtwork::with_on_instantiate`] 时脚本写成独立间接流并由
    /// `/OnInstantiate` 引用。数据格式、视图与默认视图在构建阶段校验，
    /// 非法输入返回 [`WriteError::InvalidObject`]；目标版本低于 1.6 返回
    /// [`WriteError::UnsupportedVersion`]，均不写出任何字节。
    pub fn add_three_d_artwork(&mut self, artwork: ThreeDArtwork) -> usize {
        let handle = self.three_d_artworks.len();
        self.three_d_artworks.push(artwork);
        handle
    }

    /// 在指定页开始一段可选内容：`/OC /OC<n> BDC`。
    ///
    /// 组句柄必须是 [`PdfBuilder::optional_content_group`] 的返回值；同页
    /// 首次使用分配 `/OC<n>` 资源名，后续复用。必须以
    /// [`PdfBuilder::end_optional_content`] 结束。未知组句柄或页索引被忽略。
    /// 目标版本低于 1.5 时构建返回 [`WriteError::UnsupportedVersion`]。
    pub fn begin_optional_content(&mut self, page: usize, group: usize) -> &mut Self {
        if group >= self.optional_content_groups.len() {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.optional_content.iter().position(|&g| g == group) {
            Some(i) => i,
            None => {
                p.optional_content.push(group);
                p.optional_content.len() - 1
            }
        };
        p.content.begin_marked_content_properties(
            "OC",
            MarkedContentProperties::Name(format!("OC{}", idx + 1)),
        );
        self
    }

    /// 结束当前可选内容段（`EMC`）。未知页索引被忽略。
    pub fn end_optional_content(&mut self, page: usize) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.end_marked_content();
        }
        self
    }

    /// 在指定页开始一段 Span 标记内容：`/Span << ... >> BDC`
    /// （ISO 32000-1 14.9.3–14.9.5）。
    ///
    /// 属性表由 [`SpanProperties::to_dict`] 内联写出。`/Lang` 与 `/E` 自
    /// PDF 1.4、`/Alt` 与 `/ActualText` 自 PDF 1.5 引入，目标版本不足时
    /// 构建返回 [`WriteError::UnsupportedVersion`]，且不写出任何字节。
    /// 必须以 [`PdfBuilder::end_span`] 结束；未知页索引被忽略。
    pub fn begin_span(&mut self, page: usize, properties: &SpanProperties) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.begin_marked_content_properties(
                "Span",
                MarkedContentProperties::Dict(properties.to_dict()),
            );
        }
        self
    }

    /// 结束当前 Span 标记内容段（`EMC`）。未知页索引被忽略。
    pub fn end_span(&mut self, page: usize) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.end_marked_content();
        }
        self
    }

    /// 在指定页开始一段 Artifact 标记内容：`/Artifact << ... >> BDC`
    /// （ISO 32000-1 14.8.2.2.2 与 Table 330）。
    ///
    /// 属性表由 `ArtifactProperties::to_dict` 内联写出。Artifact 属性表
    /// 自 PDF 1.4 引入，`/Type /Background` 与 `/Subtype` 自 PDF 1.7 引入；
    /// 目标版本不足时构建返回 [`WriteError::UnsupportedVersion`]，且不写出
    /// 任何字节。必须以 [`PdfBuilder::end_artifact`] 结束；未知页索引被忽略。
    pub fn begin_artifact(&mut self, page: usize, properties: &ArtifactProperties) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.begin_marked_content_properties(
                "Artifact",
                MarkedContentProperties::Dict(properties.to_dict()),
            );
        }
        self
    }

    /// 结束当前 Artifact 标记内容段（`EMC`）。未知页索引被忽略。
    pub fn end_artifact(&mut self, page: usize) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.end_marked_content();
        }
        self
    }

    /// 在指定页开始一段结构内容：`/<tag> << /MCID n >> BDC`
    /// （ISO 32000-1 14.8.2）。
    ///
    /// `mcid` 必须与结构树里 [`StructureElement::with_marked_content`] 对同一
    /// 页使用的标识符一致：`/ParentTree` 只按结构树的 MCR 生成，页面标记
    /// 内容靠 `/MCID` 与之配对，两者不一致时读端无法定位结构元素。
    /// [`SpanProperties`] 与 [`ArtifactProperties`] 刻意不带 `/MCID`：
    /// 14.8.2.3.1 规定 artifact 不得被赋予结构身份，Span 属性表只描述文本
    /// 呈现，需要结构身份时用本方法。
    ///
    /// 构建时逐页交叉校验页面 `/MCID` 与结构树 MCR：非整数 `/MCID`、同页
    /// 重复、有 `/MCID` 却没有结构树、页面与结构树不一致、标签为
    /// `Artifact` 的标记内容带 `/MCID`，均返回
    /// [`WriteError::InvalidObject`]，且不写出任何字节。未知页索引被忽略。
    /// 必须以 [`PdfBuilder::end_structure_content`] 结束。
    ///
    /// 与 [`PdfBuilder::begin_auto_structure_content`] 混用时，显式传入的
    /// `mcid` 不得复用本页已被自动分配的标识符：`build` 在对象号分配之前
    /// 返回 [`WriteError::InvalidObject`]（信息指明页、MCID 与两处来源），
    /// 零字节输出。
    pub fn begin_structure_content(
        &mut self,
        page: usize,
        tag: impl Into<String>,
        mcid: i64,
    ) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.begin_marked_content_properties(
                tag,
                MarkedContentProperties::Dict(Dict::new().insert("MCID", Value::Integer(mcid))),
            );
            self.navigation
                .structure_mcids_mut()
                .note_explicit(page, mcid);
        }
        self
    }

    /// 在指定页开始一段自动编号的结构内容：`/<tag> << /MCID n >> BDC`，
    /// 返回分配的 `n`（ISO 32000-1 14.7.4.4 NOTE 与 14.8.2）。
    ///
    /// 与 [`PdfBuilder::begin_structure_content`] 的唯一区别是 MCID 由构建器
    /// 自动分配：按页取「显式 ∪ 已自动分配」之外的最小非负整数，使纯自动
    /// 编号的页面从 0 起稠密递增——14.7.4.4 NOTE 要求标识符「as small as
    /// possible」以省父树数组空间，稠密的页面 MCID 集合让 `/ParentTree`
    /// 数组没有 `null` 空洞。调用方必须把返回值写进结构树同一页的 MCR 子项
    /// （[`StructureElement::with_marked_content`]），否则构建期交叉校验按
    /// missing/orphan 拒绝。
    ///
    /// 必须以 [`PdfBuilder::end_structure_content`] 结束。页索引越界返回
    /// [`WriteError::InvalidObject`]（本方法必须交付分配值，无法沿用既有
    /// API 对未知页静默忽略的语义）；该页标识符空间耗尽（分配值将超出
    /// 65535 上限）同样返回 [`WriteError::InvalidObject`]。两类失败都不
    /// 写出内容、不占用标识符，零字节契约成立。后续
    /// [`PdfBuilder::begin_structure_content`] 复用本页已自动分配的标识符
    /// 时，`build` 在对象号分配之前返回 [`WriteError::InvalidObject`]。
    pub fn begin_auto_structure_content(
        &mut self,
        page: usize,
        tag: impl Into<String>,
    ) -> Result<i64, WriteError> {
        if page >= self.pages.len() {
            return Err(WriteError::InvalidObject(format!(
                "structure marked content: page {page} is out of range (page count {})",
                self.pages.len()
            )));
        }
        let mcid = self
            .navigation
            .structure_mcids_mut()
            .allocate_auto(page)
            .map_err(|message| {
                WriteError::InvalidObject(format!("structure marked content: {message}"))
            })?;
        let p = &mut self.pages[page];
        p.content.begin_marked_content_properties(
            tag,
            MarkedContentProperties::Dict(Dict::new().insert("MCID", Value::Integer(mcid))),
        );
        Ok(mcid)
    }

    /// 结束当前结构内容段（`EMC`）。未知页索引被忽略。
    pub fn end_structure_content(&mut self, page: usize) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.end_marked_content();
        }
        self
    }

    /// 在指定页应用一组图形状态参数。
    ///
    /// 在页面资源字典中登记 `/ExtGState` 条目（同页相同配置去重为同一
    /// `/GS<n>`），并在当前位置追加 `/GS<n> gs`。`/Font` 条目引用的字体
    /// 会一并登记到该页字体资源，构建阶段解析为字体字典对象引用。
    ///
    /// 参数在 [`PdfBuilder::build`] 阶段校验：非法取值返回
    /// [`WriteError::InvalidObject`]；包含透明度参数且目标版本低于 1.4
    /// 时返回 [`WriteError::UnsupportedVersion`]。
    pub fn set_graphics_state(&mut self, page: usize, state: &GraphicsState) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        if let Some(gs_font) = &state.font
            && !p.fonts.iter().any(|font| same_font(font, &gs_font.font))
        {
            p.fonts.push(gs_font.font.clone());
        }
        let idx = match p.graphics_states.iter().position(|s| s == state) {
            Some(i) => i,
            None => {
                p.graphics_states.push(state.clone());
                p.graphics_states.len() - 1
            }
        };
        p.content.set_ext_g_state(format!("GS{}", idx + 1));
        self
    }
}
