//! `/HT` 半色调字典：Type 1/Type 2 色料条目（`XFrequency`/`Angle`/
//! `Frequency`/`Width`/`Uncompensated`）写出、共享函数句柄的内联解析与两处
//! 取值一致，以及多输出函数、非法频率/默认类型/色料名与未知句柄的构建期
//! 拒绝。

use super::super::*;
use super::{as_bool, as_number, assert_build_error, ext_g_state, one_output_function};

#[test]
fn halftone_dictionary_is_written_with_resolved_functions() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let spot = b.function(&one_output_function());
    let halftone = HalftoneDictionary::new(1)
        .with_colorant(
            "C",
            Halftone::Type1(Type1Halftone {
                x_frequency: 65.0,
                angle: 15.0,
                spot_function: spot,
                transfer_function: None,
            }),
        )
        .with_colorant(
            "M",
            Halftone::Type2(Type2Halftone {
                frequency: 133.3,
                angle: 75.0,
                width: 0.5,
                uncompensated: true,
                spot_function: spot,
                transfer_function: None,
            }),
        );
    b.set_graphics_state(p, &GraphicsState::new().with_halftone(halftone));
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let ht = gs
        .get("HT")
        .expect("/HT")
        .as_dict()
        .expect("/HT dictionary");
    assert_eq!(
        ht.get("DefaultHalftoneType").and_then(Value::as_integer),
        Some(1)
    );

    let cyan = ht
        .get("C")
        .expect("/C")
        .as_dict()
        .expect("type 1 dictionary");
    assert_eq!(
        cyan.get("HalftoneType").and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        as_number(cyan.get("XFrequency").expect("/XFrequency")),
        65.0
    );
    assert_eq!(as_number(cyan.get("Angle").expect("/Angle")), 15.0);
    assert!(!cyan.0.contains_key("Uncompensated"));

    let magenta = ht
        .get("M")
        .expect("/M")
        .as_dict()
        .expect("type 2 dictionary");
    assert_eq!(
        magenta.get("HalftoneType").and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        as_number(magenta.get("Frequency").expect("/Frequency")),
        133.3
    );
    assert_eq!(as_number(magenta.get("Width").expect("/Width")), 0.5);
    assert_eq!(
        as_bool(magenta.get("Uncompensated").expect("/Uncompensated")),
        Some(true)
    );

    // 两个色料共享同一函数句柄：Type 2 函数内联写出，两处值一致。
    let cyan_function = cyan.get("SpotFunction").expect("/SpotFunction");
    let magenta_function = magenta.get("SpotFunction").expect("/SpotFunction");
    assert_eq!(magenta_function, cyan_function);
    let inline = cyan_function.as_dict().expect("inline function dictionary");
    assert_eq!(
        inline.get("FunctionType").and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        numbers(inline.get("Domain").expect("/Domain")),
        vec![0.0, 1.0]
    );
}

#[test]
fn halftone_functions_require_single_output() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.function(&Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(vec![0.0, 0.0, 0.0])
            .with_c1(vec![1.0, 1.0, 1.0]),
    ));
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_halftone(HalftoneDictionary::new(1).with_colorant(
            "C",
            Halftone::Type1(Type1Halftone {
                x_frequency: 65.0,
                angle: 15.0,
                spot_function: handle,
                transfer_function: None,
            }),
        )),
    );
    assert_build_error(&mut b, "/HT function must produce 1 output");
}

#[test]
fn invalid_halftone_parameters_fail_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let spot = b.function(&one_output_function());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_halftone(HalftoneDictionary::new(1).with_colorant(
            "C",
            Halftone::Type1(Type1Halftone {
                x_frequency: 0.0,
                angle: 15.0,
                spot_function: spot,
                transfer_function: None,
            }),
        )),
    );
    assert_build_error(
        &mut b,
        "colorant C: type 1 /XFrequency must be a positive finite number",
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_halftone(HalftoneDictionary::new(3)),
    );
    assert_build_error(
        &mut b,
        "/DefaultHalftoneType 3 is not modeled (only 1 and 2)",
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let spot = b.function(&one_output_function());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_halftone(HalftoneDictionary::new(1).with_colorant(
            "Bad Name",
            Halftone::Type1(Type1Halftone {
                x_frequency: 65.0,
                angle: 15.0,
                spot_function: spot,
                transfer_function: None,
            }),
        )),
    );
    assert_build_error(
        &mut b,
        "halftone colorant name \"Bad Name\" is not a valid PDF name",
    );
}

#[test]
fn unknown_halftone_handle_fails_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_halftone(HalftoneDictionary::new(1).with_colorant(
            "C",
            Halftone::Type1(Type1Halftone {
                x_frequency: 65.0,
                angle: 15.0,
                spot_function: 99,
                transfer_function: None,
            }),
        )),
    );
    assert_build_error(&mut b, "/HT references unknown function handle 99");
}
