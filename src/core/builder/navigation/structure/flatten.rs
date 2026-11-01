//! 前序展开与逐元素校验：`Flattener` 递归展开结构元素并登记
//! MCID/OBJR/流形态 MCR 通道；全部校验先于对象号分配。

use std::collections::BTreeMap;

use crate::core::builder::navigation::invalid;
use crate::core::builder::navigation::structure_standard::{self, StandardUsage};
use crate::core::builder::types::{StructureElement, StructureKid, StructureObjectReference};
use crate::core::reader::accessibility::LanguageIdentifier;
use crate::core::reader::structure::{
    MAX_STRUCTURE_CHILDREN, MAX_STRUCTURE_DEPTH, MAX_STRUCTURE_ELEMENTS,
};
use crate::core::writer::WriteError;

use super::attributes::validate_attribute_object;
use super::limits::{is_extended_structure_type, validate_mcid_value, validate_name};
use super::objr::{StructureTargets, describe_objr_target};

/// 展开后的结构元素（前序 arena）。
pub(super) struct Flat<'a> {
    /// 原始结构元素。
    pub(super) element: &'a StructureElement,
    /// 父元素 arena 下标；顶层为 `None`。
    pub(super) parent: Option<usize>,
    /// 按文档顺序展开的子项（子元素已解析为 arena 下标）。
    pub(super) kids: Vec<PlannedKid>,
}

/// 写端展开后的 `/K` 子项。
pub(super) enum PlannedKid {
    /// 子结构元素 arena 下标。
    Element(usize),
    /// MCR：页索引 + MCID。
    MarkedContent { page: usize, mcid: i64 },
    /// 14.7.4.2 整数简写：页即元素 `/Pg`，写出为 `/K` 里的裸 MCID。
    InlineMarkedContent { mcid: i64 },
    /// OBJR（Table 325）：已解析的目标对象号 + `/Pg` 页索引。
    ObjectReference { object_num: i64, page: usize },
    /// 流形态 MCR（Table 324 `/Stm` `/StmOwn`）：解析后的 `/Stm` 流对象号、
    /// 可选 `/StmOwn` 对象号、渲染页索引、是否写出显式 `/Pg`（仅在引用
    /// 显式给出或经 `/StmOwn` 解析时为 true；沿用元素 `/Pg` 时省略）、MCID。
    StreamMarkedContent {
        stream_num: i64,
        owner_num: Option<i64>,
        page: usize,
        write_pg: bool,
        mcid: i64,
    },
}
/// 前序展开与逐元素校验。
pub(super) struct Flattener<'a> {
    pub(super) flat: Vec<Flat<'a>>,
    pub(super) ids: BTreeMap<Vec<u8>, usize>,
    /// 页面索引 → MCID → 父元素 arena 下标（同页 MCID 唯一）。
    pub(super) mcids: BTreeMap<usize, BTreeMap<i64, usize>>,
    /// OBJR 目标对象号 → 声称它的元素 arena 下标 + 目标声明（同一对象
    /// 至多被一个元素声称；按解析后的对象号判重，故跨页去重共享的同一
    /// 图像 XObject 也算重复声称）。
    pub(super) objrs: BTreeMap<i64, (usize, StructureObjectReference)>,
    /// 流形态 MCR 容器对象号 → MCID → 父元素 arena 下标（Table 324
    /// `/Stm`；容器取 `/StmOwn` 或 `/Stm`，同容器 MCID 唯一）。
    pub(super) stream_mcids: BTreeMap<i64, BTreeMap<i64, usize>>,
    pub(super) page_count: usize,
    /// OBJR 目标的句柄 → 对象号/页解析表。
    pub(super) targets: StructureTargets<'a>,
    /// 结构类型 → 标准类型的角色映射（元素适用性判定用）。
    pub(super) role_map: &'a BTreeMap<String, String>,
    /// 标准属性使用情况与待核对的 `/Headers` 引用。
    pub(super) standard: StandardUsage,
    pub(super) uses_language: bool,
    pub(super) uses_expanded: bool,
    pub(super) uses_user_properties: bool,
    /// 是否使用 PDF 1.5 扩展标准结构类型（14.8.4）。
    pub(super) uses_extended_types: bool,
}

impl<'a> Flattener<'a> {
    /// 记录一个结构元素并递归展开其 `/K`。
    pub(super) fn add(
        &mut self,
        element: &'a StructureElement,
        parent: Option<usize>,
        depth: usize,
    ) -> Result<(), WriteError> {
        if depth >= MAX_STRUCTURE_DEPTH {
            return Err(invalid(format!(
                "structure tree depth exceeds {MAX_STRUCTURE_DEPTH}"
            )));
        }
        if self.flat.len() >= MAX_STRUCTURE_ELEMENTS {
            return Err(invalid(format!(
                "structure tree has more than {MAX_STRUCTURE_ELEMENTS} elements"
            )));
        }
        if element.kids.len() > MAX_STRUCTURE_CHILDREN {
            return Err(invalid(format!(
                "structure element has {} children (limit {MAX_STRUCTURE_CHILDREN})",
                element.kids.len()
            )));
        }
        validate_name(&element.structure_type, "structure element /S")?;
        // Table 323 `/Pg`（可选间接页引用）：越界在对象号分配之前拒绝；
        // 它同时是整数简写 MCID（14.7.4.2）的定位页。
        if let Some(page) = element.page
            && page >= self.page_count
        {
            return Err(invalid(format!(
                "structure element /Pg page {page} is out of range (page count {}) (Table 323, 14.7.4.2)",
                self.page_count
            )));
        }
        // 14.8.4.1：/S 先沿 /RoleMap 一或多级解析，终点为 PDF 1.5 扩展
        // 标准类型时置位；版本门禁在 plan 末尾统一执行（仍在对象号分配前）。
        let resolved = structure_standard::resolved_type(self.role_map, &element.structure_type);
        if is_extended_structure_type(&resolved) {
            self.uses_extended_types = true;
        }
        if let Some(id) = &element.id {
            if id.is_empty() {
                return Err(invalid("structure element /ID must not be empty"));
            }
            if id.iter().any(|byte| *byte > 0x7F) {
                return Err(invalid(
                    "structure element /ID must contain ASCII bytes only (0x00-0x7F)",
                ));
            }
            if self.ids.contains_key(id) {
                return Err(invalid(format!(
                    "structure element /ID {id:?} is declared more than once"
                )));
            }
        }
        for (name, _) in &element.classes {
            validate_name(name, "structure class name")?;
        }
        if element.attributes.len() > MAX_STRUCTURE_CHILDREN {
            return Err(invalid(format!(
                "structure element has {} attributes (limit {MAX_STRUCTURE_CHILDREN})",
                element.attributes.len()
            )));
        }
        for (index, attribute) in element.attributes.iter().enumerate() {
            let context = format!("structure element /A attribute {index}");
            validate_attribute_object(&attribute.value, &context, &mut self.uses_user_properties)?;
            structure_standard::validate_element_attribute(
                &attribute.value,
                &context,
                &element.structure_type,
                self.role_map,
                &mut self.standard,
            )?;
        }
        if element.lang.is_some() || element.actual_text.is_some() {
            self.uses_language = true;
        }
        // 14.9.2.2：结构元素 `/Lang` 必须是空串或 RFC 3066 Language-Tag。
        // 语法校验先于任何对象号分配；错误信息带上前序下标与原串。
        if let Some(lang) = &element.lang {
            LanguageIdentifier::parse(lang).map_err(|error| {
                invalid(format!(
                    "structure element {} /Lang {lang:?} is not a valid language tag: {error}",
                    self.flat.len()
                ))
            })?;
        }
        if element.expanded.is_some() {
            self.uses_expanded = true;
        }

        let index = self.flat.len();
        self.flat.push(Flat {
            element,
            parent,
            kids: Vec::new(),
        });
        if let Some(id) = &element.id {
            self.ids.insert(id.clone(), index);
        }

        let mut kids = Vec::with_capacity(element.kids.len());
        for kid in &element.kids {
            match kid {
                StructureKid::Element(child) => {
                    let child_index = self.flat.len();
                    self.add(child, Some(index), depth + 1)?;
                    kids.push(PlannedKid::Element(child_index));
                }
                StructureKid::MarkedContent { page, mcid } => {
                    self.note_mcid(*page, *mcid, index)?;
                    kids.push(PlannedKid::MarkedContent {
                        page: *page,
                        mcid: *mcid,
                    });
                }
                StructureKid::InlineMarkedContent { page, mcid } => {
                    // 14.7.4.2：整数简写只在「序列位于元素 `/Pg` 指定页
                    // 的内容流」这一常见场景合法——元素必须自带 `/Pg`
                    // 且与简写的页一致；MCR 字典的 `/Pg` 才允许覆盖元素
                    // `/Pg`（Table 324），简写没有可覆盖的对象。
                    match element.page {
                        None => {
                            return Err(invalid(format!(
                                "structure integer MCID {mcid} needs the element /Pg to name page {page} (14.7.4.2)"
                            )));
                        }
                        Some(pg) if pg != *page => {
                            return Err(invalid(format!(
                                "structure integer MCID {mcid} names page {page} but the element /Pg is page {pg} (14.7.4.2)"
                            )));
                        }
                        Some(_) => {}
                    }
                    self.note_mcid(*page, *mcid, index)?;
                    kids.push(PlannedKid::InlineMarkedContent { mcid: *mcid });
                }
                StructureKid::ObjectReference { object } => {
                    // 14.7.4.3：`/Obj` 可以是注解、Form XObject 或图像
                    // XObject；目标句柄、绘制页与资源序号的越界检查同样
                    // 先于对象号分配。
                    let (object_num, page) = self.resolve_objr(object, element.page)?;
                    if self.objrs.insert(object_num, (index, *object)).is_some() {
                        return Err(invalid(format!(
                            "structure OBJR {} is claimed by more than one structure element (object number {object_num})",
                            describe_objr_target(object)
                        )));
                    }
                    if self.stream_mcids.contains_key(&object_num) {
                        return Err(invalid(format!(
                            "structure OBJR {} object {object_num} is also a /Stm container for marked-content sequences; an object may be a content item or a container for marked-content sequences, but not both (Table 326)",
                            describe_objr_target(object)
                        )));
                    }
                    kids.push(PlannedKid::ObjectReference { object_num, page });
                }
                StructureKid::StreamMarkedContent {
                    stream,
                    owner,
                    page,
                    mcid,
                } => {
                    // 14.7.4.2 Table 324 流形态：`/Stm` 是持有序列的内容流
                    // （写端目前只解析文档级 Form XObject 句柄），`/StmOwn`
                    // 是其所有者（注解句柄）。句柄解析、页三路回落与 MCID
                    // 判重都先于任何对象号分配。
                    let Some(&stream_num) = self.targets.form_nums.get(*stream) else {
                        return Err(invalid(format!(
                            "structure MCR /Stm form XObject handle {stream} is out of range (count {})",
                            self.targets.form_nums.len()
                        )));
                    };
                    let mut owner_num = None;
                    let mut owner_page = None;
                    if let Some(handle) = owner {
                        let Some(&num) = self.targets.annotation_nums.get(*handle) else {
                            return Err(invalid(format!(
                                "structure MCR /StmOwn annotation handle {handle} is out of range (count {})",
                                self.targets.annotation_nums.len()
                            )));
                        };
                        let Some(&pg) = self.targets.annotation_pages.get(*handle) else {
                            return Err(invalid(format!(
                                "structure MCR /StmOwn annotation handle {handle} is out of range (count {})",
                                self.targets.annotation_pages.len()
                            )));
                        };
                        if pg >= self.page_count {
                            return Err(invalid(format!(
                                "structure MCR /StmOwn annotation handle {handle} is attached to page {pg} out of range (page count {})",
                                self.page_count
                            )));
                        }
                        owner_num = Some(num);
                        owner_page = Some(pg);
                    }
                    // Table 324 `/Pg`：「This entry overrides any Pg entry
                    // in the structure element containing the
                    // marked-content reference; it shall be required if the
                    // structure element has no such entry」——引用显式给页
                    // 优先（与注解绑定页冲突即谎报渲染页，拒绝），否则回落
                    // `/StmOwn` 绑定页、再回落元素 `/Pg`；三路皆空拒绝。
                    // 仅前两路生效时写出显式 `/Pg`，继承元素 `/Pg` 时省略。
                    let (page, write_pg) = match page {
                        Some(pg) => {
                            if *pg >= self.page_count {
                                return Err(invalid(format!(
                                    "structure MCR /Stm page {pg} is out of range (page count {})",
                                    self.page_count
                                )));
                            }
                            if let (Some(handle), Some(owner_pg)) = (*owner, owner_page)
                                && owner_pg != *pg
                            {
                                return Err(invalid(format!(
                                    "structure MCR /StmOwn annotation handle {handle} is attached to page {owner_pg} but the reference names page {pg} (Table 324: /Pg is the page on which the sequence is rendered)"
                                )));
                            }
                            (*pg, true)
                        }
                        None => match owner_page.or(element.page) {
                            Some(pg) => (pg, owner_page.is_some()),
                            None => {
                                return Err(invalid(format!(
                                    "structure MCR /Stm on form XObject handle {stream} needs a page: the reference gives no /Pg, there is no /StmOwn page to fall back to, and the element has no /Pg (Table 324)"
                                )));
                            }
                        },
                    };
                    validate_mcid_value(*mcid)?;
                    let container = owner_num.unwrap_or(stream_num);
                    if self.objrs.contains_key(&container) {
                        return Err(invalid(format!(
                            "structure MCR /Stm container object {container} is also claimed as an OBJR content item; an object may be a content item or a container for marked-content sequences, but not both (Table 326)"
                        )));
                    }
                    if self
                        .stream_mcids
                        .entry(container)
                        .or_default()
                        .insert(*mcid, index)
                        .is_some()
                    {
                        return Err(invalid(format!(
                            "structure MCR /Stm container object {container} MCID {mcid} is claimed by more than one structure element"
                        )));
                    }
                    kids.push(PlannedKid::StreamMarkedContent {
                        stream_num,
                        owner_num,
                        page,
                        write_pg,
                        mcid: *mcid,
                    });
                }
            }
        }
        self.flat[index].kids = kids;
        Ok(())
    }
}

impl<'a> Flattener<'a> {
    /// 校验并登记一个标记内容标识符：MCR 字典与 14.7.4.2 整数简写共用
    /// 同一通道——页越界、负 MCID、超 `MAX_STRUCTURE_MCID` 上限、同页
    /// MCID 被多个元素声称均在对象号分配之前拒绝；成功后该页父树数组
    /// 的对应 MCID 槽位归属 `index` 元素。
    fn note_mcid(&mut self, page: usize, mcid: i64, index: usize) -> Result<(), WriteError> {
        if page >= self.page_count {
            return Err(invalid(format!(
                "structure MCR page {page} is out of range (page count {})",
                self.page_count
            )));
        }
        validate_mcid_value(mcid)?;
        if self
            .mcids
            .entry(page)
            .or_default()
            .insert(mcid, index)
            .is_some()
        {
            return Err(invalid(format!(
                "page {page} MCID {mcid} is claimed by more than one structure element"
            )));
        }
        Ok(())
    }
}
