//! CMap 流字典的 `/UseCMap` 基座解析（9.7.5.3，Table 120）。
//!
//! Table 120 只给出一条核心语义：`/UseCMap` 的值是「预定义 CMap 的名字，
//! 或一个含 CMap 的流」，且「若此条目存在，引用方 CMap 只声明与所引用 CMap
//! 不同的字符映射」。规则 (a) 要求流内 `usecmap` 引用的 CMap 同时由流字典
//! `/UseCMap` 指明。
//!
//! 本模块负责把这一条目还原成基座 CMap（含链式嵌套、环检测与深度上限），
//! 合并仍由既有 `CidCMap::parse_with_base` 与
//! `ToUnicodeMap::parse_with_base`/`parse_lenient` 完成，因此「本地映射优先
//! 于继承映射」的不变量不在此重复实现。基座不可解析时一律降级为「无基座」，
//! 绝不合成映射数据。

use std::collections::BTreeSet;

use crate::core::types::{Dict, Document, Value};

use super::FontResult;
use super::cid::CidCMap;
use super::to_unicode::{self, ToUnicodeMap};

/// Table 120 – `/UseCMap` 链的嵌套深度上限（库策略，与 `reader/fdf` 的
/// 64 层字段树上限同形）：读端解析不可信 PDF 时，任何可递归的结构都要有界。
pub(in crate::core::reader::font) const MAX_USE_CMAP_DEPTH: usize = 64;

/// Table 120 – `/UseCMap` 的两种取值形态（`name or stream`）。
enum UseCMap {
    /// 预定义 CMap 的名字。
    Name(String),
    /// 内嵌 CMap 流；间接引用时记录对象号用于环检测。
    Stream { value: Value, number: Option<i64> },
}

/// 一条 `/UseCMap` 链的护栏：深度上限 + 已展开过的流对象号。
#[derive(Debug, Default)]
struct Chain {
    depth: usize,
    visited: BTreeSet<i64>,
}

impl Chain {
    /// 展开一个基座流；深度超限或对象号重复（成环）时返回 `false`。
    fn enter(&mut self, number: Option<i64>) -> bool {
        if self.depth >= MAX_USE_CMAP_DEPTH {
            return false;
        }
        self.depth += 1;
        match number {
            Some(number) => self.visited.insert(number),
            None => true,
        }
    }
}

/// 读取 CMap 流字典的 `/UseCMap`（Table 120），间接引用解析一次。
///
/// 条目缺失、引用悬空或解析后既非名字也非流时返回 `None`，按「基座不可
/// 解析」降级。
fn entry(document: &Document, dict: &Dict) -> Option<UseCMap> {
    let raw = dict.get("UseCMap")?;
    let number = match raw {
        Value::Ref(number, _) => Some(*number),
        _ => None,
    };
    match document.resolve_value(raw).ok()? {
        Value::Name(name) => Some(UseCMap::Name(name)),
        value @ Value::Stream(_, _) => Some(UseCMap::Stream { value, number }),
        _ => None,
    }
}

/// 解析 Type 0 `/Encoding` CMap 流字典的 `/UseCMap` 基座（Table 120）。
///
/// 名字形态按 `roster`（内置名册：`Identity-H`/`Identity-V` + `predefined_cmaps`）
/// 解析；流形态解码后递归解析其自身的 `/UseCMap`。基座缺失、悬空、损坏、
/// 超深或成环时返回 `None`，引用方的本地映射照常可用。
pub(in crate::core::reader::font) fn resolve_cid_base(
    document: &Document,
    dict: &Dict,
    roster: fn(&str) -> FontResult<CidCMap>,
) -> Option<CidCMap> {
    cid_base_in_chain(document, dict, roster, &mut Chain::default())
}

fn cid_base_in_chain(
    document: &Document,
    dict: &Dict,
    roster: fn(&str) -> FontResult<CidCMap>,
    chain: &mut Chain,
) -> Option<CidCMap> {
    match entry(document, dict)? {
        UseCMap::Name(name) => roster(&name).ok(),
        UseCMap::Stream { value, number } => {
            if !chain.enter(number) {
                return None;
            }
            let Value::Stream(base_dict, _) = &value else {
                return None;
            };
            let bytes = document.decode_stream_bytes(&value).ok()?;
            let nested = cid_base_in_chain(document, base_dict, roster, chain);
            CidCMap::parse_with_base(&bytes, nested, roster).ok()
        }
    }
}

/// 解析 `/ToUnicode` CMap 流，连同其流字典的 `/UseCMap` 基座（Table 120）。
///
/// 非流值、解码失败或流本身不可解析时返回 `None`，与既有宽松提取路径一致：
/// 文本提取随后退回按编码解码，而不是让整个字体报错。
pub(in crate::core::reader::font) fn parse_to_unicode_stream(
    document: &Document,
    value: &Value,
) -> Option<ToUnicodeMap> {
    to_unicode_stream_in_chain(document, value, &mut Chain::default())
}

fn to_unicode_stream_in_chain(
    document: &Document,
    value: &Value,
    chain: &mut Chain,
) -> Option<ToUnicodeMap> {
    let Value::Stream(dict, _) = value else {
        return None;
    };
    let bytes = document.decode_stream_bytes(value).ok()?;
    let base = match entry(document, dict) {
        None => None,
        Some(UseCMap::Name(name)) => to_unicode::builtin_base(&name).ok(),
        Some(UseCMap::Stream { value, number }) => {
            if chain.enter(number) {
                to_unicode_stream_in_chain(document, &value, chain)
            } else {
                // 深度超限或成环：按「基座不可解析」降级，本流映射仍然可用，
                // 不能让此前可提取文本的字体因为一个坏基座而整体失去 /ToUnicode。
                None
            }
        }
    };
    match base {
        Some(base) => ToUnicodeMap::parse_with_base(&bytes, Some(base), to_unicode::builtin_base),
        None => ToUnicodeMap::parse_lenient(&bytes),
    }
}
