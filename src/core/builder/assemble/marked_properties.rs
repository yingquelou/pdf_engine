//! 标记内容属性表（14.6）的装配阶段：注册句柄 → 共享对象号游标取号 →
//! 页面内容占位替换 → 字典写成真正的间接对象。
//!
//! 通路存在的理由：本库里对象号只在装配期由 `&mut i64` 游标分配，调用方无法
//! 预先知道属性表会落到哪个号；手写裸号（`ContentStream::begin_marked_content_reference`）
//! 因此要么悬空要么与游标产物撞车。注册句柄把「声明属性表」与「取号」解耦，
//! 装配期再把占位换成 `<号> 0 R`、把字典写成该号的间接对象。
//!
//! 一致性规则（全部在对象号分配前后、任何字节写出之前成立，失败即
//! [`WriteError::InvalidObject`] 零字节拒绝）：
//! 1. **未登记的句柄被使用 → 拒绝**：内容流里出现的每个未定型占位句柄都
//!    必须来自 `PdfBuilder::add_marked_content_properties`；检查在
//!    `assemble_preflight` 与取号前各做一次（写出顺序若变动，越界仍是
//!    拒绝而非 panic）。
//! 2. **登记了但没有任何标记内容引用 → 不分配、不写出**（取舍依据：与
//!    `alloc_groups` 对可选内容组/渐变/图案的「只为实际引用取号」做法一致；
//!    未使用的句柄只是调用方的声明，不构成任何引用不一致，拒绝它会把
//!    「先注册后按需引用」的增量写法变成负担；不取号因此也不回收对象号，
//!    输出与从未注册时逐字节相同）。
//! 3. **同一句柄多处复用只分配一个对象号**：按首次出现顺序去重取号，
//!    区段（BDC）与点标记（DP）、跨页引用都指向同一间接对象。
//! 4. **对象号来自文档共享游标**（与页面/注解/大纲等同一 `&mut i64` 序列，
//!    先例见 13.2.6 MCS 片段与注解/大纲共用游标的做法），不会与任何既有
//!    分配相撞。
//! 5. **代数一致性**：本通路把属性表字典永远写成代号 0 的间接对象，回填的
//!    引用同样是 `<号> 0 R`；若内容里另有一条**手写裸号**引用指向本池分配
//!    出的对象号但代号不是 0，它引用的其实不是那个对象（本库从不以非 0
//!    代号写对象），属于会静默产出错误引用的不一致，在替换占位之前拒绝。
//!    指向池外对象号的手写引用不在校验范围——那条通路明确是「调用方自己
//!    负责对象是否存在」（见 `begin_marked_content_reference` 文档），
//!    猜疑它反而破坏其用途。
//!
//! 写出形态：句柄通路产出 `/Tag /MC<n> BDC`（`/Properties` 命名资源，
//! ISO 32000-1 授权的操作数形态，Table 320 只授权内联字典与 `/Properties`
//! 命名资源两种，L29061–29078；14.6.2 L29087–29091 要求值含间接引用的属性
//! 表 **shall** 做成 `/Properties` 命名资源并按名称引用）。资源名 `MC<n>` 在
//! 页面 `/Resources /Properties` 登记为指向属性表对象的引用；同一句柄跨页
//! 复用时各页登记同名同对象。读端 `reader/content/marked.rs` 把名称经
//! `/Properties` 解引用为属性表字典。页面与 Form XObject 内容流同等处理：
//! 表单内容里的句柄同样取号并登记，名字写入表单自有 `/Resources /Properties`
//! （若表单自带资源）以及绘制该表单的页面 `/Properties`（表单继承页面资源时
//! 供解析）。调用方手写的裸号通路（`begin_marked_content_reference`）仍按原样
//! 透传，不受本通路影响。

use std::collections::BTreeSet;

use crate::core::builder::PdfBuilder;
use crate::core::content::{MarkedContentProperties, Op};
use crate::core::types::{Document, Object, Value};
use crate::core::writer::WriteError;

use super::plan::{FormPlan, MarkedPropertiesPlan};

/// 内容操作数序列里出现的未定型属性表句柄（按出现顺序，可能重复）。
///
/// 只识别 `BDC`/`DP` 两个带属性表的操作符；句柄形态即
/// `MarkedContentProperties::pending_handle`，替换后不再匹配。
fn pending_handles(ops: &[Op]) -> Vec<usize> {
    let mut out = Vec::new();
    for op in ops {
        let properties = match op {
            Op::BeginMarkedContentProperties { properties, .. }
            | Op::MarkedContentPointProperties { properties, .. } => properties,
            _ => continue,
        };
        if let Some(handle) = properties.as_pending_handle() {
            out.push(handle);
        }
    }
    out
}

/// 内容操作数序列里出现的**已定型**（非占位）引用操作数 `(对象号, 代号)`。
///
/// 同样只扫 `BDC`/`DP`；占位形态（代号为 `PENDING_GENERATION`）不属于
/// 手写裸号通路，由 [`pending_handles`] 负责。
fn resolved_references(ops: &[Op]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for op in ops {
        let properties = match op {
            Op::BeginMarkedContentProperties { properties, .. }
            | Op::MarkedContentPointProperties { properties, .. } => properties,
            _ => continue,
        };
        if let MarkedContentProperties::Reference { object, generation } = properties
            && properties.as_pending_handle().is_none()
        {
            out.push((*object, *generation));
        }
    }
    out
}

impl PdfBuilder {
    /// 零字节门禁：页面内容引用的属性表句柄必须已注册。
    ///
    /// 在对象号分配与任何字节写出之前拒绝（由 `assemble_preflight` 调用），
    /// 越界句柄返回 [`WriteError::InvalidObject`]。同一句柄被多处引用是合法的
    /// 共享属性表，不做重复检查。
    pub(super) fn validate_marked_property_handles(&self) -> Result<(), WriteError> {
        let registered = self.marked_properties.len();
        for (page, entry) in self.pages.iter().enumerate() {
            for handle in pending_handles(entry.content.ops()) {
                if handle >= registered {
                    return Err(WriteError::InvalidObject(format!(
                        "marked content property list: handle {handle} was never registered (page {page}, registered count {registered})"
                    )));
                }
            }
        }
        Ok(())
    }

    /// 为被引用的属性表句柄分配对象号与 `/Properties` 资源名，把页面与
    /// Form XObject 内容里的占位原地替换成资源名操作数（Table 320 授权
    /// 形态）；返回分配计划供写出阶段使用。
    ///
    /// 只为实际被引用的句柄取号（规则 2），取号使用文档共享游标（规则 4），
    /// 同一句柄只取一个号/一个资源名（规则 3）；替换前先做代数一致性校验
    /// （规则 5，针对调用方手写的裸号通路）。
    pub(super) fn allocate_marked_property_objects(
        &mut self,
        num: &mut i64,
        forms: &FormPlan,
    ) -> Result<MarkedPropertiesPlan, WriteError> {
        // 页面句柄上界在此再校一次：写出顺序若变动，越界仍是拒绝而非 panic。
        self.validate_marked_property_handles()?;
        let registered = self.marked_properties.len();
        // 表单句柄在 preflight 阶段尚未分配，故在此按同样口径校验（仍在任何
        // 字节写出之前，零字节契约不变）。
        for &form in &forms.used_forms {
            for handle in pending_handles(self.forms[form].content.ops()) {
                if handle >= registered {
                    return Err(WriteError::InvalidObject(format!(
                        "marked content property list: handle {handle} was never registered (form {form}, registered count {registered})"
                    )));
                }
            }
        }

        // 首次出现顺序去重（BTreeSet 会改变取号顺序，句柄顺序即用户标记顺序）：
        // 先页面、后实际用到的表单（按句柄序）。
        let mut seen = vec![false; registered];
        let mut used: Vec<usize> = Vec::new();
        for entry in self.pages.iter() {
            for handle in pending_handles(entry.content.ops()) {
                if !seen[handle] {
                    seen[handle] = true;
                    used.push(handle);
                }
            }
        }
        for &form in &forms.used_forms {
            for handle in pending_handles(self.forms[form].content.ops()) {
                if !seen[handle] {
                    seen[handle] = true;
                    used.push(handle);
                }
            }
        }

        let mut plan = MarkedPropertiesPlan::default();
        plan.page_properties = vec![Vec::new(); self.pages.len()];
        plan.form_properties = vec![Vec::new(); self.forms.len()];
        let mut object_of: Vec<Option<i64>> = vec![None; registered];
        let mut name_of: Vec<Option<String>> = vec![None; registered];
        for (k, &handle) in used.iter().enumerate() {
            let object = *num;
            *num += 1;
            let name = format!("MC{}", k + 1);
            object_of[handle] = Some(object);
            name_of[handle] = Some(name.clone());
            plan.entries.push((handle, object, name));
        }

        // 各容器登记自身实际引用的属性表（容器内首次出现顺序去重）；
        // 同一句柄跨页/跨表单复用时各处登记同名同对象。
        for (page, entry) in self.pages.iter().enumerate() {
            let mut container_seen: BTreeSet<usize> = BTreeSet::new();
            for handle in pending_handles(entry.content.ops()) {
                if !container_seen.insert(handle) {
                    continue;
                }
                let (Some(object), Some(name)) = (object_of[handle], name_of[handle].clone())
                else {
                    continue;
                };
                plan.page_properties[page].push((name, object));
            }
        }
        for &form in &forms.used_forms {
            let mut container_seen: BTreeSet<usize> = BTreeSet::new();
            for handle in pending_handles(self.forms[form].content.ops()) {
                if !container_seen.insert(handle) {
                    continue;
                }
                let (Some(object), Some(name)) = (object_of[handle], name_of[handle].clone())
                else {
                    continue;
                };
                plan.form_properties[form].push((name, object));
            }
        }

        // 代数一致性（规则 5）：手写裸号引用若指向本池刚分到的对象号，
        // 代号必须是 0——属性表字典永远以代号 0 写出，其余代号引用不到它。
        let allocated: BTreeSet<i64> = plan.entries.iter().map(|&(_, object, _)| object).collect();
        let check_refs = |object: i64, generation: i64, where_: String| -> Result<(), WriteError> {
            if allocated.contains(&object) && generation != 0 {
                return Err(WriteError::InvalidObject(format!(
                    "marked content property list: {where_} references object {object} with generation {generation}, but property lists are written with generation 0"
                )));
            }
            Ok(())
        };
        for (page, entry) in self.pages.iter().enumerate() {
            for (object, generation) in resolved_references(entry.content.ops()) {
                check_refs(object, generation, format!("page {page}"))?;
            }
        }
        for &form in &forms.used_forms {
            for (object, generation) in resolved_references(self.forms[form].content.ops()) {
                check_refs(object, generation, format!("form {form}"))?;
            }
        }

        // 占位 → `/Properties` 资源名：写进内容操作数模型本身，随后 `page_tree`
        // 与 `form_stream` 阶段照常把操作数序列化成内容流字节，并把名字登记
        // 进各自的 `/Resources /Properties`。
        for entry in self.pages.iter_mut() {
            for op in entry.content.ops_mut() {
                Self::replace_pending_property_handle(op, &name_of);
            }
        }
        for &form in &forms.used_forms {
            for op in self.forms[form].content.ops_mut() {
                Self::replace_pending_property_handle(op, &name_of);
            }
        }
        Ok(plan)
    }

    /// 把一个未定型属性表占位替换为对应的 `/Properties` 资源名；未命名时
    /// 保留占位，让序列化阶段的类型化拒绝兜住，绝不输出假名字。
    fn replace_pending_property_handle(op: &mut Op, name_of: &[Option<String>]) {
        let properties = match op {
            Op::BeginMarkedContentProperties { properties, .. }
            | Op::MarkedContentPointProperties { properties, .. } => properties,
            _ => return,
        };
        let Some(handle) = properties.as_pending_handle() else {
            return;
        };
        if let Some(name) = name_of[handle].clone() {
            *properties = MarkedContentProperties::Name(name);
        }
    }

    /// 按计划把注册时的字典**原样**写成间接对象（代号为 0），使其成为页面
    /// `/Resources /Properties` 里 `MC<n>` 名字真正指向的对象。
    pub(super) fn write_marked_property_objects(
        &self,
        doc: &mut Document,
        plan: &MarkedPropertiesPlan,
    ) {
        for &(handle, object, _) in &plan.entries {
            doc.set_object(Object::new(
                object,
                0,
                Value::Dict(self.marked_properties[handle].clone()),
            ));
        }
    }
}
