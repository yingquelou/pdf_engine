use super::{
    EmbeddedFile, Function, OptionalContentGroup, Pattern, PdfBuilder, Shading, ShadingPattern,
    TilingPattern,
};

impl PdfBuilder {
    /// 注册一个可选内容组（`/OCG`，PDF 1.5），返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::begin_optional_content`] 使用。只有页面实际
    /// 标记过的组才写出 `/OCG` 对象与目录 `/OCProperties` 配置；组名称与
    /// `/Intent` 在 [`PdfBuilder::build`] 阶段校验，非法值返回
    /// [`WriteError::InvalidObject`]。
    pub fn optional_content_group(&mut self, group: &OptionalContentGroup) -> usize {
        let idx = self.optional_content_groups.len();
        self.optional_content_groups.push(group.clone());
        idx
    }

    /// 注册一个嵌入文件（7.11.3–7.11.4），返回文档级句柄。
    ///
    /// `build` 时每个注册项展开为文件规格字典 + 嵌入文件流一对间接对象，
    /// 并登记进目录 `/Names /EmbeddedFiles` 名称树；句柄供
    /// [`PdfAnnotation::with_filespec`]（FileAttachment 注解 `/FS`）引用。
    /// 名称为空/含路径分隔符/重复或名称树键非法返回
    /// [`WriteError::InvalidObject`]；嵌入文件要求目标版本 ≥1.3，`/Desc`
    /// 要求 ≥1.6，非 ASCII 展示名（`/UF`）要求 ≥1.7（ISO 32000-1 Table 44），
    /// 不满足返回 [`WriteError::UnsupportedVersion`]，均零字节输出。
    pub fn embed_file(&mut self, file: EmbeddedFile) -> usize {
        let handle = self.embedded.len();
        self.embedded.push(file);
        handle
    }

    /// 注册一个 PDF 函数（§7.10 Type 0/2/3/4），返回文档级句柄。
    ///
    /// 句柄供 [`GraphicsState::with_transfer_function`] 等引用函数的配置
    /// 使用。只有被实际引用的函数才写出对象：Type 0/4 写成流对象，
    /// Type 2/3 写成直接字典；Type 3 的子函数在组装阶段递归生成。
    /// 函数参数在 [`PdfBuilder::build`] 阶段校验，非法值返回
    /// [`WriteError::InvalidObject`]。
    pub fn function(&mut self, function: &Function) -> usize {
        let idx = self.functions.len();
        self.functions.push(function.clone());
        idx
    }

    /// 注册一个渐变（§8.7.4.2，Type 1/2/3），返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::paint_shading`] 使用。只有页面实际绘制的渐变
    /// 才写出对象并登记到该页 `/Shading` 资源字典；参数在构建阶段校验，
    /// 非法值返回 [`WriteError::InvalidObject`]。
    pub fn shading(&mut self, shading: &Shading) -> usize {
        let idx = self.shadings.len();
        self.shadings.push(shading.clone());
        idx
    }

    /// 注册一个着色图案（PatternType 2，§8.7.2），返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::fill_rect_with_pattern`] 使用。图案字典内联
    /// 引用其渐变；只有页面实际使用的图案才写出对象并登记到该页
    /// `/Pattern` 资源字典。
    pub fn shading_pattern(&mut self, pattern: &ShadingPattern) -> usize {
        let idx = self.patterns.len();
        self.patterns
            .push(Pattern::Shading(Box::new(pattern.clone())));
        idx
    }

    /// 注册一个平铺图案（PatternType 1，§8.7.3），返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::fill_rect_with_pattern`] 使用。当前仅支持
    /// 彩色图案（`/PaintType 1`）。图案以流对象写出：图块内容独立于
    /// 页面内容，`/Resources` 未提供时写空字典，因此图案内目前无法
    /// 引用构建器管理的字体或图片。只有页面实际使用的图案才写出对象
    /// 并登记到该页 `/Pattern` 资源字典。平铺图案自 PDF 1.2 引入，
    /// 低于该版本的目标版本在构建时返回
    /// [`WriteError::UnsupportedVersion`]。
    pub fn tiling_pattern(&mut self, pattern: &TilingPattern) -> usize {
        let idx = self.patterns.len();
        self.patterns
            .push(Pattern::Tiling(Box::new(pattern.clone())));
        idx
    }
}
