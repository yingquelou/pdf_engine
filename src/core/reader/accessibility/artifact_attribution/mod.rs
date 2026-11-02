//! Artifact 与逻辑结构的归属校验（ISO 32000-1 14.8.2.2.1、14.8.2.2.2、14.8.2.3.1）。
//!
//! 14.8.2.2.1 规定逻辑结构 encompassed 所有真实内容的图形对象，但**不**包含
//! 版面与生产过程残留的 artifact；14.8.2.3.1 进一步规定 artifact 出现在页面
//! 内容序列中、却**不**进入逻辑结构序列。本模块据此做**诊断**：
//! [`Document::artifact_attribution_violations`] 返回发现列表，读取阶段不因
//! 归属问题报错，由调用方决定是否处理（与 14.8.4 的
//! [`content_model`](crate::core::reader::structure::content_model_violations)
//! 诊断同一形态）。
//!
//! 七条规则与 shall/should 划分：
//!
//! - [`ArtifactAttributionRule::ArtifactMarkedAsStructure`]（shall，
//!   14.8.2.3.1）：`Artifact` 区段的属性表（Table 330 之外还混入了结构条目）
//!   携带合法 `/MCID`，等于把 artifact 赋予了结构内容身份；
//! - [`ArtifactAttributionRule::ArtifactInLogicalStructure`]（shall，
//!   14.8.2.2.1）：本页某结构元素的 `/K` 子项（整数 MCID 或 Table 324 的
//!   MCR）指向的 MCID 恰好被页上的 `Artifact` 区段使用；只有**生效页**为本页
//!   的子项参与判定（MCR 的 `/Pg` 覆盖元素的 `/Pg`，整数 MCID 子项沿用元素
//!   的 `/Pg`）；MCR 带 `/Stm` 或 `/StmOwn` 时序列不在页面流中，不做归属；
//! - [`ArtifactAttributionRule::RealContentNotInStructure`]（should，
//!   14.8.2.2.1）：非 `Artifact` 区段沿 [`MarkedSection::parent`] 链向外
//!   （含自身）都找不到合法 `/MCID`，真实内容未进入逻辑结构。
//! - [`ArtifactAttributionRule::TagSuspectMissingProperty`]（shall，
//!   14.8.2.3.1）：标签为精确 `TagSuspect` 的区段没有属性表、属性表缺
//!   `TagSuspect` 条目，或条目值不是名字 `Ordering`。
//! - [`ArtifactAttributionRule::TagSuspectSuspectsFlagNotSet`]（shall，
//!   14.8.2.3.1）：本页含 `TagSuspect` 区段，但目录 `/MarkInfo` 没有
//!   `/Suspects true`。
//! - [`ArtifactAttributionRule::VisibleAnnotationNotInStructure`]（should，
//!   14.8.2.2.1）：注解有含常态（`/N`）外观的外观流、`/F` 未置 Hidden 位，
//!   却没有任何结构元素的 `/K` 以 OBJR 声称它为内容项——「注解属于真实
//!   内容」三个条件里前两条成立、第三条（进入逻辑结构）缺失；
//! - [`ArtifactAttributionRule::AnnotationObjrParentTreeMismatch`]（shall，
//!   14.7.4.3、14.7.4.4 与 Table 325/326）：某结构元素的 `/K` 以 OBJR 引用
//!   本页注解，但注解字典没有整数 `/StructParent` 键，或该键在父树中的
//!   条目不是指向**该元素**的裸（单元）间接引用。
//!
//! 后两条依据 14.8.2.3.1 原文（tag suspects 自 PDF 1.6 引入）：
//!
//! > In some situations, conforming writer may be unable to determine correct
//! > page content order for part of a document's contents. In such cases, tag
//! > suspects (PDF 1.6) can be used. The conforming writer shall identify
//! > suspect content by using marked content (see 14.6, "Marked Content") with
//! > a tag of TagSuspect, as shown in next Example. The marked content shall
//! > have a properties dictionary with an entry whose name is TagSuspect and
//! > whose value is Ordering, which indicates that the ordering of the enclosed
//! > marked content does not meet Tagged PDF specifications.
//! > EXAMPLE  /TagSuspect <</TagSuspect /Ordering>> BDC .... % Problem page contents EMC
//! > Documents containing tag suspects shall contain a Suspects entry with a
//! > value of true in the mark information dictionary (see Table 321).
//!
//! 规则 6 依据 14.8.2.2.1 原文（注解参与真实内容的三个条件）：
//!
//! > A document's real content includes not only the page content stream and
//! > subsidiary form XObjects but also associated annotations that meet all of
//! > the following conditions:
//! > • The annotation has an appearance stream (see 12.5.5, "Appearance
//! > Streams") containing a normal (N) appearance.
//! > • The annotation's Hidden flag (see 12.5.3, "Annotation Flags") is not
//! > set.
//! > • The annotation is included in the document's logical structure (see
//! > 14.7, "Logical Structure").
//!
//! 该句是「真实内容包含……」的定义式表述，第三个条件本身也在条件里，
//! 没有独立的 shall，故规则 6 与同样派生自「逻辑结构 encompassed 所有
//! 真实内容」一句的规则 3 同列 should 级。规则 7 的 shall 形态取自
//! 14.7.4.4（并经 Table 326 与注解公共条目 Table 164 的
//! `/StructParent`）：
//!
//! > The tree shall contain an entry for each object that is a content item
//! > of at least one structure element .... The key for each entry shall be
//! > an integer given as the value of the StructParent or StructParents entry
//! > in the object (see Table 326). .... For an object identified as a content
//! > item by means of an object reference (see 14.7.4.3, "PDF Objects as
//! > Content Items"), the value shall be an indirect reference to the parent
//! > structure element.
//!
//! 注解规则（6/7）的生效范围：仅当目录存在 `/StructTreeRoot`（逻辑结构树
//! 读取成功）时评估。14.8.2 建立在 14.7 的逻辑结构框架之上（14.8.1：
//! "Tagged PDF (PDF 1.4) is a stylized use of PDF that builds on the logical
//! structure framework described in 14.7"），14.8.2.2.1 的第三个条件要求
//! 注解"included in the document's logical structure"；没有结构树的文档
//! 根本不存在可纳入的逻辑结构，把可见注解的存在记为注解归属发现只会
//! 造成噪音，且与规则 2 在结构树缺失时不评估的行为不一致。规则 6 判定
//! 「纳入逻辑结构」看 OBJR 的 `/Obj` 对象号是否等于注解的间接引用
//! （忽略 generation，与结构树其余引用的库内语义一致）；直接嵌入
//! `/Annots` 的非间接注解字典无法被 Table 325 要求的（间接）OBJR 声称，
//! 可见时同样记规则 6。注解读取失败（`/Annots` 损坏、注解字典非法、
//! 数量越限）一律降级为「规则 6/7 不产生任何发现」，诊断不因此报错。
//!
//! `/MCID` 的合法性与 [`span`](super::span) 模块共用同一取值规则
//! （[`span::mcid_value`](super::span)）：整数或整数值的有限浮点数。
//! 页内容归属只比页节点**对象号**（忽略 generation），与结构树其余
//! 引用的库内语义一致；表单 XObject 的内容流不会被递归解析（沿用
//! [`Document::page_marked_content`] 的行为）。
//!
//! 规则 3 的前提是 Tagged PDF（14.8.2）：对没有结构树的文档，它把每个没有
//! `/MCID` 的标记区段都记为 should 级发现，正好构成「内容已标记但未挂结构」
//! 的待补标签清单；调用方可先用 [`Document::mark_info`] 判断文档是否声明为
//! tagged，再决定是否采纳这类发现。
//!
//! 豁免：标签为精确 `TagSuspect` 的区段（14.8.2.3.1 的 PDF 1.6 tag
//! suspect）及其整个子树仍不参与 `RealContentNotInStructure` 判定——
//! 规范承认这段内容的页序本就不合 Tagged PDF，不能据此再判它「未入
//! 结构」。目录 `/MarkInfo /Suspects` 的交叉校验由
//! `TagSuspectSuspectsFlagNotSet` 承担：读 `/MarkInfo` 失败（缺失、条目
//! 非布尔、结构损坏）时一律按「未声明 tag suspects」处理，诊断本身
//! 从不因此报错。
//!
//! 输出顺序确定：先全部 `ArtifactMarkedAsStructure`（区段下标升序），再
//! `ArtifactInLogicalStructure`（元素 arena 下标、子项位置升序；同一 MCID
//! 命中多个 artifact 区段时取最小下标），再 `RealContentNotInStructure`
//! （区段下标升序），再 `TagSuspectMissingProperty`（区段下标升序）与
//! 至多一条 `TagSuspectSuspectsFlagNotSet`；最后追加两条注解归属规则：
//! `VisibleAnnotationNotInStructure`（注解在本页 `/Annots` 数组中的下标
//! 升序）与 `AnnotationObjrParentTreeMismatch`（元素 arena 下标、`/K`
//! 子项位置升序，与 `ArtifactInLogicalStructure` 同一约定）。

mod document;
mod error;
mod model;
mod sections;
mod structure;

pub use error::ArtifactAttributionError;
pub use model::{ArtifactAttributionRule, ArtifactAttributionViolation};
