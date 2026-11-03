//! CMap 流字典（9.7.5.2，Table 120）类型化读端＋CMap↔CIDFont 一致性诊断的
//! 集成测试：手搭最小文档 → [`core::writer::write_document`] → 真实解析器
//! `parse_buf` 回读 → 断言类型化条目、诊断内容与**文本抽取结果不变**。
//!
//! 复用父模块的 `/UseCMap` 链夹具（`ENCODING_BASE`/`ENCODING_LOCAL`/
//! `TOUNICODE_BASE`/`TOUNICODE_LOCAL`），因此每个用例同时证明：诊断出口只看
//! 字典条目，不触碰映射合并逻辑——「本地覆盖基座」的既有结果（CID
//! `9,2,3`、文本 `PNO`）在所有非法形态下都原样保持。

use super::*;
use pdf_engine::core::reader::font::{
    CMapBaseReference, CMapConsistencyIssue, CMapConsistencyRule, CMapEntry, CMapPosition,
    CMapWritingMode,
};

/// 带 Table 120 全条目的 CMap 流字典；`/UseCMap` 指向 8 号基座流。
fn cmap_dict(name: &str, info: Value, mode: i64, cmap_type: i64) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("CMap".into()))
        .insert("CMapName", Value::Name(name.into()))
        .insert("CIDSystemInfo", info)
        .insert("WMode", Value::Integer(mode))
        .insert("CMapType", Value::Integer(cmap_type))
        .insert("UseCMap", Value::Ref(8, 0))
}

/// 只有 `/UseCMap` 的裸 CMap 流字典（Required 条目全缺）。
fn bare_dict() -> Dict {
    Dict::new().insert("UseCMap", Value::Ref(8, 0))
}

fn collection(registry: &str, ordering: &str, supplement: i64) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Registry", Value::String(registry.into()))
            .insert("Ordering", Value::String(ordering.into()))
            .insert("Supplement", Value::Integer(supplement)),
    )
}

/// 流内声明与字典条目对照用的 CMap 数据。
fn cmap_body(name: &str, mode: i64, cmap_type: i64) -> String {
    format!(
        "/CMapName /{name} def\n/CMapType {cmap_type} def\n/WMode {mode} def\n{}",
        ENCODING_LOCAL
    )
}

/// 可配置的 Type 0 字体单页文档。
struct Fixture {
    encoding_dict: Dict,
    encoding_body: String,
    /// `/Encoding` 的替代取值（预定义名）；`None` 表示指向 7 号 CMap 流。
    encoding_name: Option<Value>,
    font_info: Value,
    to_unicode_dict: Dict,
    embedded: bool,
}

impl Fixture {
    /// 一致形态：字典与流内声明互相吻合，字符集与后代字体同为 Adobe-Identity-0。
    fn clean() -> Self {
        Self {
            encoding_dict: cmap_dict(
                "Example-Consistency",
                collection("Adobe", "Identity", 0),
                0,
                2,
            ),
            encoding_body: cmap_body("Example-Consistency", 0, 2),
            encoding_name: None,
            font_info: collection("Adobe", "Identity", 0),
            to_unicode_dict: Dict::new()
                .insert("Type", Value::Name("CMap".into()))
                .insert("UseCMap", Value::Ref(10, 0)),
            embedded: false,
        }
    }

    /// 换掉 `/Encoding` 流字典与流内容。
    fn encoding(mut self, dict: Dict, body: String) -> Self {
        self.encoding_dict = dict;
        self.encoding_body = body;
        self
    }

    /// `/Encoding` 改用预定义名（如 `Identity-H`）。
    fn predefined_encoding(mut self, name: &str) -> Self {
        self.encoding_name = Some(Value::Name(name.into()));
        self
    }

    fn font_collection(mut self, info: Value) -> Self {
        self.font_info = info;
        self
    }

    fn with_to_unicode_dict(mut self, dict: Dict) -> Self {
        self.to_unicode_dict = dict;
        self
    }

    fn embedded(mut self, embedded: bool) -> Self {
        self.embedded = embedded;
        self
    }

    fn document(&self) -> Document {
        let mut content = core::content::ContentStream::new();
        content
            .begin_text()
            .set_font("F1", 18.0)
            .move_next_line(72.0, 700.0)
            .show_text_hex([0x00u8, 0x41, 0x00, 0x42, 0x00, 0x43])
            .end_text();

        let mut descriptor = Dict::new()
            .insert("Type", Value::Name("FontDescriptor".into()))
            .insert("FontName", Value::Name("ExampleComposite".into()));
        if self.embedded {
            // 诊断只看「描述符有没有 `/FontFile*` 条目」，不读字体程序。这里用
            // 间接引用 20 号：写出器会把字典打包进对象流，而对象流不能容纳内联
            // 流，直接字典里嵌 `Value::Stream` 往返后会读不回来。
            descriptor = descriptor.insert("FontFile2", Value::Ref(20, 0));
        }

        let mut doc = Document::new();
        doc.set_object(Object::new(
            1,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Catalog".into()))
                    .insert("Pages", Value::Ref(2, 0)),
            ),
        ));
        doc.set_object(Object::new(
            2,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Pages".into()))
                    .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                    .insert("Count", Value::Integer(1)),
            ),
        ));
        doc.set_object(Object::new(
            3,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Page".into()))
                    .insert("Parent", Value::Ref(2, 0))
                    .insert("Contents", Value::Ref(5, 0))
                    .insert(
                        "Resources",
                        Value::Dict(Dict::new().insert(
                            "Font",
                            Value::Dict(Dict::new().insert("F1", Value::Ref(4, 0))),
                        )),
                    ),
            ),
        ));
        doc.set_object(Object::new(
            4,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Font".into()))
                    .insert("Subtype", Value::Name("Type0".into()))
                    .insert("BaseFont", Value::Name("ExampleComposite".into()))
                    .insert(
                        "Encoding",
                        self.encoding_name.clone().unwrap_or(Value::Ref(7, 0)),
                    )
                    .insert(
                        "DescendantFonts",
                        Value::Array(Array::new().push(Value::Ref(6, 0))),
                    )
                    .insert("ToUnicode", Value::Ref(9, 0)),
            ),
        ));
        doc.set_object(Object::new(
            5,
            0,
            Value::Stream(Dict::new(), StreamData::Owned(content.to_bytes())),
        ));
        doc.set_object(Object::new(
            6,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Font".into()))
                    .insert("Subtype", Value::Name("CIDFontType2".into()))
                    .insert("BaseFont", Value::Name("ExampleComposite".into()))
                    .insert("CIDSystemInfo", self.font_info.clone())
                    .insert("CIDToGIDMap", Value::Name("Identity".into()))
                    .insert("FontDescriptor", Value::Ref(11, 0)),
            ),
        ));
        doc.set_object(Object::new(
            7,
            0,
            Value::Stream(
                self.encoding_dict.clone(),
                StreamData::Owned(self.encoding_body.clone().into_bytes()),
            ),
        ));
        doc.set_object(Object::new(
            8,
            0,
            Value::Stream(
                Dict::new(),
                StreamData::Owned(ENCODING_BASE.as_bytes().to_vec()),
            ),
        ));
        doc.set_object(Object::new(
            9,
            0,
            Value::Stream(
                self.to_unicode_dict.clone(),
                StreamData::Owned(TOUNICODE_LOCAL.as_bytes().to_vec()),
            ),
        ));
        doc.set_object(Object::new(
            10,
            0,
            Value::Stream(
                Dict::new(),
                StreamData::Owned(TOUNICODE_BASE.as_bytes().to_vec()),
            ),
        ));
        doc.set_object(Object::new(11, 0, Value::Dict(descriptor)));
        if self.embedded {
            // 20 号是「字体程序」流（顶层流对象可完整往返；诊断不看其内容）。
            doc.set_object(Object::new(
                20,
                0,
                Value::Stream(Dict::new(), StreamData::Owned(b"dumy".to_vec())),
            ));
        }
        doc.trailer = Trailer {
            dict: Dict::new()
                .insert("Root", Value::Ref(1, 0))
                .insert("Size", Value::Integer(21)),
        };
        doc
    }

    /// 写出→真实解析器回读。
    fn parse_roundtrip(&self) -> Document {
        let mut buf = Vec::new();
        core::writer::write_document(&self.document(), &mut buf).expect("write");
        parse_buf(&buf)
    }
}

fn encoding_rules(issues: &[CMapConsistencyIssue]) -> Vec<CMapConsistencyRule> {
    issues
        .iter()
        .filter(|issue| issue.position == CMapPosition::Encoding)
        .map(|issue| issue.rule.clone())
        .collect()
}

fn to_unicode_rules(issues: &[CMapConsistencyIssue]) -> Vec<CMapConsistencyRule> {
    issues
        .iter()
        .filter(|issue| issue.position == CMapPosition::ToUnicode)
        .map(|issue| issue.rule.clone())
        .collect()
}

/// 既有抽取与解码结果：非法字典条目一律不得改变它。
fn assert_extraction_unchanged(document: &Document) {
    assert_eq!(
        document.extract_page_text(0).as_deref(),
        Some("PNO"),
        "诊断增量不得改变文本抽取（对照父模块的 PNO 断言）"
    );
    let font = find_font_dict(document, "Type0");
    let font = document
        .type0_font(&font)
        .expect("字体仍可解析")
        .expect("Type 0 字体");
    let decoded = font
        .decode_codes(&[0x00, 0x41, 0x00, 0x42, 0x00, 0x43])
        .unwrap();
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        vec![9, 2, 3],
        "本地覆盖 + /UseCMap 继承的 CID 序列必须原样保持"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.clone())
            .collect::<Vec<_>>(),
        vec![Some("P".into()), Some("N".into()), Some("O".into())]
    );
    assert!(!font.is_vertical());
}

#[test]
fn table_120_entries_are_typed_after_the_real_parser() {
    let fixture = Fixture::clean();
    let document = fixture.parse_roundtrip();
    let font = find_font_dict(&document, "Type0");
    let cmaps = document.type0_cmap_dictionaries(&font);
    let cmap = cmaps.encoding.expect("/Encoding 流应被类型化读取");
    assert_eq!(cmap.object_type, CMapEntry::Present("CMap".into()));
    assert_eq!(
        cmap.cmap_name,
        CMapEntry::Present("Example-Consistency".into())
    );
    assert_eq!(
        cmap.writing_mode,
        CMapEntry::Present(CMapWritingMode::Mode(0)),
        "/WMode 0 必须读成水平书写模式"
    );
    assert_eq!(
        cmap.use_cmap,
        Some(CMapBaseReference::Stream { number: Some(8) }),
        "/UseCMap 的流形态经解析器回读后仍是间接引用对象号"
    );
    assert_eq!(cmap.cmap_type, CMapEntry::Present(2));
    let info = cmap.system_info.value().expect("CIDSystemInfo 可读");
    assert_eq!(
        (info.registry.as_str(), info.ordering.as_str()),
        ("Adobe", "Identity")
    );
    assert_eq!(info.supplement, 0);
    assert!(cmap.declarations.parsed);
    assert_eq!(cmap.declarations.writing_mode, Some(0));
    assert!(cmap.uid.is_missing(), "/UID 非 Table 120 条目，缺省不物化");
    assert!(
        document.cmap_consistency_issues(&font).is_empty(),
        "一致形态不应有任何发现"
    );
    assert_extraction_unchanged(&document);
}

#[test]
fn incompatible_collections_are_diagnosed_without_changing_extraction() {
    // CMap 声明 Adobe-GB1-3，后代 CIDFont 声明 Adobe-Identity-0（9.7.3 不兼容）。
    let document = Fixture::clean()
        .encoding(
            cmap_dict("Example-Consistency", collection("Adobe", "GB1", 3), 0, 2),
            cmap_body("Example-Consistency", 0, 2),
        )
        .font_collection(collection("Adobe", "Identity", 0))
        .parse_roundtrip();
    let font = find_font_dict(&document, "Type0");
    assert_eq!(
        document.cmap_consistency_issues(&font),
        vec![CMapConsistencyIssue {
            position: CMapPosition::Encoding,
            rule: CMapConsistencyRule::CidSystemInfoIncompatible {
                cmap: "Adobe/GB1".into(),
                font: "Adobe/Identity".into(),
            },
        }]
    );
    assert!(
        document.cmap_consistency_issues(&font)[0]
            .rule
            .is_mandatory()
    );
    assert_extraction_unchanged(&document);
}

#[test]
fn supplement_difference_is_reported_as_should_only() {
    // 同一字符集（Adobe-GB1）只 Supplement 不同：Table 116 L14560–14561 明确
    // 该值不参与兼容性判定，故只出 should 线索。
    let document = Fixture::clean()
        .encoding(
            cmap_dict("Example-Consistency", collection("Adobe", "GB1", 5), 0, 2),
            cmap_body("Example-Consistency", 0, 2),
        )
        .font_collection(collection("Adobe", "GB1", 3))
        .parse_roundtrip();
    let font = find_font_dict(&document, "Type0");
    let issues = document.cmap_consistency_issues(&font);
    assert_eq!(
        encoding_rules(&issues),
        vec![CMapConsistencyRule::CidSystemInfoSupplementDiffers { cmap: 5, font: 3 }]
    );
    assert!(!issues[0].rule.is_mandatory());
    assert!(!issues.iter().any(|issue| matches!(
        &issue.rule,
        CMapConsistencyRule::CidSystemInfoIncompatible { .. }
    )));
    assert_extraction_unchanged(&document);
}

#[test]
fn illegal_wmode_and_missing_required_entries_are_typed_not_fatal() {
    // 字典 /WMode 5，流内 /WMode 0：报取值非法，书写模式仍由流内声明决定。
    let document = Fixture::clean()
        .encoding(
            cmap_dict(
                "Example-Consistency",
                collection("Adobe", "Identity", 0),
                5,
                2,
            ),
            cmap_body("Example-Consistency", 0, 2),
        )
        .parse_roundtrip();
    let font = find_font_dict(&document, "Type0");
    assert_eq!(
        document.cmap_consistency_issues(&font),
        vec![CMapConsistencyIssue {
            position: CMapPosition::Encoding,
            rule: CMapConsistencyRule::WModeOutOfRange { value: 5 },
        }]
    );
    assert_extraction_unchanged(&document);

    // Required 条目全缺（只有 /UseCMap）：三条缺失发现，抽取照旧。
    let document = Fixture::clean()
        .encoding(bare_dict(), ENCODING_LOCAL.to_string())
        .parse_roundtrip();
    let font = find_font_dict(&document, "Type0");
    let issues = document.cmap_consistency_issues(&font);
    assert_eq!(
        encoding_rules(&issues),
        vec![
            CMapConsistencyRule::RequiredEntryMissing { key: "Type".into() },
            CMapConsistencyRule::RequiredEntryMissing {
                key: "CMapName".into()
            },
            CMapConsistencyRule::RequiredEntryMissing {
                key: "CIDSystemInfo".into()
            },
        ],
        "缺失条目必须逐条类型化报出：{issues:?}"
    );
    assert!(issues.iter().all(|issue| issue.rule.is_mandatory()));
    assert_extraction_unchanged(&document);
}

#[test]
fn identity_name_encoding_is_diagnosed_against_the_embedded_program() {
    // 9.7.5.2 L14912–14913：Identity-H/V 不得搭配非内嵌字体。
    let document = Fixture::clean()
        .predefined_encoding("Identity-H")
        .parse_roundtrip();
    let font = find_font_dict(&document, "Type0");
    let issues = document.cmap_consistency_issues(&font);
    assert_eq!(
        issues,
        vec![CMapConsistencyIssue {
            position: CMapPosition::Encoding,
            rule: CMapConsistencyRule::IdentityEncodingWithoutEmbeddedFont,
        }],
        "无 /FontFile* 时该条 shall 必须报出：{issues:?}"
    );
    assert!(issues[0].rule.is_mandatory());
    // 预定义名不是 CMap 流，故只出这一条，两处字典读取均为 None/ToUnicode。
    let cmaps = document.type0_cmap_dictionaries(&font);
    assert!(
        cmaps.encoding.is_none(),
        "/Encoding 为预定义名时不构造 Table 120 字典"
    );
    // 描述符声称内嵌后，同一条不再报出。
    let embedded = Fixture::clean()
        .predefined_encoding("Identity-V")
        .embedded(true)
        .parse_roundtrip();
    let font = find_font_dict(&embedded, "Type0");
    assert!(
        embedded
            .cmap_consistency_issues(&font)
            .iter()
            .all(|issue| !matches!(
                &issue.rule,
                CMapConsistencyRule::IdentityEncodingWithoutEmbeddedFont
            )),
        "声称内嵌时不得报出该条"
    );
    // 自定义 CMap 声明 Adobe-Identity 字符集不受该条（按名字）约束。
    let custom = Fixture::clean().parse_roundtrip();
    let font = find_font_dict(&custom, "Type0");
    assert!(
        custom.cmap_consistency_issues(&font).is_empty(),
        "一致形态的自定义 CMap（未内嵌）不应报出任何发现"
    );
}

#[test]
fn to_unicode_cmap_is_read_but_not_held_to_the_required_entries() {
    // 裸 `/ToUnicode` 字典（ISO 自身的例子只有 /Length）：不判 Required。
    let document = Fixture::clean()
        .with_to_unicode_dict(Dict::new())
        .parse_roundtrip();
    let font = find_font_dict(&document, "Type0");
    assert!(document.cmap_consistency_issues(&font).is_empty());
    assert!(document.type0_cmap_dictionaries(&font).to_unicode.is_some());
    // 已写出的条目仍判形态：/Type 非 CMap ＋ /WMode 9。
    let document = Fixture::clean()
        .with_to_unicode_dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("UseCMap", Value::Ref(10, 0))
                .insert("WMode", Value::Integer(9)),
        )
        .parse_roundtrip();
    let font = find_font_dict(&document, "Type0");
    let issues = document.cmap_consistency_issues(&font);
    assert_eq!(
        to_unicode_rules(&issues),
        vec![
            CMapConsistencyRule::TypeNotCMap {
                found: "Pages".into()
            },
            CMapConsistencyRule::WModeOutOfRange { value: 9 },
        ]
    );
    assert!(encoding_rules(&issues).is_empty());
    assert_extraction_unchanged(&document);
}
