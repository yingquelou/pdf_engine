//! builder 单元测试：对象布局、字体编码写出与内容操作符生成。
//!
//! 按主题拆分：
//! - `font`：简单字体编码与描述符；
//! - `widths`：简单字体 `/Widths` 与描述符必需项兜底；
//! - `font_program`：字体程序嵌入与 TrueType/Type 1 子集化；
//! - `font_cff`：name-keyed CFF 子集化写入；
//! - `type3`：Type 3 字体；
//! - `cid`：CIDFont 描述符与内容流；
//! - `to_unicode`：ToUnicode CMap；
//! - `encoding`：自定义 CID 编码 CMap；
//! - `annotation`：注解字典与 `/Annots` 通用条目与子类型专有条目；
//! - `annotation_border`：旧式 `/Border` 数组（Table 164）与通用 `/BS` 门禁；
//! - `annotation_line`：Line 注解专有条目（Table 175/176）与版本门禁；
//! - `annotation_free_text`：FreeText 注解专有条目（Table 174）与版本门禁；
//! - `annotation_geometry`：Square/Circle/Polygon/PolyLine 几何条目（Table 177/178）与版本门禁；
//! - `annotation_link`：Link 注解 `/H` `/PA` 条目（Table 173）与版本门禁；
//! - `annotation_subtype_table`：写端子类型集与读端 Table 169 的一致性与子类型版本门禁；
//! - `annotation_popup`：`/Popup` 与 `/Parent` 的单侧声明补出、配对冲突与必填校验（12.5.2）；
//! - `annotation_redact`：Redact 注解条目（Table 192）的取值契约与 PDF 1.7 门禁；
//! - `annotation_printer_mark`：Printer's mark 注解 `/F` `/MN` 与表单条目（Table 362/363）的写出、校验与 PDF 1.4 门禁；
//! - `annotation_trap_net`：Trap 网络注解跟踪条目（Table 366）与表单条目（Table 367）的写出、校验、每页唯一与 PDF 1.3/1.4 门禁；
//! - `annotation_exdata`：`/ExData` 3D 标记（13.6.6 Table 313）的 `/3DA` `/3DV` `/MD5` 写出、跨注解目标校验与 PDF 1.7 门禁；
//! - `annotation_mk`：外观特性字典 `/MK`（Table 189）与图标适配 `/IF`（Table 247）的取值契约、Screen 归属与 PDF 1.5 门禁；
//! - `hide_action`：Hide 动作 `/T` 目标解析与 `/H`（Table 210）的写出、校验与 PDF 1.2 门禁；
//! - `sound`：声音对象（13.3 Table 294）、Sound 动作（Table 208）与 Sound 注解（Table 185）的流对象分配、引用解析与 PDF 1.2 门禁；
//! - `action_ocg`：Set-OCG-State 动作 `/State`（Table 213）的组对象分配、写出与 PDF 1.5 门禁；
//! - `action_transition`：Transition 动作 `/Trans`（Table 215）的过渡字典写出、条目校验与 PDF 1.5 门禁；
//! - `action_thread`：Thread 动作 `/D` `/B` 索引（Table 205）的写出、越界校验与 PDF 1.1 门禁；
//! - `rendition`：多媒体 rendition 与 media clip（13.2 Table 266–278）、Rendition 动作（Table 214）的写出、Screen 目标校验与 PDF 1.5 门禁；
//! - `page_presentation`：页面 `/Dur` 与 `/Trans`（12.4.4 Table 30）的写出、PDF 1.1 门禁与 1.5 样式条目门禁；
//! - `page_box`：页面边界（14.11.2 Table 30）与框颜色信息（Table 360/361）的写出、PDF 1.3/1.4 门禁与取值契约；
//! - `separation`：分色字典（14.11.4 Table 364）的组共享 `/Pages`、色料与颜色空间写出、组契约与 PDF 1.3 门禁；
//! - `opi`：图像与表单 XObject 的 `/OPI` 代理字典（14.11.7 Table 368–370）写出、PDF 1.2 门禁与取值契约；
//! - `thumbnail`：页面缩略图 `/Thumb`（12.3.4 Table 30）的图像引用、去重与颜色空间契约；
//! - `articles`：文章线（12.4.3 Table 160/161）的线程/bead 对象、`/B` 与 PDF 1.1 门禁；
//! - `collection`：便携集合（12.3.5 Table 155–158）的模式、`/D` `/View` `/Sort` 与 PDF 1.7 门禁；
//! - `templates`：命名模板页（12.7.6 Table 31）的 `/Names /Templates` 名称树、`/Type /Template` 对象与 PDF 1.3 门禁；
//! - `alternate_presentation`：备用演示（13.5 Table 297）的幻灯片对象、`/Names /AlternatePresentations` 名称树、嵌入文件引用与 PDF 1.4 门禁；
//! - `structure`：逻辑结构树（14.7 Table 321–326）的目录条目、父树/`/IDTree`、版本门禁与输入校验；
//! - `structure_standard`：标准属性 owners 与 List/PrintField/Table 条目（14.8.5.2、14.8.5.5–14.8.5.7）的往返、适用性与版本门禁；
//! - `structure_types`：PDF 1.5 扩展标准结构类型（14.8.4）的直接/`/RoleMap` 到达方式与版本门禁；
//! - `structure_layout`：Layout 标准属性（14.8.5.4 Table 342–346）的往返、缺省省略、形态/适用性校验与 1.4/1.5/1.6 门禁；
//! - `three_d`：3D artwork 流（13.6 Table 300）与 3D 注解（Table 298）的流对象分配、视图选择、激活条目与 PDF 1.6 门禁；
//! - `three_d_view_extensions`：3D 视图 PDF 1.7 条目（`/RM` `/LS` `/SA` `/NA` `/NR`，13.6.4.1 Table 304）的版本门禁与 `/NA` → `/NR` 依赖；
//! - `three_d_payloads`：3D 视图负载字典（渲染模式/光照方案/截面/节点，Table 307–312）的类型化序列化、缺省省略与嵌套校验；
//! - `three_d_resources`：3D 流 `/Resources` 名称树（13.6.3.1 Table 300）的文本串键、排序写出与直接对象校验；
//! - `action_goto3d`：Go-To-3D-View 动作 `/TA` `/V`（12.6.4.15 Table 216）的写出、目标注解/`/VA` 相对校验与 PDF 1.6 门禁；
//! - `support`：注解字典取用的共享夹具；
//! - `form_action`：表单数据动作（SubmitForm/ResetForm/ImportData，12.7.5）与版本门禁；
//! - `form_appearance`：Widget `/AP /N` 状态子字典与 `/AS` 校验；
//! - `form_merged`：同全限定名字段多重表示的 `/V`、`/DV` 一致性（12.7.3.2）；
//! - `acro_form`：AcroForm 字段树、Widget 与 `/AP` 外观流；
//! - `acro_form_resources`：AcroForm `/DR` 非字体默认资源（ExtGState/ColorSpace/ProcSet）；
//! - `embedded_file`：嵌入文件流、文件规格与 `/EmbeddedFiles` 名称树；
//! - `form`：Form XObject 字典与资源；
//! - `page_group`：页字典组属性（`/Group`）；
//! - `optional_content`：可选内容组（`/OCG`）与目录 `/OCProperties`；
//! - `function`：PDF 函数（Type 0/2/3/4）与 ExtGState `/TR`；
//! - `shading`：渐变（Shading Type 1–5）与着色图案（PatternType 2）；
//! - `pattern`：平铺图案（PatternType 1）；
//! - `color_space`：页级命名颜色空间（`/ColorSpace` 资源与 `CS`/`cs`）；
//! - 本文件：公共夹具/渲染辅助与对象布局测试。

mod acro_form;
mod acro_form_resources;
mod action_goto3d;
mod action_gotoe;
mod action_ocg;
mod action_thread;
mod action_transition;
mod alternate_presentation;
mod annotation;
mod annotation_appearance;
mod annotation_border;
mod annotation_contract;
mod annotation_exdata;
mod annotation_free_text;
mod annotation_geometry;
mod annotation_line;
mod annotation_link;
mod annotation_measure;
mod annotation_mk;
mod annotation_popup;
mod annotation_printer_mark;
mod annotation_redact;
mod annotation_subtype_table;
mod annotation_trap_net;
mod annotation_watermark;
mod articles;
mod choice_indices;
mod cid;
mod cid_cmap_multibyte;
mod collection;
mod collection_item;
mod color_space;
mod compression;
mod embedded_file;
mod encoding;
mod font;
mod font_cff;
mod font_program;
mod form;
mod form_action;
mod form_appearance;
mod form_merged;
mod form_option;
mod form_resources;
mod function;
mod graphics_state;
mod hide_action;
mod image_entries;
mod metadata;
mod movie;
mod navigation;
mod opi;
mod optional_content;
mod output_intent;
mod page_box;
mod page_group;
mod page_presentation;
mod pattern;
mod rendition;
mod separation;
mod shading;
mod sound;
mod structure;
mod structure_layout;
mod structure_standard;
mod structure_types;
mod support;
mod templates;
mod three_d;
mod three_d_payloads;
mod three_d_resources;
mod three_d_view_extensions;
mod thumbnail;
mod to_unicode;
mod type3;
mod viewport;
mod widths;

use super::*;
use crate::core::types::{Dict, Document, StreamData, Value};

fn render(b: &mut PdfBuilder) -> String {
    let mut out = Vec::new();
    b.build(&mut out).expect("build should succeed");
    // PDF 是二进制；用 lossy 转成字符串做子串断言。
    String::from_utf8_lossy(&out).into_owned()
}

/// 合成一个最小可解析的 TrueType 程序：head/maxp/hhea/hmtx/cmap/loca/glyf，
/// 1000 upem、3 个字形、cmap format 4（U+0041 → gid 2）。末尾保留 54 字节
/// 填充，模拟真实字体中未被表目录覆盖的数据，使子集化后的程序明显更小。
fn sample_truetype_program() -> Vec<u8> {
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
fn sample_opentype_sfnt(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
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
fn find_font_object(doc: &Document, subtype: &str) -> (i64, Dict) {
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
fn embedded_program_stream(doc: &Document, font_dict: &Dict, key: &str) -> (Dict, Vec<u8>) {
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
fn embedded_program(doc: &Document, font_dict: &Dict, key: &str) -> (i64, Vec<u8>) {
    let (stream_dict, bytes) = embedded_program_stream(doc, font_dict, key);
    let length = stream_dict
        .get("Length")
        .and_then(Value::as_integer)
        .expect("stream length");
    (length, bytes)
}

/// 取出字体字典引用的 `/FontDescriptor` 字典。
fn font_descriptor(doc: &Document, font_dict: &Dict) -> Dict {
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
fn numbers(value: &Value) -> Vec<f64> {
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
fn scalar(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Integer(number) => Some(*number as f64),
        Value::Float(number) => Some(*number),
        _ => None,
    }
}

#[test]
fn builds_minimal_text_pdf() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(24.0), 72.0, 700.0, "Hello");
    let s = render(&mut b);

    assert!(
        s.contains("%PDF-1.7"),
        "应输出 1.7 版本头: {}",
        &s[..s.len().min(200)]
    );
    assert!(s.contains("/Type /Page"), "应含 Page 类型");
    assert!(s.contains("/Type /Catalog"), "应含 Catalog 类型");
    assert!(s.contains("/Root 1 0 R"), "trailer 应指向 Catalog");
    assert!(s.contains("BT"), "内容流应含 BT");
    assert!(s.contains("Tf"), "内容流应含 Tf");
    assert!(s.contains("Td"), "内容流应含 Td");
    assert!(s.contains("(Hello) Tj"), "内容流应含 (Hello) Tj");
    assert!(s.contains("ET"), "内容流应含 ET");
    // 字体对象
    assert!(s.contains("/Type /Font"));
    assert!(s.contains("/BaseFont /Helvetica"));
    assert!(s.contains("/Encoding /WinAnsiEncoding"));
}

#[test]
fn text_escapes_parens_and_backslash() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 10.0, r"a(b)c\d");
    let s = render(&mut b);
    assert!(s.contains(r"(a\(b\)c\\d) Tj"), "应转义 ( ) \\");
}

#[test]
fn image_xobject_emitted() {
    // 2x2 RGB 全红。
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Letter);
    b.image(p, 100.0, 100.0, 50.0, 50.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/Type /XObject"));
    assert!(s.contains("/Subtype /Image"));
    assert!(s.contains("/Width 2"));
    assert!(s.contains("/Height 2"));
    assert!(s.contains("/ColorSpace /DeviceRGB"));
    assert!(s.contains("/BitsPerComponent 8"));
    // 内容流应含 Do
    assert!(s.contains("/Im1 Do"));
    assert!(s.contains("cm"));
    assert!(!s.contains("/Decode"), "未提供 Decode 时不应写出: {s}");
    assert!(!s.contains("/Interpolate"), "未启用插值时不应写出: {s}");
    assert!(!s.contains("/SMask"), "无软蒙版时不应写出: {s}");
}

#[test]
fn cmyk_image_xobject_emitted() {
    // 1x1 CMYK（C 满、M/Y/K 为零）：每像素 4 字节。
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceCMYK,
        bits: 8,
        data: vec![255, 0, 0, 0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Letter);
    b.image(p, 10.0, 10.0, 20.0, 20.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace /DeviceCMYK"));
    assert!(s.contains("/BitsPerComponent 8"));
    assert!(s.contains("/Length 4"), "CMYK 像素应原样写出 4 字节: {s}");
}

#[test]
fn cmyk_image_data_length_mismatch_is_invalid_object() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceCMYK,
        bits: 8,
        // RGB 尺寸的缓冲：2*2*3 = 12，而 CMYK 需要 16。
        data: vec![0; 12],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Letter);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CMYK 图片数据长度不匹配应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("does not match expected 16"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn image_with_unsupported_bits_is_invalid_object() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 3,
        data: vec![0; 1],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Letter);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("BitsPerComponent 仅允许 1/2/4/8/16");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("bits per component 3"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn image_writes_decode_and_interpolate_entries() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![10, 20, 30],
        decode: Some(vec![0.0, 1.0, 1.0, 0.0, 0.0, 0.5]),
        interpolate: true,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/Decode [0.0 1.0 1.0 0.0 0.0 0.5]"), "{s}");
    assert!(s.contains("/Interpolate true"), "{s}");
}

fn assert_image_rejected(img: ImageData, expected: &str) {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("非法图片参数应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn invalid_image_decode_fails_the_build() {
    let base = || ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0; 3],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    assert_image_rejected(
        base().with_decode(vec![0.0, 1.0]),
        "Decode must contain 6 numbers for a 3-component image, got 2",
    );
    assert_image_rejected(
        base().with_decode(vec![0.0, f64::NAN, 0.0, 1.0, 0.0, 1.0]),
        "Decode must contain finite numbers",
    );
}

#[test]
fn image_with_smask_writes_soft_mask_object_and_reference() {
    let mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0, 64, 128, 255],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0; 12],
        decode: None,
        interpolate: false,
        smask: Some(Box::new(mask)),
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    b.image(p, 20.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert_eq!(
        s.matches("/Subtype /Image").count(),
        2,
        "基图与软蒙版各一个流对象: {s}"
    );
    assert_eq!(s.matches("/SMask ").count(), 1, "{s}");
    assert!(s.contains("/ColorSpace /DeviceGray"), "{s}");
    // 每次绘制分配独立资源名，但重复绘制复用同一基图对象号。
    assert_eq!(s.matches("/Im1 Do").count(), 1, "{s}");
    assert_eq!(s.matches("/Im2 Do").count(), 1, "{s}");
    let resources = &s[s.find("/XObject <<").expect("应写出 XObject 资源字典")..];
    let base_ref = resources
        .split("/Im1 ")
        .nth(1)
        .and_then(|rest| rest.split(" /Im2 ").next())
        .expect("应写出 Im1 资源引用");
    assert!(
        resources.contains(&format!("/Im2 {base_ref}")),
        "重复绘制应复用同一基图对象: {s}"
    );
    assert!(!s.contains("/Im3 "), "蒙版不应登记为页面资源: {s}");
    assert_eq!(s.matches("/Length 4").count(), 1, "蒙版 4 字节: {s}");
}

#[test]
fn invalid_smask_parameters_fail_the_build() {
    fn gray_mask(width: u32, height: u32, data: Vec<u8>) -> ImageData {
        ImageData {
            intent: None,
            alternates: Vec::new(),
            matte: None,
            width,
            height,
            color_space: ColorSpace::DeviceGray,
            bits: 8,
            data,
            decode: None,
            interpolate: false,
            smask: None,
            image_mask: false,
            mask: None,
            opi: None,
        }
    }
    fn base_with(mask: ImageData) -> ImageData {
        ImageData {
            intent: None,
            alternates: Vec::new(),
            matte: None,
            width: 2,
            height: 2,
            color_space: ColorSpace::DeviceRGB,
            bits: 8,
            data: vec![0; 12],
            decode: None,
            interpolate: false,
            smask: Some(Box::new(mask)),
            image_mask: false,
            mask: None,
            opi: None,
        }
    }

    assert_image_rejected(
        base_with(gray_mask(1, 1, vec![0])),
        "SMask dimensions 1x1 do not match image 2x2",
    );
    assert_image_rejected(
        base_with(ImageData {
            intent: None,
            alternates: Vec::new(),
            matte: None,
            width: 2,
            height: 2,
            color_space: ColorSpace::DeviceRGB,
            bits: 8,
            data: vec![0; 12],
            decode: None,
            interpolate: false,
            smask: None,
            image_mask: false,
            mask: None,
            opi: None,
        }),
        "SMask color space must be DeviceGray",
    );
    assert_image_rejected(
        base_with(gray_mask(2, 2, vec![0; 4]).with_smask(gray_mask(2, 2, vec![0; 4]))),
        "SMask must not contain another SMask",
    );
    assert_image_rejected(
        base_with(gray_mask(2, 2, vec![0; 3])),
        "SMask: data length 3 does not match expected 4",
    );
}

#[test]
fn smask_requires_pdf_1_4() {
    let mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0],
        decode: None,
        interpolate: false,
        smask: Some(Box::new(mask)),
        image_mask: false,
        mask: None,
        opi: None,
    };

    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.3 must reject SMask");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("transparency"),
        "错误信息应说明透明度特性: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports SMask");
    assert!(String::from_utf8_lossy(&out).contains("/SMask "));
}

#[test]
fn stencil_image_mask_writes_image_mask_and_omits_color_space() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 8,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0b1010_1010],
        decode: Some(vec![1.0, 0.0]),
        interpolate: true,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ImageMask true"), "{s}");
    assert!(!s.contains("/ColorSpace"), "图像蒙版不得写 ColorSpace: {s}");
    assert!(s.contains("/BitsPerComponent 1"), "{s}");
    assert!(s.contains("/Decode [1.0 0.0]"), "{s}");
    assert!(s.contains("/Interpolate true"), "{s}");
    assert!(s.contains("/Im1 Do"), "{s}");
}

#[test]
fn explicit_mask_writes_mask_reference_and_stencil_object() {
    let mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 4,
        height: 2,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0b1111_0000, 0b0000_1111],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0; 12],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: Some(ImageMask::explicit(mask)),
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    let mask_num = s
        .split("/Mask ")
        .nth(1)
        .and_then(|rest| rest.split(" 0 R").next())
        .expect("基图应写出 /Mask 引用");
    let start = s
        .find(&format!("\n{mask_num} 0 obj"))
        .unwrap_or_else(|| panic!("蒙版对象 {mask_num} 应存在: {s}"));
    let mask_object = &s[start..start + s[start..].find("endobj").expect("endobj")];
    assert!(mask_object.contains("/ImageMask true"), "{mask_object}");
    assert!(!mask_object.contains("/ColorSpace"), "{mask_object}");
    assert!(mask_object.contains("/Width 4"), "{mask_object}");
    assert!(mask_object.contains("/Height 2"), "{mask_object}");
    assert!(mask_object.contains("/BitsPerComponent 1"), "{mask_object}");
}

#[test]
fn colour_key_mask_writes_integer_ranges() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![10, 20, 30, 40, 50, 60],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: Some(ImageMask::color_key(vec![0, 10, 20, 30, 40, 50])),
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/Mask [0 10 20 30 40 50]"), "{s}");
}

#[test]
fn invalid_image_mask_parameters_fail_the_build() {
    let base = || ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 8,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    assert_image_rejected(
        ImageData {
            intent: None,
            alternates: Vec::new(),
            matte: None,
            bits: 8,
            data: vec![0; 8],
            ..base()
        },
        "image mask requires BitsPerComponent 1",
    );
    assert_image_rejected(
        ImageData {
            intent: None,
            alternates: Vec::new(),
            matte: None,
            color_space: ColorSpace::DeviceRGB,
            ..base()
        },
        "image mask color space must be DeviceGray",
    );
    assert_image_rejected(
        base().with_decode(vec![0.0, 0.5]),
        "image mask Decode must be [0 1] or [1 0]",
    );
    let smask = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 8,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0; 8],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    assert_image_rejected(
        ImageData {
            intent: None,
            alternates: Vec::new(),
            matte: None,
            smask: Some(Box::new(smask)),
            ..base()
        },
        "image mask must not have an SMask",
    );
    assert_image_rejected(
        ImageData {
            intent: None,
            alternates: Vec::new(),
            matte: None,
            mask: Some(ImageMask::color_key(vec![0, 1])),
            ..base()
        },
        "image mask must not have a Mask entry",
    );
}

#[test]
fn invalid_explicit_and_colour_key_masks_fail_the_build() {
    let base = || ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0; 6],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let plain_gray = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0; 2],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    assert_image_rejected(
        base().with_mask(ImageMask::explicit(plain_gray)),
        "Mask image must be an image mask (ImageMask true)",
    );
    let broken_mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0; 2],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    assert_image_rejected(
        base().with_mask(ImageMask::explicit(broken_mask)),
        "Mask: image mask requires BitsPerComponent 1",
    );
    assert_image_rejected(
        base().with_mask(ImageMask::color_key(vec![0, 1])),
        "color key Mask must contain 6 integers for a 3-component image, got 2",
    );
    assert_image_rejected(
        base().with_mask(ImageMask::color_key(vec![0, 1, 2, 3, 4, 300])),
        "color key Mask ranges must satisfy 0 <= min <= max <= 255",
    );
    assert_image_rejected(
        base().with_mask(ImageMask::color_key(vec![0, 10, 20, 15, 0, 255])),
        "color key Mask ranges must satisfy 0 <= min <= max <= 255",
    );
}

#[test]
fn image_mask_entry_requires_pdf_1_3() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0; 6],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: Some(ImageMask::color_key(vec![0, 255, 0, 255, 0, 255])),
        opi: None,
    };
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.2 must reject /Mask");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("image Mask entry"),
        "错误信息应说明 /Mask 特性: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.3 supports /Mask");
    assert!(String::from_utf8_lossy(&out).contains("/Mask ["));

    // `/ImageMask true` 本身自 PDF 1.0 起可用，不触发 1.3 门控。
    let stencil = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 8,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &stencil);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.2 supports stencil masks");
    assert!(String::from_utf8_lossy(&out).contains("/ImageMask true"));
}

#[test]
fn cal_gray_image_writes_white_black_and_gamma() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::CalGray(CalGray {
            white_point: [0.9505, 1.0, 1.089],
            black_point: Some([0.0, 0.0, 0.0]),
            gamma: Some(2.2),
        }),
        bits: 8,
        data: vec![0, 255],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 10.0, 10.0, 20.0, 20.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/CalGray <<"), "{s}");
    assert!(s.contains("/WhitePoint [0.9505 1.0 1.089]"), "{s}");
    assert!(s.contains("/BlackPoint [0.0 0.0 0.0]"), "{s}");
    assert!(s.contains("/Gamma 2.2"), "{s}");
}

#[test]
fn cal_rgb_image_writes_gamma_and_matrix() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::CalRGB(CalRGB {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            gamma: Some([2.2, 2.2, 2.2]),
            matrix: Some([0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5]),
        }),
        bits: 8,
        data: vec![1, 2, 3],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/CalRGB <<"), "{s}");
    assert!(s.contains("/Gamma [2.2 2.2 2.2]"), "{s}");
    assert!(
        s.contains("/Matrix [0.5 0.0 0.0 0.0 0.5 0.0 0.0 0.0 0.5]"),
        "{s}"
    );
    assert!(!s.contains("/BlackPoint"), "未提供的黑点应省略: {s}");
}

#[test]
fn lab_image_writes_range() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::Lab(Lab {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            range: Some([-128.0, 127.0, -128.0, 127.0]),
        }),
        bits: 8,
        data: vec![128, 0, 0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/Lab <<"), "{s}");
    assert!(s.contains("/Range [-128.0 127.0 -128.0 127.0]"), "{s}");
    assert!(s.contains("/WhitePoint [0.9505 1.0 1.089]"), "{s}");
}

#[test]
fn icc_based_image_writes_profile_stream_and_alternative() {
    let profile = vec![0, 1, 2, 3];
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: profile,
            alternate: Some(Box::new(ColorSpace::DeviceRGB)),
            range: Some(vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0]),
        }),
        bits: 8,
        data: vec![1, 2, 3, 4, 5, 6],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/ICCBased "), "{s}");
    assert!(s.contains(" 0 R]"), "ICCBased 应引用配置文件流: {s}");
    assert!(s.contains("/N 3"), "{s}");
    assert!(s.contains("/Alternate /DeviceRGB"), "{s}");
    assert!(s.contains("/Range [0.0 1.0 0.0 1.0 0.0 1.0]"), "{s}");
    assert!(s.contains("/Length 4"), "配置文件 4 字节原样写入: {s}");
}

#[test]
fn indexed_image_writes_base_hival_and_lookup_hex() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::Indexed(Indexed::new(
            ColorSpace::DeviceRGB,
            1,
            vec![0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00],
        )),
        bits: 8,
        data: vec![0, 1],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(
        s.contains("/ColorSpace [/Indexed /DeviceRGB 1 <FF000000FF00>]"),
        "{s}"
    );
    assert!(s.contains("/BitsPerComponent 8"), "{s}");
}

#[test]
fn indexed_image_embeds_cie_base_definition() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 3,
        height: 1,
        color_space: ColorSpace::Indexed(Indexed::new(
            ColorSpace::CalGray(CalGray {
                white_point: [0.9505, 1.0, 1.089],
                black_point: None,
                gamma: Some(2.2),
            }),
            2,
            vec![0x00, 0x80, 0xFF],
        )),
        bits: 8,
        data: vec![0, 1, 2],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/Indexed [/CalGray <<"), "{s}");
    assert!(s.contains("/Gamma 2.2"), "{s}");
    assert!(s.contains("2 <0080FF>]"), "{s}");
}

#[test]
fn separation_image_writes_colorant_alternate_and_tint_transform() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::Separation(Separation::new(
            "Orange",
            ColorSpace::DeviceCMYK,
            Function::Exponential(
                ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                    .with_c0(vec![0.0, 0.0, 0.0, 0.0])
                    .with_c1(vec![0.0, 0.4, 0.8, 0.0]),
            ),
        )),
        bits: 8,
        data: vec![0x20, 0xE0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(
        s.contains("/ColorSpace [/Separation /Orange /DeviceCMYK <<"),
        "{s}"
    );
    assert!(s.contains("/FunctionType 2"), "{s}");
    assert!(s.contains("/Domain [0.0 1.0]"), "{s}");
    assert!(s.contains("/C0 [0.0 0.0 0.0 0.0]"), "{s}");
    assert!(s.contains("/C1 [0.0 0.4 0.8 0.0]"), "{s}");
}

#[test]
fn device_n_image_writes_names_and_sampled_tint_transform() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceN(DeviceN::new(
            vec!["Orange".to_string(), "Green".to_string()],
            ColorSpace::DeviceRGB,
            Function::Sampled(SampledFunction::new(
                vec![0.0, 1.0, 0.0, 1.0],
                vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
                vec![2, 1],
                8,
                vec![0.0; 6],
            )),
        )),
        bits: 8,
        data: vec![0x10, 0x20, 0x30, 0x40],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(
        s.contains("/ColorSpace [/DeviceN [/Orange /Green] /DeviceRGB "),
        "{s}"
    );
    assert!(s.contains("/FunctionType 0"), "{s}");
    assert!(s.contains("/Domain [0.0 1.0 0.0 1.0]"), "{s}");
    assert!(s.contains("/Range [0.0 1.0 0.0 1.0 0.0 1.0]"), "{s}");
    assert!(s.contains("/Size [2 1]"), "{s}");
    assert!(s.contains("/BitsPerSample 8"), "{s}");
    assert!(s.contains("/Length 6"), "6 个采样点各 1 字节: {s}");
}

#[test]
fn device_n_allows_repeated_none_colorant() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceN(DeviceN::new(
            vec!["None".to_string(), "None".to_string()],
            ColorSpace::DeviceGray,
            Function::Calculator(CalculatorFunction::new(
                vec![0.0, 1.0, 0.0, 1.0],
                vec![0.0, 1.0],
                b"pop pop 0",
            )),
        )),
        bits: 8,
        data: vec![0x00, 0xFF],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(
        s.contains("/ColorSpace [/DeviceN [/None /None] /DeviceGray "),
        "{s}"
    );
    assert!(s.contains("/FunctionType 4"), "{s}");
}

#[test]
fn invalid_indexed_parameters_fail_the_build() {
    // 调色板长度必须为 (hival + 1) × 基础空间通道数。
    assert_color_space_rejected(
        ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceRGB, 1, vec![0; 5])),
        "Indexed lookup must contain 6 bytes",
    );
    // 嵌套 Indexed 不是合法的 base。
    assert_color_space_rejected(
        ColorSpace::Indexed(Indexed::new(
            ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceGray, 0, vec![0])),
            0,
            vec![0],
        )),
        "Indexed base must not be another Indexed color space",
    );
    // base 参数错误应带上下文向上传播。
    assert_color_space_rejected(
        ColorSpace::Indexed(Indexed::new(
            ColorSpace::Lab(Lab {
                white_point: [f64::NAN, 1.0, 1.0],
                black_point: None,
                range: None,
            }),
            0,
            vec![0, 0, 0],
        )),
        "Indexed base: Lab WhitePoint must contain 3 finite numbers",
    );
}

fn assert_color_space_rejected(color_space: ColorSpace, expected: &str) {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space,
        bits: 8,
        data: vec![0; 4],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("非法颜色空间应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn invalid_cie_color_space_parameters_fail_the_build() {
    assert_color_space_rejected(
        ColorSpace::CalGray(CalGray {
            white_point: [f64::NAN, 1.0, 1.0],
            black_point: None,
            gamma: None,
        }),
        "CalGray WhitePoint must contain 3 finite numbers",
    );
    assert_color_space_rejected(
        ColorSpace::CalGray(CalGray {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            gamma: Some(0.0),
        }),
        "CalGray Gamma must be a finite positive number",
    );
    assert_color_space_rejected(
        ColorSpace::CalRGB(CalRGB {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            gamma: Some([1.0, -1.0, 1.0]),
            matrix: None,
        }),
        "CalRGB Gamma must contain 3 finite positive numbers",
    );
    assert_color_space_rejected(
        ColorSpace::CalRGB(CalRGB {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            gamma: None,
            matrix: Some([f64::INFINITY, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
        }),
        "CalRGB Matrix must contain 9 finite numbers",
    );
    assert_color_space_rejected(
        ColorSpace::Lab(Lab {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            range: Some([0.0, -1.0, -100.0, 100.0]),
        }),
        "Lab Range minimum must not exceed maximum",
    );
}

#[test]
fn invalid_icc_based_parameters_fail_the_build() {
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased::new(2, vec![0])),
        "ICCBased N must be 1, 3 or 4",
    );
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased::new(3, Vec::new())),
        "ICCBased profile data must not be empty",
    );
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: vec![0],
            alternate: None,
            range: Some(vec![0.0, 1.0, 0.0, 1.0]),
        }),
        "ICCBased Range must contain 6 numbers",
    );
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: vec![0],
            alternate: Some(Box::new(ColorSpace::DeviceCMYK)),
            range: None,
        }),
        "ICCBased Alternate has 4 components, expected 3",
    );
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: vec![0],
            alternate: Some(Box::new(ColorSpace::ICCBased(ICCBased::new(3, vec![1])))),
            range: None,
        }),
        "ICCBased Alternate must not be another ICCBased color space",
    );
}

#[test]
fn invalid_separation_and_device_n_color_spaces_fail_the_build() {
    assert_color_space_rejected(
        ColorSpace::Separation(Separation::new(
            "",
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "Separation colorant must not be empty",
    );
    assert_color_space_rejected(
        ColorSpace::Separation(Separation::new(
            "Orange",
            ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceRGB, 0, vec![0, 0, 0])),
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "Separation alternate color space must be a device or CIE-based color space",
    );
    assert_color_space_rejected(
        ColorSpace::Separation(Separation::new(
            "Orange",
            ColorSpace::DeviceCMYK,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "Separation tint transform must produce 4 output(s)",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            Vec::new(),
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN must have at least one colorant",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            vec!["All".to_string()],
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN must not use the reserved colorant name All",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            vec!["Orange".to_string(), "Orange".to_string()],
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN colorant names must be distinct, got duplicate Orange",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            vec!["Orange".to_string(), "Green".to_string()],
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN tint transform must take 2 input(s), got 1",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            vec!["Orange".to_string()],
            ColorSpace::Separation(Separation::new(
                "Orange",
                ColorSpace::DeviceGray,
                Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
            )),
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN alternate color space must be a device or CIE-based color space",
    );
}

#[test]
fn multi_page_count() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let p1 = b.page(PageSize::A4);
    b.text(p0, &Font::helvetica(12.0), 10.0, 700.0, "First");
    b.text(p1, &Font::helvetica(12.0), 10.0, 700.0, "Second");
    let s = render(&mut b);

    assert!(s.contains("/Count 2"), "Pages Count 应为 2");
    // Kids 数组应含 2 个引用（对象 3 与 5）。
    assert!(s.contains("/Kids [3 0 R 5 0 R]"), "Kids 应引用两个 Page");
}

#[test]
fn custom_page_size_mediabox() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Custom(200.0, 300.0));
    b.text(p, &Font::helvetica(10.0), 5.0, 5.0, "x");
    let s = render(&mut b);
    assert!(s.contains("/MediaBox [0 0 200.0 300.0]"));
}

#[test]
fn version_override_in_header() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "hi");
    let s = render(&mut b);
    assert!(s.contains("%PDF-1.4"));
}
