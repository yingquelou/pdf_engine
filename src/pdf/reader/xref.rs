//! xref 解析策略链、XRef 流与全文扫描回退。

use crate::pdf::filters::decode_stream;
use crate::pdf::types::{
    PdfFeature, PdfVersion, SubXref, Trailer, Value, Xref, XrefEntry, XrefKind,
};
use std::collections::BTreeMap;

use super::lexer::Lexer;
use super::object::{parse_indirect_object, parse_trailer, parse_xref, read_field};
use super::parser::ObjectSource;
use super::{DEFAULT_WINDOW, Result, SCAN_CHUNK, SCAN_OVERLAP, err, find_subslice};

// ── xref 解析策略链（策略模式 + 责任链） ──────────────────────────
//
// 每种 xref 解析方式实现 [`XrefStrategy`]，由 [`XrefChain`] 按固定
// 顺序依次尝试。新增解析方式只需实现 trait 并加入链，不改主干
// （开闭原则）。策略链工作于「已读入的 xref 窗口」`&[u8]`，
// 经 [`ObjectSource`] 按需读取流字节 / 倒找窗口，不把整个文件加载
// 到内存。

/// xref 解析上下文：携带 startxref 偏移与声明版本。偏移为绝对
/// 文件偏移（`u64`）。
///
/// `version` 供策略链记「宽读说明」（声明版本与实际内容不符时提示），
/// 但**不再作为硬门控**：XRef 流自描述（`/Type /XRef`），按内容判定。
pub(super) struct XrefContext {
    pub xref_off: u64,
    pub version: PdfVersion,
}

/// xref 解析策略。
///
/// `try_resolve` 返回：
/// - `Ok(Some(pair))`：本策略成功解析出 `(xref, trailer)`；
/// - `Ok(None)`：本策略不适用当前上下文，交给下一策略；
/// - `Err(e)`：本策略适用但解析失败，错误由链记为 warning。
pub(super) trait XrefStrategy {
    fn try_resolve(
        &self,
        xref_win: &[u8],
        xref_off: u64,
        source: &mut dyn ObjectSource,
        ctx: &XrefContext,
    ) -> Result<Option<(Xref, Trailer)>>;
}

/// 策略链：依次尝试各策略，收集 warning，全部失败才返回 Err。
pub(super) struct XrefChain {
    strategies: Vec<Box<dyn XrefStrategy>>,
}

impl Default for XrefChain {
    /// 默认链顺序：经典表 → XRef 流 → 全文扫描。
    fn default() -> Self {
        Self {
            strategies: vec![
                Box::new(ClassicXrefStrategy),
                Box::new(XrefStreamStrategy),
                Box::new(FullScanStrategy),
            ],
        }
    }
}

impl XrefChain {
    /// 依次尝试策略；某策略返回 `Ok(Some)` 即返回；返回 `Ok(None)`
    /// 跳到下一策略；`Err` 记 warning 后继续。
    pub(super) fn resolve(
        &self,
        xref_win: &[u8],
        xref_off: u64,
        source: &mut dyn ObjectSource,
        ctx: &XrefContext,
        warnings: &mut Vec<String>,
    ) -> Result<(Xref, Trailer)> {
        for (i, strat) in self.strategies.iter().enumerate() {
            match strat.try_resolve(xref_win, xref_off, source, ctx) {
                Ok(Some(pair)) => {
                    // 宽读说明：文件头声明版本 < 1.5（不支持 XRef 流），但
                    // 实际解析结果是一份 XRef 流（trailer 字典带
                    // `/Type /XRef`）。按内容而非声明版本判定，仅提示一句。
                    if !ctx.version.supports(PdfFeature::XRefStream)
                        && pair.1.dict.get("Type").and_then(|v| v.as_name()) == Some("XRef")
                    {
                        warnings.push(format!(
                            "declared version {} does not support xref streams, but an xref \
                             stream was found and parsed by content (wide read)",
                            ctx.version
                        ));
                    }
                    return Ok(pair);
                }
                Ok(None) => continue,
                Err(e) => {
                    let name = strategy_name(i);
                    warnings.push(format!("{name} parse failed: {e}"));
                }
            }
        }
        Err(err(0, "all xref strategies failed"))
    }
}

fn strategy_name(idx: usize) -> &'static str {
    match idx {
        0 => "classic xref",
        1 => "xref stream",
        2 => "full scan",
        _ => "strategy",
    }
}

/// 经典交叉引用表策略：startxref 偏移处是 `xref` 关键字。
/// 若偏移无效，退而在 startxref 之前倒找最近的 `xref`。
struct ClassicXrefStrategy;

impl XrefStrategy for ClassicXrefStrategy {
    fn try_resolve(
        &self,
        xref_win: &[u8],
        xref_off: u64,
        source: &mut dyn ObjectSource,
        ctx: &XrefContext,
    ) -> Result<Option<(Xref, Trailer)>> {
        // xref_win 是从 xref_off 起读的窗口；在其中定位 `xref`。
        let target_rel = if ctx.xref_off > 0 && Some(ctx.xref_off) <= Some(source.file_len()) {
            // 窗口正好从 xref_off 起，相对偏移 0
            0usize
        } else {
            // 倒找：在 xref_win 内找最后一个 `xref`
            let mut found = None;
            let mut from = 0usize;
            while let Some(p) = find_subslice(xref_win, b"xref", from) {
                found = Some(p);
                from = p + 4;
            }
            match found {
                Some(p) => p,
                None => return Ok(None), // 本策略不适用
            }
        };
        let mut lex = Lexer::at(xref_win, xref_off);
        lex.pos = target_rel;
        lex.skip_ws();
        if !lex.starts_with(b"xref") {
            return Ok(None);
        }
        let x = parse_xref(&mut lex)?;
        let t = parse_trailer(&mut lex)?;
        Ok(Some((x, t)))
    }
}

/// PDF 1.5 XRef 流策略：startxref 指向流对象（对象号以数字开头）且该
/// 对象是 `/Type /XRef` 流。
///
/// **按内容而非声明版本判定**：XRef 流自描述（`/Type /XRef`），故不因
/// 文件头声明 < 1.5 而跳过——真实世界存在声明版本偏低但实际含 XRef 流
/// 的文件。压缩流亦尝试解码（见 [`parse_xref_stream`]）；解码失败返回
/// Err 由链记 warning 并回退。
struct XrefStreamStrategy;

impl XrefStrategy for XrefStreamStrategy {
    fn try_resolve(
        &self,
        xref_win: &[u8],
        xref_off: u64,
        source: &mut dyn ObjectSource,
        ctx: &XrefContext,
    ) -> Result<Option<(Xref, Trailer)>> {
        if ctx.xref_off == 0 {
            return Ok(None);
        }
        // xref_win 从 xref_off 起；XRef 流对象以数字开头（对象号）。
        let mut lex = Lexer::at(xref_win, xref_off);
        lex.skip_ws();
        if !matches!(lex.peek(), Some(b) if b.is_ascii_digit()) {
            return Ok(None);
        }
        let pair = parse_xref_stream(&mut lex, xref_off, source)?;
        Ok(Some(pair))
    }
}

/// 解析 XRef 流对象。流字典含 /W /Index /Size；流字节先按
/// `/Filter` + `/DecodeParms`（含 Flate 与 Predictor）解码，再按行宽
/// 解析为 xref 条目。压缩流因此可读；解码失败返回 Err 由调用方回退。
///
/// 流字节不是 `Vec<u8>` 而是 `StreamData::Ref`，故需经
/// [`ObjectSource::read_at`] 从文件读取流字节再解码。
pub(super) fn parse_xref_stream(
    lex: &mut Lexer<'_>,
    offset: u64,
    source: &mut dyn ObjectSource,
) -> Result<(Xref, Trailer)> {
    let obj = parse_indirect_object(lex)?;
    let (dict, stream_ref) = match obj.value {
        Value::Stream(d, ref sd) => (d, sd.clone()),
        _ => return Err(err(offset, "xref target is not a stream")),
    };
    // 校验类型
    if dict.get("Type").and_then(|v| v.as_name()) != Some("XRef") {
        return Err(err(offset, "stream is not /Type /XRef"));
    }
    let w = dict
        .get("W")
        .and_then(|v| v.as_array())
        .ok_or_else(|| err(offset, "xref stream missing /W"))?;
    if w.len() < 3 {
        return Err(err(offset, "xref stream /W must have 3 entries"));
    }
    let w1 = w.iter().next().unwrap().as_integer().unwrap_or(0) as usize;
    let w2 = w.iter().nth(1).unwrap().as_integer().unwrap_or(0) as usize;
    let w3 = w.iter().nth(2).unwrap().as_integer().unwrap_or(0) as usize;
    let row = w1 + w2 + w3;
    if row == 0 {
        return Err(err(offset, "xref stream /W sums to zero"));
    }

    // 取流字节：从 source.read_at 读 StreamData::Ref 指向的范围。
    let raw = match stream_ref {
        crate::pdf::types::StreamData::Owned(b) => b,
        crate::pdf::types::StreamData::Ref { offset: so, len } => source
            .read_at(so, len)
            .ok_or_else(|| err(offset, "failed to read xref stream bytes from file"))?,
        crate::pdf::types::StreamData::EncryptedRef { .. } => {
            return Err(err(offset, "xref streams must not be encrypted"));
        }
    };
    // 按 /Filter + /DecodeParms 解码（压缩 XRef 流常见 /FlateDecode +
    // /Predictor）。解码失败（不支持的过滤器 / 数据损坏）返回 Err，
    // 由策略链记 warning 并回退。
    let bytes = decode_stream(&dict, &raw)
        .map_err(|e| err(offset, format!("xref stream decode failed: {e}")))?;

    // /Index [[start count] ...]，缺省 [[0 Size]]
    let size = dict.get("Size").and_then(|v| v.as_integer()).unwrap_or(0);
    let mut index: Vec<(i64, i64)> = Vec::new();
    if let Some(arr) = dict.get("Index").and_then(|v| v.as_array()) {
        let nums: Vec<i64> = arr.iter().filter_map(|v| v.as_integer()).collect();
        let mut i = 0;
        while i + 1 < nums.len() {
            index.push((nums[i], nums[i + 1]));
            i += 2;
        }
    } else {
        index.push((0, size));
    }

    let mut subs: Vec<SubXref> = Vec::new();
    let mut pos = 0usize;
    for (start, count) in index {
        let mut entries = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count {
            if pos + row > bytes.len() {
                break;
            }
            // ISO 32000-1 Table 17：某字段宽度为 0 表示该字段缺省。
            // type（w1）缺省为 1（in-use、非压缩、非对象流内）；
            // offset（w2）与 generation（w3）缺省为 0。
            let t = if w1 == 0 {
                1
            } else {
                read_field(&bytes, pos, w1)
            };
            let f2 = if w2 == 0 {
                0
            } else {
                read_field(&bytes, pos + w1, w2)
            };
            let f3 = if w3 == 0 {
                0
            } else {
                read_field(&bytes, pos + w1 + w2, w3)
            };
            pos += row;
            let entry = match t {
                0 => XrefEntry {
                    offset: f2 as i64,
                    generation: f3 as i64,
                    kind: XrefKind::Free,
                },
                1 => XrefEntry {
                    offset: f2 as i64,
                    generation: f3 as i64,
                    kind: XrefKind::Normal,
                },
                2 => {
                    // 对象流内：f2 为对象流对象号、f3 为流内索引，
                    // 由 kind 显式表达，不再塞进 offset/generation。
                    XrefEntry {
                        offset: 0,
                        generation: 0,
                        kind: XrefKind::InObjStream {
                            stream_num: f2 as i64,
                            index: f3 as i64,
                        },
                    }
                }
                _ => continue,
            };
            entries.push(entry);
        }
        subs.push(SubXref::new(start, entries));
    }

    let xref = Xref(subs);
    // trailer 字段就在该流字典里
    let trailer = Trailer { dict };
    Ok((xref, trailer))
}

/// 把补充 XRef 流的条目并入基础 xref：同一对象号以 `extra` 为准。
///
/// 实现为把 `extra` 的各子段插到最前——[`Xref::get`] 按顺序返回
/// 首个命中的子段，故 `extra` 覆盖的号优先，其余号仍由基础表提供。
/// 用于混合引用文件（hybrid-reference）：经典 trailer 的 `/XRefStm`
/// 指向的 XRef 流条目对经典表不可见，需以补充信息并入。
pub(super) fn merge_xref(base: &mut Xref, extra: &Xref) {
    let mut subs = extra.0.clone();
    subs.append(&mut base.0);
    base.0 = subs;
}

/// 全文扫描兜底策略：无视 startxref，从文件头分块流式扫描所有
/// `N G obj` 起始偏移自建 xref + 取最后 `trailer << ... >>` 作 trailer。
///
/// **分块流式**：从文件头顺序读 256KB 块，维护滑动窗口（前块尾部
/// 与当前块拼接）处理跨块关键字，对每个 `N G obj` 起始记录文件偏移，
/// 不解析对象内容。trailer 用块内查找定位后单独读 64KB 窗口解析。
/// 不一次性加载整个文件。
struct FullScanStrategy;

impl XrefStrategy for FullScanStrategy {
    fn try_resolve(
        &self,
        _xref_win: &[u8],
        _xref_off: u64,
        source: &mut dyn ObjectSource,
        _ctx: &XrefContext,
    ) -> Result<Option<(Xref, Trailer)>> {
        let pair = scan_all_objects(source)?;
        Ok(Some(pair))
    }
}

/// 全文扫描实现：分块流式遍历文件，记录每个 `N G obj` 起始偏移与
/// 最后一个 `trailer` 偏移。trailer 单独读窗口解析。
pub(crate) fn scan_all_objects(source: &mut dyn ObjectSource) -> Result<(Xref, Trailer)> {
    let file_len = source.file_len();
    let mut entries: BTreeMap<i64, XrefEntry> = BTreeMap::new();
    let mut last_trailer_off: Option<u64> = None;

    // 滑动窗口：保留前块尾部 SCAN_OVERLAP 字节 + 当前 SCAN_CHUNK 块，
    // 总大小 ≤ SCAN_CHUNK + SCAN_OVERLAP，控制内存。
    let mut win: Vec<u8> = Vec::new();
    let mut win_base: u64 = 0u64;
    let mut file_pos: u64 = 0u64;
    while file_pos < file_len {
        let take = (SCAN_CHUNK as u64).min(file_len - file_pos) as usize;
        let mut chunk = match source.read_at(file_pos, take as u64) {
            Some(c) => c,
            None => break,
        };
        // 拼接：保留前块尾部以处理跨块关键字
        let keep = win.len().min(SCAN_OVERLAP);
        let mut new_win = Vec::with_capacity(keep + chunk.len());
        let tail_start = win.len().saturating_sub(keep);
        new_win.extend_from_slice(&win[tail_start..]);
        new_win.append(&mut chunk);
        win_base = file_pos.saturating_sub(keep as u64);
        // 注意：win_base 是 new_win[0] 对应的文件偏移。
        // 但 new_win 前 keep 字节来自 file_pos 之前，故 win_base = file_pos - keep。
        win = new_win;
        file_pos += take as u64;

        // 在 win 内扫描 `N G obj` 与 `trailer` 起始偏移。
        scan_window(&win, win_base, &mut entries, &mut last_trailer_off);

        // 控制内存：保留尾部 SCAN_OVERLAP 给下一轮拼接。
        if win.len() > SCAN_OVERLAP {
            let cut = win.len() - SCAN_OVERLAP;
            win.drain(..cut);
            win_base += cut as u64;
        }
    }

    // 扫描最后残留窗口
    if !win.is_empty() {
        scan_window(&win, win_base, &mut entries, &mut last_trailer_off);
    }

    // 解析最后一个 trailer（若有）：单独读 64KB 窗口
    let last_trailer = match last_trailer_off {
        Some(off) => {
            let twin = source
                .read_at(off, DEFAULT_WINDOW as u64)
                .ok_or_else(|| err(off, "failed to read trailer window"))?;
            let mut lex = Lexer::at(&twin, off);
            parse_trailer(&mut lex).unwrap_or_default()
        }
        None => Trailer::default(),
    };

    if entries.is_empty() && last_trailer_off.is_none() {
        return Err(err(0, "full scan found no objects and no trailer"));
    }

    // 补齐缺失号为自由条目，使 entries 从 0 起连续编号，
    // 与 Xref::get 的 idx 查找语义一致。
    let max_num = *entries.keys().max().unwrap_or(&0);
    let mut full: Vec<XrefEntry> = Vec::with_capacity(max_num as usize + 1);
    for n in 0..=max_num {
        match entries.get(&n) {
            Some(e) => full.push(e.clone()),
            None => full.push(XrefEntry {
                offset: 0,
                generation: 65535,
                kind: XrefKind::Free,
            }),
        }
    }
    let subs = vec![SubXref::new(0, full)];
    let xref = Xref(subs);
    Ok((xref, last_trailer))
}

/// 在单个滑动窗口内扫描 `N G obj` 起始与 `trailer` 起始偏移。
///
/// `N G obj` 模式：数字 空白 数字 空白 `obj`。匹配成功则记录
/// `win_base + 数字起始偏移` 为对象起始（即对象号数字的起始位置，
/// 与经典 xref 偏移语义一致）。`trailer` 关键字起始偏移同样记录。
fn scan_window(
    win: &[u8],
    win_base: u64,
    entries: &mut BTreeMap<i64, XrefEntry>,
    last_trailer_off: &mut Option<u64>,
) {
    let mut i = 0usize;
    while i < win.len() {
        // 跳过空白
        while i < win.len() && Lexer::is_whitespace(win[i]) {
            i += 1;
        }
        if i >= win.len() {
            break;
        }
        let b = win[i];
        if b == b't' && win[i..].starts_with(b"trailer") {
            // 确认后跟空白/分隔符（避免匹配 trailerXXX）
            let after = i + 7;
            let ok = after >= win.len()
                || Lexer::is_whitespace(win[after])
                || Lexer::is_delim(win[after]);
            if ok {
                *last_trailer_off = Some(win_base + i as u64);
            }
            i += 7;
            continue;
        }
        if b.is_ascii_digit() {
            // 尝试匹配 N G obj
            if let Some((num, generation, end)) = try_match_obj_header(win, i) {
                let abs = win_base + i as u64;
                entries.insert(
                    num,
                    XrefEntry {
                        offset: abs as i64,
                        generation,
                        kind: XrefKind::Normal,
                    },
                );
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

/// 尝试在 `win` 的 `start` 位置匹配 `N G obj` 头。
/// 成功返回 `Some((num, generation, end_pos))`，`end_pos` 为 `obj` 之后的索引。
fn try_match_obj_header(win: &[u8], start: usize) -> Option<(i64, i64, usize)> {
    let mut i = start;
    let num = read_int_at(win, &mut i)?;
    skip_ws_inline(win, &mut i);
    let generation = read_int_at(win, &mut i)?;
    skip_ws_inline(win, &mut i);
    if win[i..].starts_with(b"obj") {
        let after = i + 3;
        // obj 后应跟空白/分隔符/EOF
        let ok =
            after >= win.len() || Lexer::is_whitespace(win[after]) || Lexer::is_delim(win[after]);
        if ok {
            return Some((num, generation, after));
        }
    }
    None
}

/// 从 `*i` 起读一个十进制整数（允许前导 `-`/`+`）。成功返回值并推进 `*i`。
fn read_int_at(win: &[u8], i: &mut usize) -> Option<i64> {
    let start = *i;
    if *i < win.len() && (win[*i] == b'-' || win[*i] == b'+') {
        *i += 1;
    }
    let digits_start = *i;
    while *i < win.len() && win[*i].is_ascii_digit() {
        *i += 1;
    }
    if *i == digits_start {
        // 没有数字：回退符号
        *i = start;
        return None;
    }
    let s = std::str::from_utf8(&win[start..*i]).ok()?;
    s.parse::<i64>().ok()
}

fn skip_ws_inline(win: &[u8], i: &mut usize) {
    while *i < win.len() && Lexer::is_whitespace(win[*i]) {
        *i += 1;
    }
}
