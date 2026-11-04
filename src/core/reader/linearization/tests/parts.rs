//! 附录 F 夹具的分部写手：对象体、交叉引用段与 trailer。
//!
//! 装配顺序与收敛驱动在 [`super::fixture`]；`Knobs`/`Guess` 与对象号常量在
//! [`super`]。

use super::*;

/// 经典主交叉引用表：返回 `(/T 值, 表关键字偏移)`。
pub(super) fn write_main_table(
    knobs: &Knobs,
    buf: &mut Vec<u8>,
    size: i64,
    second_group: [u64; 4],
    dict_at: u64,
) -> (u64, u64) {
    let main_at = buf.len() as u64;
    let mut entries: Vec<(u64, char)> = vec![(
        0,
        if knobs.main_first_entry_used {
            'n'
        } else {
            'f'
        },
    )];
    entries.extend(second_group.iter().map(|at| (*at, 'n')));
    if knobs.main_extra_entry {
        entries.push((dict_at, 'n'));
    }
    push(buf, &format!("xref\n0 {}", entries.len()));
    // Table F.1 `/T`：首条目**之前**的那个空白字符。
    let t_val = buf.len() as u64;
    push(buf, "\n");
    for (offset, kind) in entries {
        let generation = if kind == 'f' { 65535 } else { 0 };
        push(buf, &xref_row(offset, generation, kind));
    }
    // F.3.11：主 trailer 只有 `Size`，且没有 `Prev`。
    let extras = if knobs.main_trailer_extras {
        format!(" /Root {CATALOGUE} 0 R")
    } else {
        String::new()
    };
    push(buf, &format!("trailer\n<< /Size {size}{extras} >>\n"));
    (t_val, main_at)
}

/// 只有一份交叉引用表的形态（违反 F.3.1「exactly two sections」）。
pub(super) fn write_single_table(
    buf: &mut Vec<u8>,
    size: i64,
    second_group: [u64; 4],
    first_group: [u64; 6],
    length_at: u64,
) -> (u64, u64) {
    let main_at = buf.len() as u64;
    let mut entries: Vec<(u64, char)> = vec![(0, 'f')];
    entries.extend(second_group.iter().map(|at| (*at, 'n')));
    entries.extend(first_group.iter().map(|at| (*at, 'n')));
    if length_at > 0 {
        entries.push((length_at, 'n'));
    }
    push(buf, &format!("xref\n0 {}", entries.len()));
    let t_val = buf.len() as u64;
    push(buf, "\n");
    for (offset, kind) in entries {
        let generation = if kind == 'f' { 65535 } else { 0 };
        push(buf, &xref_row(offset, generation, kind));
    }
    push(
        buf,
        &format!("trailer\n<< /Size {size} /Root {CATALOGUE} 0 R >>\n"),
    );
    (t_val, main_at)
}

/// XRef 流承担主交叉引用（Table F.1 的 PDF 1.5 形态）：`/T` 取流对象偏移。
pub(super) fn write_main_xref_stream(
    knobs: &Knobs,
    buf: &mut Vec<u8>,
    size: i64,
    second_group: [u64; 4],
) -> (u64, u64) {
    let mut rows: Vec<u8> = vec![0, 0, 0, 0, 0, 0, 0, 0, 255];
    for offset in second_group {
        rows.extend_from_slice(&[1]);
        rows.extend_from_slice(&(offset as u32).to_be_bytes());
        rows.push(0);
    }
    let main_at = buf.len() as u64;
    let extras = if knobs.main_trailer_extras {
        format!(" /Root {CATALOGUE} 0 R")
    } else {
        String::new()
    };
    push(
        buf,
        &format!(
            "{MAIN_STREAM} 0 obj\n<< /Type /XRef /Size {size} /Index [0 5] /W [1 4 1] /Length {}{extras} >>\nstream\n",
            rows.len()
        ),
    );
    buf.extend_from_slice(&rows);
    push(buf, "\nendstream\nendobj\n");
    (main_at, main_at)
}

/// Table F.1 的字典正文：条目顺序即表内顺序，取值来自布局猜测。
pub(super) fn dict_body(knobs: &Knobs, g: Guess, dict_at: u64) -> String {
    let page_total = if knobs.wrong_page_count {
        PAGE_TOTAL + 1
    } else {
        PAGE_TOTAL
    };
    let hint = if knobs.bad_hint_target {
        format!("[{} 4]", dict_at)
    } else if knobs.overflow_hint {
        // Table F.1 的四整数形态：第二对（溢出 hint 流）复用首页内容流的范围。
        format!("[{} {} {} 20]", g.hint_off, g.hint_len, g.contents)
    } else {
        format!("[{} {}]", g.hint_off, g.hint_len)
    };
    let main_target = if knobs.bad_main_target { g.prev } else { g.t };
    let mut fields: Vec<String> = Vec::new();
    {
        let mut add = |key: &'static str, real: String| {
            if knobs.omit.contains(&key) {
                return;
            }
            match knobs.replaced.iter().find(|(name, _)| *name == key) {
                Some((_, raw)) => fields.push(format!("/{key} {raw}")),
                None => fields.push(format!("/{key} {real}")),
            }
        };
        add("Linearized", "1.0".to_string());
        add("L", g.l.to_string());
        add("H", hint);
        add("O", PAGE1.to_string());
        add("E", g.e.to_string());
        add("N", page_total.to_string());
        add("T", main_target.to_string());
    }
    // `/P` 是可选项：默认不写出（读端按 Default value: 0），显式取值或被
    // 替换时才进入字典。
    match knobs.replaced.iter().find(|(name, _)| *name == "P") {
        Some((_, raw)) => fields.push(format!("/P {raw}")),
        None => {
            if let Some(number) = knobs.first_page_number {
                fields.push(format!("/P {number}"));
            }
        }
    }
    format!("<< {} >>", fields.join(" "))
}

pub(super) fn object(number: i64, body: &str) -> String {
    format!("{number} 0 obj\n{body}\nendobj\n")
}

pub(super) fn stream_object(number: i64, extra_entries: &str) -> String {
    let payload = std::str::from_utf8(CONTENT_PAYLOAD).expect("content payload is ascii");
    format!(
        "{number} 0 obj\n<< /Length {}{extra_entries} >>\nstream\n{payload}\nendstream\nendobj\n",
        CONTENT_PAYLOAD.len()
    )
}

/// 定长 20 字节的经典 xref 条目。
pub(super) fn xref_row(offset: u64, generation: u32, kind: char) -> String {
    format!("{offset:010} {generation:05} {kind} \n")
}

pub(super) fn pages_body() -> String {
    format!("<< /Type /Pages /Kids [{PAGE1} 0 R {PAGE2} 0 R] /Count {PAGE_TOTAL} >>")
}

pub(super) fn catalogue_body(knobs: &Knobs) -> String {
    let mut items = vec!["/Type /Catalog".to_string(), format!("/Pages {PAGES} 0 R")];
    if knobs.catalogue_refs_dict {
        items.push(format!("/Linearization {DICT} 0 R"));
    }
    format!("<< {} >>", items.join(" "))
}

pub(super) fn first_page_body(knobs: &Knobs) -> String {
    let mut items = vec![
        "/Type /Page".to_string(),
        format!("/Parent {PAGES} 0 R"),
        format!("/Contents {CONTENTS1} 0 R"),
    ];
    if knobs.missing_page_attribute != "MediaBox" {
        items.push("/MediaBox [0 0 612 792]".to_string());
    }
    if knobs.missing_page_attribute != "Resources" {
        items.push(format!("/Resources << /Font << /F1 {FONT} 0 R >> >>"));
    }
    if knobs.first_page_refs_dict {
        items.push(format!("/Linear {DICT} 0 R"));
    }
    format!("<< {} >>", items.join(" "))
}

pub(super) fn page2_body() -> String {
    format!(
        "<< /Type /Page /Parent {PAGES} 0 R /MediaBox [0 0 612 792] /Contents {CONTENTS2} 0 R /Resources << /Font << /F1 {FONT} 0 R >> >> >>"
    )
}

pub(super) fn font_body() -> String {
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()
}

pub(super) fn info_body() -> String {
    "<< /Producer (pdf_engine Annex F fixture) >>".to_string()
}

pub(super) fn push(target: &mut Vec<u8>, text: &str) {
    target.extend_from_slice(text.as_bytes());
}
