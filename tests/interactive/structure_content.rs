//! 结构标记内容写出 → 解析 → 回读往返（ISO 32000-1 14.8.2、14.8.2.3.1、14.7.4）。
//!
//! 页面内容流以 `/P << /MCID n >> BDC` 标记两段真实内容，结构树用 MCR 子项
//! 声明同一页的 MCID 0 与 1：解析后页面 `/StructParents`、目录 `/ParentTree`
//! 与页上标记内容三者对齐，14.8.2 的归属诊断在写端产物上为空，文本提取
//! 不受标记影响。
//!
//! 另一条往返覆盖 14.8.2.3.1 的 tag suspects：页上混入
//! `/TagSuspect << /TagSuspect /Ordering >> BDC` 且目录写出
//! `/MarkInfo << /Suspects true >>`，回读后属性表形态、`/Suspects` 声明与
//! 归属诊断的空结果同时成立。
//!
//! 第三条往返覆盖 14.7.4.3 的 OBJR 内容项（Table 325）：同一结构元素同时
//! 挂 MCR 与注解对象引用，写出 `/K` 的 `/Type /OBJR` 字典、注解字典的
//! `/StructParent`（Table 326）与父树的单元型条目；回读后 `/Obj` 指向该
//! 注解对象、`/Pg` 与 MCR 同页、`/ParentTreeNextKey` 覆盖页面键 + OBJR 键，
//! 页面 MCID 归属与文本提取不受影响。
//!
//! 第四条起覆盖 14.8.2.2.1 的注解归属条件（14.7.4.3 与 14.7.4.4）：
//! 带 `/AP /N` 常态外观的可见注解被 OBJR 正确声称时七条诊断为空；
//! 去掉声称恰好命中 `VisibleAnnotationNotInStructure`；置 Hidden 位
//! 的注解即使带外观流也不要求入结构。文本提取在三条往返中不变。
//!
//! 第五条往返覆盖 14.7.4.4 NOTE 的自动 MCID 编号通道
//! （`begin_auto_structure_content`）：按页从 0 稠密递增、跳过显式已占用
//! 值，回读后每页父树数组无 `null` 空洞并与结构树 MCR 对齐，
//! `/ParentTreeNextKey` 严格大于全部在用父树键。

use super::*;

#[test]
fn builder_structure_marked_content_roundtrip_links_parent_tree() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::structure::StructureKid;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "First line");
    builder.end_structure_content(page);
    builder.begin_structure_content(page, "P", 1);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "Second line");
    builder.end_structure_content(page);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("Span").with_marked_content(page, 1)),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build tagged page with /MCID marked content");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/P << /MCID 0 >> BDC",
        "/P << /MCID 1 >> BDC",
        "/StructParents 0",
        "/ParentTreeNextKey 1",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    // arena 前序：0 = Document、1 = P、2 = Span。
    let document = &tree.elements()[0];
    assert!(
        matches!(
            document.kids[..],
            [StructureKid::Element(1), StructureKid::Element(2)]
        ),
        "{:?}",
        document.kids
    );
    for (element, mcid) in [(1, 0), (2, 1)] {
        match &tree.elements()[element].kids[..] {
            [StructureKid::MarkedContent(mcr)] => {
                assert_eq!(mcr.mcid, mcid, "元素 {element} 的 MCR MCID");
                assert!(mcr.page.is_some(), "MCR 应带显式 /Pg");
            }
            other => panic!("元素 {element} 应只有一个 MCR，实际 {other:?}"),
        }
    }
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0, "唯一带标记内容的页面得到父树键 0");
    assert_eq!(parents.parents, vec![Some(1), Some(2)]);
    assert_eq!(tree.mcid_parent(parents.key, 0), Some(1));
    assert_eq!(tree.mcid_parent(parents.key, 1), Some(2));
    assert_eq!(tree.mcid_parent(parents.key, 2), None);

    let sections = doc.page_marked_content(page).expect("marked content");
    let paragraphs: Vec<Option<i64>> = sections
        .iter()
        .filter(|section| section.tag == "P")
        .map(|section| {
            section
                .properties
                .as_ref()
                .and_then(|dict| dict.get("MCID"))
                .and_then(|value| value.as_integer())
        })
        .collect();
    assert_eq!(paragraphs, vec![Some(0), Some(1)], "两段 /P 各带 MCID");

    assert_eq!(
        doc.artifact_attribution_violations(page)
            .expect("diagnose writer produced page"),
        vec![],
        "标记内容已通过 /MCID 挂上结构树，归属诊断应为空"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("First line\nSecond line"),
        "标记内容段内的文本照常提取"
    );
}

#[test]
fn builder_tag_suspect_roundtrip_keeps_document_conforming() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::types::{PdfVersion, Value};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "First line");
    builder.end_structure_content(page);
    // 页序无法判定的一段内容：14.8.2.3.1 的 tag suspect。
    builder.begin_tag_suspect(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "Second line");
    builder.end_tag_suspect(page);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0)),
        ),
    );
    builder.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_suspects(true),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with structure content and tag suspect");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/P << /MCID 0 >> BDC",
        "/TagSuspect << /TagSuspect /Ordering >> BDC",
        "/StructParents 0",
        "/MarkInfo <<",
        "/Suspects true",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let info = doc
        .mark_info()
        .expect("mark info")
        .expect("/MarkInfo present");
    assert!(info.marked, "文档应声明符合标记内容约定");
    assert!(
        info.suspects,
        "含 tag suspects 的文档必须声明 /Suspects true"
    );

    let sections = doc.page_marked_content(page).expect("marked content");
    let suspects: Vec<Option<&str>> = sections
        .iter()
        .filter(|section| section.tag == "TagSuspect")
        .map(|section| {
            section
                .properties
                .as_ref()
                .and_then(|dict| dict.get("TagSuspect"))
                .and_then(Value::as_name)
        })
        .collect();
    assert_eq!(suspects, vec![Some("Ordering")], "唯一一段 tag suspect");

    assert_eq!(
        doc.artifact_attribution_violations(page)
            .expect("diagnose writer produced page"),
        vec![],
        "写端产出的 tag suspect 应满足 14.8.2.3.1 的两条 shall 与真实内容豁免"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("First line\nSecond line"),
        "tag suspect 段内的文本照常提取"
    );
}

#[test]
fn builder_object_reference_roundtrip_links_struct_parent() {
    use pdf_engine::core::builder::{
        AnnotationKind, Font, PageSize, PdfAnnotation, PdfBuilder, StructureElement,
        StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::structure::{ParentTreeEntry, StructureKid};
    use pdf_engine::core::types::{PdfVersion, Value};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Tagged line");
    builder.end_structure_content(page);
    let annotation = builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 660.0, 92.0, 680.0])
            .with_contents("Figure note"),
    );
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document").with_element(
                StructureElement::new("P")
                    .with_marked_content(page, 0)
                    .with_object_reference(annotation),
            ),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with OBJR structure content");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/Type /OBJR",
        "/StructParent 1",
        "/StructParents 0",
        "/ParentTreeNextKey 2",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    // arena 前序：0 = Document、1 = P。
    let annotations = doc.page_annotations(page).expect("page annotations");
    assert_eq!(annotations.len(), 1, "页上唯一注解");
    let element = &tree.elements()[1];
    let (mcr, objr) = match element.kids.as_slice() {
        [
            StructureKid::MarkedContent(mcr),
            StructureKid::ObjectReference(objr),
        ] => (mcr, objr),
        other => panic!("元素 1 应为 MCR + OBJR 两个内容项，实际 {other:?}"),
    };
    assert_eq!(
        objr.object,
        annotations[0].reference.expect("indirect annotation"),
        "/Obj 指向注解对象本身"
    );
    assert_eq!(
        objr.dict.get("Type"),
        Some(&Value::Name("OBJR".into())),
        "OBJR 字典带 /Type /OBJR"
    );
    let objr_page = objr.page.expect("OBJR 带显式 /Pg");
    assert_eq!(objr_page, mcr.page.expect("MCR 带显式 /Pg"));
    let page_dict = doc
        .get_object(objr_page.0)
        .expect("read /Pg object")
        .expect("/Pg object exists");
    assert_eq!(
        page_dict
            .value
            .as_dict()
            .and_then(|dict| dict.get("Type"))
            .and_then(Value::as_name),
        Some("Page"),
        "/Pg 解析到页对象"
    );
    assert_eq!(
        annotations[0]
            .dict
            .get("StructParent")
            .and_then(Value::as_integer),
        Some(1),
        "注解字典追加父树键 1（页面键 0 之后）"
    );

    assert!(
        matches!(tree.parent_tree_entry(1), Some(ParentTreeEntry::Object(_)),),
        "OBJR 键的父树条目是单元引用，实际 {:?}",
        tree.parent_tree_entry(1)
    );
    assert_eq!(tree.object_parent(1), Some(1), "单元引用指向元素 P");
    assert_eq!(tree.mcid_parent(1, 0), None, "单元条目不参与 MCID 查找");

    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0, "唯一带标记内容的页面得到父树键 0");
    assert_eq!(parents.parents, vec![Some(1)]);
    assert_eq!(tree.mcid_parent(0, 0), Some(1), "页面 MCID 归属仍成立");
    assert_eq!(
        tree.root().parent_tree_next_key,
        Some(2),
        "/ParentTreeNextKey 覆盖页面键与 OBJR 键"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("Tagged line"),
        "OBJR 内容项不影响页文本提取"
    );
}

/// 带标记内容与一条 `/AP /N` 可见 Text 注解的整页写出。
///
/// `claim` 为真时 P 元素同时以 OBJR 声称该注解（写端会追加
/// `/StructParent` 与父树单元条目）；`hidden` 为真时注解 `/F` 置
/// Hidden 位（Table 165 bit 1）。返回写出的 PDF 字节。
fn build_annotation_appearance_page(claim: bool, hidden: bool) -> Vec<u8> {
    use pdf_engine::core::builder::{
        AnnotationKind, Font, FormXObject, PageSize, PdfAnnotation, PdfBuilder, StructureElement,
        StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Tagged line");
    builder.end_structure_content(page);
    let appearance = builder.form_xobject(&FormXObject::new(
        [72.0, 660.0, 92.0, 680.0],
        ContentStream::new(),
    ));
    let mut annotation = PdfAnnotation::new(AnnotationKind::Text, [72.0, 660.0, 92.0, 680.0])
        .with_contents("Figure note")
        .with_appearance(appearance);
    if hidden {
        annotation = annotation.with_flags(2);
    }
    let handle = builder.add_annotation(page, annotation);
    let mut paragraph = StructureElement::new("P").with_marked_content(page, 0);
    if claim {
        paragraph = paragraph.with_object_reference(handle);
    }
    builder.set_structure_tree(
        StructureTree::new()
            .with_element(StructureElement::new("Document").with_element(paragraph)),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build tagged page with appearance-stream annotation");
    buf
}

/// 可见注解被 OBJR 正确声称的往返：14.8.2.2.1 的三条件齐备
/// （`/AP /N`、未置 Hidden、纳入逻辑结构），回读后归属诊断整体为空，
/// 既有标记内容与文本提取不受影响。
#[test]
fn builder_visible_annotation_object_reference_roundtrip_keeps_document_conforming() {
    let buf = build_annotation_appearance_page(true, false);
    let doc = parse_buf(&buf);

    let annotations = doc.page_annotations(0).expect("page annotations");
    assert_eq!(annotations.len(), 1, "页上唯一注解");
    assert!(
        annotations[0]
            .appearance
            .as_ref()
            .is_some_and(|streams| streams.normal.is_some()),
        "回读注解应带常态 (N) 外观流"
    );
    assert!(!annotations[0].flags.hidden(), "/F 未置 Hidden 位");
    assert_eq!(
        doc.artifact_attribution_violations(0)
            .expect("diagnose writer produced page"),
        vec![],
        "可见注解经 OBJR + /StructParent + 父树单元条目正确声称，七条归属诊断应为空"
    );
    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("Tagged line"),
        "注解外观与 OBJR 声称不影响页文本提取"
    );
}

/// 去掉 OBJR 声称的同一文档：可见注解不满足 14.8.2.2.1 第三个条件，
/// 回读后应恰好命中一条 `VisibleAnnotationNotInStructure`（should 级），
/// 其余规则保持干净。
#[test]
fn builder_visible_annotation_without_object_reference_roundtrip_reports_missing_structure() {
    use pdf_engine::core::reader::accessibility::{
        ArtifactAttributionRule, ArtifactAttributionViolation,
    };

    let buf = build_annotation_appearance_page(false, false);
    let doc = parse_buf(&buf);

    let found = doc
        .artifact_attribution_violations(0)
        .expect("diagnose writer produced page");
    assert_eq!(
        found,
        vec![ArtifactAttributionViolation {
            page: 0,
            section: None,
            element: None,
            kid: None,
            annotation: Some(0),
            rule: ArtifactAttributionRule::VisibleAnnotationNotInStructure,
        }],
        "未被逻辑结构纳入的可见注解应只记一条规则 6 发现，annotation 为 /Annots 下标"
    );
    assert!(
        !found[0].rule.is_mandatory(),
        "14.8.2.2.1 的注解条件派生自定义式表述，应为 should 级"
    );
    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("Tagged line"),
        "文本提取不受归属诊断影响"
    );
}

/// Hidden 注解即使带 `/AP /N` 也不属于真实内容（14.8.2.2.1 第二条件
/// 不成立）：不声称、不置结构，回读后也没有注解归属发现。
#[test]
fn builder_hidden_annotation_appearance_roundtrip_reports_no_annotation_finding() {
    let buf = build_annotation_appearance_page(false, true);
    let doc = parse_buf(&buf);

    let annotations = doc.page_annotations(0).expect("page annotations");
    assert!(annotations[0].flags.hidden(), "回读注解应带 Hidden 位");
    assert!(
        annotations[0]
            .appearance
            .as_ref()
            .is_some_and(|streams| streams.normal.is_some()),
        "外观流仍在，只是可见性条件不成立"
    );
    assert_eq!(
        doc.artifact_attribution_violations(0)
            .expect("diagnose writer produced page"),
        vec![],
        "Hidden 注解不要求进入逻辑结构，归属诊断应为空"
    );
}

/// 自动编号通道往返（14.7.4.4 NOTE：标识符应尽可能小）：两页用
/// `begin_auto_structure_content` 取回自动分配的 MCID 写进结构树 MCR，
/// 页 1 混用一次显式 MCID 0 后自动分配跳过取 1。解析后每页父树数组
/// `0..n-1` 全部 Some 无 `null` 空洞、与结构树 MCR 逐项对齐，
/// `/ParentTreeNextKey` 严格大于全部在用键，文本提取不受标记影响。
#[test]
fn builder_auto_structure_content_roundtrip_keeps_parent_tree_dense_and_next_key_ahead() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = builder.page(PageSize::A4);
    let second = builder.page(PageSize::A4);
    let a0 = builder
        .begin_auto_structure_content(first, "P")
        .expect("first page auto MCID");
    builder.text(first, &Font::helvetica(12.0), 72.0, 700.0, "First line");
    builder.end_structure_content(first);
    let a1 = builder
        .begin_auto_structure_content(first, "Span")
        .expect("second auto MCID on first page");
    builder.text(first, &Font::helvetica(12.0), 72.0, 680.0, "Second line");
    builder.end_structure_content(first);
    // 显式占用 0 后，自动编号取「显式 ∪ 已分配」之外的最小整数 1。
    builder.begin_structure_content(second, "P", 0);
    builder.text(second, &Font::helvetica(12.0), 72.0, 700.0, "Explicit line");
    builder.end_structure_content(second);
    let b1 = builder
        .begin_auto_structure_content(second, "Span")
        .expect("auto MCID skips the explicit 0");
    builder.text(second, &Font::helvetica(12.0), 72.0, 680.0, "Auto line");
    builder.end_structure_content(second);
    assert_eq!(
        (a0, a1, b1),
        (0, 1, 1),
        "自动编号按页从 0 稠密递增且跳过显式值"
    );

    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(first, a0))
                .with_element(StructureElement::new("Span").with_marked_content(first, a1))
                .with_element(StructureElement::new("P").with_marked_content(second, 0))
                .with_element(StructureElement::new("Span").with_marked_content(second, b1)),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build pages with auto-numbered structure content");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/P << /MCID 0 >> BDC",
        "/Span << /MCID 1 >> BDC",
        "/StructParents 0",
        "/StructParents 1",
        "/ParentTreeNextKey 2",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    // arena 前序：0 = Document、1 = P、2 = Span（页 0）；3 = P、4 = Span（页 1）。
    // 自动编号使两页父树数组 0..n-1 全部命中，无任何 null 空洞。
    let first_parents = doc
        .page_struct_parents(first)
        .expect("first page parents")
        .expect("/StructParents");
    assert_eq!(first_parents.key, 0, "页序首个带标记内容的页面得键 0");
    assert_eq!(first_parents.parents, vec![Some(1), Some(2)]);
    assert!(
        first_parents.parents.iter().all(|slot| slot.is_some()),
        "自动编号页面数组不应有 null 空洞: {:?}",
        first_parents.parents
    );
    let second_parents = doc
        .page_struct_parents(second)
        .expect("second page parents")
        .expect("/StructParents");
    assert_eq!(second_parents.key, 1, "第二页得键 1");
    assert_eq!(second_parents.parents, vec![Some(3), Some(4)]);
    assert!(
        second_parents.parents.iter().all(|slot| slot.is_some()),
        "显式 0 + 自动 1 混用页同样稠密: {:?}",
        second_parents.parents
    );
    assert_eq!(tree.mcid_parent(0, 0), Some(1));
    assert_eq!(tree.mcid_parent(0, 1), Some(2));
    assert_eq!(tree.mcid_parent(1, 0), Some(3));
    assert_eq!(tree.mcid_parent(1, 1), Some(4));
    assert_eq!(tree.mcid_parent(0, 2), None, "稠密区间之外无条目");

    // shall：NextKey 严格大于全部在用父树键，且等于「新条目取当前键再自增」
    // 后的下一个可用值。
    let next_key = tree
        .root()
        .parent_tree_next_key
        .expect("/ParentTreeNextKey present");
    assert_eq!(next_key, 2);
    for key in tree.root().parent_tree.keys() {
        assert!(
            *key < next_key,
            "在用父树键 {key} 必须小于 /ParentTreeNextKey {next_key}"
        );
    }

    assert_eq!(
        doc.extract_page_text(first).as_deref(),
        Some("First line\nSecond line"),
        "自动编号标记段内的文本照常提取"
    );
    assert_eq!(
        doc.extract_page_text(second).as_deref(),
        Some("Explicit line\nAuto line"),
        "混用页文本照常提取"
    );
}
