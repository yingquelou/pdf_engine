//! 集成测试：解析 → 生成 → 再解析的往返，以及 API 契约。
//!
//! 阶段 E/F 交互特性（导航、注解、表单、嵌入文件、增量更新、签名）已按
//! 主题拆到 `tests/interactive.rs`；共享辅助在 `tests/support/mod.rs`。

use aes::Aes128;
use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
use md5::{Digest, Md5};
use pdf_engine::core;
use pdf_engine::core::reader::font::{
    CffPathSegment, EncodingMapping, FontProgramSource, PredefinedEncoding, SfntFlavor,
    Type1BuiltinEncoding, Type1PathSegment, strip_subset_tag,
};
use pdf_engine::core::types::{
    Array, Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Trailer, Value,
};
use rc4::{
    Rc4, StreamCipher,
    consts::{U5, U10, U16},
};

#[path = "../support/mod.rs"]
mod support;

use support::*;

mod annotation_field_language_escape;
mod builder_cff_subset;
mod builder_cid_font;
mod builder_cmap_extract;
mod builder_color_space;
mod builder_font_embed;
mod builder_font_text;
mod builder_form_content;
mod builder_gstate_functions;
mod builder_image;
mod builder_image_model;
mod builder_mesh;
mod builder_pattern;
mod builder_separation_intent;
mod builder_shading;
mod builder_transparency;
mod builder_type3;
mod content_text;
mod crypto;
mod fdf;
mod font_truetype;
mod font_type1;
mod font_type1_builtin;
mod hybrid_reference;
mod linearization_hints;
mod outline_language_escape;
mod xref_streams;

/// 合成一个最小 Type 1 程序：明文头 + eexec 加密的 Private 字典，
/// `/lenIV 4`，含 1 个 Subr 与 `.notdef`/`A` 两个 CharString。
fn sample_type1_program() -> Vec<u8> {
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
    fn number(output: &mut Vec<u8>, value: i32) {
        match value {
            -107..=107 => output.push((value + 139) as u8),
            108..=1131 => {
                let encoded = value - 108;
                output.push((247 + (encoded >> 8)) as u8);
                output.push((encoded & 0xFF) as u8);
            }
            -1131..=-108 => {
                let encoded = -value - 108;
                output.push((251 + (encoded >> 8)) as u8);
                output.push((encoded & 0xFF) as u8);
            }
            _ => {
                output.push(255);
                output.extend_from_slice(&value.to_be_bytes());
            }
        }
    }

    let mut subr = Vec::new();
    number(&mut subr, 0);
    subr.push(11); // return

    let mut notdef = Vec::new();
    number(&mut notdef, 0);
    number(&mut notdef, 500);
    notdef.extend_from_slice(&[13, 14]); // hsbw endchar

    let mut glyph_a = Vec::new();
    number(&mut glyph_a, 20);
    number(&mut glyph_a, 600);
    glyph_a.push(13); // hsbw
    number(&mut glyph_a, 100);
    number(&mut glyph_a, 0);
    glyph_a.push(21); // rmoveto
    for delta in [[0, 700], [-100, 0], [0, -700]] {
        number(&mut glyph_a, delta[0]);
        number(&mut glyph_a, delta[1]);
        glyph_a.push(5); // rlineto
    }
    glyph_a.extend_from_slice(&[9, 14]); // closepath endchar

    let subr = segment(&subr);
    let notdef = segment(&notdef);
    let glyph_a = segment(&glyph_a);
    let mut private = Vec::new();
    private.extend_from_slice(b"dup /Private 8 dict dup begin\n");
    private.extend_from_slice(b"/RD {string currentfile exch readstring pop} executeonly def\n");
    private.extend_from_slice(b"/lenIV 4 def\n");
    private.extend_from_slice(format!("/Subrs 1 array\ndup 0 {} RD ", subr.len()).as_bytes());
    private.extend_from_slice(&subr);
    private.extend_from_slice(b" NP\ndef\n");
    private.extend_from_slice(b"2 index /CharStrings 2 dict dup begin\n");
    private.extend_from_slice(format!("/.notdef {} RD ", notdef.len()).as_bytes());
    private.extend_from_slice(&notdef);
    private.extend_from_slice(b" ND\n");
    private.extend_from_slice(format!("/A {} RD ", glyph_a.len()).as_bytes());
    private.extend_from_slice(&glyph_a);
    private.extend_from_slice(b" ND\nend\nend\nmark currentfile closefile\n");

    let mut eexec_plain = vec![0x5Au8; 4];
    eexec_plain.extend_from_slice(&private);
    let eexec = encrypt(&eexec_plain, 55665);

    let mut program = Vec::new();
    program.extend_from_slice(b"%!PS-AdobeFont-1.0: EmbeddedSans 001.000\n");
    program.extend_from_slice(b"/FontName /EmbeddedSans def\n");
    program.extend_from_slice(b"/FamilyName (Embedded Sans) def\n");
    program.extend_from_slice(b"/FontBBox {0 -200 1000 900} readonly def\n");
    program.extend_from_slice(b"/Encoding StandardEncoding def\n");
    program.extend_from_slice(b"currentfile eexec\n");
    program.extend_from_slice(&eexec);
    program.extend_from_slice(&[b'0'; 512]);
    program.extend_from_slice(b"\ncleartomark\n");
    program
}

/// 合成一个最小可解析的 TrueType 程序：head/maxp/hhea/hmtx/cmap/loca/glyf，
/// 1000 upem、3 个字形、cmap format 4（A → glyph 2）。末尾保留 54 字节
/// 填充，模拟真实字体中未被表目录覆盖的数据，使子集化后的程序明显更小。
fn sample_sfnt_font() -> Vec<u8> {
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

/// 合成一个最小的 CFF-flavored OpenType 程序：`OTTO` 版本标签 + 单个
/// `CFF ` 表。写端只校验版本标签，载荷用于断言字节原样嵌入。
fn sample_opentype_cff_sfnt() -> Vec<u8> {
    let payload = b"sample-cff-table";
    let mut sfnt = Vec::new();
    sfnt.extend_from_slice(b"OTTO");
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

/// 在重解析后的文档中查找指定 `/Subtype` 的字体字典。
fn find_font_dict(doc: &Document, subtype: &str) -> Value {
    for num in 1..=64 {
        if let Ok(Some(obj)) = doc.get_object(num)
            && let Value::Dict(dict) = &obj.value
            && dict.get("Type").and_then(Value::as_name) == Some("Font")
            && dict.get("Subtype").and_then(Value::as_name) == Some(subtype)
        {
            return Value::Dict(dict.clone());
        }
    }
    panic!("font dict with subtype {subtype} not found");
}

/// 解析页面树第一页的字典（builder 输出用直接 Resources 字典）。
fn first_page_dict(doc: &Document) -> Dict {
    fn resolve(doc: &Document, value: &Value) -> Value {
        match value {
            Value::Ref(num, generation) => doc
                .get_object(*num)
                .expect("object lookup")
                .filter(|object| object.generation == *generation)
                .map(|object| object.value)
                .expect("resolved object"),
            other => other.clone(),
        }
    }
    let root = resolve(doc, doc.trailer.dict.get("Root").expect("/Root"));
    let pages = resolve(
        doc,
        root.as_dict()
            .expect("catalog")
            .get("Pages")
            .expect("/Pages"),
    );
    let kids = pages
        .as_dict()
        .expect("pages")
        .get("Kids")
        .and_then(Value::as_array)
        .expect("/Kids");
    resolve(doc, &kids.0[0])
        .as_dict()
        .expect("page dictionary")
        .clone()
}
