//! 文档级导航写端：页面标签、命名目标、命名页面、命名模板页、大纲、
//! 文章线、便携集合、备用演示与扩展字典（ISO 32000-1 12.3、12.4.2、
//! 12.4.3、12.7.6、13.5、7.12）。
//!
//! 本模块按主题目录化：`outline.rs`（大纲条目与 `/Outlines` 树）、
//! `names.rs`（名称树与 `/Dest` 值装配）、`pages.rs`（页面标签与目录/
//! 页面树取址）、`templates.rs`、`alternate_presentation.rs`、
//! `articles.rs`、`collection.rs`、`extensions.rs`、`structure*`、
//! `validate.rs` 各成一篇；本文件只保留 [`NavigationState`] 定义与
//! 入口 [`apply`] 编排。
//!
//! 这些结构都挂在目录（catalog）上。构建器在 [`PdfBuilder::build`] 完成
//! 对象组装后统一写入：目录字典被覆盖写回，大纲条目、模板页对象、文章线
//! 与幻灯片对象从现有最大对象号之后分配，因此不影响既有对象布局。
//!
//! 版本门控（构建期返回 [`WriteError::UnsupportedVersion`]，零字节输出）：
//! 大纲与旧式命名目标（目录 `/Dests`，12.3.2.3 的 PDF 1.1 形态）自 1.1 起、
//! 命名目标自 1.2 起、页面标签、命名页面与命名模板页
//! 自 1.3 起、备用演示自 1.4 起、扩展字典（目录 `/Extensions`）自 1.7 起。输入非法（页码越界、名称重复、标签起点
//! 乱序、颜色越界、未知嵌入文件等）返回 [`WriteError::InvalidObject`]，
//! 同样零字节输出。

use crate::core::reader::navigation::{Destination, PageLabelRange};
use crate::core::types::{Dict, Document, Object, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::PdfBuilder;
use super::types::Action;
use super::types::ActionRefs;
use super::types::PageSize;
use super::types::StructureMarkInfo;
use super::types::StructureTree;
use outline::{OutlineContext, outline_has_actions, write_outline};
use pages::{catalog_dict, page_labels_dict, page_refs};
use structure::StructureTargets;

mod alternate_presentation;
mod articles;
mod collection;
mod extensions;
mod names;
mod outline;
mod pages;
pub(in crate::core::builder) mod structure;
pub(super) mod structure_content;
mod structure_layout;
mod structure_standard;
pub(super) mod tag_suspect;
mod templates;
mod validate;

pub use alternate_presentation::Slideshow;
pub use articles::{ArticleBead, ArticleThread};
pub use collection::{
    Collection, CollectionField, CollectionFieldType, CollectionSort, CollectionView,
};
pub use outline::OutlineEntry;

use names::legacy_dests_dict;
pub(super) use names::{destination_value, name_tree};

/// 构建器持有的文档级导航配置；空配置时不改写目录。
#[derive(Debug, Default)]
pub(crate) struct NavigationState {
    page_labels: Vec<PageLabelRange>,
    named_destinations: Vec<(String, Destination)>,
    /// 旧式命名目标（12.3.2.3 的 PDF 1.1 形态）：名字 + 显式目标，
    /// 写入目录 `/Dests` 字典（Table 28 要求该字典是间接引用）。
    legacy_named_destinations: Vec<(String, Destination)>,
    named_pages: Vec<(String, usize)>,
    /// 命名模板页（12.7.6 `/Names /Templates`）：名称 + 不可见页尺寸。
    templates: Vec<(String, PageSize)>,
    /// 备用演示幻灯片（13.5 `/Names /AlternatePresentations`）。
    alternate_presentations: Vec<Slideshow>,
    outline: Vec<OutlineEntry>,
    actions: Vec<Action>,
    article_threads: Vec<ArticleThread>,
    collection: Option<Collection>,
    /// 扩展字典条目（7.12 `/Extensions`）：前缀 + BaseVersion + ExtensionLevel。
    extensions: Vec<extensions::RegisteredExtension>,
    /// 逻辑结构树（14.7.2）；None 时目录不写 `/StructTreeRoot`。
    structure_tree: Option<StructureTree>,
    /// 目录 `/MarkInfo`（14.7.1 Table 321）；None 时不写出。
    mark_info: Option<StructureMarkInfo>,
    /// 结构标记内容 MCID 登记：自动编号分配与自动/显式撞号检测
    /// （14.7.4.4），`build` 在对象号分配之前消费撞号信息。
    structure_mcids: structure_content::McidRegistry,
}

impl NavigationState {
    /// 结构标记内容 MCID 登记的可变借用；供 `begin_structure_content` /
    /// `begin_auto_structure_content` 登记来源，`build` 在对象号分配之前
    /// 取出撞号信息，失败保持零字节契约。
    pub(in crate::core::builder) fn structure_mcids_mut(
        &mut self,
    ) -> &mut structure_content::McidRegistry {
        &mut self.structure_mcids
    }

    /// 各文章线的 bead 数，按注册顺序；供 Thread 动作索引校验。
    pub(super) fn article_thread_bead_counts(&self) -> Vec<usize> {
        self.article_threads
            .iter()
            .map(|thread| thread.beads.len())
            .collect()
    }

    /// 集合配置；供嵌入文件写端交叉校验模式与初始文档。
    pub(super) fn collection(&self) -> Option<&Collection> {
        self.collection.as_ref()
    }

    /// 逻辑结构树；供 assemble 阶段在任何对象号分配之前交叉校验页面
    /// `/MCID` 与结构树 MCR 的一致性。
    pub(super) fn structure_tree(&self) -> Option<&StructureTree> {
        self.structure_tree.as_ref()
    }

    /// 目录 `/MarkInfo` 配置；供 assemble 阶段在任何对象号分配之前校验
    /// 含 tag suspects 的文档必须声明 `/Suspects true`（14.8.2.3.1）。
    pub(super) fn mark_info(&self) -> Option<&StructureMarkInfo> {
        self.mark_info.as_ref()
    }

    /// 大纲条目实际引用到的动作，按写出顺序（子项先于父项），同一动作
    /// 句柄只出现一次。
    ///
    /// 写出顺序与 [`write_outline`] 一致：每个条目先写子项，再写
    /// 自身 `/A`，且动作对象按句柄缓存。Rendition section 的目标对象分配
    /// 依赖此顺序与注解写出顺序对齐。
    pub(super) fn outline_actions(&self) -> Vec<&Action> {
        fn collect<'a>(
            entries: &'a [OutlineEntry],
            actions: &'a [Action],
            used: &mut Vec<&'a Action>,
            seen: &mut std::collections::HashSet<usize>,
        ) {
            for entry in entries {
                collect(&entry.children, actions, used, seen);
                if let Some(handle) = entry.action
                    && let Some(action) = actions.get(handle)
                    && seen.insert(handle)
                {
                    used.push(action);
                }
            }
        }
        let mut used = Vec::new();
        let mut seen = std::collections::HashSet::new();
        collect(&self.outline, &self.actions, &mut used, &mut seen);
        used
    }
}

impl PdfBuilder {
    /// 设置逻辑结构树（14.7.2），写入目录 `/StructTreeRoot`。
    ///
    /// 结构树根、全部结构元素与 `/IDTree` 从现有最大对象号之后分配，
    /// 含 MCR 的页面追加 `/StructParents`。结构类型须为合法非空名字、
    /// 深度/元素数/子项数/映射条目不得超过读端上限、页索引须在范围内、
    /// MCID 须非负且同页唯一、`/ID` 须非空且 ASCII 唯一，否则构建返回
    /// [`WriteError::InvalidObject`]。结构树要求 PDF 1.3、`/Lang` 与
    /// `/ActualText` 要求 1.4、`/E` 要求 1.5，否则返回
    /// [`WriteError::UnsupportedVersion`]。两类失败均零字节输出。
    pub fn set_structure_tree(&mut self, tree: StructureTree) -> &mut Self {
        self.navigation.structure_tree = Some(tree);
        self
    }

    /// 设置目录 `/MarkInfo`（14.7.1 Table 321），要求 PDF 1.4；
    /// `/UserProperties` 与 `/Suspects` 要求 PDF 1.6，否则构建返回
    /// [`WriteError::UnsupportedVersion`]。
    pub fn set_mark_info(&mut self, info: StructureMarkInfo) -> &mut Self {
        self.navigation.mark_info = Some(info);
        self
    }
}

/// 把导航配置写入已组装的 `Document` 目录；；无配置时不产生任何对象。
///
/// `structure_targets` 是 OBJR 内容项（14.7.4.3 Table 325）的句柄解析表：
/// 注解/Form XObject 句柄与页内图像序号 → 对象号与所在页，只在结构树
/// 带对象引用子项时被消费。
pub(super) fn apply(
    doc: &mut Document,
    version: PdfVersion,
    page_count: usize,
    navigation: &NavigationState,
    action_refs: ActionRefs<'_>,
    structure_targets: StructureTargets<'_>,
) -> Result<(), WriteError> {
    let has_actions = outline_has_actions(&navigation.outline);
    let required = [
        (
            !navigation.page_labels.is_empty(),
            PdfVersion::V1_3,
            "page labels",
        ),
        (
            !navigation.named_destinations.is_empty(),
            PdfVersion::V1_2,
            "named destinations",
        ),
        (
            !navigation.legacy_named_destinations.is_empty(),
            PdfVersion::V1_1,
            "legacy named destinations (catalog /Dests)",
        ),
        (
            !navigation.named_pages.is_empty(),
            PdfVersion::V1_3,
            "named pages",
        ),
        (
            !navigation.templates.is_empty(),
            PdfVersion::V1_3,
            "page templates",
        ),
        (
            !navigation.alternate_presentations.is_empty(),
            PdfVersion::V1_4,
            "alternate presentations",
        ),
        (!navigation.outline.is_empty(), PdfVersion::V1_1, "outlines"),
        (has_actions, PdfVersion::V1_2, "outline actions (/A)"),
        (
            !navigation.article_threads.is_empty(),
            PdfVersion::V1_1,
            "article threads",
        ),
        (
            navigation.collection.is_some(),
            PdfVersion::V1_7,
            "collections",
        ),
        (
            !navigation.extensions.is_empty(),
            PdfVersion::V1_7,
            "extensions dictionaries (catalog /Extensions)",
        ),
        (
            navigation.structure_tree.is_some(),
            PdfVersion::V1_3,
            "structure trees",
        ),
        (
            navigation.mark_info.is_some(),
            PdfVersion::V1_4,
            "mark info",
        ),
    ];
    for (present, minimum, feature) in required {
        if present && version < minimum {
            return Err(WriteError::UnsupportedVersion(format!(
                "{feature} require PDF {}, got {version}",
                minimum.as_str()
            )));
        }
    }
    if required.iter().all(|(present, ..)| !present) {
        return Ok(());
    }

    validate::page_labels(&navigation.page_labels, page_count)?;
    validate::named_destinations(&navigation.named_destinations, page_count)?;
    validate::legacy_named_destinations(&navigation.legacy_named_destinations, page_count)?;
    validate::named_pages(&navigation.named_pages, page_count)?;
    templates::validate(&navigation.templates)?;
    validate::outline(&navigation.outline, page_count)?;
    articles::validate(&navigation.article_threads, page_count)?;
    if let Some(collection) = &navigation.collection {
        collection::validate(collection)?;
    }
    extensions::validate(&navigation.extensions, version)?;

    let (catalog_num, catalog_generation, mut catalog) = catalog_dict(doc)?;
    let page_refs = page_refs(doc, &catalog)?;
    if page_refs.len() != page_count {
        return Err(invalid(format!(
            "page tree has {} pages but the builder tracks {page_count}",
            page_refs.len()
        )));
    }

    // 备用演示的资源值引用目录 `/Names /EmbeddedFiles` 中的文件规格；
    // 在分配任何新对象号之前读取名称树并校验，保持零字节输出契约。
    let embedded_files = alternate_presentation::catalog_embedded_files(doc, &catalog)?;
    alternate_presentation::validate(&navigation.alternate_presentations, &embedded_files)?;

    if !navigation.page_labels.is_empty() {
        catalog = catalog.insert(
            "PageLabels",
            Value::Dict(page_labels_dict(&navigation.page_labels)),
        );
    }

    // 模板页对象在构造名称树之前分配：引用其对象号写入 `/Templates`。
    // 模板页不进入页面树，故页面 `/Kids` 与 `/Count` 保持不变。
    let template_refs = if navigation.templates.is_empty() {
        Vec::new()
    } else {
        templates::write(doc, &navigation.templates)
    };

    // 幻灯片对象同样在构造名称树之前分配：引用其对象号写入
    // `/AlternatePresentations`。
    let show_refs =
        alternate_presentation::write(doc, &navigation.alternate_presentations, &embedded_files);

    if !navigation.named_destinations.is_empty()
        || !navigation.named_pages.is_empty()
        || !template_refs.is_empty()
        || !show_refs.is_empty()
    {
        // 与 assemble 阶段已写入的 `/Names` 类别（如 /EmbeddedFiles）合并，
        // 不覆盖既有字典。
        let mut names = catalog
            .get("Names")
            .and_then(Value::as_dict)
            .cloned()
            .unwrap_or_else(Dict::new);
        if !navigation.named_destinations.is_empty() {
            let entries = navigation
                .named_destinations
                .iter()
                .map(|(name, destination)| {
                    Ok((name.clone(), destination_value(destination, &page_refs)?))
                })
                .collect::<Result<Vec<_>, WriteError>>()?;
            names = names.insert("Dests", name_tree(entries));
        }
        if !navigation.named_pages.is_empty() {
            let entries = navigation
                .named_pages
                .iter()
                .map(|(name, page)| {
                    let (num, generation) = page_refs[*page];
                    (name.clone(), Value::Ref(num, generation))
                })
                .collect();
            names = names.insert("Pages", name_tree(entries));
        }
        if !template_refs.is_empty() {
            let entries = navigation
                .templates
                .iter()
                .zip(&template_refs)
                .map(|((name, _), reference)| (name.clone(), reference.clone()))
                .collect();
            names = names.insert("Templates", name_tree(entries));
        }
        if !show_refs.is_empty() {
            names = names.insert("AlternatePresentations", name_tree(show_refs));
        }
        catalog = catalog.insert("Names", Value::Dict(names));
    }

    if !navigation.legacy_named_destinations.is_empty() {
        // Table 28：目录 `/Dests`「shall be an indirect reference」——字典本身
        // 写成独立间接对象，目录条目只存引用；对象号沿用 templates::write 的
        // max+1 分配法，且全部校验已在分配之前完成（零字节契约）。
        let dict = legacy_dests_dict(&navigation.legacy_named_destinations, &page_refs)?;
        let num = doc
            .objects_for_write()
            .iter()
            .map(|object| object.num)
            .max()
            .unwrap_or(0)
            + 1;
        doc.set_object(Object::new(num, 0, dict));
        catalog = catalog.insert("Dests", Value::Ref(num, 0));
        doc.features
            .borrow_mut()
            .insert(PdfFeature::LegacyNamedDestinations);
    }

    if !navigation.outline.is_empty() {
        let mut next_num = doc
            .objects_for_write()
            .iter()
            .map(|object| object.num)
            .max()
            .unwrap_or(0)
            + 1;
        let context = OutlineContext {
            page_refs: &page_refs,
            actions: &navigation.actions,
            action_refs,
        };
        let root = write_outline(doc, &navigation.outline, &context, &mut next_num)?;
        catalog = catalog.insert("Outlines", Value::Ref(root, 0));
    }

    if !navigation.article_threads.is_empty() {
        let threads =
            articles::write_threads(doc, version, &page_refs, &navigation.article_threads)?;
        catalog = catalog.insert("Threads", Value::Array(threads));
    }

    if let Some(collection) = &navigation.collection {
        catalog = catalog.insert(
            "Collection",
            Value::Dict(collection::collection_dict(collection)),
        );
        doc.features.borrow_mut().insert(PdfFeature::Collection);
    }

    if !navigation.extensions.is_empty() {
        // 7.12.1：扩展字典全部为直接对象，嵌套写入目录，不分配对象号。
        catalog = catalog.insert(
            "Extensions",
            Value::Dict(extensions::extensions_dict(&navigation.extensions)),
        );
        doc.features
            .borrow_mut()
            .insert(PdfFeature::ExtensionsDictionary);
    }

    let structure = structure::write(
        doc,
        version,
        &page_refs,
        &structure_targets,
        navigation.structure_tree.as_ref(),
        navigation.mark_info.as_ref(),
    )?;
    if let Some(root) = structure.struct_tree_root {
        catalog = catalog.insert("StructTreeRoot", root);
    }
    if let Some(info) = structure.mark_info {
        catalog = catalog.insert("MarkInfo", info);
    }

    doc.set_object(Object::new(
        catalog_num,
        catalog_generation,
        Value::Dict(catalog),
    ));
    Ok(())
}

fn invalid(message: impl Into<String>) -> WriteError {
    WriteError::InvalidObject(message.into())
}
