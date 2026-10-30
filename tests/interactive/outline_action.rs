//! E3 大纲条目 `/A` 动作的写出 → 解析 → 回读往返（12.3.3 / 12.6）。

use super::*;

/// 写入 → 重新解析：builder 注册的 URI 动作经大纲 `/A` 间接引用写出，
/// 读端 `Document::outline_action` 复用动作模型还原；无 `/A` 的条目返回
/// `None`。
#[test]
fn builder_outline_actions_parse_through_reader() {
    use pdf_engine::core::builder::{Action, OutlineEntry, PageSize, PdfBuilder};
    use pdf_engine::core::reader::action::Action as ReadAction;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    let handle = builder.action(Action::Uri("https://example.com/outline".into()));
    builder.outline(&[
        OutlineEntry {
            title: "Chapter".into(),
            destination: None,
            action: Some(handle),
            color: None,
            flags: 0,
            expanded: false,
            children: Vec::new(),
        },
        OutlineEntry {
            title: "Plain".into(),
            destination: None,
            action: None,
            color: None,
            flags: 0,
            expanded: false,
            children: Vec::new(),
        },
    ]);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build outline actions");

    let doc = parse_buf(&buf);
    let outline = doc.outline().expect("outline").expect("/Outlines");
    assert_eq!(outline.items.len(), 2);
    assert_eq!(
        doc.outline_action(&outline.items[0])
            .expect("parse outline /A"),
        Some(ReadAction::Uri("https://example.com/outline".into()))
    );
    assert_eq!(doc.outline_action(&outline.items[1]).expect("no /A"), None);
}
