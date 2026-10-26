//! PDF 函数（§7.10）构建器测试：字典/流对象生成与 ExtGState `/TR` 接线。

use super::*;
use crate::core::builder::{CalculatorFunction, ExponentialFunction, SampledFunction};

/// 取第一页 `/GS<name>` 的图形状态字典与 `/TR` 条目。
fn transfer_value(doc: &Document, name: &str) -> (Dict, Value) {
    let page = doc
        .get_object(3)
        .expect("page lookup")
        .expect("page object");
    let page_dict = page.value.as_dict().expect("page dict");
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page resources");
    let ext = resources
        .get("ExtGState")
        .and_then(Value::as_dict)
        .expect("ExtGState resources");
    let num = ext
        .get(name)
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .unwrap_or_else(|| panic!("missing /{name} graphics state"));
    let object = doc
        .get_object(num)
        .expect("graphics state lookup")
        .expect("graphics state object");
    let dict = object.value.as_dict().expect("graphics state dict").clone();
    let transfer = dict.get("TR").expect("/TR entry").clone();
    (dict, transfer)
}

fn as_number(value: &Value) -> f64 {
    match value {
        Value::Integer(number) => *number as f64,
        Value::Float(number) => *number,
        other => panic!("expected a number, got {other:?}"),
    }
}

fn find_function_objects(doc: &Document) -> Vec<(i64, Dict, Option<Vec<u8>>)> {
    let mut found = Vec::new();
    for num in 1..=32 {
        let Ok(Some(object)) = doc.get_object(num) else {
            continue;
        };
        match &object.value {
            Value::Dict(dict) if dict.get("FunctionType").is_some() => {
                found.push((num, dict.clone(), None));
            }
            Value::Stream(dict, StreamData::Owned(bytes)) if dict.get("FunctionType").is_some() => {
                found.push((num, dict.clone(), Some(bytes.clone())));
            }
            _ => {}
        }
    }
    found
}

/// 用单个 `/TR` 构建并返回 `assemble_document` 结果。
fn assemble_with_transfer(function: Function) -> Document {
    let mut b = PdfBuilder::new();
    let handle = b.function(&function);
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_transfer_function(handle));
    b.assemble_document().expect("assemble")
}

#[test]
fn exponential_transfer_function_is_written_as_direct_dictionary() {
    let function = Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 2.0)
            .with_c0(vec![0.0])
            .with_c1(vec![1.0])
            .with_range(vec![0.0, 1.0]),
    );
    let doc = assemble_with_transfer(function);

    let (_, transfer) = transfer_value(&doc, "GS1");
    let dict = transfer
        .as_dict()
        .expect("Type 2 /TR should be a direct dictionary");
    assert_eq!(
        dict.get("FunctionType").and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        numbers(dict.get("Domain").expect("/Domain")),
        vec![0.0, 1.0]
    );
    assert_eq!(as_number(dict.get("N").expect("/N")), 2.0);
    assert_eq!(numbers(dict.get("C0").expect("/C0")), vec![0.0]);
    assert_eq!(numbers(dict.get("C1").expect("/C1")), vec![1.0]);
    assert_eq!(numbers(dict.get("Range").expect("/Range")), vec![0.0, 1.0]);

    // 纯字典函数不占用独立对象号。
    assert!(
        doc.get_object(6).expect("lookup").is_none(),
        "Type 2 function should not allocate an object"
    );
}

#[test]
fn sampled_transfer_function_packs_bits_msb_first() {
    // 4 个样本、每样本 4 位：0xA, 0x2, 0xF 应打包为 0x0A 0x2F。
    let function = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0],
        vec![0.0, 1.0],
        vec![4],
        4,
        vec![0.0, 10.0, 2.0, 15.0],
    ));
    let doc = assemble_with_transfer(function);

    let (_, transfer) = transfer_value(&doc, "GS1");
    let (num, _) = transfer.as_ref().expect("Type 0 /TR should be a reference");
    let object = doc
        .get_object(num)
        .expect("function lookup")
        .expect("function stream object");
    let (dict, bytes) = match object.value {
        Value::Stream(dict, StreamData::Owned(bytes)) => (dict, bytes),
        other => panic!("Type 0 function should be an owned stream, got {other:?}"),
    };
    assert_eq!(bytes, vec![0x0A, 0x2F]);
    assert_eq!(
        dict.get("FunctionType").and_then(Value::as_integer),
        Some(0)
    );
    assert_eq!(
        dict.get("Size")
            .and_then(Value::as_array)
            .map(|a| a.0.len()),
        Some(1)
    );
    assert_eq!(
        dict.get("BitsPerSample").and_then(Value::as_integer),
        Some(4)
    );
    assert_eq!(
        dict.get("Length").and_then(Value::as_integer),
        Some(2),
        "/Length should match the packed sample bytes"
    );
}

#[test]
fn sampled_function_rejects_non_byte_aligned_packing_errors() {
    // 12 位样本跨字节边界：0x123, 0x456 → 0x12 0x34 0x56。
    let function = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0],
        vec![0.0, 1.0],
        vec![2],
        12,
        vec![0x123 as f64, 0x456 as f64],
    ));
    let mut b = PdfBuilder::new();
    let handle = b.function(&function);
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_transfer_function(handle));
    let doc = b.assemble_document().expect("assemble");

    let (_, transfer) = transfer_value(&doc, "GS1");
    let (num, _) = transfer.as_ref().expect("stream reference");
    let object = doc.get_object(num).unwrap().unwrap();
    match object.value {
        Value::Stream(_, StreamData::Owned(bytes)) => {
            assert_eq!(bytes, vec![0x12, 0x34, 0x56]);
        }
        other => panic!("expected owned stream, got {other:?}"),
    }
}

#[test]
fn calculator_transfer_function_streams_code() {
    let code = b"2 mul 1 add";
    let function = Function::Calculator(CalculatorFunction::new(
        vec![0.0, 1.0],
        vec![0.0, 3.0],
        code,
    ));
    let doc = assemble_with_transfer(function);

    let (_, transfer) = transfer_value(&doc, "GS1");
    let (num, _) = transfer.as_ref().expect("Type 4 /TR should be a reference");
    let object = doc.get_object(num).unwrap().unwrap();
    let (dict, bytes) = match object.value {
        Value::Stream(dict, StreamData::Owned(bytes)) => (dict, bytes),
        other => panic!("expected owned stream, got {other:?}"),
    };
    assert_eq!(bytes, code.to_vec());
    assert_eq!(
        dict.get("FunctionType").and_then(Value::as_integer),
        Some(4)
    );
    assert_eq!(numbers(dict.get("Range").expect("/Range")), vec![0.0, 3.0]);
    assert_eq!(
        dict.get("Length").and_then(Value::as_integer),
        Some(code.len() as i64)
    );
}

#[test]
fn stitching_transfer_recursively_writes_nested_functions() {
    let stitching = Function::Stitching(
        StitchingFunction::new(
            vec![0.0, 1.0],
            vec![
                Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
                Function::Sampled(SampledFunction::new(
                    vec![0.0, 1.0],
                    vec![0.0, 1.0],
                    vec![2],
                    8,
                    vec![0.0, 255.0],
                )),
            ],
            vec![0.5],
        )
        .with_encode(vec![0.0, 1.0, 0.0, 1.0]),
    );
    let doc = assemble_with_transfer(stitching);

    let (_, transfer) = transfer_value(&doc, "GS1");
    let dict = transfer
        .as_dict()
        .expect("Type 3 /TR should be a direct dictionary");
    assert_eq!(
        dict.get("FunctionType").and_then(Value::as_integer),
        Some(3)
    );
    assert_eq!(numbers(dict.get("Bounds").expect("/Bounds")), vec![0.5]);
    assert_eq!(
        numbers(dict.get("Encode").expect("/Encode")),
        vec![0.0, 1.0, 0.0, 1.0]
    );
    let functions = dict
        .get("Functions")
        .and_then(Value::as_array)
        .expect("/Functions array");
    assert_eq!(functions.0.len(), 2);
    let first = functions.0[0]
        .as_dict()
        .expect("exponential subfunction stays inline");
    assert_eq!(
        first.get("FunctionType").and_then(Value::as_integer),
        Some(2)
    );
    let (sampled_num, _) = functions.0[1]
        .as_ref()
        .expect("sampled subfunction should be an indirect stream");
    let sampled = doc.get_object(sampled_num).unwrap().unwrap();
    match sampled.value {
        Value::Stream(nested_dict, StreamData::Owned(bytes)) => {
            assert_eq!(
                nested_dict.get("FunctionType").and_then(Value::as_integer),
                Some(0)
            );
            assert_eq!(bytes, vec![0x00, 0xFF]);
        }
        other => panic!("expected sampled stream, got {other:?}"),
    }
}

#[test]
fn repeated_transfer_function_handle_reuses_one_object() {
    let function = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0],
        vec![0.0, 1.0],
        vec![2],
        8,
        vec![0.0, 255.0],
    ));
    let mut b = PdfBuilder::new();
    let handle = b.function(&function);
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_line_width(1.0)
            .with_transfer_function(handle),
    );
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_line_width(2.0)
            .with_transfer_function(handle),
    );
    let doc = b.assemble_document().expect("assemble");

    let (_, first) = transfer_value(&doc, "GS1");
    let (_, second) = transfer_value(&doc, "GS2");
    assert_eq!(first.as_ref(), second.as_ref());
    assert_eq!(
        find_function_objects(&doc).len(),
        1,
        "same handle should write one function stream"
    );
}

#[test]
fn registered_but_unused_function_is_not_written() {
    let function = Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0));
    let mut b = PdfBuilder::new();
    b.function(&function);
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 700.0, "x");
    let doc = b.assemble_document().expect("assemble");

    assert!(
        find_function_objects(&doc).is_empty(),
        "unreferenced functions should not be written"
    );
}

#[test]
fn unknown_transfer_function_handle_fails_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_transfer_function(0));

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("unknown handle must be rejected");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("unknown function handle 0"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn invalid_function_parameters_fail_the_build() {
    fn assert_rejected(function: Function, expected: &str) {
        let mut b = PdfBuilder::new();
        let handle = b.function(&function);
        let p = b.page(PageSize::A4);
        b.set_graphics_state(p, &GraphicsState::new().with_transfer_function(handle));

        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("invalid function must be rejected");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    assert_rejected(
        Function::Sampled(SampledFunction::new(
            vec![0.0, 1.0],
            vec![0.0, 1.0],
            vec![1],
            8,
            vec![256.0],
        )),
        "does not fit in 8 bit(s)",
    );
    assert_rejected(
        Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0, 0.0, 1.0], 1.0)),
        "requires 1 input dimension",
    );
    assert_rejected(
        Function::Stitching(StitchingFunction::new(
            vec![0.0, 1.0],
            vec![
                Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
                Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
            ],
            Vec::new(),
        )),
        "/Bounds must have 1 value(s)",
    );
    assert_rejected(
        Function::Calculator(CalculatorFunction::new(
            vec![0.0, 1.0],
            vec![0.0, 1.0],
            b"{ 1 add",
        )),
        "invalid /Code stream",
    );
}
