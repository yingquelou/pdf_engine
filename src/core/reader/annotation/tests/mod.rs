//! 注读端单元测试：`support` 为共享构造辅助，
//! `entries` 覆盖通用条目与页面树/错误契约，`subtype` 覆盖子类型专有条目（12.5.6），
//! `geometry` 覆盖 Square/Circle/Polygon/PolyLine 的 Table 177/178 条目，
//! `link` 覆盖 Link 的 Table 173 条目，
//! `redact` 覆盖 Redact 的 Table 192 条目，
//! `mk` 覆盖外观特性字典 `/MK`（Table 189）与图标适配字典 `/IF`（Table 247），
//! `measure` 覆盖 12.9 的 measure 字典（Table 261–263），
//! `exdata` 覆盖 3D 标记的 `/ExData` 外部数据字典（13.6.6 Table 313），
//! `watermark` 覆盖 Watermark 的固定打印字典 `/FixedPrint`（Table 190/191），
//! `printer_mark` 覆盖 Printer's mark 注解的 `/MN` 与表单项（Table 362/363），
//! `trap_net` 覆盖 Trap 网络注解的跟踪条目与表单条目（14.11.6 Table 366/367），
//! `dates` 覆盖 `/M` `/CreationDate` 的 PDF 日期解析（7.9.4），
//! `three_d` 覆盖 3D 注解与 13.6 的 3D 流/视图/投影/背景字典（Table 298–306）。

mod dates;
mod entries;
mod exdata;
mod geometry;
mod link;
mod measure;
mod mk;
mod movie;
mod printer_mark;
mod redact;
mod sound;
mod subtype;
mod support;
mod three_d;
mod trap_net;
mod watermark;
