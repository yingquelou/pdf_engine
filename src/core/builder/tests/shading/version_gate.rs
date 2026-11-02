//! 渐变写出的版本门禁：PDF 1.3 之前拒绝构建且不写出任何字节。

use super::super::*;
use super::support::axial_shading;
use crate::core::types::PdfVersion;

#[test]
fn shading_requires_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p = b.page(PageSize::A4);
    let h = b.shading(&axial_shading());
    b.paint_shading(p, h);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.2 必须拒绝渐变");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty(), "版本不足时不应写出任何字节");
}
