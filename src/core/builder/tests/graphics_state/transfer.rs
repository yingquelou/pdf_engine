//! `/TR2` 传递函数扩展字典：字典值逐字写出与非法非字典取值的构建期拒绝。

use super::super::*;
use super::{assert_build_error, ext_g_state};
use crate::core::types::Array;

#[test]
fn transfer_function_2_dictionary_is_written_verbatim() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let tr2 = Value::Dict(
        Dict::new()
            .insert("TransferType", Value::Name("DitherType".into()))
            .insert("ScreenList", Value::Array(Array(vec![Value::Integer(0)]))),
    );
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_transfer_function_2(tr2.clone()),
    );
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    assert_eq!(gs.get("TR2"), Some(&tr2));
}

#[test]
fn invalid_transfer_function_2_fails_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_transfer_function_2(Value::Integer(4)),
    );
    assert_build_error(&mut b, "/TR2 must be a dictionary");
}
