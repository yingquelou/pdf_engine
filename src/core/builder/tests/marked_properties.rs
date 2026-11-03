//! 标记内容属性表句柄通路（ISO 32000-1 14.6）的单元测试。
//!
//! 覆盖：注册 → 句柄 → 装配期取号 → 页面内容写成真实间接引用 → 用本库解析器
//! 回读；共享句柄只产出一个对象；未注册句柄在任何字节写出之前被
//! `WriteError::InvalidObject` 拒绝；注册但未使用的句柄不产生对象；手写裸号
//! 通路与句柄通路互不污染；手写引用指向池内对象号但代号非 0 时在装配期被
//! 拒绝；占位形态绕过装配直达序列化时被类型化拒绝（不 panic、绝不输出
//! `<句柄> -1 R`）。
//!
//! 写出形态 `/Tag <号> 0 R BDC` 是互操作容错/本库自有写法，不是 ISO 授权的
//! 操作数形态（14.6.2 要求含间接引用的属性表做成 `/Properties` 命名资源）。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::content::{ContentWriteError, MarkedContentProperties};
use crate::core::reader::content::Op as ReadOp;

/// 写出并重新解析；返回原始字节供文本串形态断言。
fn build_and_parse(b: &mut PdfBuilder) -> (Vec<u8>, Document) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-marked-properties-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    (out, doc)
}

/// 一张只带 `/ActualText` 的属性表（本通道原样保留字典，不建模字段）。
fn properties(text: &str) -> Dict {
    Dict::new().insert("ActualText", Value::String(text.to_string()))
}

/// 页面内容里所有 `BDC` 的间接引用操作数：`(标签, 对象号, 代号)`，按出现
/// 顺序。读端保留操作数原形，因此这是「内容流里到底写了哪个号」的直接证据。
fn marked_references(doc: &Document, page: usize) -> Vec<(String, i64, i64)> {
    doc.page_content_ops(page)
        .expect("page content ops")
        .iter()
        .filter_map(|op| match op {
            ReadOp::BeginMarkedContentWithProperties(tag, Value::Ref(object, generation)) => {
                Some((tag.clone(), *object, *generation))
            }
            _ => None,
        })
        .collect()
}

/// 指定标签的引用 `(对象号, 代号)`；同名多处时取第一个。
fn reference_of(doc: &Document, page: usize, tag: &str) -> (i64, i64) {
    marked_references(doc, page)
        .into_iter()
        .find(|(name, _, _)| name == tag)
        .map(|(_, object, generation)| (object, generation))
        .unwrap_or_else(|| panic!("page {page} has no BDC reference for tag {tag:?}"))
}

/// 整档里值恰好等于该属性表字典的间接对象号（含对象流内对象）；用于断言
/// 「共享句柄只写出一个对象」。
fn holders_of(doc: &Document, dict: &Dict) -> Vec<i64> {
    (1..64)
        .filter(|num| {
            doc.get_object(*num)
                .ok()
                .flatten()
                .is_some_and(|object| object.value == Value::Dict(dict.clone()))
        })
        .collect()
}

/// 页面字典 `/Annots` 数组里的注解对象号（按数组序）。
fn annotation_nums(doc: &Document) -> Vec<i64> {
    for num in 1..64 {
        let Ok(Some(object)) = doc.get_object(num) else {
            continue;
        };
        let Value::Dict(dict) = &object.value else {
            continue;
        };
        if dict.get("Type") != Some(&Value::Name("Page".into())) {
            continue;
        }
        let Some(Value::Array(annots)) = dict.get("Annots") else {
            return Vec::new();
        };
        return annots
            .0
            .iter()
            .filter_map(|value| match value {
                Value::Ref(object, _) => Some(*object),
                _ => None,
            })
            .collect();
    }
    Vec::new()
}

/// 注册的句柄在装配期拿到实际对象号：被引用的对象真实存在且逐字等于注册时
/// 那张字典，页面内容写成 `<分配号> 0 R`（代号如实为 0），声明式读 API 能把
/// 它解回属性表，并且 `EMC` 闭合了这一段标记内容。
#[test]
fn registered_property_handle_writes_an_allocated_indirect_reference() {
    let mut b = PdfBuilder::new();
    let page = b.page(PageSize::A4);
    let handle = b.add_marked_content_properties(properties("allocated text"));
    b.begin_marked_properties(page, "P", handle)
        .end_marked_properties(page);

    let (out, doc) = build_and_parse(&mut b);
    let text = String::from_utf8_lossy(&out).into_owned();

    // ① 被引用的对象存在、是字典、与原注册字典完全一致。
    let references = marked_references(&doc, page);
    assert_eq!(references.len(), 1, "只有一段标记内容：{references:?}");
    assert_eq!(references[0].0, "P");
    assert_eq!(references[0].2, 0, "回填的引用代号固定为 0");
    let object = references[0].1;
    let referenced = doc
        .get_object(object)
        .expect("object lookup")
        .expect("属性表必须作为间接对象写出");
    assert_eq!(
        referenced.value,
        Value::Dict(properties("allocated text")),
        "字典要逐字写出，不做字段级建模"
    );

    // ② 页面内容流里是实际分配的对象号，而不是句柄本身，也不是占位代号。
    // 单页、无字体/注解的最小文档：Catalog=1、Pages=2、Page=3、内容流=4，
    // 属性表拿游标的下一个号 5——对象号来自装配期游标（builder/mod.rs 顶部
    // 「对象号布局」文档），不是句柄值本身。
    assert_eq!(
        object, 5,
        "最小文档里句柄对象号应为装配游标的 5（句柄 {handle} 不得被写成 {handle} 0 R）"
    );
    assert!(
        text.contains(&format!("/P {object} 0 R BDC")),
        "内容流要写成前缀序 `tag properties BDC`：{text}"
    );
    assert!(!text.contains("-1 R"), "未定型占位绝不能写出：{text}");

    // ③ 声明式读 API（只读参考层）把引用解回同一张属性表。
    let sections = doc.page_marked_content(page).expect("page_marked_content");
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].tag, "P");
    assert_eq!(
        sections[0].properties,
        Some(properties("allocated text")),
        "引用解引用后应与内联字典走同一登记结果"
    );
    assert_eq!(sections[0].actual_text().as_deref(), Some("allocated text"));
    assert!(!sections[0].point, "BDC 是区段而非点标记");

    // EMC 闭合：区段末尾的下一个操作符是 EMC，且内容流里只有一个 EMC。
    let ops = doc.page_content_ops(page).expect("page content ops");
    assert_eq!(ops[sections[0].end - 1], ReadOp::EndMarkedContent);
    assert_eq!(
        ops.iter()
            .filter(|op| **op == ReadOp::EndMarkedContent)
            .count(),
        1
    );
}

/// 同一句柄被多处引用（两个页面 + 区段 + 点标记）只分配一个对象，所有引用
/// 指向同一个对象号；字典也只写出一次。
#[test]
fn shared_property_handle_yields_one_object_referenced_by_all_sections() {
    let mut b = PdfBuilder::new();
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let handle = b.add_marked_content_properties(properties("shared text"));
    b.begin_marked_properties(first, "P", handle)
        .end_marked_properties(first);
    b.begin_marked_properties(second, "Q", handle)
        .marked_point_properties(second, "R", handle)
        .end_marked_properties(second);

    let (out, doc) = build_and_parse(&mut b);
    let text = String::from_utf8_lossy(&out).into_owned();

    let first_object = reference_of(&doc, first, "P").0;
    let second_object = reference_of(&doc, second, "Q").0;
    assert_eq!(first_object, second_object, "共享句柄必须复用同一对象号");

    // 整份文档里只有一个对象的值等于这张属性表（对象可能写在对象流内，
    // 故按解析后的对象表计数，而不是数 `N 0 obj` 文本）。
    assert_eq!(
        holders_of(&doc, &properties("shared text")),
        vec![first_object],
        "同一句柄只应产出一个间接对象：{text}"
    );
    assert_eq!(
        text.match_indices("<< /ActualText (shared text) >>")
            .count(),
        1,
        "字典字节也只出现一次：{text}"
    );

    // 两页的区段都解引用到同一张字典；第二页的 DP 点标记同样解出属性表。
    let first_sections = doc.page_marked_content(first).expect("first page");
    let second_sections = doc.page_marked_content(second).expect("second page");
    assert_eq!(
        first_sections[0].properties,
        Some(properties("shared text"))
    );
    assert_eq!(
        second_sections
            .iter()
            .filter(|section| section.properties.is_some())
            .count(),
        2,
        "BDC 与 DP 两处引用都应解出属性表"
    );
    assert!(
        second_sections.iter().any(|section| section.point),
        "DP 点标记应作为 point 区段出现"
    );
}

/// 未注册（越界）句柄在写出任何字节之前被拒绝，输出零字节；错误说明点名
/// 「never registered」。同页里合法句柄的存在不影响拒绝。
#[test]
fn unknown_property_handle_is_rejected_without_bytes() {
    let mut b = PdfBuilder::new();
    let page = b.page(PageSize::A4);
    let handle = b.add_marked_content_properties(properties("registered"));
    b.begin_marked_properties(page, "P", handle)
        .end_marked_properties(page);
    b.begin_marked_properties(page, "Q", handle + 1)
        .end_marked_properties(page);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("未注册句柄必须被拒绝");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "期望 InvalidObject，得到 {error:?}"
    );
    assert!(
        error.to_string().contains("never registered"),
        "错误应说明句柄未注册：{error}"
    );
    assert!(
        error.to_string().contains("handle"),
        "错误应点名句柄：{error}"
    );
    assert_eq!(out.len(), 0, "零字节契约：{out:?}");
}

/// 注册但从未被内容流引用的句柄不分配对象号、不写出对象：输出与不注册时
/// 逐字节相同（与可选内容组/渐变的「只为实际引用取号」一致，取舍依据见
/// `assemble::marked_properties` 模块文档规则 2）。
#[test]
fn registered_but_unused_property_handle_writes_no_object() {
    let mut with_unused = PdfBuilder::new();
    let page = with_unused.page(PageSize::A4);
    with_unused.add_marked_content_properties(properties("never referenced"));

    let mut baseline = PdfBuilder::new();
    baseline.page(PageSize::A4);
    assert_eq!(page, 0);

    let mut unused_out = Vec::new();
    with_unused
        .build(&mut unused_out)
        .expect("build with unused handle");
    let mut baseline_out = Vec::new();
    baseline.build(&mut baseline_out).expect("baseline build");
    assert_eq!(
        unused_out, baseline_out,
        "未使用的句柄不应改变任何输出字节（不占对象号、不写对象）"
    );
}

/// 两条通路共存：调用方手写的裸号引用（本车道不校验其存在性，见
/// `add_marked_content_properties` 文档）与句柄通路互不污染——句柄拿到的对象
/// 号仍来自装配期游标，文档照样能被本库解析器读回。
#[test]
fn hand_written_reference_and_handle_channels_do_not_collide() {
    let mut b = PdfBuilder::new();
    let page = b.page(PageSize::A4);
    let handle = b.add_marked_content_properties(properties("via handle"));
    // 手写裸号：指向一个文档里并不存在的对象（99），调用方自担其责。
    b.pages[page]
        .content
        .begin_marked_content_reference("Raw", 99, 0)
        .end_marked_content();
    b.begin_marked_properties(page, "P", handle)
        .end_marked_properties(page);

    let (out, doc) = build_and_parse(&mut b);
    let text = String::from_utf8_lossy(&out).into_owned();
    let references = marked_references(&doc, page);
    assert_eq!(
        references.iter().map(|(tag, ..)| tag).collect::<Vec<_>>(),
        vec!["Raw", "P"],
        "两条通路各写各的操作数：{references:?}"
    );
    let object = reference_of(&doc, page, "P").0;

    // 游标没有被手写号污染：句柄拿到的号仍来自顺序取号，且对象真实存在；
    // 手写的 99 原样保留（本车道不校验裸号，见通路文档）。
    assert_ne!(object, 99, "句柄对象号不应等于手写号");
    assert!(
        object < 99,
        "句柄通路的对象号应来自装配期游标，而不是被 99 带偏：{object}"
    );
    assert_eq!(references[0].1, 99, "手写裸号原样写出");
    assert!(text.contains(&format!("/P {object} 0 R BDC")));
    assert!(text.contains("/Raw 99 0 R BDC"), "手写通路原样保留：{text}");
    assert_eq!(holders_of(&doc, &properties("via handle")), vec![object]);

    let sections = doc.page_marked_content(page).expect("page_marked_content");
    assert_eq!(sections.len(), 2);
    // 手写裸号悬空 → 读端容错降级为 properties = None（不报错）。
    assert_eq!(sections[0].tag, "Raw");
    assert_eq!(sections[0].properties, None);
    // 句柄通路的那段解引用成功。
    assert_eq!(sections[1].tag, "P");
    assert_eq!(
        sections[1].properties,
        Some(properties("via handle")),
        "同一文档里两条通路各自成立"
    );
}

/// 代数一致性（`assemble::marked_properties` 模块文档规则 5）：手写裸号引用
/// 指向属性表池刚分配的对象号、但代号不是该字典写出用的 0 时，装配期在替换
/// 占位之前拒绝，零字节输出——那个代号下并没有对象可指，静默写出只会产出
/// 错误引用。先用同一形状的成功构建固定池对象号，避免把断言押在游标常量上。
#[test]
fn reference_to_pool_object_with_nonzero_generation_is_rejected_without_bytes() {
    // ① 同形状（单页 + 一个被引用句柄）先成功构建，固定句柄拿到的对象号。
    let mut probe = PdfBuilder::new();
    let probe_page = probe.page(PageSize::A4);
    let probe_handle = probe.add_marked_content_properties(properties("allocated text"));
    probe
        .begin_marked_properties(probe_page, "P", probe_handle)
        .end_marked_properties(probe_page);
    let (_, probe_doc) = build_and_parse(&mut probe);
    let pool_object = reference_of(&probe_doc, probe_page, "P").0;

    // ② 相同形状 + 一条指向池内对象号但代号为 7 的手写引用 → 装配期拒绝。
    let mut b = PdfBuilder::new();
    let page = b.page(PageSize::A4);
    let handle = b.add_marked_content_properties(properties("allocated text"));
    b.pages[page]
        .content
        .begin_marked_content_reference("X", pool_object, 7)
        .end_marked_content();
    b.begin_marked_properties(page, "P", handle)
        .end_marked_properties(page);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("指向池内对象号的非 0 代号必须被拒绝");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "期望 InvalidObject，得到 {error:?}"
    );
    assert!(
        error.to_string().contains("generation"),
        "错误应点名代号不一致：{error}"
    );
    assert_eq!(out.len(), 0, "零字节契约：{out:?}");
}

/// 对象号与既有分配不冲突：同一文档里注解先占号、属性表句柄随后取号，两者
/// 号互异且各自可解——属性表引用解回注册字典（不是注解字典），注解仍是
/// `/Annots` 指向的那本字典（共享游标，见模块文档规则 4）。
#[test]
fn marked_property_objects_do_not_collide_with_annotation_object_numbers() {
    let mut b = PdfBuilder::new();
    let page = b.page(PageSize::A4);
    b.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [10.0, 10.0, 30.0, 30.0]),
    );
    let handle = b.add_marked_content_properties(properties("no collision"));
    b.begin_marked_properties(page, "P", handle)
        .end_marked_properties(page);

    let (out, doc) = build_and_parse(&mut b);
    let text = String::from_utf8_lossy(&out).into_owned();
    let object = reference_of(&doc, page, "P").0;
    let annot_nums = annotation_nums(&doc);
    assert_eq!(annot_nums.len(), 1, "页面应登记一个注解引用");
    let annot_num = annot_nums[0];
    assert_ne!(
        object, annot_num,
        "属性表与注解不得共用对象号：{object} vs {annot_num}"
    );

    // 属性表引用解回注册字典本身。
    assert_eq!(
        doc.get_object(object)
            .expect("object lookup")
            .expect("属性表对象存在")
            .value,
        Value::Dict(properties("no collision"))
    );
    // 注解对象仍是注解字典（`/Subtype /Text`），没有被属性表字典顶替。
    let Value::Dict(annot_dict) = &doc
        .get_object(annot_num)
        .expect("object lookup")
        .expect("注解对象存在")
        .value
    else {
        panic!("注解对象应是字典");
    };
    assert_eq!(annot_dict.get("Subtype"), Some(&Value::Name("Text".into())));
    assert!(text.contains(&format!("/P {object} 0 R BDC")));
    assert_eq!(
        doc.page_marked_content(page).expect("page_marked_content")[0].properties,
        Some(properties("no collision"))
    );
}

/// 占位形态绕过装配替换直达序列化：类型化拒绝、无 panic；兜底入口
/// `to_bytes` 也不输出 `<句柄> -1 R`；正常装配仍成功。
#[test]
fn unresolved_placeholder_reaching_serialization_is_refused_without_panic() {
    let mut b = PdfBuilder::new();
    let page = b.page(PageSize::A4);
    let handle = b.add_marked_content_properties(properties("placeholder"));
    b.begin_marked_properties(page, "P", handle)
        .end_marked_properties(page);

    // 装配前直接序列化页面内容：必须返回类型化错误，而不是写出假引用。
    let error = b.pages[page]
        .content
        .try_to_bytes()
        .expect_err("未替换的占位必须被拒绝");
    assert_eq!(
        error,
        ContentWriteError::UnresolvedPropertiesHandle { handle }
    );
    assert!(
        error.to_string().contains("never resolved"),
        "错误说明应指向未解析句柄：{error}"
    );

    // 兜底入口：不 panic、不含 `-1 R` 假引用。
    let fallback = String::from_utf8_lossy(&b.pages[page].content.to_bytes()).into_owned();
    assert!(!fallback.contains("-1"), "{fallback}");

    // 占位确实是未定型形态（句柄 + 保留代号），且只此一种来源。
    let pending = MarkedContentProperties::pending_handle(handle);
    assert_eq!(
        pending,
        MarkedContentProperties::Reference {
            object: handle as i64,
            generation: MarkedContentProperties::PENDING_GENERATION,
        }
    );
    assert_eq!(pending.as_pending_handle(), Some(handle));

    // 正常装配路径把占位换成真实引用，构建成功。
    let (out, doc) = build_and_parse(&mut b);
    assert!(!out.is_empty());
    let object = reference_of(&doc, page, "P").0;
    assert_eq!(
        doc.get_object(object)
            .expect("object lookup")
            .expect("handle object exists")
            .value,
        Value::Dict(properties("placeholder")),
        "句柄通路的引用应指向注册时那张字典"
    );
    assert_eq!(
        doc.page_marked_content(page).expect("sections")[0].properties,
        Some(properties("placeholder"))
    );
    // 取号来自装配期游标（Catalog=1、Pages=2 之后），不是句柄值本身。
    assert!(
        object > 2 && object != handle as i64,
        "句柄 {handle} 不该被当成对象号：{object}"
    );
}
