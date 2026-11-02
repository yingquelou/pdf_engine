//! `/BG` `/BG2` `/UCR` `/UCR2` 黑生成与底色去除函数引用：共享句柄只写一个
//! 函数对象、四键引用一致与 Type 0 样本函数流字典写出，以及多输出函数与
//! 未知句柄的构建期拒绝（零字节输出）。

use super::super::*;
use super::ext_g_state;

fn four_input_function() -> Function {
    Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        vec![0.0, 1.0],
        vec![2, 2, 2, 2],
        8,
        (0..16)
            .map(|value| value as f64 * 17.0)
            .collect::<Vec<f64>>(),
    ))
}

#[test]
fn black_generation_and_undercolor_functions_are_written() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.function(&four_input_function());
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_black_generation(handle)
            .with_black_generation_2(handle)
            .with_undercolor_removal(handle)
            .with_undercolor_removal_2(handle),
    );
    let doc = b.assemble_document().expect("assemble");
    let (_, gs) = ext_g_state(&doc, 0, "GS1");

    let mut function_num = None;
    for key in ["BG", "BG2", "UCR", "UCR2"] {
        let num = gs
            .get(key)
            .and_then(Value::as_ref)
            .map(|(num, _)| num)
            .unwrap_or_else(|| panic!("missing /{key} function reference"));
        assert_eq!(*function_num.get_or_insert(num), num, "/{key} should share");
        let object = doc
            .get_object(num)
            .expect("lookup")
            .expect("function object");
        let dict = object.value.as_dict().expect("function stream dictionary");
        assert_eq!(
            dict.get("FunctionType").and_then(Value::as_integer),
            Some(0)
        );
        assert_eq!(numbers(dict.get("Domain").expect("/Domain")).len(), 8);
        assert_eq!(numbers(dict.get("Range").expect("/Range")), vec![0.0, 1.0]);
    }

    let function_objects = (1..=32)
        .filter(|num| {
            matches!(
                doc.get_object(*num),
                Ok(Some(object)) if object
                    .value
                    .as_dict()
                    .and_then(|dict| dict.get("FunctionType"))
                    .is_some()
            )
        })
        .count();
    assert_eq!(function_objects, 1, "shared handle should be written once");
}

#[test]
fn invalid_black_generation_functions_fail_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.function(&Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(vec![0.0, 0.0, 0.0])
            .with_c1(vec![1.0, 1.0, 1.0]),
    ));
    b.set_graphics_state(p, &GraphicsState::new().with_black_generation(handle));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("three-output /BG should be rejected");
    assert!(
        error
            .to_string()
            .contains("/BG function must produce 1 output"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn unknown_black_generation_handle_fails_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_black_generation(99));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("unknown handle should be rejected");
    assert!(
        error
            .to_string()
            .contains("/BG references unknown function handle 99"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}
