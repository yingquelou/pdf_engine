//! 自然语言与多语言文本读端测试（ISO 32000-1 14.9.2）。
//!
//! 按主题目录化拆分（与 `builder/tests/shading/` 的目录化形态一致）：
//! - `support`：跨主题共享夹具（对象号常量、字典/数组构造器、语言解析、
//!   `marked_section` 装配与 `assert_malformed` 期望值辅助）；
//! - `language_identifier`：RFC 3066 语言标识符的接受、拒绝与大小写无关比较；
//! - `multi_language_text`：多语言文本数组的配对/去重/上限解析、匹配优先级、
//!   `AlternateText` 与 media clip `/Alt` 的解码；
//! - `document_language`：目录 `/Lang`、结构元素 `/Lang` 沿父链继承与生效语言、
//!   环与深度限制、非法语言条目的类型化错误；
//! - `span_properties`：Span 属性表（`/Alt`、`/ActualText`、`/E`、`/Lang`）；
//! - `section_language`：标记内容区段语言解析与页面标记内容语言序列；
//! - `artifact_properties`：Artifact 属性表（Table 330）解析、未知名保留与
//!   类型化错误。
//!
//! 断言、夹具与注释自原单文件逐行搬运，测试叶名零改动。

mod alt_sources;
mod artifact_properties;
mod document_language;
mod language_identifier;
mod multi_language_text;
mod section_language;
mod span_properties;
mod support;
