//! Type 1 程序解析入口、`/FontFile` 长度切分明细与明文头单元测试。

use std::collections::BTreeMap;

use super::super::FontResult;
use super::header::parse_header;
use super::scanner::Scanner;
use super::{Type1BuiltinEncoding, Type1FontProgram, Type1Header, font_file_lengths};

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

fn segment(charstring: &[u8], len_iv: i32) -> Vec<u8> {
    if len_iv < 0 {
        return charstring.to_vec();
    }
    let mut padded = vec![0x41u8; len_iv as usize];
    padded.extend_from_slice(charstring);
    encrypt(&padded, 4330)
}

fn push_segment(output: &mut Vec<u8>, prefix: &str, charstring: &[u8], len_iv: i32, suffix: &str) {
    let data = segment(charstring, len_iv);
    output.extend_from_slice(format!("{prefix}{} RD ", data.len()).as_bytes());
    output.extend_from_slice(&data);
    output.extend_from_slice(suffix.as_bytes());
}

/// Build a complete Type 1 program with a two-subr Private dictionary and
/// three charstrings. `len_iv` of `None` omits `/lenIV` (default 4).
fn sample_program(len_iv: Option<i32>) -> Vec<u8> {
    let value = len_iv.unwrap_or(4);
    let mut private = Vec::new();
    private.extend_from_slice(b"dup /Private 8 dict dup begin\n");
    private.extend_from_slice(b"/RD {string currentfile exch readstring pop} executeonly def\n");
    private.extend_from_slice(b"/ND {noaccess def} executeonly def\n");
    private.extend_from_slice(b"/NP {noaccess put} executeonly def\n");
    private.extend_from_slice(b"/password 5839 def\n");
    if let Some(len_iv) = len_iv {
        private.extend_from_slice(format!("/lenIV {len_iv} def\n").as_bytes());
    }
    private.extend_from_slice(b"/Subrs 2 array\n");
    push_segment(&mut private, "dup 0 ", b"\x8b\x0c", value, " NP\n");
    push_segment(&mut private, "dup 1 ", b"\x8b\x0e\x0c", value, " NP\n");
    private.extend_from_slice(b"def\n");
    private.extend_from_slice(b"2 index /CharStrings 3 dict dup begin\n");
    push_segment(&mut private, "/.notdef ", b"\x8b\x8b\x0c", value, " ND\n");
    push_segment(&mut private, "/A ", b"\x8b\x0e\x0c", value, " ND\n");
    push_segment(&mut private, "/B ", b"\x8b\x10\x0c", value, " ND\n");
    private.extend_from_slice(b"end\nend\nreadonly put\nnoaccess put\n");
    private.extend_from_slice(b"dup /FontName get exch definefont pop\n");
    private.extend_from_slice(b"mark currentfile closefile\n");

    let mut eexec_plain = vec![0x5Au8; 4];
    eexec_plain.extend_from_slice(&private);
    let eexec = encrypt(&eexec_plain, 55665);

    let mut program = Vec::new();
    program.extend_from_slice(b"%!PS-AdobeFont-1.0: ExampleSans 001.000\n");
    program.extend_from_slice(b"11 dict begin\n");
    program.extend_from_slice(b"/FontInfo 10 dict dup begin\n");
    program.extend_from_slice(b"/FamilyName (Example Sans) def\n");
    program.extend_from_slice(b"/FullName (Example Sans Regular) def\n");
    program.extend_from_slice(b"/Weight (Regular) def\n");
    program.extend_from_slice(b"/ItalicAngle -12.5 def\n");
    program.extend_from_slice(b"/isFixedPitch false def\n");
    program.extend_from_slice(b"/UnderlinePosition -100 def\n");
    program.extend_from_slice(b"/UnderlineThickness 50 def\n");
    program.extend_from_slice(b"end readonly def\n");
    program.extend_from_slice(b"/FontName /ExampleSans def\n");
    program.extend_from_slice(b"/PaintType 0 def\n/FontType 1 def\n");
    program.extend_from_slice(b"/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n");
    program.extend_from_slice(b"/Encoding 256 array\n");
    program.extend_from_slice(b"0 1 255 {1 index exch /.notdef put} for\n");
    program.extend_from_slice(b"dup 65 /A put\ndup 66 /B put\n");
    program.extend_from_slice(b"readonly def\n");
    program.extend_from_slice(b"/FontBBox {0 -200 1000 900} readonly def\n");
    program.extend_from_slice(b"currentdict end\ncurrentfile eexec\n");
    program.extend_from_slice(&eexec);
    program.extend_from_slice(&[b'0'; 512]);
    program.extend_from_slice(b"\ncleartomark\n");
    program
}

fn parse_header_bytes(bytes: &[u8]) -> FontResult<Type1Header> {
    let mut scanner = Scanner::new(bytes);
    parse_header(&mut scanner)
}

#[test]
fn font_file_lengths_split_clear_encrypted_and_fixed_sections() {
    let program = sample_program(None);
    let (length1, length2, length3) = font_file_lengths(&program);

    assert_eq!(length1 + length2 + length3, program.len());
    let eexec_end = program
        .windows(b"eexec".len())
        .position(|window| window == b"eexec")
        .expect("eexec keyword")
        + b"eexec".len();
    assert_eq!(
        length1,
        eexec_end + 1,
        "Length1 应覆盖明文头与 eexec 后的单个分隔符"
    );
    let guard = program
        .windows(512)
        .position(|window| window.iter().all(|&byte| byte == b'0'))
        .expect("512 零守卫");
    assert_eq!(length2, guard - length1, "Length2 应为加密段字节数");
    assert_eq!(length3, program.len() - guard, "Length3 应为尾部固定内容");
    assert!(length3 > 0);
}

#[test]
fn font_file_lengths_without_eexec_are_all_clear_text() {
    let data = b"%!PS-AdobeFont-1.0: TestFont\n";
    assert_eq!(font_file_lengths(data), (data.len(), 0, 0));
}

fn custom_encoding(header: &Type1Header) -> &BTreeMap<u8, String> {
    match &header.encoding {
        Some(Type1BuiltinEncoding::Custom(entries)) => entries,
        other => panic!("expected a custom encoding, got {other:?}"),
    }
}

#[test]
fn parses_header_private_dict_and_charstrings() {
    let program = Type1FontProgram::parse(&sample_program(None)).unwrap();

    assert_eq!(program.header.font_name.as_deref(), Some("ExampleSans"));
    assert_eq!(program.header.family_name.as_deref(), Some("Example Sans"));
    assert_eq!(
        program.header.full_name.as_deref(),
        Some("Example Sans Regular")
    );
    assert_eq!(program.header.weight.as_deref(), Some("Regular"));
    assert_eq!(program.header.italic_angle, Some(-12.5));
    assert_eq!(program.header.is_fixed_pitch, Some(false));
    assert_eq!(program.header.underline_position, Some(-100.0));
    assert_eq!(program.header.underline_thickness, Some(50.0));
    assert_eq!(program.header.font_bbox, Some([0.0, -200.0, 1000.0, 900.0]));
    assert_eq!(
        program.header.font_matrix,
        Some([0.001, 0.0, 0.0, 0.001, 0.0, 0.0])
    );

    // 生成式 `0 1 255 { ... put } for` 不产生显式条目。
    let encoding = custom_encoding(&program.header);
    assert_eq!(encoding.len(), 2);
    assert_eq!(encoding.get(&65).map(String::as_str), Some("A"));
    assert_eq!(encoding.get(&66).map(String::as_str), Some("B"));

    assert_eq!(program.len_iv, 4);
    assert_eq!(
        program.subrs.get(&0).map(Vec::as_slice),
        Some(b"\x8b\x0c".as_slice())
    );
    assert_eq!(
        program.subrs.get(&1).map(Vec::as_slice),
        Some(b"\x8b\x0e\x0c".as_slice())
    );
    assert_eq!(
        program.charstrings.get(".notdef").map(Vec::as_slice),
        Some(b"\x8b\x8b\x0c".as_slice())
    );
    assert_eq!(
        program.charstrings.get("A").map(Vec::as_slice),
        Some(b"\x8b\x0e\x0c".as_slice())
    );
    assert_eq!(
        program.charstrings.get("B").map(Vec::as_slice),
        Some(b"\x8b\x10\x0c".as_slice())
    );
}

#[test]
fn defaults_len_iv_to_four() {
    let program = Type1FontProgram::parse(&sample_program(None)).unwrap();
    assert_eq!(program.len_iv, 4);
    assert_eq!(
        program.charstrings.get("A").map(Vec::as_slice),
        Some(b"\x8b\x0e\x0c".as_slice())
    );
}

#[test]
fn len_iv_minus_one_keeps_charstrings_unencrypted() {
    let program = Type1FontProgram::parse(&sample_program(Some(-1))).unwrap();
    assert_eq!(program.len_iv, -1);
    assert_eq!(
        program.charstrings.get("A").map(Vec::as_slice),
        Some(b"\x8b\x0e\x0c".as_slice())
    );
    assert_eq!(
        program.subrs.get(&1).map(Vec::as_slice),
        Some(b"\x8b\x0e\x0c".as_slice())
    );
}

#[test]
fn captures_predefined_encoding_name() {
    let header = parse_header_bytes(
        b"/FontName /Foo def\n/Encoding StandardEncoding def\ncurrentfile eexec\n",
    )
    .unwrap();
    assert_eq!(
        header.encoding,
        Some(Type1BuiltinEncoding::Predefined(
            "StandardEncoding".to_string()
        ))
    );
}

#[test]
fn rejects_program_without_eexec() {
    let error = Type1FontProgram::parse(b"%!PS-AdobeFont-1.0: Missing\n/FontName /Missing def\n")
        .unwrap_err();
    assert!(error.to_string().contains("eexec"), "{error}");
}

#[test]
fn rejects_private_dict_without_charstrings() {
    let mut plain = vec![0u8; 4];
    plain.extend_from_slice(b"dup /Private 8 dict dup begin\nend\nmark currentfile closefile\n");
    let encrypted = encrypt(&plain, 55665);
    let mut program = b"currentfile eexec\n".to_vec();
    program.extend_from_slice(&encrypted);
    program.extend_from_slice(&[b'0'; 512]);

    let error = Type1FontProgram::parse(&program).unwrap_err();
    assert!(error.to_string().contains("CharStrings"), "{error}");
}

#[test]
fn rejects_truncated_rd_segment() {
    let mut plain = vec![0u8; 4];
    plain.extend_from_slice(
        b"dup /Private 8 dict dup begin\n/lenIV 0 def\n2 index /CharStrings 1 dict dup begin\n/.notdef 40 RD short",
    );
    let encrypted = encrypt(&plain, 55665);
    let mut program = b"currentfile eexec\n".to_vec();
    program.extend_from_slice(&encrypted);
    program.extend_from_slice(&[b'0'; 512]);

    let error = Type1FontProgram::parse(&program).unwrap_err();
    assert!(error.to_string().contains("truncated"), "{error}");
}

#[test]
fn rejects_charstring_shorter_than_len_iv() {
    let mut plain = vec![0u8; 4];
    plain.extend_from_slice(
        b"dup /Private 8 dict dup begin\n/lenIV 4 def\n2 index /CharStrings 1 dict dup begin\n/.notdef 2 RD ab ND end",
    );
    let encrypted = encrypt(&plain, 55665);
    let mut program = b"currentfile eexec\n".to_vec();
    program.extend_from_slice(&encrypted);
    program.extend_from_slice(&[b'0'; 512]);

    let error = Type1FontProgram::parse(&program).unwrap_err();
    assert!(error.to_string().contains("lenIV"), "{error}");
}
