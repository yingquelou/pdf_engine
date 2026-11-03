//! 附录 F 夹具的单轮装配：按分部顺序写字节流并回测实际布局。
//!
//! 分部写手（对象体、交叉引用段与 trailer）在 [`super::parts`]；`Knobs`/`Guess`
//! 与定点迭代收敛驱动在 [`super`]。

use super::parts::*;
use super::*;

pub(super) fn build_once(knobs: &Knobs, g: Guess) -> (Vec<u8>, Guess) {
    let size: i64 = if knobs.main_is_stream { 12 } else { 11 };
    let mut buf: Vec<u8> = Vec::new();

    // part 1（F.3.2：「shall begin with the standard header line」）。
    push(&mut buf, &format!("%PDF-{}\n", knobs.version));
    if knobs.padding > 0 {
        push(&mut buf, &format!("%{}\n", "0".repeat(knobs.padding)));
    }

    // 首页表起始号前移时，对象 4 必须先于该表写出（否则表项偏移未知）。
    let mut early_info: Option<u64> = None;
    if knobs.dict_entry_not_first {
        let at = buf.len() as u64;
        push(&mut buf, &object(INFO, &info_body()));
        early_info = Some(at);
    }

    // part 2（F.3.3：正文第一个对象 = 线性化参数字典）。
    let mut decoy_at: Option<u64> = None;
    if knobs.decoy_first {
        decoy_at = Some(buf.len() as u64);
        push(&mut buf, &object(PAGES, &pages_body()));
    }
    let dict_at = buf.len() as u64;
    push(
        &mut buf,
        &format!(
            "{DICT} {} obj\n{}\nendobj\n",
            knobs.dict_generation,
            dict_body(knobs, g, dict_at)
        ),
    );

    // `/Length` 的间接引用对象紧随 hint 流（Table F.1 的开销规则）。
    let mut length_at = g.length_obj;

    // part 3（F.3.4：首页交叉引用表 + 其 trailer + 可忽略的 startxref 块）。
    let mut first_xref = g.first_xref;
    if !knobs.single_table {
        first_xref = buf.len() as u64;
        let start = if knobs.dict_entry_not_first {
            INFO
        } else {
            DICT
        };
        let count = HINT - start + 1;
        let estimate = |n: i64| -> u64 {
            match n {
                INFO => early_info.unwrap_or(0),
                DICT => dict_at,
                CATALOGUE => g.cat,
                PAGE1 => g.page,
                CONTENTS1 => g.contents,
                FONT => g.font,
                _ => g.hint_off,
            }
        };
        let mut rows: Vec<u64> = (start..=HINT).map(estimate).collect();
        if knobs.swap_hint_entry {
            let len = rows.len();
            rows.swap(len - 2, len - 1);
        }
        push(&mut buf, &format!("xref\n{start} {count}\n"));
        for (index, number) in (start..=HINT).enumerate() {
            // 两个旋钮各把首页表里的一个条目写成 free（F.3.3/F.3.4 反例）。
            let free = (number == DICT && knobs.dict_entry_free)
                || (number == FONT && knobs.first_page_free_entry);
            let kind = if free { 'f' } else { 'n' };
            let generation = if number == DICT {
                knobs.dict_entry_generation
            } else {
                0
            };
            push(&mut buf, &xref_row(rows[index], generation, kind));
        }
        let mut trailer = format!("trailer\n<< /Size {size} /Prev {}", g.prev);
        if knobs.first_page_trailer_refs_dict {
            trailer.push_str(&format!(" /Linear {DICT} 0 R"));
        }
        // 表尾可选的 startxref/整数/%%EOF：F.3.4「This information shall be
        // ignored」，夹具照样写出以证明读端不受它干扰。
        trailer.push_str(&format!(
            " /Root {CATALOGUE} 0 R /Info {INFO} 0 R >>\nstartxref\n0\n%%EOF\n"
        ));
        push(&mut buf, &trailer);
    }

    // part 5（F.3.6：hint 表封装在流对象里，取最后一个对象号）。
    let hint_at = buf.len() as u64;
    let payload = knobs.hint_payload.unwrap_or(HINT_PAYLOAD);
    let entries = knobs.hint_entries;
    let hint_overhead = if knobs.indirect_length() {
        format!("{HINT} 0 obj\n<< /Length {INDIRECT_LENGTH} 0 R{entries} >>\nstream\n")
    } else {
        format!(
            "{HINT} 0 obj\n<< /Length {}{entries} >>\nstream\n",
            payload.len()
        )
    };
    push(&mut buf, &hint_overhead);
    buf.extend_from_slice(payload);
    push(&mut buf, "\nendstream\nendobj\n");
    if knobs.indirect_length() {
        length_at = buf.len() as u64;
        push(
            &mut buf,
            &format!(
                "{INDIRECT_LENGTH} 0 obj\n{}\nendobj\n",
                (payload.len() as i64) + hint_overhead.len() as i64 + 21
            ),
        );
    }
    let hint_len = buf.len() as u64 - hint_at;

    // part 6（F.3.5/F.3.7：首页对象组 = 目录 + 首页页对象 + 其资源）。
    let cat_at = buf.len() as u64;
    push(&mut buf, &object(CATALOGUE, &catalogue_body(knobs)));
    let page_at = buf.len() as u64;
    push(&mut buf, &object(PAGE1, &first_page_body(knobs)));
    let contents_at = buf.len() as u64;
    push(&mut buf, &stream_object(CONTENTS1));
    let font_at = buf.len() as u64;
    push(&mut buf, &object(FONT, &font_body()));
    // `/E`：首页对象组末尾（F.3.1 第 6 项与第 7 项的分界）。
    let e_val = buf.len() as u64;

    // part 7（其余页的对象组，第二组自 1 起编号）。
    let pages_at = match decoy_at {
        Some(at) => at,
        None => {
            let at = buf.len() as u64;
            push(&mut buf, &object(PAGES, &pages_body()));
            at
        }
    };
    let page2_at = buf.len() as u64;
    push(&mut buf, &object(PAGE2, &page2_body()));
    let contents2_at = buf.len() as u64;
    push(&mut buf, &stream_object(CONTENTS2));
    let info_at = match early_info {
        Some(at) => at,
        None => {
            let at = buf.len() as u64;
            push(&mut buf, &object(INFO, &info_body()));
            at
        }
    };

    // part 9/11（主交叉引用表与主 trailer）。
    let (t_val, main_at) = if knobs.main_is_stream {
        write_main_xref_stream(
            knobs,
            &mut buf,
            size,
            [pages_at, page2_at, contents2_at, info_at],
        )
    } else if knobs.single_table {
        write_single_table(
            &mut buf,
            size,
            [pages_at, page2_at, contents2_at, info_at],
            [dict_at, cat_at, page_at, contents_at, font_at, hint_at],
            length_at,
        )
    } else {
        write_main_table(
            knobs,
            &mut buf,
            size,
            [pages_at, page2_at, contents2_at, info_at],
            dict_at,
        )
    };

    // 文件尾：F.3.11「The startxref line shall give the offset of the
    // first-page cross-reference table」（单表形态即该表自身）。
    let startxref_target = if knobs.single_table {
        main_at
    } else {
        first_xref
    };
    push(&mut buf, &format!("startxref\n{startxref_target}\n%%EOF\n"));
    first_xref = startxref_target;

    let measured = Guess {
        l: buf.len() as u64,
        hint_off: hint_at,
        hint_len,
        e: e_val,
        t: t_val,
        prev: main_at,
        first_xref,
        cat: cat_at,
        page: page_at,
        contents: contents_at,
        font: font_at,
        length_obj: length_at,
    };
    (buf, measured)
}
