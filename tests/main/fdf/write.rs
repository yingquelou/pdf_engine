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
