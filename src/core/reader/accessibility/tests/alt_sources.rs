//! 14.9.3–14.9.4 替代文本来源解析：引擎降级规则、Span 属性表候选采集、
//! `/MCID` 结构元素定位与 media clip `/Alt` 的出处判定。

use crate::core::reader::accessibility::{
    AlternativeCandidate, AlternativeTextProblem, AlternativeTextSource, AlternativesResolution,
    MarkedContentLanguageError, SemanticText, resolve_alternatives,
};
use crate::core::reader::content::MarkedSection;
use crate::core::types::{Dict, Document, StreamData, Trailer, Value};

use super::support::{
    CATALOG, ROOT, arr, element, insert, language, marked_section, multi_language, name,
    struct_root, text,
};

const PAGES: i64 = 3;
const PAGE: i64 = 4;
const ELEMENT: i64 = 5;

fn candidate(source: AlternativeTextSource, raw: &str) -> AlternativeCandidate {
    AlternativeCandidate::decoded(source, SemanticText::parse(raw).expect("parse"), None)
}

fn malformed_candidate(source: AlternativeTextSource) -> AlternativeCandidate {
    AlternativeCandidate::malformed(
        source,
        crate::core::reader::accessibility::LanguageError::Malformed("test".to_owned()),
        None,
    )
}

/// 单一来源单独在场时各自胜出，且只填自己的槽位。
#[test]
fn resolve_alternatives_each_single_source_wins_its_own_slot() {
    let one = |source: AlternativeTextSource, raw: &str| {
        resolve_alternatives(vec![candidate(source, raw)])
    };

    let resolution = one(AlternativeTextSource::MarkedContentAlt, "six-point star");
    assert_eq!(
        resolution.description.map(|a| (a.source, a.text)),
        Some((
            AlternativeTextSource::MarkedContentAlt,
            "six-point star".to_owned()
        )),
        "14.9.3：/Alt 单独在场即生效描述槽"
    );
    assert_eq!(resolution.replacement, None, "描述候选不得填入替换槽");

    let resolution = one(AlternativeTextSource::MarkedContentActualText, "Druk");
    assert_eq!(resolution.description, None, "替换候选不得填入描述槽");
    assert_eq!(
        resolution.replacement.map(|a| (a.source, a.text)),
        Some((
            AlternativeTextSource::MarkedContentActualText,
            "Druk".to_owned()
        )),
        "14.9.4：/ActualText 是替换而非描述"
    );

    for source in [
        AlternativeTextSource::AnnotationContents,
        AlternativeTextSource::FieldAlternateName,
        AlternativeTextSource::MediaClipAlt,
        AlternativeTextSource::StructureElementAlt,
    ] {
        let resolution = one(source, "text");
        assert_eq!(resolution.description.map(|a| a.source), Some(source));
    }
    let resolution = one(AlternativeTextSource::StructureElementActualText, "x");
    assert_eq!(
        resolution.replacement.map(|a| a.source),
        Some(AlternativeTextSource::StructureElementActualText)
    );
}

/// 两个竞争来源同槽位时取高秩者；`/ActualText` 与 `/Alt` 各归各槽、
/// 互不混同（14.9.4 NOTE 2）。
#[test]
fn resolve_alternatives_competing_sources_pick_winner_and_keep_roles_apart() {
    let resolution = resolve_alternatives(vec![
        candidate(AlternativeTextSource::MarkedContentAlt, "span alt"),
        candidate(AlternativeTextSource::StructureElementAlt, "element alt"),
        candidate(
            AlternativeTextSource::MarkedContentActualText,
            "span actual",
        ),
        candidate(
            AlternativeTextSource::StructureElementActualText,
            "element actual",
        ),
    ]);
    assert_eq!(
        resolution.description.as_ref().map(|a| a.source),
        Some(AlternativeTextSource::StructureElementAlt),
        "库策略：结构元素 /Alt 秩高于 Span 属性表 /Alt（与 14.9.2.3 语言取向一致）"
    );
    assert_eq!(
        resolution.replacement.as_ref().map(|a| a.source),
        Some(AlternativeTextSource::StructureElementActualText),
    );
    assert_eq!(
        resolution.description.as_ref().map(|a| a.text.as_str()),
        Some("element alt")
    );
    assert_eq!(
        resolution.replacement.as_ref().map(|a| a.text.as_str()),
        Some("element actual"),
        "/ActualText 文本绝不进描述槽、/Alt 绝不进替换槽"
    );
    assert!(resolution.problems.is_empty(), "{:?}", resolution.problems);

    let resolution = resolve_alternatives(vec![
        candidate(AlternativeTextSource::MarkedContentAlt, "describe me"),
        candidate(AlternativeTextSource::MarkedContentActualText, "replace me"),
    ]);
    assert_eq!(
        resolution.description.map(|a| (a.source, a.text)),
        Some((
            AlternativeTextSource::MarkedContentAlt,
            "describe me".to_owned()
        ))
    );
    assert_eq!(
        resolution.replacement.map(|a| (a.source, a.text)),
        Some((
            AlternativeTextSource::MarkedContentActualText,
            "replace me".to_owned()
        ))
    );
}

/// 空串与纯空白候选降级为类型化问题并把胜利让给低秩真实文本。
#[test]
fn resolve_alternatives_demotes_empty_and_whitespace_only_candidates() {
    let resolution = resolve_alternatives(vec![
        candidate(AlternativeTextSource::MarkedContentAlt, ""),
        candidate(AlternativeTextSource::AnnotationContents, "real contents"),
    ]);
    assert_eq!(
        resolution.description.map(|a| (a.source, a.text)),
        Some((
            AlternativeTextSource::AnnotationContents,
            "real contents".to_owned()
        )),
        "14.9.3：空 /Alt 不得压过真实 /Contents"
    );
    assert_eq!(
        resolution.problems,
        vec![AlternativeTextProblem::Empty {
            source: AlternativeTextSource::MarkedContentAlt
        }]
    );

    let resolution = resolve_alternatives(vec![
        candidate(
            AlternativeTextSource::StructureElementAlt,
            " \u{a0}\u{2003}",
        ),
        candidate(AlternativeTextSource::MarkedContentActualText, "kept"),
    ]);
    assert_eq!(resolution.description, None);
    assert_eq!(
        resolution.problems,
        vec![AlternativeTextProblem::WhitespaceOnly {
            source: AlternativeTextSource::StructureElementAlt
        }],
        "纯空白（含 NBSP/em-space）降级，替换槽不受影响"
    );

    let resolution = resolve_alternatives(vec![
        candidate(AlternativeTextSource::MarkedContentAlt, ""),
        candidate(AlternativeTextSource::FieldAlternateName, "  "),
    ]);
    assert_eq!(resolution.description, None);
    assert_eq!(resolution.problems.len(), 2);
}

/// 形态非法候选记 Malformed 并继续考察下一来源；同文本低秩候选记
/// Duplicate。全程不 panic。
#[test]
fn resolve_alternatives_records_malformed_and_duplicate_problems() {
    let resolution = resolve_alternatives(vec![
        malformed_candidate(AlternativeTextSource::MarkedContentAlt),
        candidate(AlternativeTextSource::AnnotationContents, "fallback"),
    ]);
    assert_eq!(
        resolution.description.as_ref().map(|a| a.text.as_str()),
        Some("fallback")
    );
    assert!(matches!(
        &resolution.problems[..],
        [AlternativeTextProblem::Malformed {
            source: AlternativeTextSource::MarkedContentAlt,
            ..
        }]
    ));

    let resolution = resolve_alternatives(vec![
        candidate(AlternativeTextSource::MarkedContentAlt, "same text"),
        candidate(AlternativeTextSource::StructureElementAlt, "same text"),
    ]);
    assert_eq!(
        resolution.description.map(|a| a.source),
        Some(AlternativeTextSource::StructureElementAlt)
    );
    assert_eq!(
        resolution.problems,
        vec![AlternativeTextProblem::Duplicate {
            kept: AlternativeTextSource::StructureElementAlt,
            dropped: AlternativeTextSource::MarkedContentAlt,
        }],
        "重复给出：胜者取高秩，低秩者记问题而非静默丢弃"
    );
}

/// Span 属性表采集：非文本串条目转类型化问题；非 Span 标签与缺属性表
/// 不产生候选。
#[test]
fn span_alternative_candidates_reject_non_text_entries_without_panic() {
    let section = marked_section(
        "Span",
        Some(
            Dict::new()
                .insert("Alt", Value::Integer(3))
                .insert("ActualText", name("not-a-string")),
        ),
        None,
    );
    let resolution = resolve_alternatives(section.span_alternative_candidates(None));
    assert_eq!(resolution.description, None);
    assert_eq!(resolution.replacement, None);
    let kinds: Vec<_> = resolution
        .problems
        .iter()
        .map(|problem| match problem {
            AlternativeTextProblem::Malformed { source, .. } => Some(*source),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            Some(AlternativeTextSource::MarkedContentActualText),
            Some(AlternativeTextSource::MarkedContentAlt),
        ],
        "非文本串条目必须是类型化问题，不得胜出也不得 panic（替换槽先遍历）"
    );

    let non_span = marked_section(
        "Figure",
        Some(Dict::new().insert("Alt", text("ignored"))),
        None,
    );
    assert!(
        non_span.span_alternative_candidates(None).is_empty(),
        "14.9.3 只给 Span 标签属性表定义 /Alt"
    );
    let bare = MarkedSection {
        properties: None,
        ..marked_section("Span", None, None)
    };
    assert!(bare.span_alternative_candidates(None).is_empty());
}

/// 整页解析：`/MCID` 定位结构元素候选；Span 空 `/Alt` 落到结构元素
/// `/Alt`；页码越界是类型化错误。
#[test]
fn page_marked_content_alternatives_resolves_span_and_structure_sources() {
    let content = b"/Span <</MCID 0 /Alt () /ActualText (span actual)>> BDC (X) Tj EMC \
                    /Figure <</MCID 0>> BDC (Y) Tj EMC \
                    /Span <</Alt (plain span)>> BDC (Z) Tj EMC";
    let root = struct_root(Dict::new())
        .insert("K", arr(vec![Value::Ref(ELEMENT, 0)]))
        .insert(
            "ParentTree",
            Value::Dict(Dict::new().insert(
                "Nums",
                arr(vec![Value::Integer(0), arr(vec![Value::Ref(ELEMENT, 0)])]),
            )),
        );
    let element = element("Figure", ROOT).insert("Alt", text("structured description"));
    let doc = alternative_page_document(content, root, element);

    let resolutions = doc
        .page_marked_content_alternatives(0)
        .expect("page alternatives");
    assert_eq!(resolutions.len(), 3, "{resolutions:?}");

    let first = &resolutions[0];
    assert_eq!(
        first
            .description
            .as_ref()
            .map(|a| (a.source, a.text.as_str())),
        Some((
            AlternativeTextSource::StructureElementAlt,
            "structured description",
        )),
        "空 Span /Alt 降级后由结构元素 /Alt 胜出"
    );
    assert_eq!(
        first.replacement.as_ref().map(|a| a.source),
        Some(AlternativeTextSource::MarkedContentActualText),
        "结构元素没有对应替换竞争时 Span /ActualText 胜出"
    );
    assert!(first.problems.contains(&AlternativeTextProblem::Empty {
        source: AlternativeTextSource::MarkedContentAlt,
    }));
    assert_eq!(
        first.description.as_ref().and_then(|a| a.language.as_ref()),
        Some(&language("en-US")),
        "结构内容 prevailing 语言经 14.9.2.3 解析后随条目返回"
    );

    let second = &resolutions[1];
    assert_eq!(
        second.description.as_ref().map(|a| a.source),
        Some(AlternativeTextSource::StructureElementAlt),
        "非 Span 标签但带 /MCID：结构元素候选照常采集"
    );
    assert!(second.problems.is_empty());

    let third = &resolutions[2];
    assert_eq!(
        third
            .description
            .as_ref()
            .map(|a| (a.source, a.text.as_str())),
        Some((AlternativeTextSource::MarkedContentAlt, "plain span",)),
        "不在结构层级（无 /MCID）时 Span /Alt 独立胜出，不借用他段元素"
    );

    let out_of_range = doc.page_marked_content_alternatives(99);
    assert!(matches!(
        out_of_range,
        Err(MarkedContentLanguageError::MarkedContent(_))
            | Err(MarkedContentLanguageError::Language(_))
    ));
}

/// media clip `/Alt`（文本串或多语言数组）：选中后入描述槽；空/空白/
/// 残缺转义/数组非法都是类型化问题。
#[test]
fn media_clip_alt_resolves_multi_language_selection_and_problems() {
    use crate::core::reader::accessibility::{AlternateText, media_clip_alternatives};

    let prevailing = language("fr-FR");
    let parsed = |value: Value| AlternateText::parse(&value).expect("parse alt");

    let alt = parsed(multi_language(&[("en", "English text")]));
    assert_eq!(
        media_clip_alternatives(&alt, Some(&prevailing)),
        AlternativesResolution::default(),
        "fr-FR 选不中任何条目：无候选、无问题"
    );

    let alt = parsed(multi_language(&[("fr", "  ")]));
    let resolution = media_clip_alternatives(&alt, Some(&language("fr")));
    assert_eq!(resolution.description, None);
    assert_eq!(
        resolution.problems,
        vec![AlternativeTextProblem::WhitespaceOnly {
            source: AlternativeTextSource::MediaClipAlt,
        }],
        "多语言数组选中的纯空白文本降级，不静默胜出"
    );

    let alt = parsed(multi_language(&[("fr", "Description en français")]));
    let resolution = media_clip_alternatives(&alt, Some(&language("fr")));
    assert_eq!(
        resolution.description.map(|a| (a.source, a.text)),
        Some((
            AlternativeTextSource::MediaClipAlt,
            "Description en français".to_owned(),
        ))
    );

    // 残缺转义必须走 UTF-16BE：PDFDocEncoding 的 0x1B 是 U+02D9 而非 ESC
    // （见 language_escape 模块文档），只有 UTF-16 串里 U+001B 才可达。
    let alt = parsed(Value::HexString(
        "FEFF001B006A0061".to_owned(), // UTF-16BE「ESC j a」，未闭合
    ));
    let resolution = media_clip_alternatives(&alt, None);
    assert_eq!(resolution.description, None);
    assert!(matches!(
        &resolution.problems[..],
        [AlternativeTextProblem::Malformed {
            source: AlternativeTextSource::MediaClipAlt,
            ..
        }]
    ));

    // 数组形态非法（奇数项）由采集层（MediaClipData 载体）转成
    // Malformed 候选，不 panic。
    use crate::core::reader::rendition::{MediaClipData, MediaClipDataSource};
    let clip = MediaClipData {
        name: None,
        data: MediaClipDataSource::Stream(Value::Integer(0)),
        content_type: None,
        permissions: None,
        alt: Some(arr(vec![name("en")])),
        players: None,
        mh: None,
        be: None,
    };
    let resolution = clip.resolved_alternative_text(None);
    assert_eq!(resolution.description, None);
    assert!(matches!(
        &resolution.problems[..],
        [AlternativeTextProblem::Malformed {
            source: AlternativeTextSource::MediaClipAlt,
            ..
        }]
    ));
}

/// 单页文档：目录 `/Lang`、结构树根（含父树）与带 `/StructParents` 的页面。
fn alternative_page_document(content: &[u8], root: Dict, element: Dict) -> Document {
    let mut doc = Document::new();
    insert(&doc, ROOT, Value::Dict(root));
    insert(&doc, ELEMENT, Value::Dict(element));
    insert(
        &doc,
        PAGES,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGE, 0)]))
                .insert("Count", Value::Integer(1)),
        ),
    );
    insert(
        &doc,
        PAGE,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(PAGES, 0))
                .insert("StructParents", Value::Integer(0))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
                ),
        ),
    );
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES, 0))
                .insert("StructTreeRoot", Value::Ref(ROOT, 0))
                .insert("Lang", text("en-US")),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}
