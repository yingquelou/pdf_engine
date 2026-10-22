//! `demo` PDF 处理库：解析与生成 PDF 1.x 文件。
//!
//! 顶层只导出 [`pdf`] 模块。详见各子模块文档：
//! - [`pdf::types`]：对象模型
//! - [`pdf::reader`]：解析器
//! - [`pdf::writer`]：序列化器
//!
//! # 使用示例
//!
//! 以下示例与 README 保持一致，由 `cargo test` 做**编译级**验证
//! （`no_run`：不实际执行文件 I/O，仅保证代码可编译、API 签名正确）。
//!
//! 解析已有 PDF 并按需取对象（懒加载）：
//! ```no_run
//! use demo::pdf::reader::Parser;
//!
//! let file = std::fs::File::open("input.pdf")?;
//! let doc = Parser::new(file)?.parse_document()?;
//! println!("declared version: {}", doc.source_version());
//! if let Some((root_num, _)) = doc.trailer().root() {
//!     let catalog = doc.get_object(root_num)?.expect("root present");
//!     println!("root: {}", catalog.value.type_name());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! 生成一个含文字的最小 PDF：
//! ```no_run
//! use demo::pdf::builder::{Font, PageSize, PdfBuilder};
//! use demo::pdf::types::PdfVersion;
//!
//! let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
//! let page = builder.page(PageSize::A4);
//! builder.text(page, &Font::helvetica(24.0), 72.0, 720.0, "Hello, PDF!");
//!
//! let mut out = Vec::new();
//! builder.build(&mut out)?;
//! std::fs::write("output.pdf", out)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! 版本转换：读 1.5（XRef 流 + 对象流）→ 写 1.3（经典 xref 表 + 顶层对象）：
//! ```no_run
//! use demo::pdf::reader::Parser;
//! use demo::pdf::types::PdfVersion;
//! use demo::pdf::writer::DocumentWriter;
//!
//! let file = std::fs::File::open("input-1.5.pdf")?;
//! let doc = Parser::new(file)?.parse_document()?;
//!
//! let mut out = Vec::new();
//! DocumentWriter::new()
//!     .with_version(PdfVersion::V1_3)
//!     .build(&doc, &mut out)?;
//!
//! // 回读：1.3 头、经典 xref 表、所有对象顶层可见
//! std::fs::write("converted.pdf", &out)?;
//! let file2 = std::fs::File::open("converted.pdf")?;
//! let doc2 = Parser::new(file2)?.parse_document()?;
//! assert_eq!(doc2.source_version(), PdfVersion::V1_3);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod pdf;
