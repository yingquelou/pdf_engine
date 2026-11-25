use super::{
    AcroForm, DocumentInfo, FileIdentifier, LanguageIdentifier, OutputIntent, PdfBuilder,
    PieceInfo, SeparationGroup, SeparationInfo, XmpPacket,
};
use crate::core::types::PdfVersion;
use crate::core::writer::StreamCompression;

impl PdfBuilder {
    /// 启用写端流压缩（D4）：`build` 时所有未声明 `/Filter` 的明文流
    /// （页面内容流、图像、字体程序与 writer 生成的结构流）按
    /// [`StreamCompression`] 策略以 `FlateDecode` 压缩写出；已声明过滤器的
    /// 流（如 `DCTDecode`/`JPXDecode` 透传）保持原样。不调用本方法则输出
    /// 与既往一致为明文。`FlateDecode` 要求目标版本 ≥1.2，图像 PNG 预测器
    /// 要求 ≥1.3，不满足时 [`build`](Self::build) 报
    /// [`WriteError::UnsupportedVersion`] 且不写出任何字节；后端失败报
    /// [`WriteError::Compression`]，同样零字节、不静默回退明文。
    pub fn compress_streams(&mut self, compression: StreamCompression) -> &mut Self {
        self.compression = Some(compression);
        self
    }

    /// 设置 trailer `/Info` 文档信息字典（14.3.3）；`build` 时分配独立
    /// 对象写出，trailer 以 `/Info` 间接引用挂接。空字典（全部字段未
    /// 设置）不写出，trailer 与既往一致。
    pub fn set_document_info(&mut self, info: DocumentInfo) -> &mut Self {
        self.document_info = info;
        self
    }

    /// 设置文档默认自然语言（14.9.2.1：目录 `/Lang`，PDF 1.4）。
    ///
    /// 接收已按 RFC 3066 解析的 [`LanguageIdentifier`]；空标识符（未知
    /// 语言）合法并写出空文本串。目标版本低于 1.4 时 [`build`](Self::build)
    /// 返回 [`WriteError::UnsupportedVersion`] 且零字节输出。
    pub fn set_document_language(&mut self, language: LanguageIdentifier) -> &mut Self {
        self.document_language = Some(language);
        self
    }

    /// 设置目录 `/Version`（7.7.2 Table 28，PDF 1.4），覆盖文件头声明
    /// 的版本（增量更新升版时由调用方显式给出）。目标版本低于 1.4 时
    /// 构建返回 `UnsupportedVersion` 且不写出任何字节。
    pub fn set_catalog_version(&mut self, version: PdfVersion) -> &mut Self {
        self.catalog_version = Some(version);
        self
    }

    /// 设置目录页面件字典（`/PieceInfo`，14.5 Table 28/318，PDF 1.4）。
    ///
    /// 产品名须非空且唯一，违反者在校验阶段返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.4 返回
    /// [`WriteError::UnsupportedVersion`]。
    pub fn set_catalog_piece_info(&mut self, piece_info: PieceInfo) -> &mut Self {
        self.catalog_piece_info = Some(piece_info);
        self
    }

    /// 设置 trailer `/ID` 文件标识符（14.4）。不调用则不写 `/ID`。
    /// 增量更新场景（E7）经 [`FileIdentifier::generate`] 生成后复用
    /// 永久 ID、更新 `update` 字段。
    pub fn set_file_identifier(&mut self, id: FileIdentifier) -> &mut Self {
        self.file_identifier = Some(id);
        self
    }

    /// 设置 XMP 元数据包（14.3.2）：`build` 时写成独立
    /// `/Type /Metadata /Subtype /XML` 流对象并在目录挂 `/Metadata`
    /// 引用。调用方负责 XML良构性；本层原样透传字节。要求目标版本
    /// ≥1.4，否则 [`build`](Self::build) 返回
    /// [`WriteError::UnsupportedVersion`] 且零字节输出。
    ///
    /// 与类型化入口 [`set_xmp_packet`](Self::set_xmp_packet) 互斥：
    /// 两者同时设置时 `build` 返回 [`WriteError::InvalidObject`]，
    /// 不静默择一。
    pub fn set_xmp_metadata(&mut self, xml: impl Into<Vec<u8>>) -> &mut Self {
        self.xmp_metadata = Some(xml.into());
        self
    }

    /// 设置类型化 XMP 包（14.3.2）：`build` 时由
    /// [`XmpPacket::to_xml`] 序列化为 RDF/XML，写成独立
    /// `/Type /Metadata /Subtype /XML` 流对象并在目录挂 `/Metadata`。
    ///
    /// 序列化失败（非法 Clark 键、XML 1.0 非法字符、非法
    /// `rdf:datatype` URI 等）返回 [`WriteError::InvalidObject`] 且零
    /// 字节输出。要求目标版本 ≥1.4，否则返回
    /// [`WriteError::UnsupportedVersion`] 且零字节输出。
    ///
    /// 与原文透传入口 [`set_xmp_metadata`](Self::set_xmp_metadata)
    /// 互斥：两者同时设置时 `build` 返回
    /// [`WriteError::InvalidObject`]，不静默择一。
    pub fn set_xmp_packet(&mut self, packet: XmpPacket) -> &mut Self {
        self.xmp_packet = Some(packet);
        self
    }

    /// 追加一个输出意图（14.11.5 Table 365），按注册顺序写入目录
    /// `/OutputIntents` 数组（PDF 1.4）。
    ///
    /// `/DestOutputProfile` 写成独立 ICC 配置文件流，`/N` 须为 1–4；
    /// `/OutputConditionIdentifier` 须非空；标识为 `Custom` 时须同时声明
    /// `/Info` 与 `/DestOutputProfile`（Table 365 的条件性要求）。违反者
    /// [`build`](Self::build) 返回 [`WriteError::InvalidObject`]；目标版本
    /// 低于 1.4 返回 [`WriteError::UnsupportedVersion`]，均零字节输出。
    pub fn add_output_intent(&mut self, intent: OutputIntent) -> &mut Self {
        self.output_intents.push(intent);
        self
    }

    /// 为一组分色页设置共享的分色字典（14.11.4 Table 364）。
    ///
    /// `pages` 为 0 基页号；同组各页写出完全相同的 `/SeparationInfo`
    /// （相同 `/Pages` 数组），`/DeviceColorant` 为名字或文本串色料，
    /// 可选 `/ColorSpace` 只接受 Separation 与 DeviceN 颜色空间。
    ///
    /// 本方法只收集，不做校验；[`build`](Self::build) 在写出任何字节前
    /// 要求：组非空、页号在范围内、组内页号不重复、组间不重叠、
    /// `/DeviceColorant` 为合法 PDF 名字或非空文本串、`/ColorSpace`
    /// 为合法 Separation/DeviceN。违反者返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.3（PDF 1.3 引入）
    /// 返回 [`WriteError::UnsupportedVersion`]，两者均零字节输出。
    pub fn add_separation_group(&mut self, pages: &[usize], info: SeparationInfo) -> &mut Self {
        self.separation_groups.push(SeparationGroup {
            pages: pages.to_vec(),
            info,
        });
        self
    }

    /// 设置交互表单（AcroForm，12.7.2–12.7.4）。
    ///
    /// 字段树在 [`PdfBuilder::build`] 时写成独立间接对象：目录
    /// `/AcroForm /Fields` 引用顶层字段，页面 `/Annots` 引用各 Widget，
    /// 字段间 `/Kids`/`/Parent` 互引。Widget 外观流 `/AP` 复用
    /// [`PdfBuilder::form_xobject`] 池。页号越界、字段树结构非法或
    /// `/AP` 句柄未知返回 [`WriteError::InvalidObject`]；目标版本低于
    /// 1.2 返回 [`WriteError::UnsupportedVersion`]，均不写出任何字节。
    pub fn set_acro_form(&mut self, form: AcroForm) -> &mut Self {
        self.acro_form = Some(form);
        self
    }
}
