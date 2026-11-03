//! FDF 写端单元测试（12.7.7）：文件结构形状、Table 242/243/246 版本
//! 门禁（`UnsupportedVersion` 零字节）与结构校验（`InvalidObject`
//! 零字节）、读模型序列化（含未建模条目保留与 `/Version` 补写）。
//! `/JavaScript`（Table 243 续表 + Table 245）的写出与校验见子模块
//! javascript.rs。

use super::*;
use crate::core::reader::fdf::{FdfDictionary, FdfDocument, FdfField};
use crate::core::types::{Array, PdfVersion};

/// Table 245 JavaScript 字典的写出、原值保留与装配前校验。
mod javascript;

/// 最小合法规格目录（Table 242：`/FDF` 必填直接字典）。
fn minimal_catalog() -> Dict {
    Dict::new().insert("FDF", Value::Dict(Dict::new()))
}

/// 最小合法字段字典（Table 246：`/T` 必填）。
fn field_dict(partial_name: &str) -> Dict {
    Dict::new().insert("T", Value::String(partial_name.into()))
}

fn catalog_of(fdf: Dict) -> Dict {
    Dict::new().insert("FDF", Value::Dict(fdf))
}

/// 全 None 裸字段（Table 246 读模型）。
fn bare_field(partial: &str) -> FdfField {
    FdfField {
        partial_name: partial.into(),
        fully_qualified_name: String::new(),
        value: None,
        field_flags: None,
        set_field_flags: None,
        clear_field_flags: None,
        widget_flags: None,
        set_widget_flags: None,
        clear_widget_flags: None,
        options: None,
        appearance: None,
        appearance_reference: None,
        icon_fit: None,
        action: None,
        additional_actions: None,
        rich_value: None,
        kids: Vec::new(),
        dict: Dict::new(),
    }
}

/// 全 None 裸 FDF 字典（Table 243 读模型）。
fn bare_fdf_dictionary() -> FdfDictionary {
    FdfDictionary {
        source_file: None,
        identifier: None,
        fields: None,
        status: None,
        pages: None,
        encoding: None,
        annots: None,
        differences: None,
        target: None,
        target_frame: None,
        embedded_fdfs: None,
        javascript: None,
        javascript_scripts: None,
        dict: Dict::new(),
    }
}

fn unsupported(version: PdfVersion) -> bool {
    matches!(
        PdfBuilder::new().build_fdf(&minimal_catalog(), version),
        Err(WriteError::UnsupportedVersion(_))
    )
}

#[test]
fn fdf_file_shape_header_object_xref_trailer_eof() {
    let bytes = PdfBuilder::new()
        .build_fdf(&minimal_catalog(), PdfVersion::V1_2)
        .expect("最小 FDF 应可写出");
    let text = String::from_utf8(bytes).expect("FDF 头部为 ASCII");
    // 12.7.7.2.2 文件头；Table 241 /Root 间接引用；12.7.7.2.4 %%EOF 收尾。
    assert!(text.starts_with("%FDF-1.2\n"), "头部：{text}");
    assert!(text.contains("1 0 obj\n<< /FDF << >> >>\nendobj\n"));
    assert!(text.contains("xref\n0 2\n0000000000 65535 f \n"));
    assert!(text.contains("trailer\n<< /Root 1 0 R /Size 2 >>\n"));
    assert!(text.ends_with("%%EOF\n"));
}

#[test]
fn fdf_file_version_must_be_1_2_through_1_7() {
    assert!(unsupported(PdfVersion::V1_0));
    assert!(unsupported(PdfVersion::V1_1));
    assert!(unsupported(PdfVersion::V2_0));
    assert!(
        PdfBuilder::new()
            .build_fdf(&minimal_catalog(), PdfVersion::V1_7)
            .is_ok()
    );
}

#[test]
fn fdf_catalog_version_entry_requires_1_4() {
    let catalog = minimal_catalog().insert("Version", Value::Name("1.4".into()));
    // Table 242 — `/Version`（Optional; PDF 1.4）。
    assert!(matches!(
        PdfBuilder::new().build_fdf(&catalog, PdfVersion::V1_3),
        Err(WriteError::UnsupportedVersion(_))
    ));
    assert!(
        PdfBuilder::new()
            .build_fdf(&catalog, PdfVersion::V1_4)
            .is_ok()
    );
    // `/Version` 必须是可解析的版本名字。
    let bad = minimal_catalog().insert("Version", Value::Name("20.0".into()));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&bad, PdfVersion::V1_7),
        Err(WriteError::InvalidObject(_))
    ));
    let not_name = minimal_catalog().insert("Version", Value::Integer(4));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&not_name, PdfVersion::V1_7),
        Err(WriteError::InvalidObject(_))
    ));
}

#[test]
fn fdf_dictionary_1_3_and_1_4_entries_gated() {
    // Table 243 — `/Encoding` `/Pages` `/Annots`：PDF 1.3。
    for key in ["Encoding", "Pages", "Annots"] {
        let fdf = Dict::new().insert(key, Value::Integer(1));
        let catalog = catalog_of(fdf);
        assert!(
            matches!(
                PdfBuilder::new().build_fdf(&catalog, PdfVersion::V1_2),
                Err(WriteError::UnsupportedVersion(_))
            ),
            "/{key} 应要求 1.3（Table 243）"
        );
        assert!(
            PdfBuilder::new()
                .build_fdf(&catalog, PdfVersion::V1_3)
                .is_ok()
        );
    }
    // Table 243 — `/Differences` `/Target` `/EmbeddedFDFs` `/JavaScript`：PDF 1.4。
    for key in ["Differences", "Target", "EmbeddedFDFs", "JavaScript"] {
        let fdf = Dict::new().insert(key, Value::Integer(1));
        let catalog = catalog_of(fdf);
        assert!(
            matches!(
                PdfBuilder::new().build_fdf(&catalog, PdfVersion::V1_3),
                Err(WriteError::UnsupportedVersion(_))
            ),
            "/{key} 应要求 1.4（Table 243）"
        );
    }
}

#[test]
fn fdf_field_entries_gated() {
    // Table 246 — `/APRef` `/IF`：PDF 1.3。
    for key in ["APRef", "IF"] {
        let field = field_dict("f").insert(key, Value::Dict(Dict::new()));
        let catalog =
            catalog_of(Dict::new().insert("Fields", Value::Array(Array(vec![Value::Dict(field)]))));
        assert!(
            matches!(
                PdfBuilder::new().build_fdf(&catalog, PdfVersion::V1_2),
                Err(WriteError::UnsupportedVersion(_))
            ),
            "/{key} 应要求 1.3（Table 246）"
        );
        assert!(
            PdfBuilder::new()
                .build_fdf(&catalog, PdfVersion::V1_3)
                .is_ok()
        );
    }
    // Table 246 — `/RV`：PDF 1.5。
    let field = field_dict("f").insert("RV", Value::HexString("FEFF0052".into()));
    let catalog =
        catalog_of(Dict::new().insert("Fields", Value::Array(Array(vec![Value::Dict(field)]))));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&catalog, PdfVersion::V1_4),
        Err(WriteError::UnsupportedVersion(_))
    ));
    assert!(
        PdfBuilder::new()
            .build_fdf(&catalog, PdfVersion::V1_5)
            .is_ok()
    );
}

#[test]
fn fdf_effective_version_is_max_of_header_and_catalog_version() {
    let field = field_dict("f").insert("RV", Value::HexString("FEFF0052".into()));
    let fields =
        catalog_of(Dict::new().insert("Fields", Value::Array(Array(vec![Value::Dict(field)]))));
    // Table 242 — `/Version` 只在「晚于文件头版本」时生效：
    // 目录 1.5 + 声明 1.4 → 生效 1.5，`/RV` 可写。
    let later = fields.clone().insert("Version", Value::Name("1.5".into()));
    assert!(
        PdfBuilder::new()
            .build_fdf(&later, PdfVersion::V1_4)
            .is_ok(),
        "目录 /Version 1.5 晚于声明 1.4，生效版本应为 1.5"
    );
    // 目录 1.3 早于声明 1.4 → 以文件头为准，`/RV` 仍要求 1.5。
    let earlier = fields.clone().insert("Version", Value::Name("1.3".into()));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&earlier, PdfVersion::V1_4),
        Err(WriteError::UnsupportedVersion(_))
    ));
    // 目录 /Version 超出本规范定义的 FDF 档位（12.7.7.1 只到 1.7）：
    // 保守拒绝，零字节。
    let too_high = fields.insert("Version", Value::Name("2.0".into()));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&too_high, PdfVersion::V1_7),
        Err(WriteError::InvalidObject(_))
    ));
}

#[test]
fn fdf_structure_violations_rejected_zero_bytes() {
    // Table 242 — `/FDF` 缺失或非直接字典。
    assert!(matches!(
        PdfBuilder::new().build_fdf(&Dict::new(), PdfVersion::V1_2),
        Err(WriteError::InvalidObject(_))
    ));
    let ref_fdf = Dict::new().insert("FDF", Value::Ref(2, 0));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&ref_fdf, PdfVersion::V1_2),
        Err(WriteError::InvalidObject(_))
    ));
    // Table 243 — `/Fields` 与 `/Pages`、`/Status` 与 `/Pages` shall not 同现。
    let conflict = catalog_of(
        Dict::new()
            .insert("Fields", Value::Array(Array::new()))
            .insert("Pages", Value::Array(Array::new())),
    );
    assert!(matches!(
        PdfBuilder::new().build_fdf(&conflict, PdfVersion::V1_7),
        Err(WriteError::InvalidObject(_))
    ));
    let conflict_status = catalog_of(
        Dict::new()
            .insert("Status", Value::String("ok".into()))
            .insert("Pages", Value::Array(Array::new())),
    );
    assert!(matches!(
        PdfBuilder::new().build_fdf(&conflict_status, PdfVersion::V1_7),
        Err(WriteError::InvalidObject(_))
    ));
    // Table 246 — 字段缺 `/T`。
    let no_t = catalog_of(Dict::new().insert(
        "Fields",
        Value::Array(Array(vec![Value::Dict(Dict::new())])),
    ));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&no_t, PdfVersion::V1_2),
        Err(WriteError::InvalidObject(_))
    ));
    // Table 246 — 整型标志条目非整数。
    let bad_ff = catalog_of(Dict::new().insert(
        "Fields",
        Value::Array(Array(vec![Value::Dict(
            field_dict("f").insert("Ff", Value::Name("x".into())),
        )])),
    ));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&bad_ff, PdfVersion::V1_2),
        Err(WriteError::InvalidObject(_))
    ));
    // Table 246 — 写端字段必须是直接字典（间接对象不在本切片）。
    let indirect =
        catalog_of(Dict::new().insert("Fields", Value::Array(Array(vec![Value::Ref(9, 0)]))));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&indirect, PdfVersion::V1_2),
        Err(WriteError::InvalidObject(_))
    ));
}

#[test]
fn fdf_export_model_writes_version_at_1_4_and_preserves_raw_extras() {
    let mut field = bare_field("name");
    field.value = Some(Value::String("Ada".into()));
    field.field_flags = Some(4);
    // 未建模原始条目：overlay 保留。
    field.dict = field.dict.insert("Foo", Value::String("raw-extra".into()));
    let mut fdf = bare_fdf_dictionary();
    fdf.fields = Some(vec![field]);
    let model = FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf,
    };
    let bytes = PdfBuilder::new()
        .export_fdf(&model, PdfVersion::V1_4)
        .expect("模型导出应成功");
    let text = String::from_utf8(bytes).expect("ASCII 头部");
    // Table 242 — 声明 ≥1.4 且模型缺 `/Version` 时补写名字。
    assert!(text.contains("/Version /1.4"), "缺 /Version 补写：{text}");
    // Table 246 — `/T` 写成十六进制串，`/Ff` 整数，未建模 `/Foo` 保留。
    assert!(text.contains("/Ff 4"));
    assert!(text.contains("/T <6E616D65>"));
    assert!(text.contains("/V (Ada)"));
    assert!(text.contains("/Foo (raw-extra)"));
    // 1.2 声明：头部承载版本，不写 `/Version`（Table 242：PDF 1.4 条目）。
    let text_12 = String::from_utf8(
        PdfBuilder::new()
            .export_fdf(&model, PdfVersion::V1_2)
            .expect("1.2 导出"),
    )
    .expect("ASCII");
    assert!(text_12.starts_with("%FDF-1.2\n"));
    assert!(!text_12.contains("/Version"));
}

#[test]
fn fdf_export_keeps_model_catalog_version_verbatim() {
    let mut fdf = bare_fdf_dictionary();
    fdf.fields = Some(Vec::new());
    let model = FdfDocument {
        catalog_version: Some("1.4".into()),
        catalog: Dict::new(),
        fdf,
    };
    let text = String::from_utf8(
        PdfBuilder::new()
            .export_fdf(&model, PdfVersion::V1_7)
            .expect("导出"),
    )
    .expect("ASCII");
    assert!(text.contains("/Version /1.4"), "模型原值优先：{text}");
    assert!(
        text.contains("/Fields []"),
        "空数组与 None 区分写出：{text}"
    );
}

#[test]
fn fdf_export_catalog_version_below_1_4_is_gated() {
    let model = FdfDocument {
        catalog_version: Some("1.4".into()),
        catalog: Dict::new(),
        fdf: bare_fdf_dictionary(),
    };
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model, PdfVersion::V1_3),
        Err(WriteError::UnsupportedVersion(_))
    ));
}

#[test]
fn fdf_export_target_frame_typed_wins_and_absent_not_materialized() {
    // Table 243 续表（raw L24927）——`/Target` 类型化字段为准：Some 写成
    // 十六进制文本串并覆盖原值（"main" → 6D61696E，PDFDocEncoding 优先）。
    let mut fdf = bare_fdf_dictionary();
    fdf.target_frame = Some("main".into());
    fdf.target = Some(Value::String("stale".into()));
    let model = FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf,
    };
    let text = String::from_utf8(
        PdfBuilder::new()
            .export_fdf(&model, PdfVersion::V1_4)
            .expect("类型化 /Target 导出"),
    )
    .expect("ASCII");
    assert!(
        text.contains("/Target <6D61696E>"),
        "十六进制文本串：{text}"
    );
    assert!(!text.contains("stale"), "类型化字段应覆盖原值：{text}");

    // 类型化为 None 时原值（串形态）透传，不丢弃。
    let mut fdf = bare_fdf_dictionary();
    fdf.target = Some(Value::String("_top".into()));
    let model = FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf,
    };
    let text = String::from_utf8(
        PdfBuilder::new()
            .export_fdf(&model, PdfVersion::V1_4)
            .expect("原值透传"),
    )
    .expect("ASCII");
    assert!(text.contains("/Target (_top)"), "原值串透传：{text}");

    // 全缺省不物化：类型化与原值都 None 时删除原字典里的同名条目。
    let mut fdf = bare_fdf_dictionary();
    fdf.dict = fdf.dict.insert("Target", Value::String("ghost".into()));
    let model = FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf,
    };
    let text = String::from_utf8(
        PdfBuilder::new()
            .export_fdf(&model, PdfVersion::V1_4)
            .expect("缺省导出"),
    )
    .expect("ASCII");
    assert!(!text.contains("/Target"), "缺省条目不物化：{text}");
}

#[test]
fn fdf_target_non_string_rejected_before_assembly() {
    // Table 243——`/Target` 类型是 string：字典形态（Adobe FDF 2.0，
    // 非本规范基线，待核）在装配前拒绝，零字节。
    let dict_target = catalog_of(Dict::new().insert("Target", Value::Dict(Dict::new())));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&dict_target, PdfVersion::V1_7),
        Err(WriteError::InvalidObject(_))
    ));
    // 模型导出路径同样拒绝（类型化 None、原值为字典）。
    let mut fdf = bare_fdf_dictionary();
    fdf.target = Some(Value::Dict(Dict::new()));
    let model = FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf,
    };
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model, PdfVersion::V1_7),
        Err(WriteError::InvalidObject(_))
    ));
    // 档位门禁先于类型校验：1.3 声明下的非串 /Target 仍报
    // UnsupportedVersion（与既有门禁测试口径一致）。
    assert!(matches!(
        PdfBuilder::new().build_fdf(&dict_target, PdfVersion::V1_3),
        Err(WriteError::UnsupportedVersion(_))
    ));
}
