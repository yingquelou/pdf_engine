//! FDF 写端集成测试（12.7.7）：构造 FDF 字节 → 真实 Parser 回读 →
//! 断言字段值、全限定名与 `/Ff` 等条目穿透；并验证读模型
//! 导出→重解析的保真（含未建模原始条目）与版本门禁零字节。

use super::*;
use pdf_engine::core::builder::PdfBuilder;
use pdf_engine::core::reader::fdf::{FdfDictionary, FdfDocument, FdfField};
use pdf_engine::core::writer::WriteError;

/// 全 None 裸字段（Table 246 读模型；`fully_qualified_name` 由读端
/// 重新计算，写端不消费）。
fn field(partial: &str) -> FdfField {
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
fn dictionary() -> FdfDictionary {
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
        dict: Dict::new(),
    }
}

fn model(fdf: &FdfDictionary) -> FdfDocument {
    FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf: fdf.clone(),
    }
}

#[test]
fn fdf_writer_roundtrip_through_reader() {
    // person → [name(/V Ada /Ff 4), address → [空 /T 子项(/V Blankville)]]
    let mut name = field("name");
    name.value = Some(Value::String("Ada".into()));
    name.field_flags = Some(4);
    let mut blank = field("");
    blank.value = Some(Value::String("Blankville".into()));
    let mut address = field("address");
    address.kids = vec![blank];
    let mut person = field("person");
    person.kids = vec![name, address];
    // agree(/V Yes /Opt [Yes No] /RV 富文本十六进制)
    let mut agree = field("agree");
    agree.value = Some(Value::String("Yes".into()));
    agree.options = Some(Value::Array(Array(vec![
        Value::String("Yes".into()),
        Value::String("No".into()),
    ])));
    agree.rich_value = Some(Value::HexString("FEFF0052".into()));

    let mut fdf = dictionary();
    fdf.fields = Some(vec![person, agree]);
    fdf.status = Some("Imported OK".into());
    fdf.encoding = Some("WinAnsiEncoding".into());
    fdf.source_file = Some(Value::String("tax-return.pdf".into()));

    let bytes = PdfBuilder::new()
        // Table 246 — `/RV` 需 PDF 1.5，故声明 1.5。
        .export_fdf(&model(&fdf), PdfVersion::V1_5)
        .expect("写端应产出 FDF 字节");
    assert!(!bytes.is_empty());

    // 真实 Parser 回读（Table 242/243/246 类型化读模型）。
    let document = parse_buf(&bytes);
    let out = document
        .fdf()
        .expect("重解析不应报错")
        .expect("输出应被识别为 FDF");
    // Table 242 — 声明 1.5 且模型缺 `/Version` 时按声明补写。
    assert_eq!(out.catalog_version.as_deref(), Some("1.5"));
    // Table 246 — 全限定名（含空 `/T` 不扩展父名）与深度优先序。
    let names: Vec<&str> = out
        .all_fields()
        .iter()
        .map(|f| f.fully_qualified_name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "person",
            "person.name",
            "person.address",
            "person.address",
            "agree"
        ]
    );
    // Table 246 — 字段值与 `/Ff` 穿透。
    let name_field = &out.all_fields()[1];
    assert_eq!(name_field.partial_name, "name");
    assert_eq!(name_field.value, Some(Value::String("Ada".into())));
    assert_eq!(name_field.field_flags, Some(4));
    let agree_out = out.field("agree").expect("按全限定名查找");
    assert_eq!(agree_out.partial_name, "agree");
    assert_eq!(
        agree_out.options,
        Some(Value::Array(Array(vec![
            Value::String("Yes".into()),
            Value::String("No".into()),
        ])))
    );
    assert_eq!(
        agree_out.rich_value,
        Some(Value::HexString("FEFF0052".into()))
    );
    // Table 243 — 值枚举与 Status/Encoding/F 穿透。
    assert_eq!(
        document
            .fdf_field_values()
            .expect("枚举不应报错")
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        vec![
            "person".to_string(),
            "person.name".to_string(),
            "person.address".to_string(),
            "person.address".to_string(),
            "agree".to_string()
        ]
    );
    assert_eq!(out.fdf.status.as_deref(), Some("Imported OK"));
    assert_eq!(out.fdf.encoding.as_deref(), Some("WinAnsiEncoding"));
    assert_eq!(
        out.fdf.source_file,
        Some(Value::String("tax-return.pdf".into()))
    );
}

#[test]
fn fdf_read_export_reparse_preserves_read_model() {
    // 读端夹具（含间接对象字段树与未建模条目）→ 导出 → 再读：
    // 模型等值（间接字段转为直接对象，原始未建模条目保留）。
    let document = parse_buf(&fdf_fixture_bytes());
    let mut fdf = document
        .fdf()
        .expect("读取不应报错")
        .expect("夹具应被识别为 FDF");
    // Table 243：`/Fields` 与 `/Pages` shall not 同现——读端宽容，
    // 写端执行规范，先按库策略摘除 /Pages（类型化与原值同步）。
    fdf.fdf.pages = None;
    fdf.fdf.dict.0.remove("Pages");

    let bytes = PdfBuilder::new()
        // Table 246 — 夹具含 `/RV`（PDF 1.5）：生效版本 = max(文件头 1.5,
        // 目录 /Version 1.4) = 1.5，故须声明 1.5（Table 242）。
        .export_fdf(&fdf, PdfVersion::V1_5)
        .expect("导出应成功（生效版本 1.5 覆盖全部条目档位）");
    let reparsed = parse_buf(&bytes);
    let out = reparsed
        .fdf()
        .expect("重解析不应报错")
        .expect("导出应被识别为 FDF");

    // 字段值枚举与源模型逐条目相等（含空 /T 子项与 /V 原值）。
    assert_eq!(
        reparsed.fdf_field_values().expect("枚举"),
        document.fdf_field_values().expect("枚举")
    );
    // 全限定名（间接字段 9 0 R 转为直接对象后仍成立）。
    let names: Vec<&str> = out
        .all_fields()
        .iter()
        .map(|f| f.fully_qualified_name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "person",
            "person.name",
            "person.address",
            "person.address",
            "agree"
        ]
    );
    // Table 246 — `/Ff` 与未建模原始条目保留。
    let name_field = out.field("person.name").expect("子字段可达");
    assert_eq!(name_field.field_flags, Some(4));
    assert_eq!(
        name_field.dict.get("Foo"),
        Some(&Value::String("raw-extra".into())),
        "未建模条目必须随导出保留"
    );
    // Table 242/243 — 目录 `/Version`、`/Status`、`/Encoding` 与
    // `/Annots` 原值（悬空引用按原值保留，读端不解析）。
    assert_eq!(out.catalog_version.as_deref(), Some("1.4"));
    assert_eq!(out.fdf.status.as_deref(), Some("Imported OK"));
    assert_eq!(out.fdf.encoding.as_deref(), Some("WinAnsiEncoding"));
    assert_eq!(out.fdf.annots, Some(Value::Ref(8, 0)));
    assert_eq!(
        out.fdf.source_file,
        Some(Value::String("tax-return.pdf".into()))
    );
}

#[test]
fn fdf_writer_gates_and_conflicts_produce_zero_bytes() {
    // Table 243 — `/Encoding` 要求 1.3。
    let mut fdf = dictionary();
    fdf.encoding = Some("WinAnsiEncoding".into());
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_2),
        Err(WriteError::UnsupportedVersion(_))
    ));

    // Table 246 — `/RV` 要求 1.5；字段树里的孙级条目也要被走到。
    let mut leaf = field("leaf");
    leaf.rich_value = Some(Value::HexString("FEFF0052".into()));
    let mut parent = field("parent");
    parent.kids = vec![leaf];
    let mut fdf = dictionary();
    fdf.fields = Some(vec![parent]);
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_4),
        Err(WriteError::UnsupportedVersion(_))
    ));
    // 声明 1.5 后成功。
    assert!(
        PdfBuilder::new()
            .export_fdf(&model(&fdf), PdfVersion::V1_5)
            .is_ok()
    );

    // Table 243 — `/Fields` 与 `/Pages` 同现：InvalidObject，零字节。
    let mut fdf = dictionary();
    fdf.fields = Some(Vec::new());
    fdf.pages = Some(Value::Array(Array::new()));
    let err = PdfBuilder::new()
        .export_fdf(&model(&fdf), PdfVersion::V1_7)
        .expect_err("/Fields 与 /Pages 同现应被拒绝");
    assert!(
        matches!(err, WriteError::InvalidObject(_)),
        "错误形态：{err}"
    );
}

/// 单间接对象形态的最小 FDF 文件（目录 `/FDF` 为直接字典，`/Target`
/// 值由调用方给出源码文本），供字典形态 `/Target` 的读端宽容测试。
fn fdf_bytes_with_target(target: &str) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%FDF-1.4\n");
    let object_at = buf.len();
    buf.extend_from_slice(
        format!("1 0 obj\n<< /FDF << /Target {target} >> >>\nendobj\n").as_bytes(),
    );
    let xref_at = buf.len();
    buf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{object_at:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(b"trailer\n<< /Root 1 0 R /Size 2 >>\n");
    buf.extend_from_slice(format!("startxref\n{xref_at}\n%%EOF\n").as_bytes());
    buf
}

#[test]
fn fdf_target_typed_roundtrip_restores_frame_and_fields() {
    // Table 243 续表（raw L24927）确证条目全形态：类型化 `/Target`
    // 浏览器框架名 + `/Status` `/Encoding` `/F` `/Fields` → 导出 →
    // 真实 Parser 重解析，逐项还原且字段值仍可枚举。
    let mut name = field("name");
    name.value = Some(Value::String("Ada".into()));
    let mut fdf = dictionary();
    fdf.target_frame = Some("result frame".into());
    fdf.status = Some("Imported".into());
    fdf.encoding = Some("utf_16".into());
    fdf.source_file = Some(Value::String("tax.pdf".into()));
    fdf.fields = Some(vec![name]);
    let bytes = PdfBuilder::new()
        // Table 243 — `/Target` 需 PDF 1.4。
        .export_fdf(&model(&fdf), PdfVersion::V1_4)
        .expect("导出应成功");
    assert!(!bytes.is_empty());
    let document = parse_buf(&bytes);
    let out = document
        .fdf()
        .expect("重解析不应报错")
        .expect("应识别为 FDF");
    // 类型化条目还原；原值为写端十六进制文本串形态。
    assert_eq!(out.fdf.target_frame.as_deref(), Some("result frame"));
    assert!(
        matches!(out.fdf.target, Some(Value::HexString(_))),
        "/Target 应写成十六进制文本串：{:?}",
        out.fdf.target
    );
    assert_eq!(out.fdf.status.as_deref(), Some("Imported"));
    assert_eq!(out.fdf.encoding.as_deref(), Some("utf_16"));
    assert_eq!(out.fdf.source_file, Some(Value::String("tax.pdf".into())));
    assert_eq!(
        document.fdf_field_values().expect("枚举"),
        vec![("name".to_string(), Some(Value::String("Ada".into())))]
    );
}

#[test]
fn fdf_target_absent_not_materialized_on_roundtrip() {
    // 缺省条目不物化：模型不带 `/Target` 时导出字节里没有同名键，
    // 重解析后类型化与原值都是 None。
    let mut leaf = field("a");
    leaf.value = Some(Value::String("1".into()));
    let mut fdf = dictionary();
    fdf.fields = Some(vec![leaf]);
    let bytes = PdfBuilder::new()
        .export_fdf(&model(&fdf), PdfVersion::V1_4)
        .expect("导出");
    let text = String::from_utf8(bytes.clone()).expect("ASCII");
    assert!(!text.contains("/Target"), "缺省条目不应出现：{text}");
    let document = parse_buf(&bytes);
    let out = document.fdf().expect("重解析").expect("FDF");
    assert_eq!(out.fdf.target, None);
    assert_eq!(out.fdf.target_frame, None);
}

#[test]
fn fdf_target_version_gate_and_shall_conflicts_reject_zero_bytes() {
    // Table 243 — `/Target`：PDF 1.4。1.3 声明 → UnsupportedVersion；
    // Err 在装配字节之前返回（emit 先 validate），即零字节。
    let mut fdf = dictionary();
    fdf.target_frame = Some("_top".into());
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_3),
        Err(WriteError::UnsupportedVersion(_))
    ));
    // Table 243 shall（`/Fields` 与 `/Pages` shall not 同现）与类型化
    // `/Target` 共存时仍被 InvalidObject 拒绝，零字节。
    let mut fdf = dictionary();
    fdf.target_frame = Some("_top".into());
    fdf.fields = Some(Vec::new());
    fdf.pages = Some(Value::Array(Array::new()));
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_7),
        Err(WriteError::InvalidObject(_))
    ));
}

#[test]
fn fdf_target_dict_form_parses_leniently_and_export_rejects() {
    // 字典式 `/Target`（Adobe FDF 2.0 形态，非 ISO 32000-1 基线，待核）：
    // 未确证的形态原值保留不丢弃——读端宽容解析、类型化为 None；
    // 该模型再导出时写端按 Table 243 的 string 类型装配前拒绝。
    let bytes = fdf_bytes_with_target("<< /F (form.pdf) >>");
    let document = parse_buf(&bytes);
    let fdf = document
        .fdf()
        .expect("非串 /Target 读端应宽容")
        .expect("应识别为 FDF");
    assert_eq!(fdf.fdf.target_frame, None);
    let raw = fdf.fdf.target.as_ref().expect("原值保留");
    match raw.as_dict() {
        Some(dict) => assert_eq!(dict.get("F"), Some(&Value::String("form.pdf".into()))),
        None => panic!("字典形态原值应不丢弃: {raw:?}"),
    }
    assert!(matches!(
        PdfBuilder::new().export_fdf(&fdf, PdfVersion::V1_4),
        Err(WriteError::InvalidObject(_))
    ));
}
