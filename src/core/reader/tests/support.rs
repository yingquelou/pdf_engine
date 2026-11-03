//! 线性化 PDF 测试基础设施：构造器、压缩辅助、共享夹具。

use super::*;
use crate::core::filters::decode_stream;
use crate::core::types::{Document, Value};

/// zlib 压缩（仅测试用；生产端只解压不压缩）。
fn zlib(data: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    let mut enc = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).expect("zlib compress");
    enc.finish().expect("zlib finish")
}

/// 首页内容流操作符（线性化样本的原始载荷）。
pub(super) const LINEARIZED_CONTENT_OPS: &[u8] = b"BT /F1 12 Tf 72 720 Td (Hello linearized) Tj ET";

/// 线性化字典四个布局敏感键 `/L /H /E /T` 的取值（`H` 为展开后的数对序列）。
type LinearizedOffsets = (u64, Vec<u64>, u64, u64);

/// 定点迭代收敛包装：线性化字典自身的字节数影响其后所有对象的偏移，而
/// `/L /H /E /T` 的数值位数又反过来影响字典字节数。以 `initial` 为猜测
/// 起步反复重建，直到用猜测值建出的布局测量出的四键与猜测一致（布局
/// 自洽），最多 5 轮。
fn converge_linearized(
    initial: LinearizedOffsets,
    build: impl Fn(&LinearizedOffsets) -> (Vec<u8>, LinearizedOffsets),
) -> Vec<u8> {
    let mut guess = initial;
    for _ in 0..5 {
        let (bytes, values) = build(&guess);
        if values == guess {
            return bytes;
        }
        guess = values;
    }
    panic!("linearized fixture failed to converge");
}

/// XRef 流条目（`/W [1 2 1]`：1 字节 type + 2 字节字段 + 1 字节字段）。
fn xref_row(t: u8, f2: u16, f3: u8) -> Vec<u8> {
    let mut v = vec![t];
    v.extend_from_slice(&f2.to_be_bytes());
    v.push(f3);
    v
}

/// 取对象流字节并按 `/Filter` 解码（测试辅助：`stream_bytes` 只给原始
/// 未解码字节，与 ObjStm 解析路径一致，需再过 `decode_stream`）。
pub(super) fn decoded_stream(doc: &Document, num: i64) -> Vec<u8> {
    let obj = doc.get_object(num).expect("parse").expect("object");
    let raw = doc.stream_bytes(&obj.value).expect("stream bytes");
    match &obj.value {
        Value::Stream(dict, _) => decode_stream(dict, &raw).expect("decode stream"),
        other => panic!("object {num} is not a stream: {other:?}"),
    }
}

/// 经典 xref 表线性化 PDF（PDF 1.4）构造器。
///
/// 物理布局严格按附录 F：对象 1 = 线性化参数字典（文件第一个对象，
/// `/O 5` 指首页 Page）；对象 2 = hint 流；对象 3–7 = Catalog / Pages /
/// Page / Contents(Flate) / Font；末尾经典 xref + trailer + startxref。
///
/// - `hint_pairs`：`/H` 数组对数（1 = 仅首页 hint 对；2 = 首页 + 整体
///   hint 对，两对同指对象 2 的流）；
/// - `corrupt_hint`：真时 hint 流声明 `/FlateDecode` 但载荷为垃圾字节。
pub(super) fn build_linearized_classic(
    hint_pairs: usize,
    hint_payload: &[u8],
    corrupt_hint: bool,
) -> Vec<u8> {
    let hint_bytes: Vec<u8> = if corrupt_hint {
        b"\x00\x01not-zlib-garbage\xff".to_vec()
    } else {
        zlib(hint_payload)
    };
    let content_bytes = zlib(LINEARIZED_CONTENT_OPS);
    let initial: LinearizedOffsets = (0, vec![0; hint_pairs * 2], 0, 0);
    converge_linearized(initial, move |(l, h, e, t)| {
        let h_str = h
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let head = format!(
            "%PDF-1.4\n1 0 obj\n<< /Linearized 1 /L {l} /H [{h_str}] /O 5 /E {e} /N 1 /T {t} >>\nendobj\n"
        );
        let mut buf = head.into_bytes();

        // 对象 2：hint 流（普通 Flate 流；损坏变体载荷为不可解压垃圾）。
        let off2 = buf.len();
        buf.extend_from_slice(
            format!(
                "2 0 obj\n<< /Filter /FlateDecode /Length {} >>\nstream\n",
                hint_bytes.len()
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&hint_bytes);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        let hint_len = hint_bytes.len() as u64;

        // 对象 3–7：首页对象组（Catalog/Pages/Page/Contents）与其余对象。
        let off3 = buf.len();
        buf.extend_from_slice(b"3 0 obj\n<< /Type /Catalog /Pages 4 0 R >>\nendobj\n");
        let off4 = buf.len();
        buf.extend_from_slice(b"4 0 obj\n<< /Type /Pages /Kids [5 0 R] /Count 1 >>\nendobj\n");
        let off5 = buf.len();
        buf.extend_from_slice(b"5 0 obj\n<< /Type /Page /Parent 4 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 7 0 R >> >> >>\nendobj\n");
        let off6 = buf.len();
        buf.extend_from_slice(
            format!(
                "6 0 obj\n<< /Filter /FlateDecode /Length {} >>\nstream\n",
                content_bytes.len()
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&content_bytes);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        let e_val = buf.len() as u64; // /E：首页对象组末尾（对象 6 endobj 之后）
        let off7 = buf.len();
        buf.extend_from_slice(
            b"7 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        );

        // 经典 xref：子段 0..8（对象 0 free + 1..=7）。
        let xref_off = buf.len();
        buf.extend_from_slice(b"xref\n0 8\n");
        let t_val = buf.len() as u64; // /T：主 xref 表首条目（对象 0）偏移
        buf.extend_from_slice(b"0000000000 65535 f \n");
        for off in [
            9u64,
            off2 as u64,
            off3 as u64,
            off4 as u64,
            off5 as u64,
            off6 as u64,
            off7 as u64,
        ] {
            buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        buf.extend_from_slice(
            format!("trailer\n<< /Size 8 /Root 3 0 R >>\nstartxref\n{xref_off}\n%%EOF\n")
                .as_bytes(),
        );

        // 测量四键真值：/L = 文件总长；/H = (hint 流偏移, 流长度) × 对数。
        let l_val = buf.len() as u64;
        let mut h_val = Vec::with_capacity(hint_pairs * 2);
        for _ in 0..hint_pairs {
            h_val.push(off2 as u64);
            h_val.push(hint_len);
        }
        (buf, (l_val, h_val, e_val, t_val))
    })
}

/// XRef 流 + ObjStm 线性化 PDF（PDF 1.5）构造器。
///
/// 物理布局：对象 1 = 线性化参数字典（第一个**普通**对象——规范要求线性化
/// 字典不得打入对象流）；对象 2 = hint 流（普通 Flate 流）；对象 8 =
/// ObjStm（打包 Catalog/Pages/Page/Font，即对象 3/4/5/7）；对象 6 = 首页
/// Contents（普通 Flate 流）；对象 9 = XRef 流（`/W [1 2 1]`，trailer
/// 语义由流字典 `/Root` 承载）。`/E` 取首页对象组物理末尾（对象 6 之后）。
pub(super) fn build_linearized_xref_stream(hint_payload: &[u8]) -> Vec<u8> {
    let hint_bytes = zlib(hint_payload);
    let content_bytes = zlib(LINEARIZED_CONTENT_OPS);

    // ObjStm 载荷：头部「对象号 相对偏移」× 4 + 对象本体。相对布局与绝对
    // 偏移无关，可在此预先组装定稿。
    let objects: [(i64, &[u8]); 4] = [
        (3, b"<< /Type /Catalog /Pages 4 0 R >>"),
        (4, b"<< /Type /Pages /Kids [5 0 R] /Count 1 >>"),
        (
            5,
            b"<< /Type /Page /Parent 4 0 R /MediaBox [0 0 612 792] /Contents 6 0 R \
              /Resources << /Font << /F1 7 0 R >> >> >>",
        ),
        (7, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"),
    ];
    let mut header = Vec::new();
    let mut body = Vec::new();
    for (num, obj) in objects {
        header.extend_from_slice(format!("{num} {} ", body.len()).as_bytes());
        body.extend_from_slice(obj);
        body.push(b'\n');
    }
    let first = header.len();
    let mut stm = header;
    stm.extend_from_slice(&body);
    let stm_bytes = zlib(&stm);

    converge_linearized((0, vec![0, 0], 0, 0), move |(l, h, e, t)| {
        let head = format!(
            "%PDF-1.5\n1 0 obj\n<< /Linearized 1 /L {l} /H [{} {}] /O 5 /E {e} /N 1 /T {t} >>\nendobj\n",
            h[0], h[1]
        );
        let mut buf = head.into_bytes();

        // 对象 2：hint 流。
        let off2 = buf.len();
        buf.extend_from_slice(
            format!(
                "2 0 obj\n<< /Filter /FlateDecode /Length {} >>\nstream\n",
                hint_bytes.len()
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&hint_bytes);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        let hint_len = hint_bytes.len() as u64;

        // 对象 8：ObjStm（内含对象 3/4/5/7）。
        let off8 = buf.len();
        buf.extend_from_slice(
            format!(
                "8 0 obj\n<< /Type /ObjStm /N 4 /First {first} /Filter /FlateDecode /Length {} >>\nstream\n",
                stm_bytes.len()
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&stm_bytes);
        buf.extend_from_slice(b"\nendstream\nendobj\n");

        // 对象 6：首页 Contents。
        let off6 = buf.len();
        buf.extend_from_slice(
            format!(
                "6 0 obj\n<< /Filter /FlateDecode /Length {} >>\nstream\n",
                content_bytes.len()
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&content_bytes);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        let e_val = buf.len() as u64; // /E：首页对象组物理末尾

        // 对象 9：XRef 流（条目在内存组装后再定位 off9）。文件远小于
        // 64 KiB，2 字节偏移字段（/W [1 2 1]）足够。
        let off9 = buf.len();
        let mut xs: Vec<u8> = Vec::new();
        for (t, f2, f3) in [
            (0u8, 0u16, 255u8),  // 0：free
            (1, 9, 0),           // 1：线性化字典（"%PDF-1.5\n" 长 9）
            (1, off2 as u16, 0), // 2：hint 流
            (2, 8, 0),           // 3/4/5/7：位于 ObjStm 8 内
            (2, 8, 1),
            (2, 8, 2),
            (1, off6 as u16, 0), // 6：Contents
            (2, 8, 3),
            (1, off8 as u16, 0), // 8：ObjStm
            (1, off9 as u16, 0), // 9：XRef 流
        ] {
            xs.extend_from_slice(&xref_row(t, f2, f3));
        }
        buf.extend_from_slice(
            format!(
                "9 0 obj\n<< /Type /XRef /Size 10 /Index [0 10] /W [1 2 1] /Length {} /Root 3 0 R >>\nstream\n",
                xs.len()
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&xs);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        buf.extend_from_slice(format!("startxref\n{off9}\n%%EOF\n").as_bytes());

        let l_val = buf.len() as u64;
        (
            buf,
            (l_val, vec![off2 as u64, hint_len], e_val, off9 as u64),
        )
    })
}

/// 「对象流内嵌流对象」的违例 PDF（1.5，未压缩对象流）构造器。
///
/// 物理布局：对象 1 = Catalog（`/Marker 5 0 R`）；对象 6 = `/Type /ObjStm`
/// （`/N 1 /First 4`，载荷头部是「5 0 」），内含对象 5；对象 5 的本体自带
/// `stream … endstream` ——这正是 7.5.7「The following objects shall not be
/// stored in an object stream: ▪ Stream objects」所禁止的形态；对象 7 = XRef
/// 流（对象 5 记为 type 2 条目 `(2, 6, 0)`）。
///
/// `inner_length` 是内联流字典声明的 `/Length`：传 11 与正文
/// `"hello world"` 相符（读端可还原）；传更大的值则声明长度越过载荷窗口、
/// 尾部不可定位（读端保留字典并报告 `recovered: false`）。
pub(super) fn objstm_inline_stream_pdf(inner_length: i64) -> Vec<u8> {
    let inner =
        format!("<< /Marker /Inline /Length {inner_length} >>\nstream\nhello world\nendstream");
    let mut stm = Vec::new();
    stm.extend_from_slice(b"5 0 "); // 对象号 5，相对偏移 0
    stm.extend_from_slice(inner.as_bytes());
    let first = 4usize; // 「5 0 」的字节数

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Marker 5 0 R >>\nendobj\n");

    let off6 = buf.len();
    buf.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /ObjStm /N 1 /First {first} /Length {} >>\nstream\n",
            stm.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&stm);
    buf.extend_from_slice(b"\nendstream\nendobj\n");

    let off7 = buf.len();
    let mut xs: Vec<u8> = Vec::new();
    xs.extend_from_slice(&xref_row(0, 0, 255)); // 0：free
    xs.extend_from_slice(&xref_row(1, off1 as u16, 0)); // 1：Catalog
    for _ in 2..5 {
        xs.extend_from_slice(&xref_row(0, 0, 255)); // 2..=4：free
    }
    xs.extend_from_slice(&xref_row(2, 6, 0)); // 5：ObjStm 6 内索引 0
    xs.extend_from_slice(&xref_row(1, off6 as u16, 0)); // 6：ObjStm
    xs.extend_from_slice(&xref_row(1, off7 as u16, 0)); // 7：XRef 流
    buf.extend_from_slice(
        format!(
            "7 0 obj\n<< /Type /XRef /Size 8 /Index [0 8] /W [1 2 1] /Length {} /Root 1 0 R >>\nstream\n",
            xs.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&xs);
    buf.extend_from_slice(b"\nendstream\nendobj\nstartxref\n");
    buf.extend_from_slice(format!("{off7}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");
    buf
}
