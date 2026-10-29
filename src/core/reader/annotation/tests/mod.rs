//! 注读端单元测试：`support` 为共享构造辅助，
//! `entries` 覆盖通用条目与页面树/错误契约，`subtype` 覆盖子类型专有条目（12.5.6），
//! `geometry` 覆盖 Square/Circle/Polygon/PolyLine 的 Table 177/178 条目，
//! `link` 覆盖 Link 的 Table 173 条目，
//! `redact` 覆盖 Redact 的 Table 192 条目，
//! `mk` 覆盖外观特性字典 `/MK`（Table 189）与图标适配字典 `/IF`（Table 247），
//! `measure` 覆盖 12.9 的 measure 字典（Table 261–263），
//! `dates` 覆盖 `/M` `/CreationDate` 的 PDF 日期解析（7.9.4）。

mod dates;
mod entries;
mod geometry;
mod link;
mod measure;
mod mk;
mod redact;
mod subtype;
mod support;
