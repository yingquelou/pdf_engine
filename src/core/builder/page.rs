use super::{
    Font, FormXObject, GroupXObject, ImageData, PageBoundaries, PageBuild, PageSize, PdfBuilder,
    Transition, Viewport, same_font,
};
use crate::core::content::ContentStream;

impl PdfBuilder {
    /// 新增一页，返回该页的 0-based 索引，供后续 `text`/`image` 使用。
    pub fn page(&mut self, size: PageSize) -> usize {
        let idx = self.pages.len();
        self.pages.push(PageBuild {
            size,
            content: ContentStream::new(),
            fonts: Vec::new(),
            cid_fonts: Vec::new(),
            images: Vec::new(),
            graphics_states: Vec::new(),
            forms: Vec::new(),
            optional_content: Vec::new(),
            shadings: Vec::new(),
            patterns: Vec::new(),
            color_spaces: Vec::new(),
            group: None,
            viewports: Vec::new(),
            transition: None,
            duration: None,
            thumbnail: None,
            boundaries: None,
            invalid_code_length: None,
        });
        idx
    }

    /// 在指定页上写一段文字。
    ///
    /// 文本先按字体的 [`FontEncoding`] 转成单字节字符码：可打印 ASCII
    /// 仍输出字面量 `BT /F<n> <size> Tf x y Td (text) Tj ET`（`(` `)` `\`
    /// 由 [`ContentStream`] 转义），其他字节输出十六进制字符串
    /// `BT ... <hex> Tj ET`。无法映射的字符写成 `?`；编码名无法识别时
    /// 此调用退回原始 UTF-8 字节，但该字体在 [`PdfBuilder::build`] 阶段会
    /// 因未知 `/Encoding` 名报 [`WriteError::InvalidObject`]。
    /// 坐标原点在页面左下角，单位 pt。
    pub fn text(&mut self, page: usize, font: &Font, x: f64, y: f64, s: &str) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        // 字体去重：同 BaseFont 名 + 同字号 + 同编码 视为同一字体对象。
        let fidx = match p.fonts.iter().position(|f| same_font(f, font)) {
            Some(i) => i,
            None => {
                p.fonts.push(font.clone());
                p.fonts.len() - 1
            }
        };

        // 委托内容流引擎：资源名 /F<fidx+1>（1-based）由 builder 分配，
        // 操作符组装（BT/ET/Tf/Td/Tj）和字面量转义由 ContentStream 负责。
        let font_name = format!("F{}", fidx + 1);
        match font.encoding.encode_text(&font.name, s) {
            Some(bytes) if bytes.iter().all(|&b| (0x20..=0x7E).contains(&b)) => {
                // 纯可打印 ASCII 保持字面量输出，便于阅读。
                let literal: String = bytes.iter().map(|&b| char::from(b)).collect();
                p.content.show_text_at(font_name, font.size, x, y, literal);
            }
            Some(bytes) => {
                // 单字节码用十六进制字符串写出，避免 UTF-8 误写。
                p.content
                    .begin_text()
                    .set_font(font_name, font.size)
                    .move_next_line(x, y)
                    .show_text_hex(bytes)
                    .end_text();
            }
            None => {
                // 编码名无法识别：维持原始 UTF-8 直写。
                p.content.show_text_at(font_name, font.size, x, y, s);
            }
        }
        self
    }

    /// 在指定页上放置一张图片。
    ///
    /// 生成内容操作符：`q w 0 0 h x y cm /Im<n> Do Q`，cm 矩阵为
    /// `[w 0 0 h x y]`，即把单位图片方块缩放到 `(w,h)` 并平移到
    /// `(x,y)`（左下角）。图片像素字节作为 XObject Image 流的原始数据。
    pub fn image(
        &mut self,
        page: usize,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        data: &ImageData,
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let iidx = p.images.len();
        p.images.push(data.clone());

        // 委托内容流引擎：cm 矩阵 [w 0 0 h x y] 把单位方块缩放到 (w,h)
        // 并平移到 (x,y)（左下角）。资源名 /Im<iidx+1> 由 builder 分配。
        p.content
            .save_state()
            .concat_matrix([w, 0.0, 0.0, h, x, y])
            .draw_xobject(format!("Im{}", iidx + 1))
            .restore_state();
        self
    }

    /// 注册一个可复用的 Form XObject，返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::draw_form`] 使用。表单内容流不附加
    /// `/Resources`，按规范使用绘制页的资源字典；内容只含路径等
    /// 无资源操作符时完全自包含。
    pub fn form_xobject(&mut self, form: &FormXObject) -> usize {
        let idx = self.forms.len();
        self.forms.push(form.clone());
        idx
    }

    /// 在指定页绘制已注册的 Form XObject。
    ///
    /// `matrix` 为绘制时的 CTM `[a b c d e f]`，生成
    /// `q <matrix> cm /Fm<n> Do Q`。同一表单在一页内只登记一次资源名，
    /// 多次绘制复用同一 `/Fm<n>` 与对象号；未知句柄或页索引被忽略。
    pub fn draw_form(&mut self, page: usize, form: usize, matrix: [f64; 6]) -> &mut Self {
        if form >= self.forms.len() {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.forms.iter().position(|&f| f == form) {
            Some(i) => i,
            None => {
                p.forms.push(form);
                p.forms.len() - 1
            }
        };
        p.content
            .save_state()
            .concat_matrix(matrix)
            .draw_xobject(format!("Fm{}", idx + 1))
            .restore_state();
        self
    }

    /// 为指定页设置页组属性字典（页字典 `/Group`，PDF 1.4）。
    ///
    /// 页组把整页内容作为一个透明组合成，颜色空间、隔离（`/I`）与
    /// 挖空（`/K`）语义同 [`GroupXObject`]。参数在 [`PdfBuilder::build`]
    /// 阶段校验：非法取值返回 [`WriteError::InvalidObject`]；目标版本低于
    /// 1.4 时返回 [`WriteError::UnsupportedVersion`] 且不写出任何字节。
    /// 未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_group(&mut self, page: usize, group: &GroupXObject) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.group = Some(group.clone());
        }
        self
    }

    /// 为指定页设置视口字典数组（页面字典 `/VP`，12.9.2 Table 260，PDF 1.6）。
    ///
    /// 数组按绘制顺序排列，各视口的 `/BBox` 与 `/Measure` 决定页面不同区域
    /// 的测量比例。参数在 [`PdfBuilder::build`] 阶段校验：非有限或未按
    /// 左下/右上规范化的 `/BBox`、空 `/Name`、非法测量字典返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.6 时返回
    /// [`WriteError::UnsupportedVersion`] 且不写出任何字节。
    /// 未知页索引被忽略；重复设置替换先前数组。
    pub fn set_page_viewports(&mut self, page: usize, viewports: &[Viewport]) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.viewports = viewports.to_vec();
        }
        self
    }

    /// 为指定页设置页面过渡效果（页面 `/Trans`，12.4.4 Table 30/162，PDF 1.1）。
    ///
    /// 过渡字典与 Transition 动作共用 [`Transition`] 模型；取值与样式适用性
    /// 在 [`PdfBuilder::build`] 阶段校验，非法值返回
    /// [`WriteError::InvalidObject`]。目标版本低于 1.1，或使用 PDF 1.5 引入的
    /// 样式（Fly/Push/Cover/Uncover/Fade）与 `/SS` `/B` 条目，返回
    /// [`WriteError::UnsupportedVersion`] 且不写出任何字节。
    /// 未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_transition(&mut self, page: usize, transition: &Transition) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.transition = Some(transition.clone());
        }
        self
    }

    /// 为指定页设置演示时的停留时长（页面 `/Dur`，12.4.4 Table 30，单位秒）。
    ///
    /// 缺省（未设置）表示查看器不自动换页。取值须为非负有限数，在
    /// [`PdfBuilder::build`] 阶段校验，非法值返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.1 时返回
    /// [`WriteError::UnsupportedVersion`] 且不写出任何字节。
    /// 未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_duration(&mut self, page: usize, seconds: f64) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.duration = Some(seconds);
        }
        self
    }

    /// 为指定页设置缩略图（页面 `/Thumb`，12.3.4 Table 30，PDF 1.0）。
    ///
    /// 缩略图是页面字典直接引用的图像 XObject，不进入页面内容资源；
    /// 与页面绘制的图像共用去重池。按 12.3.4 仅接受 DeviceGray、
    /// DeviceRGB 或基于二者的 Indexed 颜色空间，且不得是图像蒙版，非法
    /// 输入在 [`PdfBuilder::build`] 阶段返回 [`WriteError::InvalidObject`]
    /// 且不写出任何字节。未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_thumbnail(&mut self, page: usize, thumbnail: &ImageData) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.thumbnail = Some(thumbnail.clone());
        }
        self
    }

    /// 为指定页设置页面边界（14.11.2 Table 30）。
    ///
    /// [`PageBoundaries`] 取代该页的 `/MediaBox`（`page(PageSize)` 声明的
    /// 尺寸不再使用），并写出显式声明的 `/CropBox` `/BleedBox` `/TrimBox`
    /// `/ArtBox` 与 `/BoxColorInfo`；缺省框不物化，由读者按 Table 30/361
    /// 推导。
    ///
    /// 取值契约（本库选择：零面积框无实际意义，一并拒绝）与版本门禁在
    /// [`PdfBuilder::build`] 阶段校验：坐标须有限且宽高为正，`/C` 分量
    /// 0.0–1.0，`/W` ≥ 0，`/D` 非空非负且不全为零并只与 `/S /D` 同用，
    /// 扩展 `/S` 名须为合法 PDF 名称；违反者返回
    /// [`WriteError::InvalidObject`]。`/BleedBox` `/TrimBox` `/ArtBox`
    /// 需 PDF 1.3，`/BoxColorInfo` 需 PDF 1.4，否则返回
    /// [`WriteError::UnsupportedVersion`]。两种失败均不写出任何字节。
    /// 未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_boundaries(&mut self, page: usize, boundaries: PageBoundaries) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.boundaries = Some(boundaries);
        }
        self
    }
}
