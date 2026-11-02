//! 子集标签：`/BaseFont` 的六字母大写前缀、标签对程序字节/子集判别/字名的
//! 确定性与区分度，以及未嵌入字体程序时保留原始 `/BaseFont`。

use super::super::*;
#[test]
fn subset_font_prefixes_base_font_with_six_letters() {
    let program = FontProgram::TrueType(vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0A]);
    let tag = program.subset_tag_for_name("Helvetica", &[]);
    assert_eq!(tag.len(), 6, "子集标签应为六个字母");
    assert!(
        tag.bytes().all(|byte| byte.is_ascii_uppercase()),
        "子集标签应为大写 ASCII 字母: {tag}"
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_font_program(program).subset();
    b.text(p, &font, 10.0, 10.0, "Test");
    let s = render(&mut b);

    assert!(
        s.contains(&format!("/BaseFont /{tag}+Helvetica")),
        "BaseFont 应带子集标签前缀: {s}"
    );
    assert!(
        !s.contains("/BaseFont /Helvetica"),
        "不应再写出无前缀的 BaseFont: {s}"
    );
}

#[test]
fn subset_tag_is_deterministic_and_content_dependent() {
    let data = b"%!PS-AdobeFont-1.0: TestFont 001.000\n".to_vec();
    let first = FontProgram::Type1(data.clone()).subset_tag_with(&[]);
    let second = FontProgram::Type1(data).subset_tag_with(&[]);
    assert_eq!(first, second, "同一字体程序应得到同一标签");

    let other = FontProgram::Type1(b"%!PS-AdobeFont-1.0: OtherFont".to_vec()).subset_tag_with(&[]);
    assert_ne!(first, other, "不同字体程序应得到不同标签");
    assert_eq!(other.len(), 6);
}

#[test]
fn subset_tag_with_discriminator_separates_different_subsets() {
    let program = FontProgram::TrueType(vec![0x00, 0x01, 0x00, 0x00]);
    let first = program.subset_tag_with(&[2, 0]);
    let same = program.subset_tag_with(&[2, 0]);
    let other = program.subset_tag_with(&[5, 0]);

    assert_eq!(first, same, "同一程序与同一子集应得到同一标签");
    assert_ne!(first, other, "同一程序的不同子集应得到不同标签");
    assert_ne!(
        first,
        program.subset_tag_with(&[]),
        "带子集判别与未带判别应得到不同标签"
    );
    assert_eq!(first.len(), 6);
}

#[test]
fn subset_tag_includes_font_name() {
    let program = FontProgram::TrueType(vec![0x00, 0x01, 0x00, 0x00]);
    let same_name = program.subset_tag_for_name("Helvetica", &[1, 0, 2]);
    assert_eq!(
        same_name,
        program.subset_tag_for_name("Helvetica", &[1, 0, 2]),
        "同名字与同子集应得到同一标签"
    );
    assert_ne!(
        same_name,
        program.subset_tag_for_name("BuilderSans", &[1, 0, 2]),
        "同一程序与子集在不同 BaseFont 名下应得到不同标签"
    );
}

#[test]
fn subset_without_font_program_keeps_base_font() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0).subset(), 10.0, 10.0, "Test");
    let s = render(&mut b);

    assert!(s.contains("/BaseFont /Helvetica"), "应写原始 BaseFont: {s}");
    assert!(
        !s.contains("+Helvetica"),
        "未嵌入字体程序时不应加子集标签: {s}"
    );
}
