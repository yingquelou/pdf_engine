//! 通用条目读取、页面树遍历与错误契约。
//!
//! 按主题目录化拆分：`page_tree` 覆盖直接/间接注解读取与页面树的空页、
//! 数量上限、环/深度、非字典条目等类型化错误；`flags` 覆盖 `/F` 标准位解码；
//! `appearance` 覆盖 `/AP` 的 `/N` `/D` `/R` 状态与空 AP 丢弃；
//! `border` 覆盖 `/BS` 字典优先于 `/Border` 遗留数组与 `/BM` 混合模式；
//! `markup` 覆盖标记类 `/QuadPoints` 与色彩/不透明度原条目保留，及 Link 的
//! `/A` `/Dest` `/Popup` 原条目保留；`malformed` 覆盖缺 `/Type` `/Subtype`
//! `/Rect` 等字典形态的类型化错误；`table_169` 覆盖未知子类型保留与 Table 169
//! 逐行名字/版本/标记归属；`destination` 覆盖 `/Dest` 数组/`/D` 字典/命名目标
//! 经导航层解析与非法 `/Dest` 的类型化错误。断言、夹具与注释自原单文件逐行搬运，
//! 测试叶名零改动。

mod appearance;
mod border;
mod destination;
mod flags;
mod malformed;
mod markup;
mod page_tree;
mod table_169;
