//! FDF `/JavaScript` 集成往返（12.7.7 / Table 243 续表 + Table 245）：
//! 真实 `PdfBuilder::export_fdf` 产字节 → 真实 Parser 经
//! [`Document::fdf`] 重解析，断言文档级脚本字典的类型化还原、缺省条目
//! 不物化、未确证形态（间接文本流）原值保留，以及非法形态在装配字节
//! 之前零字节拒绝。

use super::*;
use pdf_engine::core::builder::PdfBuilder;
use pdf_engine::core::reader::fdf::{
    FdfDictionary, FdfDocument, FdfField, FdfJavaScript, FdfJavaScriptScript,
};
use pdf_engine::core::writer::WriteError;

/// 全 None 裸字段（Table 246 读模型）。
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
        javascript_scripts: None,
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

/// 单个间接对象形态的最小 FDF 文件（目录 `/FDF` 为直接字典），
/// `/JavaScript` 值由调用方给出源码文本。
fn fdf_bytes_with_javascript(javascript: &str) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%FDF-1.4\n");
    let object_at = buf.len();
    buf.extend_from_slice(
        format!("1 0 obj\n<< /FDF << /JavaScript {javascript} >> >>\nendobj\n").as_bytes(),
    );
    let xref_at = buf.len();
    buf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{object_at:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(b"trailer\n<< /Root 1 0 R /Size 2 >>\n");
    buf.extend_from_slice(format!("startxref\n{xref_at}\n%%EOF\n").as_bytes());
    buf
}

#[test]
fn fdf_javascript_roundtrip_restores_document_scripts_and_fields() {
    // Table 245 raw L24951–L24981 全形态：`/Before`、`/After`、
    // `/AfterPermsReady`（PDF 1.6）与两对 `/Doc` 全局脚本，连同
    // `/Status`、`/Fields` 一起导出后重解析。
    let scripts = FdfJavaScript {
        before: Some("beforeImport();".into()),
        after: Some("afterImport();".into()),
        after_perms_ready: Some("permsReady();".into()),
        doc_scripts: vec![
            FdfJavaScriptScript {
                name: "globals".into(),
                script: "function f(){return 1;}".into(),
            },
            FdfJavaScriptScript {
                name: "cleanup".into(),
                script: "cleanup();".into(),
            },
        ],
        dict: Dict::new(),
    };
    let mut name = field("name");
    name.value = Some(Value::String("Ada".into()));
    let mut fdf = dictionary();
    fdf.fields = Some(vec![name]);
    fdf.status = Some("Imported".into());
    fdf.javascript_scripts = Some(scripts.clone());

    let bytes = PdfBuilder::new()
        .export_fdf(&model(&fdf), PdfVersion::V1_6)
        .expect("导出应成功");
    assert!(!bytes.is_empty());
    let document = parse_buf(&bytes);
    let out = document
        .fdf()
        .expect("重解析不应报错")
        .expect("输出应被识别为 FDF");

    let typed = out
        .fdf
        .javascript_scripts
        .as_ref()
        .expect("字典形态应被类型化");
    // 逐项还原（`dict` 是重解析得到的序列化后字典，不与模型的空字典
    // 比对，故这里逐字段断言）。
    assert_eq!(typed.before.as_deref(), Some("beforeImport();"));
    assert_eq!(typed.after.as_deref(), Some("afterImport();"));
    assert_eq!(typed.after_perms_ready.as_deref(), Some("permsReady();"));
    assert_eq!(typed.doc_scripts, scripts.doc_scripts);
    assert_eq!(
        typed.doc_script("globals").unwrap(),
        "function f(){return 1;}"
    );
    // 原值同时保留（写端把文本串写成十六进制串形态）。
    assert!(
        matches!(out.fdf.javascript, Some(Value::Dict(_))),
        "/JavaScript 原值保留：{:?}",
        out.fdf.javascript
    );
    // 与既有条目互不影响：`/Status` 与字段树照常还原。
    assert_eq!(out.fdf.status.as_deref(), Some("Imported"));
    assert_eq!(
        document.fdf_field_values().expect("枚举"),
        vec![("name".to_string(), Some(Value::String("Ada".into())))]
    );
    // Table 245 — `/AfterPermsReady` 是 PDF 1.6 条目，故声明 1.6。
    assert_eq!(out.catalog_version.as_deref(), Some("1.6"));
}

#[test]
fn fdf_javascript_absent_not_materialized_on_roundtrip() {
    // 缺省条目不物化：模型不带 `/JavaScript` 时导出字节里没有该键，
    // 重解析后类型化与原值都是 None。
    let mut leaf = field("a");
    leaf.value = Some(Value::String("1".into()));
    let mut fdf = dictionary();
    fdf.fields = Some(vec![leaf]);
    let bytes = PdfBuilder::new()
        .export_fdf(&model(&fdf), PdfVersion::V1_4)
        .expect("导出");
    let text = String::from_utf8(bytes.clone()).expect("ASCII");
    assert!(!text.contains("/JavaScript"), "缺省条目不应出现：{text}");
    let out = parse_buf(&bytes).fdf().expect("重解析").expect("FDF");
    assert_eq!(out.fdf.javascript, None);
    assert_eq!(out.fdf.javascript_scripts, None);
}

#[test]
fn fdf_javascript_illegal_forms_reject_zero_bytes() {
    // Table 243 续表 — `/JavaScript` 类型是 dictionary：非字典原值在
    // 装配字节之前拒绝（Err 先于任何字节产生）。
    let mut fdf = dictionary();
    fdf.javascript = Some(Value::String("app.init();".into()));
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_4),
        Err(WriteError::InvalidObject(_))
    ));

    // Table 245 — 类型化字典里的脚本条目非文本串/文本流：同样零字节。
    let scripts = FdfJavaScript {
        before: None,
        after: None,
        after_perms_ready: None,
        doc_scripts: Vec::new(),
        dict: Dict::new().insert("Before", Value::Integer(3)),
    };
    let mut fdf = dictionary();
    fdf.javascript_scripts = Some(scripts);
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_4),
        Err(WriteError::InvalidObject(_))
    ));

    // Table 245 raw L24963 — `/AfterPermsReady` 需 PDF 1.6：1.5 声明
    // 报 UnsupportedVersion（档位门禁先于类型校验）。
    let scripts = FdfJavaScript {
        before: None,
        after: None,
        after_perms_ready: Some("permsReady();".into()),
        doc_scripts: Vec::new(),
        dict: Dict::new(),
    };
    let mut fdf = dictionary();
    fdf.javascript_scripts = Some(scripts);
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_5),
        Err(WriteError::UnsupportedVersion(_))
    ));

    // Table 243 shall 约束不变：`/JavaScript` 与 `/Fields`+`/Pages`
    // 冲突同现时仍是 InvalidObject。
    let mut fdf = dictionary();
    fdf.javascript_scripts = Some(FdfJavaScript {
        before: Some("x();".into()),
        after: None,
        after_perms_ready: None,
        doc_scripts: Vec::new(),
        dict: Dict::new(),
    });
    fdf.fields = Some(Vec::new());
    fdf.pages = Some(Value::Array(Array::new()));
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_7),
        Err(WriteError::InvalidObject(_))
    ));
}

#[test]
fn fdf_javascript_indirect_stream_form_reads_leniently_and_keeps_raw() {
    // Table 245 的「text stream」形态（间接引用承载字节）：读端不
    // 确证 → 类型化为 None / 空配对，原值一条不丢；该模型再导出时
    // 引用按原值写出（悬空引用属单对象写出的剩余缺口，见 write.rs 对
    // `/Annots` 的同款断言），仍不静默丢弃。
    let bytes = fdf_bytes_with_javascript("<< /Before 4 0 R /Doc [/init 5 0 R /tail (t();)] >>");
    let document = parse_buf(&bytes);
    let fdf = document
        .fdf()
        .expect("非确证形态读端应宽容")
        .expect("应识别为 FDF");
    let typed = fdf
        .fdf
        .javascript_scripts
        .as_ref()
        .expect("字典形态应被类型化");
    assert_eq!(typed.before, None, "间接流形态不就地确证");
    assert_eq!(
        typed.doc_scripts,
        vec![FdfJavaScriptScript {
            name: "tail".into(),
            script: "t();".into(),
        }],
        "只有两端都可确证的对进入类型化列表"
    );
    let raw = typed.dict.get("Doc").expect("原数组保留");
    assert_eq!(raw.as_array().expect("/Doc 数组").0.len(), 4);

    let exported = PdfBuilder::new()
        .export_fdf(&fdf, PdfVersion::V1_4)
        .expect("引用形态可导出");
    let text = String::from_utf8(exported.clone()).expect("ASCII");
    assert!(
        text.contains("/Before 4 0 R") && text.contains("/init 5 0 R"),
        "未类型化条目原值透传：{text}"
    );
    let again = parse_buf(&exported).fdf().expect("重解析").expect("FDF");
    assert_eq!(again.fdf.javascript_scripts, fdf.fdf.javascript_scripts);
}
