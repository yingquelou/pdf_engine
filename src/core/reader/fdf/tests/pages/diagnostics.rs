//! `/Pages`（Table 248/249/250）与 `/Annots`（Table 251）**Required 条目
//! 违例**的类型化诊断测试：四张表里全部 shall 级必需条目（Table 248
//! `/Templates`、Table 249 `/TRef`、Table 250 `/Name`、Table 251 `/Page`）
//! 的缺失/形态不符都降级为 [`FdfPagesIssue`] 而**绝不**让整份 FDF 解析
//! 失败；每条 `Display` 自带条款与 raw 抽取行号；Table 243 的 `/Pages`
//! 互斥 shall（raw L24876–L24877）与 PDF 1.3 版本语义（只报告不拒绝）
//! 一并在此覆盖。公共装配辅助复用父模块。

use super::*;

#[test]
fn fdf_pages_required_entry_violations_are_typed_diagnostics() {
    // 四张表里全部的 Required 条目：Table 248 `/Templates`、Table 249
    // `/TRef`、Table 250 `/Name`、Table 251 `/Page`——违例只进诊断表。
    let page_2_templates = array(vec![template(&[("Rename", integer(1))])]);
    let page_3_templates = array(vec![
        // /TRef 非字典、/TRef 是字典但缺 /Name、/Name 非串、悬空引用
        template(&[("TRef", integer(7))]),
        template(&[("TRef", dict_value(&[("F", text("x.pdf"))]))]),
        template(&[("TRef", dict_value(&[("Name", integer(9))]))]),
        Value::Ref(99, 0),
    ]);
    let view = pages_view_of(
        array(vec![
            page(&[]),                                // 0：缺 /Templates（Required）
            page(&[("Templates", text("no-array"))]), // 1：/Templates 非数组
            page(&[("Templates", page_2_templates)]), // 2：模板缺 /TRef
            page(&[("Templates", page_3_templates)]), // 3：/TRef 与 /Name 形态不符
            integer(7),                               // 4：页项不是字典
        ]),
        Vec::new(),
    );
    let issues = pages_issues(&view);
    for expected in [
        FdfPagesIssue::PageTemplatesMissing { page: 0 },
        FdfPagesIssue::PageTemplatesMissing { page: 4 },
        FdfPagesIssue::PageNotDictionary {
            page: 4,
            found: "integer",
        },
        FdfPagesIssue::PageTemplatesNotArray {
            page: 1,
            found: "string",
        },
        FdfPagesIssue::TemplateReferenceMissing {
            page: 2,
            template: 0,
        },
        FdfPagesIssue::TemplateRenameNotBoolean {
            page: 2,
            template: 0,
            found: "integer",
        },
        FdfPagesIssue::TemplateReferenceNotDictionary {
            page: 3,
            template: 0,
            found: "integer",
        },
        FdfPagesIssue::TemplateNameMissing {
            page: 3,
            template: 1,
        },
        FdfPagesIssue::TemplateNameNotString {
            page: 3,
            template: 2,
            found: "integer",
        },
    ] {
        assert!(
            issues.contains(&expected),
            "缺少诊断 {expected:?}；实际：{issues:?}"
        );
    }
    // 悬空的模板引用无法在数组字面量里构造（`..` 需要基表达式），单独断言。
    assert!(
        issues.iter().any(|issue| matches!(
            issue,
            FdfPagesIssue::PageTemplateUnresolvable {
                page: 3,
                template: 3,
                ..
            }
        )),
        "{issues:?}"
    );
    // Required 违例都可被 is_required_entry_missing() 归类，且页仍被交出。
    assert_eq!(view.len(), 5);
    assert!(issues.iter().all(|issue| !issue.is_advisory()));
    assert!(
        issues
            .iter()
            .filter(|issue| issue.is_required_entry_missing())
            .count()
            >= 4
    );
    // 违例的模板不丢弃：`/TRef` 缺失仍是类型化模板，名字为 None。
    let broken = &view.page(2).expect("页").templates[0];
    assert_eq!(broken.name(), None);
    assert_eq!(broken.rename, None, "非布尔的 /Rename 不猜值");
    assert!(
        broken.rename_effective(),
        "缺省 true 只在条目缺席时生效；形态不符时同样不物化"
    );
}

#[test]
fn fdf_pages_issue_display_carries_clause_and_raw_line() {
    // 每条 Display 自带条款与 raw 抽取行号（与 linearization/requirements
    // 的诊断表同一手法）。
    let cases: Vec<(FdfPagesIssue, &str)> = vec![
        (
            FdfPagesIssue::PageTemplatesMissing { page: 0 },
            "Table 248 (raw L25087) marks Required",
        ),
        (
            FdfPagesIssue::TemplateReferenceMissing {
                page: 0,
                template: 0,
            },
            "Table 249 (raw L25115-L25116",
        ),
        (
            FdfPagesIssue::TemplateNameMissing {
                page: 0,
                template: 0,
            },
            "Table 250 (raw L25126) marks Required",
        ),
        (
            FdfPagesIssue::AnnotationPageMissing {
                index: 0,
                subtype: None,
            },
            "required for annotations in FDF files by Table 251",
        ),
        (
            FdfPagesIssue::AnnotationPageNegative { index: 0, page: -1 },
            "raw L25131-L25132",
        ),
        (
            FdfPagesIssue::AnnotationSubtypeProhibited {
                index: 0,
                subtype: "Widget".to_string(),
            },
            "Table 243 (raw L24887-L24889) excludes",
        ),
        (
            FdfPagesIssue::PagesVersionGate {
                declared: "1.2".to_string(),
            },
            "is a PDF 1.3 entry (Table 243 raw L24874)",
        ),
        (
            FdfPagesIssue::PagesWithFieldsOrStatus {
                fields: true,
                status: false,
            },
            "shall not be present together with /Fields",
        ),
    ];
    for (issue, citation) in cases {
        let message = issue.to_string();
        assert!(
            message.contains(citation),
            "{issue:?} 的 Display 应包含条款引用 {citation:?}：{message}"
        );
    }
}

#[test]
fn fdf_pages_exclusive_shall_with_fields_or_status_is_diagnosed() {
    // Table 243（raw L24876–L24877）「The Fields and Status entries shall
    // not be present together with this entry」：读端两侧都保留，只报告。
    let view = pages_view_of(
        array(vec![page(&[("Templates", array(vec![]))])]),
        Vec::new(),
    );
    let both = doc_with_fdf_entries(
        Dict::new()
            .insert("Pages", array(vec![page(&[("Templates", array(vec![]))])]))
            .insert("Fields", array(vec![field("a")]))
            .insert("Status", text("saved")),
        Vec::new(),
    )
    .fdf_pages()
    .expect("ok")
    .expect("视图");
    assert!(both.pages_exclusive_conflict());
    assert!(
        both.issues
            .contains(&FdfPagesIssue::PagesWithFieldsOrStatus {
                fields: true,
                status: true,
            }),
        "{:?}",
        both.issues
    );
    // `/Pages` 单独出现时没有互斥诊断。
    assert!(!view.pages_exclusive_conflict());
    // 只有 `/Status` 时报告的另一半文案不同。
    let status_only = doc_with_fdf_entries(
        Dict::new()
            .insert("Pages", array(vec![page(&[("Templates", array(vec![]))])]))
            .insert("Status", text("saved")),
        Vec::new(),
    )
    .fdf_pages()
    .expect("ok")
    .expect("视图");
    assert!(
        status_only
            .issues
            .contains(&FdfPagesIssue::PagesWithFieldsOrStatus {
                fields: false,
                status: true,
            })
    );
}

#[test]
fn fdf_pages_and_annots_version_gate_reports_below_1_3() {
    // Table 243 的 `/Pages`（L24874）与 `/Annots`（L24885）都是 PDF 1.3
    // 条目：声明版本低于 1.3 时仍照常读取，只额外记一条诊断。声明版本
    // 优先序：目录 `/Version`（Table 242）→ `%FDF-x.y` 文件头 →
    // `Document::source_version`。
    let entries = Dict::new()
        .insert("Pages", array(vec![page(&[("Templates", array(vec![]))])]))
        .insert("Annots", array(vec![annotation(&[("Page", integer(0))])]));
    let catalog = Dict::new()
        .insert("FDF", Value::Ref(2, 0))
        .insert("Version", name_entry("1.2"));
    let old = doc_from_objects(vec![
        Object::new(1, 0, Value::Dict(catalog)),
        fdf_dict_obj(entries.clone(), 2),
    ]);
    let pages = old.fdf_pages().expect("ok").expect("视图");
    assert!(
        pages
            .issues
            .iter()
            .any(|issue| matches!(issue, FdfPagesIssue::PagesVersionGate { declared } if declared == "1.2")),
        "{:?}",
        pages.issues
    );
    let annots = old.fdf_annots().expect("ok").expect("视图");
    assert!(
        annots
            .issues
            .iter()
            .any(|issue| matches!(issue, FdfPagesIssue::AnnotsVersionGate { declared } if declared == "1.2")),
        "{:?}",
        annots.issues
    );
    // 1.3 声明（写端 `fdf_annots_1_3_gate` 往返的同一档位）不再有门禁诊断。
    let document = doc_with_fdf_entries(entries, Vec::new());
    assert!(
        document
            .fdf_pages()
            .expect("ok")
            .expect("视图")
            .issues
            .is_empty(),
        "默认 source_version 1.7 不应报门禁"
    );
    assert!(
        document
            .fdf_annots()
            .expect("ok")
            .expect("视图")
            .issues
            .is_empty()
    );
}
