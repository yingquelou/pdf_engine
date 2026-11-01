//! 逻辑结构写端（ISO 32000-1 14.7.1–14.7.5、14.8.5.2、14.8.5.5–14.8.5.7、
//! Table 321–329、341、347–349）。
//!
//! 目录 `/StructTreeRoot`（Table 322）与 `/MarkInfo`（Table 321）、结构
//! 元素（Table 323）、MCR 内容项（Table 324）、父树（14.7.4.4）与
//! `/IDTree` 名称树都从现有最大对象号之后分配，不影响既有对象布局；
//! 页面字典追加 `/StructParents`（Table 326）。结构属性对象 `/A` 与
//! `/ClassMap`（14.7.5）按 Table 327–329 写出，属性流提升为间接对象。
//!
//! 标准属性所有者（14.8.5.2 Table 341）与 List/PrintField/Table 条目
//! （14.8.5.5–14.8.5.7 Table 347–349）由
//! [`structure_standard`] 做值形态、元素适用性与 `/Headers` ID 交叉校验；
//! Layout（14.8.5.4 Table 342–346）条目暂不校验或建模。
//!
//! 校验先于任何对象号分配：深度、元素数、单元素子项数与映射条目上限
//! 直接复用读端常量；页索引越界、负 MCID、同页 MCID 重复、空结构类型、
//! 空角色映射键/目标、类映射/属性对象取值非字典/流或 `/O` 非法、空类名、
//! ID 为空/重复/非 ASCII 一律返回 [`WriteError::InvalidObject`]；元素
//! `/Pg` 页索引越界、整数简写 MCID 缺元素 `/Pg` 或与 `/Pg` 页不一致
//! （14.7.4.2）同样拒绝。版本门禁：
//! 结构树 ≥1.3、`/Lang` `/ActualText` ≥1.4、`/E` ≥1.5、`/MarkInfo` ≥1.4、
//! `/UserProperties` `/Suspects` ≥1.6、结构属性用户属性 ≥1.6、标准属性
//! 所有者 ≥1.4、`/Headers` `/Scope` ≥1.5、PrintField 与 `/Summary` ≥1.7，
//! 不满足返回 [`WriteError::UnsupportedVersion`]。使用用户属性时还要求
//! `/MarkInfo /UserProperties true`（14.7.5.4）。两类失败均不写出任何字节。
//!
//! 14.8.4 的 PDF 1.5 扩展标准结构类型（THead/TBody/TFoot/Annot/Ruby/
//! RB/RT/RP/Warichu/WT/WP）同样设门禁：元素 `/S` 经 `/RoleMap` 解析后的
//! 终点、或 `/RoleMap` 任一目标为这些名字时要求 ≥1.5（`/RoleMap` 目标
//! 即使未被元素使用也计入，避免写出带 1.5 类型名的 1.3/1.4 文件）。
//!
//! 本切片的范围限制：`/K` 写出子元素引用、MCR 字典（带显式 `/Pg` 与
//! `/MCID`）、14.7.4.2 的整数 MCID 简写（[`StructureKid::InlineMarkedContent`]：
//! 仅当元素自身以 `with_page` 携带同页 `/Pg` 时合法，写出为 `/K` 里的
//! 裸整数）、OBJR 对象引用字典（Table 325，`/Type` `/Pg` `/Obj`）与流
//! 形态 MCR 字典（Table 324 `/Stm` `/StmOwn`，见下）。
//! 元素 `/Pg`（Table 323）写出为页对象的间接引用，页索引越界拒绝。
//!
//! 本模块按主题目录化：
//! - `limits`：MCID/深度等写端上限与名字、MCID 取值校验；
//! - `flatten`：结构树前序展开（`Flattener`/`Flat`/`PlannedKid`）与逐元素校验；
//! - `plan`：展开后的写出计划（`Plan`/`ParentEntry`）与父树键分配、页面 `/StructParents` 写出；
//! - `attributes`：类映射与属性对象校验（Table 327–329）及属性流提升；
//! - `objr`：OBJR 目标解析与校验（Table 325/326）、`/StructParent` 写出；
//! - `stream_mcr`：流形态 MCR（Table 324 `/Stm` `/StmOwn`）校验与容器 `/StructParents` 写出；
//! - `element`：结构树根字典与全部元素对象写出；
//! - `write`：入口编排与 `/MarkInfo` 字典。
//!
//! 校验先于对象号分配：句柄/页/资源序号越界、两路都取不到 `/Pg`、目标不是
//! 所声称类别的字典或流字典（注解不得是流、Form/图像必须带对应
//! `/Subtype`）、目标字典已带 `/StructParent` 或 `/StructParents`、目标落在
//! 将写 `/StructParents` 的页对象号上、同一对象被两个元素声称，一律返回
//! [`WriteError::InvalidObject`]，零字节。流形态 MCR 走同一条先分配前校验
//! 通道：`/Stm` 句柄越界、解析出的对象不存在/不是 `/Subtype /Form` 流、
//! `/StmOwn` 句柄越界、注解绑定页越界或与引用 `/Pg` 冲突、三路皆取不到页、
//! 注解 `/AP` 不指向 `/Stm`、同容器 MCID 重复声称、容器与 OBJR 声称互斥，
//! 同样零字节拒绝。
//!
//! 父树键策略（14.7.4.4 shall）：新条目以当前 `/ParentTreeNextKey` 作键、
//! 随后自增——页面键按页升序 `0..p-1`、OBJR 键按目标对象号升序接在其后、
//! `/Stm` 容器键按容器对象号升序接在 OBJR 键之后，NextKey 写出为条目总数；
//! 结构对象号分配之前由 `structure_content::validate_next_key` 复核 NextKey
//! 严格大于全部在用键。

mod attributes;
mod element;
mod flatten;
mod limits;
mod objr;
mod plan;
mod stream_mcr;
mod write;

pub(super) use limits::MAX_STRUCTURE_MCID;
pub(in crate::core::builder) use objr::StructureTargets;
pub(super) use write::write;
