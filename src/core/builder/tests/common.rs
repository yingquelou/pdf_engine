//! 跨主题共享夹具与辅助：整档渲染、字体程序夹具、数值断言与图片拒绝助手。
//!
//! 本模块条目由 `tests/mod.rs` 以 `use common::{...}` 重新绑定到 `tests` 作用域，
//! 各主题文件继续通过 `use super::*` 取用，行为与拆分前一致。

use super::*;

/// 渲染整份 PDF 为 lossy 字符串，供子串断言。
pub(super) fn render(b: &mut PdfBuilder) -> String {
    let mut out = Vec::new();
    b.build(&mut out).expect("build should succeed");
    // PDF 是二进制；用 lossy 转成字符串做子串断言。
    String::from_utf8_lossy(&out).into_owned()
}

/// 合成一个最小可解析的 TrueType 程序：head/maxp/hhea/hmtx/cmap/loca/glyf，
/// 1000 upem、3 个字形、cmap format 4（U+0041 → gid 2）。末尾保留 54 字节
/// 填充，模拟真实字体中未被表目录覆盖的数据，使子集化后的程序明显更小。
pub(super) fn sample_truetype_program() -> Vec<u8> {
    let mut sfnt = vec![0; 400];
    sfnt[..4].copy_from_slice(&[0, 1, 0, 0]);
    sfnt[4..6].copy_from_slice(&7u16.to_be_bytes());
    sfnt[12..16].copy_from_slice(b"head");
    sfnt[20..24].copy_from_slice(&124u32.to_be_bytes());
    sfnt[24..28].copy_from_slice(&54u32.to_be_bytes());
    sfnt[28..32].copy_from_slice(b"maxp");
    sfnt[36..40].copy_from_slice(&180u32.to_be_bytes());
    sfnt[40..44].copy_from_slice(&32u32.to_be_bytes());
    sfnt[44..48].copy_from_slice(b"hhea");
    sfnt[52..56].copy_from_slice(&212u32.to_be_bytes());
    sfnt[56..60].copy_from_slice(&36u32.to_be_bytes());
    sfnt[60..64].copy_from_slice(b"hmtx");
    sfnt[68..72].copy_from_slice(&248u32.to_be_bytes());
    sfnt[72..76].copy_from_slice(&10u32.to_be_bytes());
    sfnt[76..80].copy_from_slice(b"cmap");
    sfnt[84..88].copy_from_slice(&260u32.to_be_bytes());
    sfnt[88..92].copy_from_slice(&44u32.to_be_bytes());
    sfnt[92..96].copy_from_slice(b"loca");
    sfnt[100..104].copy_from_slice(&304u32.to_be_bytes());
    sfnt[104..108].copy_from_slice(&8u32.to_be_bytes());
    sfnt[108..112].copy_from_slice(b"glyf");
    sfnt[116..120].copy_from_slice(&312u32.to_be_bytes());
    sfnt[120..124].copy_from_slice(&34u32.to_be_bytes());
    sfnt[124 + 12..124 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
    sfnt[124 + 18..124 + 20].copy_from_slice(&1000u16.to_be_bytes());
    sfnt[124 + 50..124 + 52].copy_from_slice(&0i16.to_be_bytes());
    sfnt[180..184].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    sfnt[184..186].copy_from_slice(&3u16.to_be_bytes());
    sfnt[212..216].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    sfnt[246..248].copy_from_slice(&2u16.to_be_bytes());
    sfnt[248..250].copy_from_slice(&500u16.to_be_bytes());
    sfnt[252..254].copy_from_slice(&600u16.to_be_bytes());
    sfnt[262..264].copy_from_slice(&1u16.to_be_bytes());
    sfnt[264..266].copy_from_slice(&3u16.to_be_bytes());
    sfnt[266..268].copy_from_slice(&1u16.to_be_bytes());
    sfnt[268..272].copy_from_slice(&12u32.to_be_bytes());
    sfnt[272..274].copy_from_slice(&4u16.to_be_bytes());
    sfnt[274..276].copy_from_slice(&32u16.to_be_bytes());
    sfnt[278..280].copy_from_slice(&4u16.to_be_bytes());
    sfnt[280..282].copy_from_slice(&4u16.to_be_bytes());
    sfnt[282..284].copy_from_slice(&1u16.to_be_bytes());
    sfnt[286..288].copy_from_slice(&0x41u16.to_be_bytes());
    sfnt[288..290].copy_from_slice(&u16::MAX.to_be_bytes());
    sfnt[292..294].copy_from_slice(&0x41u16.to_be_bytes());
    sfnt[294..296].copy_from_slice(&u16::MAX.to_be_bytes());
    sfnt[296..298].copy_from_slice(&(-63i16).to_be_bytes());
    sfnt[298..300].copy_from_slice(&1i16.to_be_bytes());
    sfnt[304..306].copy_from_slice(&0u16.to_be_bytes());
    sfnt[306..308].copy_from_slice(&9u16.to_be_bytes());
    sfnt[308..310].copy_from_slice(&9u16.to_be_bytes());
    sfnt[310..312].copy_from_slice(&17u16.to_be_bytes());
    sfnt[312..314].copy_from_slice(&1i16.to_be_bytes());
    sfnt[314..322].copy_from_slice(&[0, 1, 0, 2, 0, 5, 0, 8]);
    sfnt[322..324].copy_from_slice(&0u16.to_be_bytes());
    sfnt[324..326].copy_from_slice(&0u16.to_be_bytes());
    sfnt[326] = 0x37;
    sfnt[327] = 1;
    sfnt[328] = 2;
    sfnt[330..332].copy_from_slice(&(-1i16).to_be_bytes());
    sfnt[332..340].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
    sfnt[340..342].copy_from_slice(&0x0002u16.to_be_bytes());
    sfnt[342..344].copy_from_slice(&0u16.to_be_bytes());
    sfnt
}

/// 合成一个最小的 SFNT 容器：4 字节版本标签 + 单个 `CFF ` 表。
///
/// 写端只校验版本标签，不解析表目录；载荷仅用于断言字节原样嵌入。
pub(super) fn sample_opentype_sfnt(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut sfnt = Vec::new();
    sfnt.extend_from_slice(tag);
    sfnt.extend_from_slice(&1u16.to_be_bytes()); // numTables
    sfnt.extend_from_slice(&16u16.to_be_bytes()); // searchRange
    sfnt.extend_from_slice(&0u16.to_be_bytes()); // entrySelector
    sfnt.extend_from_slice(&0u16.to_be_bytes()); // rangeShift
    sfnt.extend_from_slice(b"CFF "); // table tag
    sfnt.extend_from_slice(&0u32.to_be_bytes()); // checksum
    sfnt.extend_from_slice(&28u32.to_be_bytes()); // offset
    sfnt.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    sfnt.extend_from_slice(payload);
    sfnt
}

/// 在 assemble 产出的 Document 中定位指定子类型的字体字典。
pub(super) fn find_font_object(doc: &Document, subtype: &str) -> (i64, Dict) {
    for num in 1..=64 {
        if let Ok(Some(object)) = doc.get_object(num)
            && let Value::Dict(dict) = &object.value
            && dict.get("Type").and_then(Value::as_name) == Some("Font")
            && dict.get("Subtype").and_then(Value::as_name) == Some(subtype)
        {
            return (num, dict.clone());
        }
    }
    panic!("font dict with subtype {subtype} not found");
}

/// 从字体字典取出 /FontFile2（或 /FontFile、/FontFile3）流的字典与字节。
pub(super) fn embedded_program_stream(
    doc: &Document,
    font_dict: &Dict,
    key: &str,
) -> (Dict, Vec<u8>) {
    let descriptor_num = font_dict
        .get("FontDescriptor")
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .expect("font descriptor");
    let descriptor = doc
        .get_object(descriptor_num)
        .expect("descriptor lookup")
        .expect("descriptor object");
    let descriptor_dict = descriptor.value.as_dict().expect("descriptor dict");
    let program_num = descriptor_dict
        .get(key)
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .expect("program reference");
    let program = doc
        .get_object(program_num)
        .expect("program lookup")
        .expect("program object");
    match program.value {
        Value::Stream(ref stream_dict, StreamData::Owned(ref bytes)) => {
            (stream_dict.clone(), bytes.clone())
        }
        _ => panic!("program object should be an Owned stream"),
    }
}

/// 从字体字典取出 /FontFile2（或 /FontFile、/FontFile3）流，返回声明长度与字节。
pub(super) fn embedded_program(doc: &Document, font_dict: &Dict, key: &str) -> (i64, Vec<u8>) {
    let (stream_dict, bytes) = embedded_program_stream(doc, font_dict, key);
    let length = stream_dict
        .get("Length")
        .and_then(Value::as_integer)
        .expect("stream length");
    (length, bytes)
}

/// 取出字体字典引用的 `/FontDescriptor` 字典。
pub(super) fn font_descriptor(doc: &Document, font_dict: &Dict) -> Dict {
    let descriptor_num = font_dict
        .get("FontDescriptor")
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .expect("font descriptor");
    doc.get_object(descriptor_num)
        .expect("descriptor lookup")
        .expect("descriptor object")
        .value
        .as_dict()
        .cloned()
        .expect("descriptor dict")
}

/// 合成一个最小 Type 1 程序：明文头 + eexec 加密的 Private 字典，
/// `/lenIV 4`，含 1 个 Subr 与 `.notdef`/`A`/`B` 三个 CharString，
/// 便于观察子集化删除未请求字形后的体积变化。
pub(super) fn sample_type1_program() -> Vec<u8> {
    fn encrypt(plaintext: &[u8], key: u16) -> Vec<u8> {
        let mut running = key;
        let mut ciphertext = Vec::with_capacity(plaintext.len());
        for &plain in plaintext {
            let cipher = plain ^ (running >> 8) as u8;
            running = u16::from(cipher)
                .wrapping_add(running)
                .wrapping_mul(52845)
                .wrapping_add(22719);
            ciphertext.push(cipher);
        }
        ciphertext
    }
    fn segment(charstring: &[u8]) -> Vec<u8> {
        let mut padded = vec![0x41u8; 4];
        padded.extend_from_slice(charstring);
        encrypt(&padded, 4330)
    }

    // `0 <width> hsbw endchar`，宽度分别为 500、600、700。
    let subr = segment(&[139, 11]);
    let notdef = segment(&[139, 248, 136, 13, 14]);
    let glyph_a = segment(&[139, 248, 236, 13, 14]);
    let glyph_b = segment(&[139, 249, 80, 13, 14]);

    let mut private = Vec::new();
    private.extend_from_slice(b"dup /Private 7 dict dup begin\n");
    private.extend_from_slice(b"/RD {string currentfile exch readstring pop} executeonly def\n");
    private.extend_from_slice(b"/lenIV 4 def\n");
    private.extend_from_slice(format!("/Subrs 1 array\ndup 0 {} RD ", subr.len()).as_bytes());
    private.extend_from_slice(&subr);
    private.extend_from_slice(b" NP\ndef\n");
    private.extend_from_slice(b"2 index /CharStrings 3 dict dup begin\n");
    for (name, data) in [(".notdef", &notdef), ("A", &glyph_a), ("B", &glyph_b)] {
        private.extend_from_slice(format!("/{name} {} RD ", data.len()).as_bytes());
        private.extend_from_slice(data);
        private.extend_from_slice(b" ND\n");
    }
    private.extend_from_slice(b"end\nend\nmark currentfile closefile\n");

    let mut eexec_plain = vec![0x5Au8; 4];
    eexec_plain.extend_from_slice(&private);
    let eexec = encrypt(&eexec_plain, 55665);

    let mut program = Vec::new();
    program.extend_from_slice(b"%!PS-AdobeFont-1.0: BuilderSans 001.000\n");
    program.extend_from_slice(b"/FontName /BuilderSans def\n");
    program.extend_from_slice(b"/FamilyName (Builder Sans) def\n");
    program.extend_from_slice(b"/FontBBox [0 -200 1000 900] readonly def\n");
    program.extend_from_slice(b"/Encoding StandardEncoding def\n");
    program.extend_from_slice(b"currentfile eexec\n");
    program.extend_from_slice(&eexec);
    program.extend_from_slice(&[b'0'; 512]);
    program.extend_from_slice(b"\ncleartomark\n");
    program
}

/// 把数值数组（Integer/Float）转成 f64 序列，便于断言。
pub(super) fn numbers(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| match item {
            Value::Integer(number) => *number as f64,
            Value::Float(number) => *number,
            other => panic!("expected a number, got {other:?}"),
        })
        .collect()
}

/// 把单个 Integer/Float 值转成 f64，便于断言描述符度量。
pub(super) fn scalar(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Integer(number) => Some(*number as f64),
        Value::Float(number) => Some(*number),
        _ => None,
    }
}

/// 断言图片以 `WriteError::InvalidObject` 被拒绝且零字节产出。
pub(super) fn assert_image_rejected(img: ImageData, expected: &str) {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("非法图片参数应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}
