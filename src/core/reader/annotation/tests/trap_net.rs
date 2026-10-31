//! Trap 网络注解专有条目读取（14.11.6，Table 366/367）：跟踪条目与
//! `/AP`/`/AS` 选定的表单 `/PCM`、`/SeparationColorNames`、`/TrapRegions`、
//! `/TrapStyles`、`/FontFauxing`。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Object, PdfDateTimezone, StreamData, Value};

use super::support::*;

/// 构造带指定条目的 Form XObject 流；`entries` 覆盖同名默认条目。
fn form_object(num: i64, entries: Dict) -> Object {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("XObject".into()))
        .insert("Subtype", Value::Name("Form".into()))
        .insert("Length", Value::Integer(0));
    for (key, value) in entries.0 {
        dict = dict.insert(key, value);
    }
    Object::new(num, 0, Value::Stream(dict, StreamData::Owned(Vec::new())))
}

fn appearance(reference: Value) -> Value {
    Value::Dict(Dict::new().insert("N", reference))
}

/// Table 366 的 `/Version` + `/AnnotStates` 跟踪形式（空数组）。
fn version_tracking(dict: Dict) -> Dict {
    dict.insert("Version", Value::Array(Array(vec![Value::Ref(1, 0)])))
        .insert("AnnotStates", Value::Array(Array(vec![Value::Null])))
}

/// TrapNet 注解的基础字典：`/F` 为 68、外观指向表单 10。
fn trap_base() -> Dict {
    base_annotation("TrapNet")
        .insert("F", Value::Integer(68))
        .insert("AP", appearance(Value::Ref(10, 0)))
}

fn assert_malformed(error: AnnotationError, needle: &str) {
    match error {
        AnnotationError::Malformed(message) => {
            assert!(
                message.contains(needle),
                "expected {needle:?} in {message:?}"
            );
        }
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn trap_net_reads_tracking_entries_and_form_entries() {
    let form = form_object(
        10,
        Dict::new()
            .insert("PCM", Value::Name("DeviceCMYK".into()))
            .insert(
                "SeparationColorNames",
                Value::Array(Array(vec![
                    Value::Name("Cyan".into()),
                    Value::Name("Magenta".into()),
                ])),
            )
            .insert(
                "TrapRegions",
                Value::Array(Array(vec![Value::Ref(20, 0), Value::Ref(21, 0)])),
            )
            .insert("TrapStyles", Value::String("Standard".into()))
            .insert(
                "FontFauxing",
                Value::Array(Array(vec![
                    Value::Ref(30, 0),
                    Value::Dict(Dict::new().insert("Type", Value::Name("Font".into()))),
                ])),
            ),
    );
    let font = Object::new(
        30,
        0,
        Value::Dict(Dict::new().insert("BaseFont", Value::Name("Helvetica".into()))),
    );
    let document = doc(vec![catalog_object(), form, font]);
    let dict = trap_base().insert(
        "LastModified",
        Value::String("D:20240131120000+01'00'".into()),
    );
    let annotation = parse_annotation(&document, 0, None, &dict).expect("trap network annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::TrapNet);
    let trap = annotation.trap_network.expect("tracking and form entries");
    let date = trap.last_modified.expect("/LastModified");
    assert_eq!(date.year(), 2024);
    assert_eq!(date.month(), Some(1));
    assert_eq!(date.day(), Some(31));
    assert_eq!(date.hour(), Some(12));
    assert_eq!(date.minute(), Some(0));
    assert_eq!(date.second(), Some(0));
    assert_eq!(
        date.timezone(),
        PdfDateTimezone::Offset {
            positive: true,
            hours: 1,
            minutes: 0,
        }
    );
    assert_eq!(trap.version, None);
    assert_eq!(trap.annot_states, None);
    assert_eq!(trap.form.reference, Some((10, 0)));
    assert_eq!(trap.form.pcm, ProcessColorModel::DeviceCMYK);
    assert_eq!(trap.form.separation_color_names, ["Cyan", "Magenta"]);
    assert_eq!(trap.form.trap_regions, [(20, 0), (21, 0)]);
    assert_eq!(trap.form.trap_styles.as_deref(), Some("Standard"));
    assert_eq!(trap.form.font_fauxing.len(), 2);
    assert_eq!(
        trap.form.font_fauxing[0].get("BaseFont"),
        Some(&Value::Name("Helvetica".into())),
        "间接引用的字体字典应被解析"
    );
    assert_eq!(
        trap.form.font_fauxing[1].get("Type"),
        Some(&Value::Name("Font".into()))
    );

    // 第二种跟踪形式：`/Version` 引用数组 + `/AnnotStates` [Name, Null]。
    let document = doc(vec![catalog_object()]);
    let dict = base_annotation("TrapNet")
        .insert("F", Value::Integer(68))
        .insert(
            "AP",
            appearance(Value::Stream(
                Dict::new()
                    .insert("Subtype", Value::Name("Form".into()))
                    .insert("PCM", Value::Name("DeviceGray".into())),
                StreamData::Owned(Vec::new()),
            )),
        )
        .insert(
            "Version",
            Value::Array(Array(vec![Value::Ref(3, 0), Value::Ref(4, 0)])),
        )
        .insert(
            "AnnotStates",
            Value::Array(Array(vec![Value::Name("On".into()), Value::Null])),
        );
    let annotation = parse_annotation(&document, 0, None, &dict).expect("version tracking");
    let trap = annotation.trap_network.expect("trap network");
    assert_eq!(trap.last_modified, None);
    assert_eq!(
        trap.version,
        Some(vec![Value::Ref(3, 0), Value::Ref(4, 0)]),
        "/Version 保留原始引用值"
    );
    assert_eq!(trap.annot_states, Some(vec![Some("On".to_string()), None]));
    assert_eq!(trap.form.reference, None);
    assert_eq!(trap.form.pcm, ProcessColorModel::DeviceGray);
}

#[test]
fn trap_net_selects_state_appearance_via_as() {
    let document = doc(vec![
        catalog_object(),
        form_object(
            10,
            Dict::new().insert("PCM", Value::Name("DeviceGray".into())),
        ),
        form_object(
            11,
            Dict::new()
                .insert("PCM", Value::Name("DeviceRGB".into()))
                .insert("TrapStyles", Value::String("Rich".into())),
        ),
    ]);
    let dict = trap_base()
        .insert("LastModified", Value::String("D:20240131".into()))
        .insert("AS", Value::Name("Off".into()))
        .insert(
            "AP",
            appearance(Value::Dict(
                Dict::new()
                    .insert("On", Value::Ref(10, 0))
                    .insert("Off", Value::Ref(11, 0)),
            )),
        );
    let annotation =
        parse_annotation(&document, 0, None, &dict).expect("state sub-dictionary appearance");
    let trap = annotation.trap_network.expect("trap network");
    assert_eq!(
        trap.form.reference,
        Some((11, 0)),
        "/AS 选中的状态流决定 reference"
    );
    assert_eq!(trap.form.pcm, ProcessColorModel::DeviceRGB);
    assert_eq!(trap.form.trap_styles.as_deref(), Some("Rich"));
}

#[test]
fn trap_net_without_optional_form_entries_reads_required_pcm_only() {
    let document = doc(vec![catalog_object()]);
    let dict = base_annotation("TrapNet")
        .insert("F", Value::Integer(68))
        .insert(
            "AP",
            appearance(Value::Stream(
                Dict::new()
                    .insert("Subtype", Value::Name("Form".into()))
                    .insert("PCM", Value::Name("DeviceN".into())),
                StreamData::Owned(Vec::new()),
            )),
        )
        .insert("Version", Value::Array(Array(Vec::new())))
        .insert("AnnotStates", Value::Array(Array(Vec::new())));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("direct form appearance");
    let trap = annotation.trap_network.expect("trap network");
    assert_eq!(trap.form.reference, None, "直接流没有间接引用");
    assert_eq!(trap.form.pcm, ProcessColorModel::DeviceN);
    assert!(trap.form.separation_color_names.is_empty());
    assert!(trap.form.trap_regions.is_empty());
    assert_eq!(trap.form.trap_styles, None);
    assert!(trap.form.font_fauxing.is_empty());
    assert_eq!(trap.version, Some(Vec::new()));
    assert_eq!(trap.annot_states, Some(Vec::new()));
}

#[test]
fn malformed_trap_net_entries_are_typed_errors() {
    let document = doc(vec![
        catalog_object(),
        form_object(
            10,
            Dict::new().insert("PCM", Value::Name("DeviceGray".into())),
        ),
        form_object(
            11,
            Dict::new()
                .insert("Subtype", Value::Name("Image".into()))
                .insert("PCM", Value::Name("DeviceGray".into())),
        ),
        form_object(
            12,
            Dict::new()
                .insert("PCM", Value::Name("DeviceGray".into()))
                .insert("TrapStyles", Value::Integer(7)),
        ),
        form_object(
            13,
            Dict::new()
                .insert("PCM", Value::Name("DeviceGray".into()))
                .insert("TrapRegions", Value::Array(Array(vec![Value::Integer(4)]))),
        ),
        form_object(
            14,
            Dict::new()
                .insert("PCM", Value::Name("DeviceGray".into()))
                .insert("FontFauxing", Value::Array(Array(vec![Value::Integer(3)]))),
        ),
        form_object(15, Dict::new().insert("PCM", Value::Integer(3))),
        form_object(16, Dict::new()),
    ]);

    // `/F` 与 `/AP`（含 `/N`）对 TrapNet 为必填。
    let dict = base_annotation("TrapNet").insert("AP", appearance(Value::Ref(10, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("missing /F"),
        "/F",
    );
    let dict = base_annotation("TrapNet").insert("F", Value::Integer(68));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("missing /AP"),
        "/AP",
    );
    let dict = trap_base().insert("AP", Value::Dict(Dict::new()));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("missing /AP /N"),
        "/N",
    );

    // Table 366：两种跟踪形式只能出现一种。
    let dict = trap_base()
        .insert("LastModified", Value::String("D:20240131".into()))
        .insert("Version", Value::Array(Array(vec![Value::Ref(1, 0)])))
        .insert("AnnotStates", Value::Array(Array(vec![Value::Null])));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("both tracking forms"),
        "/Version",
    );
    assert_malformed(
        parse_annotation(&document, 0, None, &trap_base()).expect_err("neither tracking form"),
        "/Version",
    );
    let dict = trap_base().insert("LastModified", Value::String("2024-01-31".into()));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("invalid date"),
        "/LastModified",
    );
    let dict = trap_base()
        .insert("Version", Value::Array(Array(vec![Value::Integer(1)])))
        .insert("AnnotStates", Value::Array(Array(vec![Value::Null])));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("non-ref /Version entry"),
        "/Version",
    );
    let dict = trap_base()
        .insert("Version", Value::Array(Array(vec![Value::Ref(1, 0)])))
        .insert("AnnotStates", Value::Array(Array(vec![Value::Integer(1)])));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("bad /AnnotStates entry"),
        "/AnnotStates",
    );

    // `/AS` 须为 name 且命中状态键；流外观不接受 `/AS`。
    let dict = version_tracking(trap_base())
        .insert(
            "AP",
            appearance(Value::Dict(Dict::new().insert("On", Value::Ref(10, 0)))),
        )
        .insert("AS", Value::Name("Off".into()));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("/AS misses state"),
        "unknown appearance state",
    );
    let dict = version_tracking(trap_base()).insert("AS", Value::Name("On".into()));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("/AS with stream /N"),
        "/AS",
    );

    // 多状态无 `/AS` 与空状态字典均为错误。
    let dict = version_tracking(trap_base()).insert(
        "AP",
        appearance(Value::Dict(
            Dict::new()
                .insert("On", Value::Ref(10, 0))
                .insert("Off", Value::Ref(10, 0)),
        )),
    );
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("multiple states without /AS"),
        "multiple appearances",
    );
    let dict = version_tracking(trap_base()).insert("AP", appearance(Value::Dict(Dict::new())));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("empty state dictionary"),
        "empty",
    );

    // 选中的外观须为 Form XObject。
    let dict = version_tracking(trap_base()).insert("AP", appearance(Value::Ref(11, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("/Subtype not Form"),
        "/Subtype",
    );

    // Table 367 条目类型。
    let dict = version_tracking(trap_base()).insert("AP", appearance(Value::Ref(12, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("non-string /TrapStyles"),
        "/TrapStyles",
    );
    let dict = version_tracking(trap_base()).insert("AP", appearance(Value::Ref(13, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("non-ref /TrapRegions"),
        "/TrapRegions",
    );
    let dict = version_tracking(trap_base()).insert("AP", appearance(Value::Ref(14, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("non-dict /FontFauxing"),
        "/FontFauxing",
    );
    let dict = version_tracking(trap_base()).insert("AP", appearance(Value::Ref(15, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("non-name /PCM"),
        "/PCM",
    );
    let dict = version_tracking(trap_base()).insert("AP", appearance(Value::Ref(16, 0)));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("missing /PCM"),
        "/PCM",
    );

    // Table 366 条目只属于 TrapNet 注解。
    let dict =
        base_annotation("Text").insert("AnnotStates", Value::Array(Array(vec![Value::Null])));
    assert_malformed(
        parse_annotation(&document, 0, None, &dict).expect_err("/AnnotStates on Text"),
        "/AnnotStates",
    );
}
