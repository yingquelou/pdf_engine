//! 跨对象引用条目的构造方法与读端：附件文件规格 `/FS`（12.5.6.14）、
//! `/Sound` 声音对象（Table 185）、3D 注解的 `/3DD` `/3DV` `/3DA` `/3DI`
//! `/3DB`（Table 298）与标记类注解的 `/ExData`（13.6.6 Table 313）。

use super::{Markup3D, PdfAnnotation, ThreeDActivation, ThreeDViewSelection};

impl PdfAnnotation {
    /// 附件文件规格 `/FS`（12.5.6.14，PDF 1.3）；参数为
    /// [`PdfBuilder::embed_file`](crate::core::builder::PdfBuilder::embed_file)
    /// 注册的嵌入文件句柄。FileAttachment 子类型必须提供。
    pub fn with_filespec(mut self, handle: usize) -> Self {
        self.filespec = Some(handle);
        self
    }

    pub(crate) fn filespec(&self) -> Option<usize> {
        self.filespec
    }

    /// `/Sound` 声音对象句柄（Sound 注解必填，Table 185）。
    pub(crate) fn sound(&self) -> Option<usize> {
        self.sound
    }

    /// `/3DD` 3D artwork 句柄（3D 注解必填，Table 298）。
    pub(crate) fn three_d(&self) -> Option<usize> {
        self.three_d
    }

    /// `/3DV` 初始视图选择。
    pub(crate) fn three_d_view(&self) -> Option<&ThreeDViewSelection> {
        self.three_d_view.as_ref()
    }

    /// `/3DA` 激活字典。
    pub(crate) fn three_d_activation(&self) -> Option<&ThreeDActivation> {
        self.three_d_activation.as_ref()
    }

    /// `/3DI` 交互标志。
    pub(crate) fn three_d_interactive(&self) -> Option<bool> {
        self.three_d_interactive
    }

    /// `/3DB` 视图框（四项有限数）。
    pub(crate) fn three_d_view_box(&self) -> Option<&[f64]> {
        self.three_d_view_box.as_deref()
    }

    /// `/ExData` 3D 标记注解外部数据字典（13.6.6 Table 313）。
    pub fn ex_data(&self) -> Option<&Markup3D> {
        self.ex_data.as_ref()
    }
}
