//! 3D 标记注解的外部数据字典 `/ExData`（13.6.6，Table 170/313，PDF 1.7）。
//!
//! 标记类注解可带 `/ExData`，把一条 3D 注解与一个 3D 视图关联起来：
//! `/3DA` 引用目标 3D 注解、`/3DV` 为视图字典、可选 `/MD5` 为 3D 流数据
//! 的 16 字节摘要。`/3DV` 的视图模型复用 13.6 的 [`ThreeDView`]。
//!
//! 写端只支持经注解句柄引用目标的形式；Table 313 还允许 `/3DA` 用文本串
//! 按 3D 注解的 `/NM` 命名目标，但注解名在写出阶段才生成，写端暂不支持
//! 该形式（已知缺口），读端两种都接受。目标校验（句柄存在、目标是 3D
//! 注解、不得指向自身）在 assemble 阶段、任何字节写出前完成。
//!
//! `/Type` 按 Table 313 写为 `ExData`；13.6.6 的示例写作 `/Type /Markup3D`，
//! 读端两种都接受。

use super::super::three_d::ThreeDView;
use super::PdfAnnotation;

/// `Markup3D` 外部数据（Table 313）的构建态。
///
/// 只支持注解句柄引用形式；Table 313 允许用文本串按 `/NM` 命名目标，
/// 该形式是已知缺口（写端不提供）。
#[derive(Debug, Clone, PartialEq)]
pub struct Markup3D {
    annotation: usize,
    view: ThreeDView,
    md5: Option<[u8; 16]>,
}

impl Markup3D {
    /// 新建载荷；`annotation` 为目标 3D 注解的注册句柄（`/3DA`）。
    pub fn new(annotation: usize, view: ThreeDView) -> Self {
        Self {
            annotation,
            view,
            md5: None,
        }
    }

    /// 3D 流数据的 MD5 摘要 `/MD5`（16 字节；缺省不写）。
    pub fn with_md5(mut self, md5: [u8; 16]) -> Self {
        self.md5 = Some(md5);
        self
    }

    /// 目标 3D 注解句柄（`/3DA`）。
    pub fn annotation(&self) -> usize {
        self.annotation
    }

    /// 3D 视图字典 `/3DV`（Table 304）。
    pub fn view(&self) -> &ThreeDView {
        &self.view
    }

    /// MD5 摘要 `/MD5`。
    pub fn md5(&self) -> Option<&[u8; 16]> {
        self.md5.as_ref()
    }

    /// 校验 `/3DV` 视图（Table 304）；目标注解的归属与范围由 assemble
    /// 阶段跨注解校验（此处只有句柄，没有注解表）。
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.view
            .validate()
            .map_err(|message| format!("markup 3D view: {message}"))
    }
}

impl PdfAnnotation {
    /// 3D 标记注解的外部数据字典 `/ExData`（13.6.6 Table 313，PDF 1.7）。
    ///
    /// 仅标记类注解可用（Table 170）；`markup` 的目标句柄须指向另一条
    /// 已注册的 3D 注解，且不得指向本注解自身，否则构建在写出任何字节前
    /// 失败。
    pub fn with_markup_3d(mut self, markup: Markup3D) -> Self {
        self.ex_data = Some(markup);
        self
    }
}
