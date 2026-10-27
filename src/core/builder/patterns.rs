//! `PdfBuilder` 的渐变、图案与命名颜色空间方法。

use super::{ColorSpace, PaintType, Pattern, PdfBuilder};

impl PdfBuilder {
    /// 在指定页用已注册的渐变填充当前裁剪区域（`/Sh<n> sh`）。
    ///
    /// 同一渐变在一页内只登记一次资源名，重复调用复用同一 `/Sh<n>`
    /// 与对象号；未知句柄或页索引被忽略。渐变属于 PDF 1.3 特性，低于
    /// 1.3 的目标版本在构建时返回 [`WriteError::UnsupportedVersion`](crate::core::writer::WriteError::UnsupportedVersion)。
    pub fn paint_shading(&mut self, page: usize, shading: usize) -> &mut Self {
        if shading >= self.shadings.len() {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.shadings.iter().position(|&h| h == shading) {
            Some(i) => i,
            None => {
                p.shadings.push(shading);
                p.shadings.len() - 1
            }
        };
        p.content.paint_shading(format!("Sh{}", idx + 1));
        self
    }

    /// 注册一个命名颜色空间，返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::set_stroke_color_space`] 与
    /// [`PdfBuilder::set_fill_color_space`] 使用。仅页面实际引用的颜色
    /// 空间会写入该页 `/Resources /ColorSpace`，并按首次引用顺序命名为
    /// `/CS1`、`/CS2`……；同一句柄跨页复用时，ICCBased 配置文件与
    /// tint 变换函数等流对象只写出一次。
    pub fn color_space(&mut self, color_space: &ColorSpace) -> usize {
        let idx = self.color_spaces.len();
        self.color_spaces.push(color_space.clone());
        idx
    }

    /// 在指定页把描边颜色空间切换为已注册的命名颜色空间（`/CS<n> CS`）。
    ///
    /// 未知句柄或页索引被忽略。
    pub fn set_stroke_color_space(&mut self, page: usize, color_space: usize) -> &mut Self {
        if let Some(name) = self.page_color_space_name(page, color_space) {
            self.pages[page].content.set_stroke_color_space(name);
        }
        self
    }

    /// 在指定页把填充颜色空间切换为已注册的命名颜色空间（`/CS<n> cs`）。
    ///
    /// 未知句柄或页索引被忽略。
    pub fn set_fill_color_space(&mut self, page: usize, color_space: usize) -> &mut Self {
        if let Some(name) = self.page_color_space_name(page, color_space) {
            self.pages[page].content.set_fill_color_space(name);
        }
        self
    }

    /// 在指定页以当前颜色空间的分量设置描边颜色（`c1 … cn SC`）。
    ///
    /// 分量个数由当前颜色空间决定；构建器不校验个数。
    pub fn set_stroke_color_components(&mut self, page: usize, components: &[f64]) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.set_stroke_color_components(components);
        }
        self
    }

    /// 在指定页以当前颜色空间的分量设置填充颜色（`c1 … cn sc`）。
    ///
    /// 分量个数由当前颜色空间决定；构建器不校验个数。
    pub fn set_fill_color_components(&mut self, page: usize, components: &[f64]) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.set_fill_color_components(components);
        }
        self
    }

    /// 返回该页的资源名（`/CS<n>`），首次引用时登记；未知页或句柄返回 `None`。
    fn page_color_space_name(&mut self, page: usize, color_space: usize) -> Option<String> {
        if color_space >= self.color_spaces.len() {
            return None;
        }
        let p = self.pages.get_mut(page)?;
        let idx = match p.color_spaces.iter().position(|&h| h == color_space) {
            Some(i) => i,
            None => {
                p.color_spaces.push(color_space);
                p.color_spaces.len() - 1
            }
        };
        Some(format!("CS{}", idx + 1))
    }

    /// 在指定页用图案填充矩形：`/Pattern cs /P<n> scn x y w h re f`。
    ///
    /// 图案句柄可来自 [`PdfBuilder::shading_pattern`] 或
    /// [`PdfBuilder::tiling_pattern`]。矩形的定位由图案矩阵与填充区域
    /// 共同决定；同一图案在一页内复用同一 `/P<n>` 资源名。未知句柄或页
    /// 索引被忽略。着色图案属于 PDF 1.3 特性，平铺图案属于 PDF 1.2
    /// 特性；目标版本不足时构建返回 [`WriteError::UnsupportedVersion`](crate::core::writer::WriteError::UnsupportedVersion)。
    pub fn fill_rect_with_pattern(
        &mut self,
        page: usize,
        pattern: usize,
        rect: [f64; 4],
    ) -> &mut Self {
        if pattern >= self.patterns.len() {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.patterns.iter().position(|&h| h == pattern) {
            Some(i) => i,
            None => {
                p.patterns.push(pattern);
                p.patterns.len() - 1
            }
        };
        p.content
            .set_fill_color_space("Pattern")
            .set_fill_pattern(format!("P{}", idx + 1))
            .rect(rect[0], rect[1], rect[2], rect[3])
            .fill();
        self
    }

    /// 在指定页用无色平铺图案填充矩形：`/Pattern cs c1 … cn /P<n> scn … re f`。
    ///
    /// 图案句柄必须来自 [`PdfBuilder::tiling_pattern`]，且图案须由
    /// [`TilingPattern::uncolored`](crate::core::builder::TilingPattern::uncolored) 创建。`components` 按图案字典的
    /// `/ColorSpace` 解释，数量必须与颜色空间分量数一致且有限；否则
    /// 该调用被忽略。资源名复用与版本门控同
    /// [`PdfBuilder::fill_rect_with_pattern`]。
    pub fn fill_rect_with_uncolored_pattern(
        &mut self,
        page: usize,
        pattern: usize,
        components: &[f64],
        rect: [f64; 4],
    ) -> &mut Self {
        let expected = match self.patterns.get(pattern) {
            Some(Pattern::Tiling(pattern)) if pattern.paint_type == PaintType::Uncolored => {
                pattern.color_space.as_ref().map(ColorSpace::channels)
            }
            _ => None,
        };
        let Some(expected) = expected else {
            return self;
        };
        if components.len() != expected || components.iter().any(|value| !value.is_finite()) {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.patterns.iter().position(|&h| h == pattern) {
            Some(i) => i,
            None => {
                p.patterns.push(pattern);
                p.patterns.len() - 1
            }
        };
        p.content
            .set_fill_color_space("Pattern")
            .set_fill_pattern_with_components(components.to_vec(), format!("P{}", idx + 1))
            .rect(rect[0], rect[1], rect[2], rect[3])
            .fill();
        self
    }
}
