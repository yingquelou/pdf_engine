//! FDF 写端 **ASCII85 载荷门禁**单元测试（7.4.3 口径；提交 `96ffd15`
//! 引入的 `validate_ascii85_payloads` 挂进 `navigation/fdf::emit`）：
//! FDF 体部经不可失败的 `write_value` 落字节，非法 `<~ ... ~>` body
//! （EOD 标记 `~>`、表外字符、半组 `z`）必须出现在任何值位置时于装配
//! 字节之前返回类型化 [`WriteError::InvalidObject`]、零字节；合法取值
//! 逐字写出、输出字节不变。

use super::*;

/// 把 FDF 字典装配成目录并直接走 `build_fdf`（原值目录树路径）。
fn build_of(fdf: Dict, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    PdfBuilder::new().build_fdf(&catalog_of(fdf), version)
}

/// 带一个 `/V` 取值的字段目录（Table 246 直通值位置）。
fn catalog_with_field_value(value: Value) -> Dict {
    catalog_of(Dict::new().insert(
        "Fields",
        Value::Array(Array(vec![Value::Dict(field_dict("f").insert("V", value))])),
    ))
}

/// 裸目录里挂一个 FDF 字典条目（原值直通，档位取 1.4 全放行）。
fn catalog_with_fdf_entry(key: &str, value: Value) -> Dict {
    catalog_of(Dict::new().insert(key, value))
}

#[test]
fn fdf_export_rejects_ascii85_field_value_before_emitting_bytes() {
    // 字段 `/V` 存 EOD 标记 `~>`：`<~ a~>b ~> ` 会被读端在第一个 `~>`
    // 收尾、产出更短的串——装配字节之前类型化拒绝、零字节。
    let bad = Value::Ascii85String("a~>b".into());
    assert!(
        matches!(
            PdfBuilder::new().build_fdf(&catalog_with_field_value(bad.clone()), PdfVersion::V1_4),
            Err(WriteError::InvalidObject(_))
        ),
        "/Fields 里的非法 ASCII85 body 应在零字节下被拒绝"
    );
    // 模型导出路径（export_fdf → 同一 emit）同口径。
    let mut field = bare_field("f");
    field.value = Some(bad);
    let model = FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf: {
            let mut fdf = bare_fdf_dictionary();
            fdf.fields = Some(vec![field]);
            fdf
        },
    };
    assert!(
        matches!(
            PdfBuilder::new().export_fdf(&model, PdfVersion::V1_4),
            Err(WriteError::InvalidObject(_))
        ),
        "模型路径的非法 ASCII85 字段值同样零字节拒绝"
    );
}

#[test]
fn fdf_export_rejects_ascii85_alphabet_violation_and_midgroup_z_before_emitting_bytes() {
    // 7.4.3 的三种拒绝形态：表外字符（`v` = 76h > `u` = 75h）、
    // 单独的 `~`（非 `~>` 也是 ASCII85Decode 的提前 EOD）、
    // `z` 落在半组（前 4 个数据字符，非五字节全零组边界）。
    for body in ["9jqo^v", "9jq~o^", "9jqoz"] {
        let result = PdfBuilder::new().build_fdf(
            &catalog_with_field_value(Value::Ascii85String(body.into())),
            PdfVersion::V1_4,
        );
        assert!(
            matches!(result, Err(WriteError::InvalidObject(_))),
            "body {body:?} 应被 ASCII85 门禁拒绝"
        );
    }
}

#[test]
fn fdf_export_rejects_ascii85_in_catalog_pages_annots_and_javascript_before_emitting_bytes() {
    let bad = || Value::Ascii85String("a~>b".into());
    // Table 243 — `/Differences` `/EmbeddedFDFs` `/Status` 原值载荷。
    for key in ["Differences", "EmbeddedFDFs", "Status"] {
        assert!(
            matches!(
                build_of(catalog_with_fdf_entry(key, bad()), PdfVersion::V1_4),
                Err(WriteError::InvalidObject(_))
            ),
            "/{key} 载荷里的非法 ASCII85 应零字节拒绝"
        );
    }
    // Table 243 续表 + Table 245 — `/JavaScript /Before`（script_value_ok
    // 接受 ASCII85 串形态，随后必须过 ASCII85 门禁）。
    let javascript = catalog_with_fdf_entry(
        "JavaScript",
        Value::Dict(Dict::new().insert("Before", bad())),
    );
    assert!(matches!(
        build_of(javascript, PdfVersion::V1_4),
        Err(WriteError::InvalidObject(_))
    ));
    // Table 251 — `/Annots` 注解字典的公共条目 `/Contents`。
    let annots = catalog_with_fdf_entry(
        "Annots",
        Value::Array(Array(vec![Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Text".into()))
                .insert("Page", Value::Integer(0))
                .insert("Contents", bad()),
        )])),
    );
    assert!(matches!(
        build_of(annots, PdfVersion::V1_4),
        Err(WriteError::InvalidObject(_))
    ));
    // Table 248→249→246 — `/Pages` 模板 `/Fields` 字段字典取值。
    let pages = catalog_with_fdf_entry(
        "Pages",
        Value::Array(Array(vec![Value::Dict(Dict::new().insert(
            "Templates",
            Value::Array(Array(vec![Value::Dict(
                    Dict::new()
                        .insert(
                            "TRef",
                            Value::Dict(
                                Dict::new().insert("Name", Value::String("T.A".into())),
                            ),
                        )
                        .insert(
                            "Fields",
                            Value::Array(Array(vec![Value::Dict(
                                field_dict("tax").insert("V", bad()),
                            )])),
                        ),
                )])),
        ))])),
    );
    assert!(matches!(
        build_of(pages, PdfVersion::V1_4),
        Err(WriteError::InvalidObject(_))
    ));
}

#[test]
fn fdf_export_accepts_conforming_ascii85_field_value_and_keeps_payload_verbatim() {
    // 合法 body（五字符整组 `9jqo^`、组边界 `z`、跨行白空格）逐字写出，
    // `<~` + body + `~>` 不转义不剥离——输出字节与门禁引入前一致。
    let bytes = PdfBuilder::new()
        .build_fdf(
            &catalog_with_field_value(Value::Ascii85String("9jqo^".into())),
            PdfVersion::V1_4,
        )
        .expect("合法规则的 ASCII85 body 应可写出");
    let text = String::from_utf8(bytes).expect("ASCII");
    assert!(text.contains("/V <~9jqo^~>"), "逐字包裹写出：{text}");

    let z = PdfBuilder::new()
        .build_fdf(
            &catalog_with_field_value(Value::Ascii85String("z".into())),
            PdfVersion::V1_4,
        )
        .expect("组边界 z 合法");
    assert!(String::from_utf8(z).expect("ASCII").contains("/V <~z~>"));

    let multiline = PdfBuilder::new()
        .build_fdf(
            &catalog_with_field_value(Value::Ascii85String("9jqo^\n9jqo^".into())),
            PdfVersion::V1_4,
        )
        .expect("白空格（Table 1）任意位置合法");
    assert!(
        String::from_utf8(multiline)
            .expect("ASCII")
            .contains("/V <~9jqo^\n9jqo^~>")
    );

    // 合法 FDF 仍能构建（含类型化字段与 `/Pages` 层）。
    assert!(
        PdfBuilder::new()
            .build_fdf(&minimal_catalog(), PdfVersion::V1_2)
            .is_ok()
    );
}
