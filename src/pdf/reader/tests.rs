//! reader 模块测试。

use super::*;
use crate::pdf::filters::decode_stream;
use crate::pdf::types::{Document, PdfVersion, StreamData, Value};
use std::io::Write;
use std::process::id;

/// 生成唯一临时文件路径（无 tempfile 依赖）。
///
/// 沙箱策略按镜像路径拦截：target 目录内的二进制禁止写系统 %TEMP%。
/// 因此临时文件统一落在构建目录 target/test-tmp/ 下；无法定位 target
/// 时回退系统临时目录。
fn tmp_path(name: &str) -> std::path::PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!(
        "demo-pdf-test-{}-{}-{}",
        id(),
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

/// 把字节写入临时文件并返回其 File。
fn write_tmp(bytes: &[u8], name: &str) -> (std::fs::File, std::path::PathBuf) {
    let path = tmp_path(name);
    let mut f = std::fs::File::create(&path).expect("create tmp");
    f.write_all(bytes).expect("write tmp");
    f.flush().expect("flush");
    // 重新打开只读
    let f = std::fs::OpenOptions::new()
        .read(true)
        .open(&path)
        .expect("reopen tmp");
    (f, path)
}

struct SliceSource<'a> {
    data: &'a [u8],
    len: u64,
}
impl<'a> ObjectSource for SliceSource<'a> {
    fn read_at(&mut self, offset: u64, len: u64) -> Option<Vec<u8>> {
        let start = offset as usize;
        let end = (start + len as usize).min(self.data.len());
        if start > self.data.len() {
            return None;
        }
        Some(self.data[start..end].to_vec())
    }
    fn file_len(&self) -> u64 {
        self.len
    }
}

fn slice_source(data: &[u8]) -> SliceSource<'_> {
    SliceSource {
        data,
        len: data.len() as u64,
    }
}

#[test]
fn parse_int_and_float() {
    let mut lex = Lexer::new(b"42 3.5 -7 +1");
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Integer(42));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Float(3.5));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Integer(-7));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Integer(1));
}

#[test]
fn parse_name_and_string() {
    let mut lex = Lexer::new(b"/Type (hello\\nworld) <48656C6C6F>");
    let v = parse_object(&mut lex).unwrap();
    assert_eq!(v, Value::Name("Type".to_string()));
    let v = parse_object(&mut lex).unwrap();
    assert_eq!(v, Value::String("hello\nworld".to_string()));
    let v = parse_object(&mut lex).unwrap();
    assert_eq!(v, Value::HexString("48656C6C6F".to_string()));
}

#[test]
fn parse_bool_null_ref() {
    let mut lex = Lexer::new(b"true false null 12 0 R");
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Bool(true));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Bool(false));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Null);
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Ref(12, 0));
}

#[test]
fn parse_array_and_dict() {
    let mut lex = Lexer::new(b"[1 2 [3 4]] << /A 1 /B /C >>");
    let v = parse_object(&mut lex).unwrap();
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(a.iter().nth(2).unwrap().as_array().unwrap().len(), 2);
    let v = parse_object(&mut lex).unwrap();
    let d = v.as_dict().unwrap();
    assert_eq!(d.get("A").and_then(|v| v.as_integer()), Some(1));
    assert_eq!(d.get("B").and_then(|v| v.as_name()), Some("C"));
}

#[test]
fn parse_indirect_object_simple() {
    let mut lex = Lexer::new(b"1 0 obj 42 endobj");
    let o = parse_indirect_object(&mut lex).unwrap();
    assert_eq!(o.num, 1);
    assert_eq!(o.generation, 0);
    assert_eq!(o.value, Value::Integer(42));
}

/// 深嵌套数组触发深度上限：不得栈溢出，应返回 `Err`；
/// 合法深度（32 层）仍正常解析。
#[test]
fn deep_nesting_hits_limit_gracefully() {
    let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    let mut lex = Lexer::new(deep.as_bytes());
    assert!(parse_object(&mut lex).is_err(), "超限嵌套应报错而非栈溢出");

    let ok = format!("{}{}", "[".repeat(32), "]".repeat(32));
    let mut lex = Lexer::new(ok.as_bytes());
    assert!(parse_object(&mut lex).is_ok(), "限内嵌套应正常解析");
}

#[test]
fn parse_indirect_object_stream_ref() {
    // 流对象：compute_stream_range 只算范围，不读字节。
    let raw = b"2 0 obj\n<< /Len 5 >>\nstream\nhello\nendstream\nendobj";
    let mut lex = Lexer::new(raw);
    let o = parse_indirect_object(&mut lex).unwrap();
    match o.value {
        Value::Stream(_, StreamData::Ref { offset, len }) => {
            // offset 指向 'h' 的绝对偏移（hello 的起始）
            assert_eq!(&raw[offset as usize..(offset + len) as usize], b"hello");
            assert_eq!(len, 5);
        }
        other => panic!("expected stream ref, got {:?}", other),
    }
}

#[test]
fn stream_length_prevents_embedded_endstream_truncation() {
    let payload = b"before endstream after";
    let raw = [
        format!("2 0 obj\n<< /Length {} >>\nstream\n", payload.len()).as_bytes(),
        payload,
        b"\nendstream\nendobj",
    ]
    .concat();
    let mut lex = Lexer::new(&raw);
    let object = parse_indirect_object(&mut lex).unwrap();
    match object.value {
        Value::Stream(_, StreamData::Ref { offset, len }) => {
            assert_eq!(len as usize, payload.len());
            assert_eq!(&raw[offset as usize..(offset + len) as usize], payload);
        }
        other => panic!("expected stream ref, got {other:?}"),
    }
}

#[test]
fn parse_xref_and_trailer() {
    let raw =
        b"xref\n0 2\n0000000000 65535 f \n0000000009 00000 n \ntrailer\n<< /Size 2 /Root 1 0 R >>";
    let mut lex = Lexer::new(raw);
    let xref = parse_xref(&mut lex).unwrap();
    assert_eq!(xref.0.len(), 1);
    assert_eq!(xref.0[0].start, 0);
    assert_eq!(xref.0[0].entries.len(), 2);

    let t = parse_trailer(&mut lex).unwrap();
    assert_eq!(t.size(), Some(2));
    assert_eq!(t.root(), Some((1, 0)));
}

#[test]
fn skip_comments_and_whitespace() {
    let mut lex = Lexer::new(b"% header comment\n  42   % trailing\n");
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Integer(42));
}

#[test]
fn nul_byte_treated_as_whitespace() {
    // ISO 32000-1 §7.2：0x00 NUL 属于空白字符，token 间的 NUL 应被跳过。
    let mut lex = Lexer::new(b"\x00\x00 true \x00false");
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Bool(true));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Bool(false));
    // Name 以 NUL 终止
    let mut lex = Lexer::new(b"/Na\x00me");
    assert_eq!(
        parse_object(&mut lex).unwrap(),
        Value::Name("Na".to_string())
    );
}

#[test]
fn parse_document_from_file() {
    // 构造最小 PDF 写入临时文件，用 Parser 解析。
    let raw = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\nxref\n0 3\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \ntrailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n114\n%%EOF\n";
    let (f, _path) = write_tmp(raw, "minimal");
    let mut p = Parser::new(f).expect("parser");
    let doc = p.parse_document().expect("parse");
    assert_eq!(doc.source_version(), PdfVersion::V1_4);
    assert_eq!(doc.trailer().root(), Some((1, 0)));
    // 懒加载：打开后缓存为空
    assert_eq!(doc.cached_object_count(), 0);
    // 触发懒加载
    let cat = doc.get_object(1).expect("parse").expect("catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    assert_eq!(doc.cached_object_count(), 1);
}

#[test]
fn full_scan_finds_objects() {
    // 无 xref 关键字，强制全文扫描回退。
    let raw = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n2 0 obj\n<< /Type /Pages >>\nendobj\ntrailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n";
    let mut src = slice_source(raw);
    let (xref, trailer) = scan_all_objects(&mut src).expect("scan");
    assert!(xref.get(1).is_some());
    assert_eq!(trailer.root(), Some((1, 0)));
}

// ── 附录 F：线性化容忍验证 ─────────────────────────────────────────────
//
// 线性化 PDF（"fast web view"）把线性化参数字典放在文件第一个对象、首页
// 对象组放前部、末尾携带 hint 流。本库读端**不利用 hint 流、不解读线性化
// 字典**——本节专项验证线性化文件按普通 PDF 路径即可完整读取：解析无
// 错误、无告警回退、全部对象可解析、流可解码。样本由构造器手工生成，
// `/L /H /E /T` 按实际布局取真实值（定点迭代收敛），保证样本在规范意义
// 下成立（ISO 32000-1 附录 F）。

/// zlib 压缩（仅测试用；生产端只解压不压缩）。
fn zlib(data: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    let mut enc = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).expect("zlib compress");
    enc.finish().expect("zlib finish")
}

/// 首页内容流操作符（线性化样本的原始载荷）。
const LINEARIZED_CONTENT_OPS: &[u8] = b"BT /F1 12 Tf 72 720 Td (Hello linearized) Tj ET";

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
fn decoded_stream(doc: &Document, num: i64) -> Vec<u8> {
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
fn build_linearized_classic(hint_pairs: usize, hint_payload: &[u8], corrupt_hint: bool) -> Vec<u8> {
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
fn build_linearized_xref_stream(hint_payload: &[u8]) -> Vec<u8> {
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

/// 主验收：经典 xref 线性化 PDF 按普通路径完整可读——无错误、无告警、
/// 全部对象可解析、内容流与 hint 流均可作为普通流解码，`/L` 与真实文件
/// 长度一致（构造器收敛证据）。
#[test]
fn linearized_classic_xref_readable_via_normal_path() {
    let hint_payload: &[u8] = b"first-page hint data (never parsed by this library)";
    let raw = build_linearized_classic(2, hint_payload, false);
    let (f, _path) = write_tmp(&raw, "linearized-classic");
    let mut p = Parser::new(f).expect("parser");
    let doc = p
        .parse_document()
        .expect("linearized pdf must parse via normal path");
    assert!(
        doc.warnings().is_empty(),
        "unexpected warnings: {:?}",
        doc.warnings()
    );
    assert_eq!(doc.source_version(), PdfVersion::V1_4);
    assert_eq!(doc.trailer().root(), Some((3, 0)));

    // 对象 1：线性化参数字典按普通间接对象解析；/L 为真实文件总长。
    let lin = doc
        .get_object(1)
        .expect("parse")
        .expect("linearization dict");
    let d = lin.value.as_dict().unwrap();
    assert_eq!(d.get("Linearized").and_then(|v| v.as_integer()), Some(1));
    assert_eq!(d.get("N").and_then(|v| v.as_integer()), Some(1));
    assert_eq!(d.get("O").and_then(|v| v.as_integer()), Some(5));
    assert_eq!(
        d.get("L").and_then(|v| v.as_integer()),
        Some(raw.len() as i64)
    );
    assert_eq!(
        d.get("H").and_then(|v| v.as_array()).map(|a| a.len()),
        Some(4)
    );

    // 首页对象组与其余对象均可按号解析。
    for (num, ty) in [(3i64, "Catalog"), (4, "Pages"), (5, "Page"), (7, "Font")] {
        let o = doc.get_object(num).expect("parse").expect("object");
        assert_eq!(
            o.value
                .as_dict()
                .unwrap()
                .get("Type")
                .and_then(|v| v.as_name()),
            Some(ty)
        );
    }

    // 内容流与 hint 流：均按普通流读取并解码回原始载荷。
    assert_eq!(decoded_stream(&doc, 6), LINEARIZED_CONTENT_OPS.to_vec());
    assert_eq!(decoded_stream(&doc, 2), hint_payload.to_vec());

    // 全量预载：7 个对象全部入缓存，仍无告警。
    doc.load_all_objects().expect("load all");
    assert_eq!(doc.cached_object_count(), 7);
    assert!(doc.warnings().is_empty());
}

/// `/H` 的两种合法形态（仅首页 hint 对 / 首页 + 整体 hint 对）均可读取。
#[test]
fn linearized_hint_pair_variants_accepted() {
    for pairs in [1usize, 2] {
        let raw = build_linearized_classic(pairs, b"hint", false);
        let (f, _path) = write_tmp(&raw, "linearized-h-variants");
        let mut p = Parser::new(f).expect("parser");
        let doc = p.parse_document().expect("parse");
        assert!(
            doc.warnings().is_empty(),
            "pairs={pairs}: unexpected warnings: {:?}",
            doc.warnings()
        );
        let lin = doc.get_object(1).expect("parse").expect("dict");
        let h = lin
            .value
            .as_dict()
            .unwrap()
            .get("H")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(h.len(), pairs * 2);
    }
}

/// hint 流声明 `/FlateDecode` 但载荷损坏：文档级读取完全不受影响（读端
/// 不解析 hint 语义），流对象仍可解析、原始字节可取；仅显式解码该流时
/// 才报 `DecodeError`。
#[test]
fn linearized_corrupt_hint_stream_tolerated() {
    let raw = build_linearized_classic(1, b"unused", true);
    let (f, _path) = write_tmp(&raw, "linearized-corrupt-hint");
    let mut p = Parser::new(f).expect("parser");
    let doc = p.parse_document().expect("parse");
    assert!(
        doc.warnings().is_empty(),
        "unexpected warnings: {:?}",
        doc.warnings()
    );

    // hint 流对象可解析、原始（仍编码的）字节可取。
    let hint = doc.get_object(2).expect("parse").expect("hint stream");
    let raw_bytes = doc.stream_bytes(&hint.value).expect("raw stream bytes");
    assert!(!raw_bytes.is_empty());
    match &hint.value {
        Value::Stream(dict, _) => {
            let e = decode_stream(dict, &raw_bytes).expect_err("corrupt hint must not decode");
            assert!(!e.0.is_empty());
        }
        other => panic!("object 2 is not a stream: {other:?}"),
    }

    // 其余对象不受影响：Page 可解析、内容流可解码。
    let page = doc.get_object(5).expect("parse").expect("page");
    assert_eq!(
        page.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Page")
    );
    assert_eq!(decoded_stream(&doc, 6), LINEARIZED_CONTENT_OPS.to_vec());
}

/// PDF 1.5 变体：线性化字典仍为文件第一个普通对象（不打入 ObjStm），
/// Catalog/Pages/Page/Font 打包在对象流内，trailer 语义由 XRef 流承载。
/// 按普通路径读取同样无错误、无告警。
#[test]
fn linearized_xref_stream_and_objstm_readable() {
    let hint_payload: &[u8] = b"page and shared hint groups";
    let raw = build_linearized_xref_stream(hint_payload);
    let (f, _path) = write_tmp(&raw, "linearized-xref-stream");
    let mut p = Parser::new(f).expect("parser");
    let doc = p.parse_document().expect("parse");
    assert!(
        doc.warnings().is_empty(),
        "unexpected warnings: {:?}",
        doc.warnings()
    );
    assert_eq!(doc.source_version(), PdfVersion::V1_5);

    // 线性化字典（普通对象）与 /L 真实性。
    let lin = doc
        .get_object(1)
        .expect("parse")
        .expect("linearization dict");
    let d = lin.value.as_dict().unwrap();
    assert_eq!(d.get("Linearized").and_then(|v| v.as_integer()), Some(1));
    assert_eq!(
        d.get("L").and_then(|v| v.as_integer()),
        Some(raw.len() as i64)
    );

    // ObjStm 内对象（type 2 条目驱动）与普通流均可解析。
    for (num, ty) in [(3i64, "Catalog"), (4, "Pages"), (5, "Page"), (7, "Font")] {
        let o = doc.get_object(num).expect("parse").expect("object");
        assert_eq!(
            o.value
                .as_dict()
                .unwrap()
                .get("Type")
                .and_then(|v| v.as_name()),
            Some(ty)
        );
    }
    assert_eq!(decoded_stream(&doc, 6), LINEARIZED_CONTENT_OPS.to_vec());
    assert_eq!(decoded_stream(&doc, 2), hint_payload.to_vec());

    // 全量预载：对象 1–9 共 9 个入缓存（0 为 free 跳过）。
    doc.load_all_objects().expect("load all");
    assert_eq!(doc.cached_object_count(), 9);
    assert!(doc.warnings().is_empty());
}
