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

mod support;

use support::*;

fn rc4_5(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U5>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

fn rc4_10(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U10>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

fn rc4_16(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U16>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn encrypt_r2_fixture() -> Vec<u8> {
    const PADDING: [u8; 32] = [
        0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01,
        0x08, 0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53,
        0x69, 0x7a,
    ];
    let pad = |password: &[u8]| {
        let mut result = password[..password.len().min(32)].to_vec();
        result.extend_from_slice(&PADDING[..32 - result.len()]);
        result
    };
    let id = b"pdf-security-test";
    let user_pad = pad(b"user-password");
    let owner_pad = pad(b"owner-password");
    let owner_key = Md5::digest(&owner_pad);
    let owner_entry = rc4_5(&owner_key[..5], &user_pad);
    let permissions = -4i32;
    let mut key_material = user_pad;
    key_material.extend_from_slice(&owner_entry);
    key_material.extend_from_slice(&permissions.to_le_bytes());
    key_material.extend_from_slice(id);
    let file_key = Md5::digest(key_material);
    let user_entry = rc4_5(&file_key[..5], &PADDING);

    let mut object_key_material = file_key[..5].to_vec();
    object_key_material.extend_from_slice(&[2, 0, 0, 0, 0]);
    let object_key = Md5::digest(object_key_material);
    let encrypted_label = rc4_10(&object_key[..10], b"decrypted label");
    let encrypted_stream = rc4_10(&object_key[..10], b"decrypted stream");

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(
        format!(
            "2 0 obj\n<< /Label <{}> /Length {} >>\nstream\n",
            hex_bytes(&encrypted_label),
            encrypted_stream.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&encrypted_stream);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    let off3 = buf.len();
    buf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Filter /Standard /V 1 /R 2 /O <{}> /U <{}> /P {} >>\nendobj\n",
            hex_bytes(&owner_entry),
            hex_bytes(&user_entry),
            permissions
        )
        .as_bytes(),
    );
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off3:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 4 /Root 1 0 R /Encrypt 3 0 R /ID [<{}> <{}>] >>\nstartxref\n{off_xref}\n%%EOF\n",
            hex_bytes(id),
            hex_bytes(id)
        )
        .as_bytes(),
    );
    buf
}

/// ISO 32000-1 Standard Security Handler R2 authenticates user and owner
/// passwords, decrypts object strings and streams, and refuses unsafe writes.
#[test]
fn standard_security_r2_decrypts_strings_and_streams() {
    let buf = encrypt_r2_fixture();
    let doc = parse_buf_with_password(&buf, b"user-password");
    assert!(doc.is_encrypted());
    let s = doc.get_object(2).expect("parse").expect("stream present");
    let label = s.value.as_dict().unwrap().get("Label").unwrap();
    assert_eq!(label, &Value::HexString(hex_bytes(b"decrypted label")));
    assert_eq!(doc.try_stream_bytes(&s.value).unwrap(), b"decrypted stream");
    assert_eq!(
        doc.decode_stream_bytes(&s.value).unwrap(),
        b"decrypted stream"
    );

    let owner_doc = parse_buf_with_password(&buf, b"owner-password");
    let owner_stream = owner_doc.get_object(2).unwrap().unwrap();
    assert_eq!(
        owner_doc.try_stream_bytes(&owner_stream.value).unwrap(),
        b"decrypted stream"
    );
    assert!(parse_buf_result(&buf, Some(b"wrong-password")).is_err());

    let error = pdf_engine::core::writer::DocumentWriter::new()
        .build(&doc, &mut Vec::new())
        .unwrap_err();
    assert!(matches!(
        error,
        pdf_engine::core::writer::WriteError::EncryptedDocument
    ));

    // 加密特性被记录
    let feats = doc.features.borrow();
    assert!(
        feats.contains(PdfFeature::EncryptionRC4),
        "trailer /Encrypt 应记录 EncryptionRC4"
    );
    drop(feats);
}

#[test]
fn standard_security_r4_decrypts_explicit_crypt_filter() {
    let buf = encrypt_r4_fixture();
    let doc = parse_buf_with_password(&buf, b"user-password");
    let stream = doc.get_object(2).unwrap().unwrap();
    let label = stream.value.as_dict().unwrap().get("Label").unwrap();
    assert_eq!(label, &Value::HexString(hex_bytes(b"R4 encrypted label")));
    assert_eq!(
        doc.try_stream_bytes(&stream.value).unwrap(),
        b"R4 crypt filter stream"
    );
    assert_eq!(
        doc.decode_stream_bytes(&stream.value).unwrap(),
        b"R4 crypt filter stream"
    );
    assert!(parse_buf_result(&buf, Some(b"wrong-password")).is_err());
}

#[test]
fn crypt_identity_filter_is_supported_without_encryption() {
    let doc = Document::new();
    let stream = Value::Stream(
        Dict::new()
            .insert("Filter", Value::Name("Crypt".into()))
            .insert(
                "DP",
                Value::Dict(Dict::new().insert("Name", Value::Name("Identity".into()))),
            ),
        StreamData::Owned(b"identity stream".to_vec()),
    );
    assert_eq!(
        doc.decode_stream_bytes(&stream).unwrap(),
        b"identity stream"
    );

    let non_identity = Value::Stream(
        Dict::new()
            .insert("Filter", Value::Name("Crypt".into()))
            .insert(
                "DecodeParms",
                Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
            ),
        StreamData::Owned(b"ciphertext".to_vec()),
    );
    assert!(doc.decode_stream_bytes(&non_identity).is_err());

    let invalid_params = Value::Stream(
        Dict::new()
            .insert(
                "Filter",
                Value::Array(Array(vec![
                    Value::Name("Crypt".into()),
                    Value::Name("FlateDecode".into()),
                ])),
            )
            .insert(
                "DecodeParms",
                Value::Dict(Dict::new().insert("Name", Value::Name("Identity".into()))),
            ),
        StreamData::Owned(Vec::new()),
    );
    assert!(doc.decode_stream_bytes(&invalid_params).is_err());
}

#[test]
fn type1_font_resolves_indirect_widths_and_descriptor_metrics() {
    let doc = Document::new();
    doc.set_object(Object::new(
        10,
        0,
        Value::Array(Array(vec![Value::Integer(500), Value::Float(611.5)])),
    ));
    doc.set_object(Object::new(12, 0, Value::Integer(400)));
    doc.set_object(Object::new(
        11,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontName", Value::Name("ExampleSans".into()))
                .insert("FontFamily", Value::String("Example Sans".into()))
                .insert("FontStretch", Value::Name("Normal".into()))
                .insert("FontWeight", Value::Ref(12, 0))
                .insert("Flags", Value::Integer(32))
                .insert(
                    "FontBBox",
                    Value::Array(Array(vec![
                        Value::Integer(-20),
                        Value::Integer(-200),
                        Value::Integer(1000),
                        Value::Integer(900),
                    ])),
                )
                .insert("ItalicAngle", Value::Integer(0))
                .insert("Ascent", Value::Integer(750))
                .insert("Descent", Value::Integer(-200))
                .insert("Leading", Value::Integer(200))
                .insert("CapHeight", Value::Integer(700))
                .insert("XHeight", Value::Integer(500))
                .insert("StemV", Value::Integer(80))
                .insert("StemH", Value::Integer(60))
                .insert("AvgWidth", Value::Integer(600))
                .insert("MaxWidth", Value::Integer(1000))
                .insert("MissingWidth", Value::Integer(400))
                .insert("CharSet", Value::String("/A/B".into())),
        ),
    ));
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("FirstChar", Value::Integer(65))
            .insert("LastChar", Value::Integer(66))
            .insert("Widths", Value::Ref(10, 0))
            .insert("FontDescriptor", Value::Ref(11, 0)),
    );

    let font = doc.type1_font(&font).unwrap().unwrap();
    assert_eq!(font.base_font, "ExampleSans");
    assert_eq!(font.width_for(65), Some(500.0));
    assert_eq!(font.width_for(66), Some(611.5));
    assert_eq!(font.width_for(64), Some(400.0));
    assert_eq!(font.width_for(67), Some(400.0));
    let descriptor = font.descriptor.unwrap();
    assert_eq!(descriptor.font_name.as_deref(), Some("ExampleSans"));
    assert_eq!(descriptor.font_family.as_deref(), Some("Example Sans"));
    assert_eq!(descriptor.font_stretch.as_deref(), Some("Normal"));
    assert_eq!(descriptor.font_weight, Some(400.0));
    assert_eq!(descriptor.flags, Some(32));
    assert!(descriptor.is_nonsymbolic());
    assert!(!descriptor.is_serif());
    assert_eq!(descriptor.font_bbox, Some([-20.0, -200.0, 1000.0, 900.0]));
    assert_eq!(descriptor.italic_angle, Some(0.0));
    assert_eq!(descriptor.ascent, Some(750.0));
    assert_eq!(descriptor.descent, Some(-200.0));
    assert_eq!(descriptor.leading, Some(200.0));
    assert_eq!(descriptor.cap_height, Some(700.0));
    assert_eq!(descriptor.x_height, Some(500.0));
    assert_eq!(descriptor.stem_v, Some(80.0));
    assert_eq!(descriptor.stem_h, Some(60.0));
    assert_eq!(descriptor.avg_width, Some(600.0));
    assert_eq!(descriptor.max_width, Some(1000.0));
    assert_eq!(descriptor.missing_width, Some(400.0));
    assert_eq!(descriptor.char_set.as_deref(), Some("/A/B"));
    assert_eq!(descriptor.cid_is_present(0), None);

    let incomplete = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("CustomFont".into())),
    );
    assert!(doc.type1_font(&incomplete).is_err());

    let standard_14 = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert_eq!(
        doc.type1_font(&standard_14).unwrap().unwrap().width_for(65),
        Some(667.0)
    );

    let unsupported = Value::Dict(Dict::new().insert("Subtype", Value::Name("TrueType".into())));
    assert!(doc.type1_font(&unsupported).unwrap().is_none());
}

#[test]
fn type1_font_program_resolves_and_decodes_descriptor_streams() {
    let doc = Document::new();
    doc.set_object(Object::new(
        40,
        0,
        Value::Stream(
            Dict::new().insert("Filter", Value::Name("ASCIIHexDecode".into())),
            StreamData::Owned(b"414243>".to_vec()),
        ),
    ));
    doc.set_object(Object::new(
        41,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontFile", Value::Ref(40, 0)),
        ),
    ));
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("FontDescriptor", Value::Ref(41, 0)),
    );

    let program = doc.type1_font_program(&font).unwrap().unwrap();
    assert_eq!(program.source, FontProgramSource::FontFile);
    assert_eq!(program.subtype, None);
    assert_eq!(program.data, b"ABC");

    let type1c_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile3",
                            Value::Stream(
                                Dict::new().insert("Subtype", Value::Name("Type1C".into())),
                                StreamData::Owned(b"CFF".to_vec()),
                            ),
                        ),
                ),
            ),
    );
    let type1c_program = doc.type1_font_program(&type1c_font).unwrap().unwrap();
    assert_eq!(type1c_program.source, FontProgramSource::FontFile3);
    assert_eq!(type1c_program.subtype.as_deref(), Some("Type1C"));
    assert_eq!(type1c_program.data, b"CFF");

    let no_program = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert!(doc.type1_font_program(&no_program).unwrap().is_none());

    let unsupported_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into())),
    );
    assert!(doc.type1_font_program(&unsupported_font).unwrap().is_none());

    let invalid_file2 = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile2",
                            Value::Stream(Dict::new(), StreamData::Owned(b"font".to_vec())),
                        ),
                ),
            ),
    );
    assert!(doc.type1_font_program(&invalid_file2).is_err());
}

#[test]
fn type1_font_program_reports_malformed_descriptor_entries() {
    let doc = Document::new();
    let make_font = |descriptor| {
        Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name("ExampleSans".into()))
                .insert("FontDescriptor", descriptor),
        )
    };
    let invalid_stream = Value::Stream(
        Dict::new().insert("Filter", Value::Name("FlateDecode".into())),
        StreamData::Owned(b"not deflate data".to_vec()),
    );
    let invalid_descriptor = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("FontDescriptor".into()))
            .insert("FontFile", invalid_stream),
    );
    assert!(
        doc.type1_font_program(&make_font(invalid_descriptor))
            .is_err()
    );

    let multiple_entries = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("FontDescriptor".into()))
            .insert(
                "FontFile",
                Value::Stream(Dict::new(), StreamData::Owned(Vec::new())),
            )
            .insert(
                "FontFile2",
                Value::Stream(Dict::new(), StreamData::Owned(Vec::new())),
            ),
    );
    assert!(
        doc.type1_font_program(&make_font(multiple_entries))
            .is_err()
    );

    let missing_subtype = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("FontDescriptor".into()))
            .insert(
                "FontFile3",
                Value::Stream(Dict::new(), StreamData::Owned(b"CFF".to_vec())),
            ),
    );
    assert!(doc.type1_font_program(&make_font(missing_subtype)).is_err());
}

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

#[test]
fn type1_program_parses_embedded_font_file() {
    let doc = Document::new();
    doc.set_object(Object::new(
        60,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(sample_type1_program())),
    ));
    doc.set_object(Object::new(
        61,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontFile", Value::Ref(60, 0)),
        ),
    ));
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("EmbeddedSans".into()))
            .insert("FontDescriptor", Value::Ref(61, 0)),
    );

    let program = doc.type1_program(&font).unwrap().unwrap();
    assert_eq!(program.header.font_name.as_deref(), Some("EmbeddedSans"));
    assert_eq!(program.header.family_name.as_deref(), Some("Embedded Sans"));
    assert_eq!(program.header.font_bbox, Some([0.0, -200.0, 1000.0, 900.0]));
    assert_eq!(
        program.header.encoding,
        Some(Type1BuiltinEncoding::Predefined(
            "StandardEncoding".to_string()
        ))
    );
    assert_eq!(program.len_iv, 4);
    assert_eq!(
        program.charstrings.get(".notdef").map(Vec::as_slice),
        Some(b"\x8b\xf8\x88\x0d\x0e".as_slice())
    );
    assert_eq!(
        program.charstrings.get("A").map(Vec::as_slice),
        Some(
            b"\x9f\xf8\xec\x0d\xef\x8b\x15\x8b\xf9\x50\x05\x27\x8b\x05\x8b\xfd\x50\x05\x09\x0e"
                .as_slice()
        )
    );
    assert_eq!(
        program.subrs.get(&0).map(Vec::as_slice),
        Some(b"\x8b\x0b".as_slice())
    );

    let glyph = program.glyph("A").unwrap().unwrap();
    assert_eq!(glyph.width, 600.0);
    assert_eq!(glyph.side_bearing, [20.0, 0.0]);
    assert_eq!(
        glyph.path,
        vec![
            Type1PathSegment::MoveTo { x: 120.0, y: 0.0 },
            Type1PathSegment::LineTo { x: 120.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 0.0 },
            Type1PathSegment::ClosePath,
        ]
    );
    assert_eq!(glyph.seac, None);
    assert!(program.glyph("missing").unwrap().is_none());

    let no_program = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert!(doc.type1_program(&no_program).unwrap().is_none());

    let type1c_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("EmbeddedSans".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile3",
                            Value::Stream(
                                Dict::new().insert("Subtype", Value::Name("Type1C".into())),
                                StreamData::Owned(b"CFF".to_vec()),
                            ),
                        ),
                ),
            ),
    );
    assert!(doc.type1_program(&type1c_font).is_err());
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

#[test]
fn truetype_font_program_resolves_and_parses_fontfile2() {
    let doc = Document::new();
    let sfnt = sample_sfnt_font();
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile2",
                            Value::Stream(Dict::new(), StreamData::Owned(sfnt.clone())),
                        ),
                ),
            ),
    );

    let font = doc.truetype_font(&font).unwrap().unwrap();
    assert_eq!(font.flavor(), SfntFlavor::TrueType);
    assert_eq!(font.units_per_em(), 1000);
    assert_eq!(font.glyph_count(), 3);
    assert_eq!(font.horizontal_advance_width(0), Some(500));
    assert_eq!(font.horizontal_advance_width(2), Some(600));
    assert_eq!(font.glyph_id_for_unicode(0x41).unwrap(), Some(2));
    let glyph = font.glyph(0).unwrap().unwrap();
    assert_eq!(glyph.bounds, [1, 2, 5, 8]);
    assert_eq!(glyph.points.len(), 1);
    assert_eq!(font.glyph(2).unwrap().unwrap().points.len(), 1);
    assert_eq!(font.table(b"head").unwrap().len(), 54);

    let cid_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("CIDFontType2".into()))
            .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
            .insert(
                "CIDSystemInfo",
                Value::Dict(
                    Dict::new()
                        .insert("Registry", Value::String("Adobe".into()))
                        .insert("Ordering", Value::String("Identity".into()))
                        .insert("Supplement", Value::Integer(0)),
                ),
            )
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile2",
                            Value::Stream(Dict::new(), StreamData::Owned(sfnt)),
                        ),
                ),
            ),
    );
    let cid_font = doc.cid_font_type2(&cid_font).unwrap().unwrap();
    assert_eq!(cid_font.glyph_id_for_cid(2), Some(2));
    assert_eq!(cid_font.glyph_id_for_cid(3), None);
    assert_eq!(
        cid_font
            .true_type_font()
            .and_then(|font| font.glyph(2).ok().flatten())
            .unwrap()
            .points
            .len(),
        1
    );

    let no_program = Value::Dict(Dict::new().insert("Subtype", Value::Name("TrueType".into())));
    assert!(doc.truetype_font(&no_program).unwrap().is_none());

    let wrong_program = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile",
                            Value::Stream(Dict::new(), StreamData::Owned(vec![0; 32])),
                        ),
                ),
            ),
    );
    assert!(doc.truetype_font(&wrong_program).is_err());
}

#[test]
fn type1_font_uses_builtin_courier_widths_when_no_override_exists() {
    let doc = Document::new();
    for font_name in [
        "Courier",
        "Courier-Bold",
        "Courier-Oblique",
        "Courier-BoldOblique",
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        let font = doc.type1_font(&font).unwrap().unwrap();
        assert_eq!(font.width_for(0), Some(600.0), "{font_name}");
        assert_eq!(font.width_for(65), Some(600.0), "{font_name}");
        assert_eq!(font.width_for(255), Some(600.0), "{font_name}");
    }

    for (font_name, expected_a, expected_fi) in [
        ("Helvetica", 667.0, 500.0),
        ("Helvetica-Bold", 722.0, 611.0),
        ("Helvetica-Oblique", 667.0, 500.0),
        ("Helvetica-BoldOblique", 722.0, 611.0),
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        let font = doc.type1_font(&font).unwrap().unwrap();
        assert_eq!(font.width_for(32), Some(278.0), "{font_name} space");
        assert_eq!(font.width_for(65), Some(expected_a), "{font_name} A");
        assert_eq!(font.width_for(174), Some(expected_fi), "{font_name} fi");
        assert_eq!(font.width_for(0), None, "{font_name} undefined code");
    }

    for (font_name, expected_a, expected_fi) in [
        ("Times-Roman", 722.0, 556.0),
        ("Times-Bold", 722.0, 556.0),
        ("Times-Italic", 611.0, 500.0),
        ("Times-BoldItalic", 667.0, 556.0),
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        let font = doc.type1_font(&font).unwrap().unwrap();
        assert_eq!(font.width_for(32), Some(250.0), "{font_name} space");
        assert_eq!(font.width_for(65), Some(expected_a), "{font_name} A");
        assert_eq!(font.width_for(174), Some(expected_fi), "{font_name} fi");
        assert_eq!(font.width_for(0), None, "{font_name} undefined code");
    }

    for (font_name, code, expected_width) in [
        ("Symbol", 0x20, 250.0),
        ("Symbol", 0x41, 722.0),
        ("Symbol", 0xA0, 750.0),
        ("ZapfDingbats", 0x20, 278.0),
        ("ZapfDingbats", 0x21, 974.0),
        ("ZapfDingbats", 0xFE, 918.0),
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        assert_eq!(
            doc.type1_font(&font).unwrap().unwrap().width_for(code),
            Some(expected_width),
            "{font_name} code {code:#04x}"
        );
    }

    for font_name in ["Symbol", "ZapfDingbats"] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        assert_eq!(
            doc.type1_font(&font).unwrap().unwrap().width_for(0xFF),
            None,
            "{font_name} undefined code"
        );
    }

    let explicit_width = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Courier".into()))
            .insert("FirstChar", Value::Integer(65))
            .insert("LastChar", Value::Integer(65))
            .insert("Widths", Value::Array(Array(vec![Value::Integer(575)]))),
    );
    assert_eq!(
        doc.type1_font(&explicit_width)
            .unwrap()
            .unwrap()
            .width_for(65),
        Some(575.0)
    );

    let other_standard_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert!(
        doc.type1_font(&other_standard_font)
            .unwrap()
            .unwrap()
            .width_for(65)
            .is_some()
    );
}

#[test]
fn type1_font_encoding_resolves_predefined_encodings_and_differences() {
    let doc = Document::new();
    doc.set_object(Object::new(
        10,
        0,
        Value::Array(Array(vec![
            Value::Integer(65),
            Value::Name("A.alt".into()),
            Value::Name("Aacute.alt".into()),
            Value::Name("dalethatafpatah".into()),
            Value::Name("ffi".into()),
            Value::Name("Aacute".into()),
            Value::Integer(255),
            Value::Name("lastcode".into()),
        ])),
    ));
    doc.set_object(Object::new(
        11,
        0,
        Value::Dict(
            Dict::new()
                .insert("BaseEncoding", Value::Name("WinAnsiEncoding".into()))
                .insert("Differences", Value::Ref(10, 0)),
        ),
    ));
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("Encoding", Value::Ref(11, 0)),
    );

    let encoding = doc.type1_font_encoding(&font).unwrap().unwrap();
    assert_eq!(encoding.base_encoding(), PredefinedEncoding::WinAnsi);
    assert_eq!(
        encoding.mapping_for(65),
        EncodingMapping::GlyphName("A.alt")
    );
    assert_eq!(encoding.unicode_for(65), None);
    assert_eq!(
        encoding.unicode_sequence_for(67),
        Some("\u{05d3}\u{05b2}".into())
    );
    assert_eq!(encoding.unicode_for(67), None);
    assert_eq!(encoding.unicode_for(68), Some('\u{fb03}'));
    assert_eq!(encoding.unicode_sequence_for(68), Some("\u{fb03}".into()));
    assert_eq!(encoding.unicode_for(69), Some('\u{00c1}'));
    assert_eq!(
        encoding.mapping_for(66),
        EncodingMapping::GlyphName("Aacute.alt")
    );
    assert_eq!(
        encoding.mapping_for(64),
        EncodingMapping::Predefined {
            encoding: PredefinedEncoding::WinAnsi,
            code: 64,
        }
    );
    assert_eq!(encoding.unicode_for(65), None);
    assert_eq!(
        encoding.mapping_for(255),
        EncodingMapping::GlyphName("lastcode")
    );

    for (name, expected) in [
        ("StandardEncoding", PredefinedEncoding::Standard),
        ("WinAnsiEncoding", PredefinedEncoding::WinAnsi),
        ("MacRomanEncoding", PredefinedEncoding::MacRoman),
        ("SymbolEncoding", PredefinedEncoding::Symbol),
        ("ZapfDingbatsEncoding", PredefinedEncoding::ZapfDingbats),
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name("ExampleSans".into()))
                .insert("Encoding", Value::Name(name.into())),
        );
        assert_eq!(
            doc.type1_font_encoding(&font)
                .unwrap()
                .unwrap()
                .base_encoding(),
            expected
        );
    }

    let symbol_encoding = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Symbol".into())),
    );
    let symbol = doc.type1_font_encoding(&symbol_encoding).unwrap().unwrap();
    assert_eq!(symbol.glyph_name_for(0x21).as_deref(), Some("exclam"));
    assert_eq!(symbol.glyph_name_for(0x41).as_deref(), Some("Alpha"));
    assert_eq!(symbol.glyph_name_for(0x61).as_deref(), Some("alpha"));
    assert_eq!(symbol.glyph_name_for(0xA0).as_deref(), Some("Euro"));
    assert_eq!(symbol.glyph_name_for(0xFF), None);
    assert_eq!(symbol.unicode_for(0x41), Some('\u{0391}'));
    assert_eq!(symbol.unicode_for(0x61), Some('\u{03b1}'));
    assert_eq!(symbol.unicode_for(0xA0), Some('\u{20ac}'));
    assert_eq!(symbol.unicode_for(0x44), Some('\u{2206}'));
    assert_eq!(symbol.unicode_for(0x57), Some('\u{2126}'));
    assert_eq!(symbol.unicode_for(0xA1), Some('\u{03d2}'));
    assert_eq!(symbol.unicode_for(0xFF), None);

    let dingbats_encoding = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ZapfDingbats".into())),
    );
    let dingbats = doc
        .type1_font_encoding(&dingbats_encoding)
        .unwrap()
        .unwrap();
    assert_eq!(dingbats.glyph_name_for(0x20).as_deref(), Some("space"));
    assert_eq!(dingbats.glyph_name_for(0x21).as_deref(), Some("a1"));
    assert_eq!(dingbats.glyph_name_for(0x23).as_deref(), Some("a202"));
    assert_eq!(dingbats.glyph_name_for(0x7F), None);
    let glyph_code = |name| {
        (0..=u8::MAX)
            .find(|code| dingbats.glyph_name_for(*code).as_deref() == Some(name))
            .unwrap()
    };
    for (glyph, expected) in [
        ("a1", '\u{2701}'),
        ("a4", '\u{260e}'),
        ("a104", '\u{2764}'),
        ("a109", '\u{2660}'),
        ("a120", '\u{2460}'),
        ("a161", '\u{2192}'),
        ("a191", '\u{27be}'),
    ] {
        assert_eq!(dingbats.unicode_for(glyph_code(glyph)), Some(expected));
    }
    assert_eq!(dingbats.unicode_for(0xFF), None);

    let win_ansi_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("Encoding", Value::Name("WinAnsiEncoding".into())),
    );
    let win_ansi = doc.type1_font_encoding(&win_ansi_font).unwrap().unwrap();
    assert_eq!(win_ansi.unicode_for(65), Some('A'));
    assert_eq!(win_ansi.unicode_for(128), Some('\u{20ac}'));
    assert_eq!(win_ansi.unicode_for(147), Some('\u{201c}'));
    assert_eq!(win_ansi.unicode_for(0), None);
    assert_eq!(win_ansi.unicode_for(127), None);
    assert_eq!(win_ansi.unicode_for(129), None);
    assert_eq!(win_ansi.unicode_for(255), Some('\u{00ff}'));

    let mac_roman_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("Encoding", Value::Name("MacRomanEncoding".into())),
    );
    let mac_roman = doc.type1_font_encoding(&mac_roman_font).unwrap().unwrap();
    assert_eq!(mac_roman.unicode_for(65), Some('A'));
    assert_eq!(mac_roman.unicode_for(128), Some('\u{00c4}'));
    assert_eq!(mac_roman.unicode_for(202), Some('\u{00a0}'));
    assert_eq!(mac_roman.unicode_for(240), Some('\u{f8ff}'));
    assert_eq!(mac_roman.unicode_for(0), None);
    assert_eq!(mac_roman.unicode_for(127), None);
    for code in 0x80..=u8::MAX {
        assert!(mac_roman.unicode_for(code).is_some(), "code {code:#04x}");
    }

    let symbol_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Symbol".into())),
    );
    assert_eq!(
        doc.type1_font_encoding(&symbol_font)
            .unwrap()
            .unwrap()
            .base_encoding(),
        PredefinedEncoding::Symbol
    );

    let default_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into())),
    );
    let standard = doc.type1_font_encoding(&default_font).unwrap().unwrap();
    assert_eq!(standard.base_encoding(), PredefinedEncoding::Standard);
    assert_eq!(standard.glyph_name_for(65).as_deref(), Some("A"));
    assert_eq!(standard.glyph_name_for(48).as_deref(), Some("zero"));
    assert_eq!(standard.glyph_name_for(39).as_deref(), Some("quoteright"));
    assert_eq!(standard.glyph_name_for(96).as_deref(), Some("quoteleft"));
    assert_eq!(standard.glyph_name_for(0xA1).as_deref(), Some("exclamdown"));
    assert_eq!(standard.glyph_name_for(0xAE).as_deref(), Some("fi"));
    assert_eq!(standard.glyph_name_for(0xDF).as_deref(), Some("AE"));
    assert_eq!(standard.glyph_name_for(0xFB).as_deref(), Some("germandbls"));
    assert_eq!(standard.glyph_name_for(0xA0), None);
    assert_eq!(standard.glyph_name_for(127), None);
    assert_eq!(standard.unicode_for(65), Some('A'));
    assert_eq!(standard.unicode_for(48), Some('0'));
    assert_eq!(standard.unicode_for(39), Some('\u{2019}'));
    assert_eq!(standard.unicode_for(96), Some('\u{2018}'));
    assert_eq!(standard.unicode_for(0xA1), Some('\u{00a1}'));
    assert_eq!(standard.unicode_for(0xAE), Some('\u{fb01}'));
    assert_eq!(standard.unicode_for(0xDF), Some('\u{00c6}'));
    assert_eq!(standard.unicode_for(0xFB), Some('\u{00df}'));
    assert_eq!(standard.unicode_for(0xA0), None);
    assert_eq!(standard.unicode_for(127), None);

    let standard_with_differences = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert(
                    "Differences",
                    Value::Array(Array(vec![
                        Value::Integer(65),
                        Value::Name("quoteright".into()),
                        Value::Integer(66),
                        Value::Name("unmappedGlyph".into()),
                        Value::Integer(67),
                        Value::Name("a1".into()),
                    ])),
                )),
            ),
    );
    let standard = doc
        .type1_font_encoding(&standard_with_differences)
        .unwrap()
        .unwrap();
    assert_eq!(standard.unicode_for(65), Some('\u{2019}'));
    assert_eq!(standard.unicode_for(66), None);
    assert_eq!(standard.unicode_for(67), Some('\u{2701}'));

    let unsupported = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("Encoding", Value::Name("UnknownEncoding".into())),
    );
    assert!(doc.type1_font_encoding(&unsupported).is_err());

    let invalid_differences = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert(
                    "Differences",
                    Value::Array(Array(vec![Value::Name("noCode".into())])),
                )),
            ),
    );
    assert!(doc.type1_font_encoding(&invalid_differences).is_err());

    let overflow_differences = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert(
                    "Differences",
                    Value::Array(Array(vec![
                        Value::Integer(255),
                        Value::Name("lastcode".into()),
                        Value::Name("overflow".into()),
                    ])),
                )),
            ),
    );
    assert!(doc.type1_font_encoding(&overflow_differences).is_err());

    let invalid_code = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert(
                    "Differences",
                    Value::Array(Array(vec![
                        Value::Integer(256),
                        Value::Name("outOfRange".into()),
                    ])),
                )),
            ),
    );
    assert!(doc.type1_font_encoding(&invalid_code).is_err());

    let invalid_base_encoding = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(
                    Dict::new().insert("BaseEncoding", Value::Name("UnknownEncoding".into())),
                ),
            ),
    );
    assert!(doc.type1_font_encoding(&invalid_base_encoding).is_err());
}

#[test]
fn indirect_stream_length_handles_embedded_endstream_and_cycles() {
    let payload = b"before endstream after";
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let offset1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let offset2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Length 3 0 R >>\nstream\n");
    pdf.extend_from_slice(payload);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let offset3 = pdf.len();
    pdf.extend_from_slice(format!("3 0 obj\n{}\nendobj\n", payload.len()).as_bytes());
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{offset1:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(format!("{offset2:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(format!("{offset3:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );

    let document = parse_buf(&pdf);
    let stream = document.get_object(2).unwrap().unwrap();
    assert_eq!(document.try_stream_bytes(&stream.value).unwrap(), payload);

    let mut cyclic = b"%PDF-1.7\n".to_vec();
    let offset1 = cyclic.len();
    cyclic.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let offset2 = cyclic.len();
    cyclic.extend_from_slice(b"2 0 obj\n<< /Length 2 0 R >>\nstream\ncycle\nendstream\nendobj\n");
    let xref_offset = cyclic.len();
    cyclic.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    cyclic.extend_from_slice(format!("{offset1:010} 00000 n \n").as_bytes());
    cyclic.extend_from_slice(format!("{offset2:010} 00000 n \n").as_bytes());
    cyclic.extend_from_slice(
        format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    let document = parse_buf(&cyclic);
    let error = document.get_object(2).unwrap_err();
    assert!(error.message.contains("cyclic indirect stream /Length"));
}

fn encrypt_r4_fixture() -> Vec<u8> {
    const PADDING: [u8; 32] = [
        0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01,
        0x08, 0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53,
        0x69, 0x7a,
    ];
    let pad = |password: &[u8]| {
        let mut result = password[..password.len().min(32)].to_vec();
        result.extend_from_slice(&PADDING[..32 - result.len()]);
        result
    };
    let id = b"pdf-r4-security-test";
    let user_password = pad(b"user-password");
    let owner_password = pad(b"owner-password");
    let mut owner_digest = Md5::digest(&owner_password).to_vec();
    for _ in 0..50 {
        owner_digest = Md5::digest(&owner_digest[..16]).to_vec();
    }
    let mut owner_entry = rc4_16(&owner_digest[..16], &user_password);
    for iteration in 1..20u8 {
        let key: Vec<u8> = owner_digest[..16]
            .iter()
            .map(|byte| byte ^ iteration)
            .collect();
        owner_entry = rc4_16(&key, &owner_entry);
    }

    let permissions = -4i32;
    let mut key_material = user_password;
    key_material.extend_from_slice(&owner_entry);
    key_material.extend_from_slice(&permissions.to_le_bytes());
    key_material.extend_from_slice(id);
    key_material.extend_from_slice(&[0xff; 4]);
    let mut file_key = Md5::digest(key_material).to_vec();
    for _ in 0..50 {
        file_key = Md5::digest(&file_key[..16]).to_vec();
    }
    let mut user_material = PADDING.to_vec();
    user_material.extend_from_slice(id);
    let digest = Md5::digest(user_material);
    let mut user_entry = rc4_16(&file_key[..16], &digest);
    for iteration in 1..20u8 {
        let key: Vec<u8> = file_key[..16].iter().map(|byte| byte ^ iteration).collect();
        user_entry = rc4_16(&key, &user_entry);
    }
    user_entry.extend_from_slice(&[0; 16]);

    let mut object_key_material = file_key[..16].to_vec();
    object_key_material.extend_from_slice(&[2, 0, 0, 0, 0]);
    object_key_material.extend_from_slice(b"sAlT");
    let object_key = Md5::digest(object_key_material);
    let iv = [0x73; 16];
    let plaintext = b"R4 crypt filter stream";
    let encrypted_label = encrypt_aesv2(b"R4 encrypted label", &object_key[..16], [0x29; 16]);
    let encrypted_stream = encrypt_aesv2(plaintext, &object_key[..16], iv);
    let mut buf = b"%PDF-1.7\n".to_vec();
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(
        format!(
            "2 0 obj\n<< /Label <{}> /Filter /Crypt /DecodeParms << /Name /StdCF >> /Length {} >>\nstream\n",
            hex_bytes(&encrypted_label),
            encrypted_stream.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&encrypted_stream);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    let off3 = buf.len();
    buf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Filter /Standard /V 4 /R 4 /Length 128 /O <{}> /U <{}> /P {} /EncryptMetadata false /CF << /StdCF << /CFM /AESV2 /Length 16 >> >> /StmF /StdCF /StrF /StdCF >>\nendobj\n",
            hex_bytes(&owner_entry),
            hex_bytes(&user_entry),
            permissions
        )
        .as_bytes(),
    );
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off3:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size 4 /Root 1 0 R /Encrypt 3 0 R /ID [<{}> <{}>] >>\nstartxref\n{off_xref}\n%%EOF\n",
            hex_bytes(id),
            hex_bytes(id)
        )
        .as_bytes(),
    );
    buf
}

fn encrypt_aesv2(plaintext: &[u8], key: &[u8], iv: [u8; 16]) -> Vec<u8> {
    let padding = 16 - plaintext.len() % 16;
    let mut ciphertext = plaintext.to_vec();
    ciphertext.extend(std::iter::repeat_n(padding as u8, padding));
    let cipher = Aes128::new_from_slice(key).unwrap();
    let mut previous = iv;
    for chunk in ciphertext.chunks_mut(16) {
        for (byte, prior) in chunk.iter_mut().zip(previous) {
            *byte ^= prior;
        }
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
        previous.copy_from_slice(chunk);
    }
    [iv.as_slice(), ciphertext.as_slice()].concat()
}

/// 环形 `/Prev` 链（xref A 的 `/Prev` 指向 B，B 又指回 A）：解析必须
/// 终止（防死循环护栏生效）、记 revisited warning，且链上对象仍可解析。
#[test]
fn circular_prev_chain_terminates_with_warning() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Marker /InRing >>\nendobj\n");
    let off_xref_a = buf.len();

    // 修订 A：覆盖对象 1，/Prev 指向修订 B（B 在文件更后处）。/Prev 用
    // 固定 10 位宽度占位，使 A 段长度与占位值无关，可一次算出 B 的偏移。
    let section_a = |prev: u64| {
        let mut s = String::from("xref\n0 2\n0000000000 65535 f \n");
        s.push_str(&format!("{off1:010} 00000 n \n"));
        s.push_str(&format!(
            "trailer\n<< /Size 2 /Root 1 0 R /Prev {prev:010} >>\nstartxref\n{off_xref_a}\n%%EOF\n"
        ));
        s
    };
    let off_xref_b = off_xref_a + section_a(0).len();
    buf.extend_from_slice(section_a(off_xref_b as u64).as_bytes());

    // 修订 B：/Prev 指回修订 A，构成 A→B→A 环；startxref 指向 B（最新修订）。
    buf.extend_from_slice(b"xref\n0 1\n0000000000 65535 f \n");
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 /Root 1 0 R /Prev {off_xref_a} >>\nstartxref\n{off_xref_b}\n%%EOF\n")
            .as_bytes(),
    );

    let doc = parse_buf(&buf);
    assert!(
        doc.warnings().iter().any(|w| w.contains("revisited")),
        "环形 /Prev 应记 revisited warning：{:?}",
        doc.warnings()
    );
    let o1 = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(
        o1.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("InRing")
    );
}

/// 写端对 `StreamData::Ref`（读端未解压的流，如 DCTDecode 图像）按原样
/// 透传：`/Filter` 保留、流字节不变（不得按 Owned 流的剥离规则误删过滤器）。
#[test]
fn ref_stream_passthrough_keeps_filter_and_bytes() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(
        b"2 0 obj\n<< /Filter /DCTDecode /Length 4 >>\nstream\n\xFF\xD9\xAB\xCD\nendstream\nendobj\n",
    );
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{off_xref}\n%%EOF\n").as_bytes(),
    );

    let doc = parse_buf(&buf);
    let out = try_write(&doc, PdfVersion::V1_4).expect("write");
    let doc2 = parse_buf(&out);
    let s = doc2.get_object(2).expect("parse").expect("stream present");
    let kept = s
        .value
        .as_dict()
        .and_then(|d| d.get("Filter"))
        .and_then(|v| v.as_name());
    assert_eq!(kept, Some("DCTDecode"), "Ref 流的过滤器应原样保留");
    assert_eq!(
        doc2.stream_bytes(&s.value).expect("stream bytes"),
        [0xFF, 0xD9, 0xAB, 0xCD],
        "Ref 流字节应原样透传"
    );
}

/// xref 条目的 generation 与对象头的 generation 不一致时：以对象头为准
/// （xref gen 仅为建议值），对象仍可解析并进入缓存。
#[test]
fn xref_generation_mismatch_defers_to_object_header() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Marker /Gen0 >>\nendobj\n");
    let off_xref = buf.len();
    // 条目声明 gen=1，但对象头是 `1 0 obj`（gen=0）——刻意不一致。
    buf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00001 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{off_xref}\n%%EOF\n").as_bytes(),
    );

    let doc = parse_buf(&buf);
    let o = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(o.generation, 0, "对象头的 generation 应优先于 xref 条目");
    assert_eq!(
        o.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("Gen0")
    );
    assert_eq!(doc.cached_object_count(), 1, "解析结果应进入缓存");
}

/// 写入 → 重新解析 → 提取：Type 0 + CIDFontType2（合成 TrueType 程序、
/// ToUnicode CMap）下的十六进制 CID 文本应原样还原为 Unicode。
#[test]
fn type0_text_roundtrips_through_writer_and_extractor() {
    let mut content = core::content::ContentStream::new();
    content
        .begin_text()
        .set_font("F1", 18.0)
        .move_next_line(72.0, 700.0)
        .show_text_hex([0x00u8, 0x41, 0x00, 0x42, 0x00, 0x43])
        .end_text();

    let to_unicode = b"/CIDInit /ProcSet findresource begin\n\
12 dict begin\n\
begincmap\n\
/CMapName /Test-UCS2 def\n\
/CMapType 2 def\n\
1 begincodespacerange\n\
<0000> <FFFF>\n\
endcodespacerange\n\
3 beginbfchar\n\
<0041> <4E2D>\n\
<0042> <6587>\n\
<0043> <D83DDE00>\n\
endbfchar\n\
endcmap\n\
CMapName currentdict /CMap defineresource pop\n\
end\n\
end"
    .to_vec();

    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Contents", Value::Ref(5, 0))
                .insert(
                    "Resources",
                    Value::Dict(Dict::new().insert(
                        "Font",
                        Value::Dict(Dict::new().insert("F1", Value::Ref(4, 0))),
                    )),
                ),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type0".into()))
                .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
                .insert("Encoding", Value::Name("Identity-H".into()))
                .insert(
                    "DescendantFonts",
                    Value::Array(Array::new().push(Value::Ref(6, 0))),
                )
                .insert("ToUnicode", Value::Ref(7, 0)),
        ),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(content.to_bytes())),
    ));
    doc.set_object(Object::new(
        6,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("CIDFontType2".into()))
                .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
                .insert(
                    "CIDSystemInfo",
                    Value::Dict(
                        Dict::new()
                            .insert("Registry", Value::String("Adobe".into()))
                            .insert("Ordering", Value::String("Identity".into()))
                            .insert("Supplement", Value::Integer(0)),
                    ),
                )
                .insert("CIDToGIDMap", Value::Name("Identity".into()))
                .insert("FontDescriptor", Value::Ref(8, 0)),
        ),
    ));
    doc.set_object(Object::new(
        7,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(to_unicode)),
    ));
    doc.set_object(Object::new(
        8,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontName", Value::Name("ExampleCIDFont".into()))
                .insert("FontFile2", Value::Ref(9, 0)),
        ),
    ));
    doc.set_object(Object::new(
        9,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(sample_sfnt_font())),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(10)),
    };

    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let reparsed = parse_buf(&buf);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("中文😀"));
}

/// 写入 → 重新解析 → 提取：缺少 `/Encoding` 的 TrueType 简单字体应通过
/// 嵌入程序的 cmap（内建编码）解码，而不是按 `/BaseFont`（Symbol）的
/// 预定义编码把 0x41 解成希腊字母 Α。
#[test]
fn truetype_without_encoding_extracts_through_builtin_cmap() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Contents", Value::Ref(5, 0))
                .insert(
                    "Resources",
                    Value::Dict(Dict::new().insert(
                        "Font",
                        Value::Dict(Dict::new().insert("F1", Value::Ref(4, 0))),
                    )),
                ),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("TrueType".into()))
                .insert("BaseFont", Value::Name("Symbol".into()))
                .insert("FontDescriptor", Value::Ref(6, 0)),
        ),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Stream(
            Dict::new(),
            StreamData::Owned(b"BT /F1 12 Tf (A) Tj ET".to_vec()),
        ),
    ));
    doc.set_object(Object::new(
        6,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontName", Value::Name("Symbol".into()))
                .insert("FontFile2", Value::Ref(7, 0)),
        ),
    ));
    doc.set_object(Object::new(
        7,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(sample_sfnt_font())),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(8)),
    };

    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let reparsed = parse_buf(&buf);
    // sample_sfnt_font 的 cmap 将 0x41 映射到 glyph 2，反向为 U+0041；
    // Symbol 预定义编码则会把 0x41 解成 U+0391 (Α)。
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("A"));
}

/// 写入 → 重新解析 → 提取：builder 写端的自定义 `/Differences`
/// 应经读端编码解析后反映到文本提取。
#[test]
fn builder_custom_encoding_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontEncoding, PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::with_differences(
        Some("WinAnsiEncoding"),
        vec![(65, "eacute".to_string()), (66, "egrave".to_string())],
    ));
    builder.text(page, &font, 72.0, 700.0, "éè");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let reparsed = parse_buf(&buf);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("éè"));
}

/// 写入 → 重新解析 → 提取：WinAnsi/MacRoman 的非 ASCII 文本应经写端
/// 字符码转换后，由读端按同一编码还原。
#[test]
fn builder_encodes_non_ascii_text_for_roundtrip() {
    use pdf_engine::core::builder::{Font, FontEncoding, PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    let win_page = builder.page(PageSize::A4);
    builder.text(win_page, &Font::helvetica(12.0), 72.0, 700.0, "Café €");
    let mac_page = builder.page(PageSize::A4);
    let mac_font = Font::helvetica(12.0).with_encoding(FontEncoding::named("MacRomanEncoding"));
    builder.text(mac_page, &mac_font, 72.0, 700.0, "Café €");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let reparsed = parse_buf(&buf);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Café €"));
    assert_eq!(reparsed.extract_page_text(1).as_deref(), Some("Café €"));
}

/// 写入 → 重新解析：FontDescriptor 应可被读端解析并暴露度量。
#[test]
fn builder_font_descriptor_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontDescriptor, PageSize, PdfBuilder};
    use pdf_engine::core::types::Value;

    let descriptor = FontDescriptor::new()
        .with_family("Helvetica")
        .with_ascent(718.0)
        .with_descent(-207.0)
        .with_cap_height(718.0)
        .with_stem_v(80.0)
        .with_flags(32);
    let font = Font::helvetica(12.0).with_descriptor(descriptor);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let reparsed = parse_buf(&buf);
    // 对象 3 是字体字典（Catalog=1, Pages=2, Page=3, ContentStream=4, Font=5, Descriptor=6）
    // 但实际上对象号分配可能不同，我们需要找到字体对象
    // 简单做法：遍历所有对象找到包含 FontDescriptor 引用的字体
    for num in 1..=20 {
        if let Ok(Some(obj)) = reparsed.get_object(num) {
            if let Value::Dict(dict) = &obj.value {
                if dict.get("Type").and_then(Value::as_name) == Some("Font") {
                    if dict.get("FontDescriptor").is_some() {
                        let type1 = reparsed
                            .type1_font(&Value::Dict(dict.clone()))
                            .expect("should parse font")
                            .expect("should be Type1");
                        let desc = type1.descriptor.expect("should have descriptor");
                        assert_eq!(desc.font_family.as_deref(), Some("Helvetica"));
                        assert_eq!(desc.ascent, Some(718.0));
                        assert_eq!(desc.descent, Some(-207.0));
                        assert_eq!(desc.cap_height, Some(718.0));
                        assert_eq!(desc.stem_v, Some(80.0));
                        assert_eq!(desc.flags, Some(32));
                        return;
                    }
                }
            }
        }
    }
    panic!("Font with FontDescriptor not found");
}

/// 写入 → 重新解析： `with_widths` 写出的 `/FirstChar`、`/LastChar` 与
/// `/Widths` 应被读端按同一数值还原，文本提取不受影响。
#[test]
fn builder_widths_roundtrip_through_parser() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};

    let widths = [500.0, 600.5, 700.0];
    let font = Font::helvetica(12.0).with_widths(65, &widths);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "AB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let dict = font_dict.as_dict().expect("font dict");
    assert_eq!(dict.get("FirstChar").and_then(Value::as_integer), Some(65));
    assert_eq!(dict.get("LastChar").and_then(Value::as_integer), Some(67));
    let parsed = reparsed
        .type1_font(&font_dict)
        .expect("should parse font")
        .expect("should be Type1");
    assert_eq!(parsed.first_char, Some(65));
    assert_eq!(parsed.widths, widths.to_vec());
    assert_eq!(parsed.width_for(65), Some(500.0));
    assert_eq!(parsed.width_for(66), Some(600.5));
    assert_eq!(parsed.width_for(67), Some(700.0));
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
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

/// 写入 → 重新解析： builder 嵌入的 Type 1 程序应作为 `/FontFile`
/// 保存原始字节，并可继续按 Type 1 格式解析。
#[test]
fn builder_embeds_type1_font_program_roundtrip() {
    use pdf_engine::core::builder::{Font, FontDescriptor, FontProgram, PageSize, PdfBuilder};

    let program = sample_type1_program();
    let font = Font::helvetica(12.0)
        .with_descriptor(FontDescriptor::new().with_font_name("Helvetica"))
        .with_font_program(FontProgram::Type1(program.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let embedded = reparsed
        .type1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile);
    assert_eq!(embedded.data, program);

    let parsed = reparsed
        .type1_program(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert!(parsed.glyph("A").expect("glyph lookup").is_some());
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Test"));
}

/// 写入 → 重新解析： builder 按 Type 1 字形名子集化嵌入的 `/FontFile`，
/// BaseFont 带标签前缀，文本提取与子集后的程序都只保留 `.notdef`/`A`。
#[test]
fn builder_type1_subset_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontEncoding, FontProgram, PageSize, PdfBuilder};

    let program = sample_type1_program();
    let font = Font::helvetica(12.0)
        .with_encoding(FontEncoding::named("WinAnsiEncoding"))
        .with_font_program(FontProgram::Type1(program.clone()))
        .with_subset_type1_glyphs(&["A"]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "A");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let base_font = font_dict
        .as_dict()
        .and_then(|dict| dict.get("BaseFont"))
        .and_then(Value::as_name)
        .expect("BaseFont");
    let (tag, name) = base_font
        .split_once('+')
        .expect("子集 BaseFont 应带标签前缀");
    assert_eq!(tag.len(), 6, "标签应为六个字母: {tag}");
    assert_eq!(name, "Helvetica");

    let embedded = reparsed
        .type1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile);
    assert_ne!(embedded.data, program, "嵌入程序应被重写为子集");

    let parsed = reparsed
        .type1_program(&font_dict)
        .expect("should parse subset program")
        .expect("should have program");
    let names: Vec<&str> = parsed.charstrings.keys().map(String::as_str).collect();
    assert_eq!(names, [".notdef", "A"], "子集程序应只含 .notdef 与请求字形");
    assert!(parsed.glyph("A").expect("glyph lookup").is_some());
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("A"));
}

/// 写入 → 重新解析： builder 嵌入的 TrueType 程序应作为 `/FontFile2`
/// 保存，并让字体字典使用 `/Subtype /TrueType`。
#[test]
fn builder_embeds_truetype_font_program_roundtrip() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let sfnt = sample_sfnt_font();
    let font = Font::helvetica(12.0).with_font_program(FontProgram::TrueType(sfnt.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "TrueType");
    let embedded = reparsed
        .truetype_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_eq!(embedded.data, sfnt);

    let parsed = reparsed
        .truetype_font(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert_eq!(parsed.units_per_em(), 1000);
    assert_eq!(parsed.glyph_count(), 3);
    assert_eq!(parsed.glyph_id_for_unicode(0x41).unwrap(), Some(2));
}

/// 写入 → 重新解析：TrueType-flavored OpenType 程序按 ISO 32000-1 §9.9
/// 写入 `/FontFile2`，字体保持 `/Subtype /TrueType`，文本可提取。
#[test]
fn builder_opentype_truetype_program_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let sfnt = sample_sfnt_font(); // 版本标签 0x00010000
    let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(sfnt.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "TrueType");
    let embedded = reparsed
        .truetype_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_eq!(embedded.data, sfnt);

    let parsed = reparsed
        .truetype_font(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert_eq!(parsed.glyph_id_for_unicode(0x41).unwrap(), Some(2));
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Test"));
}

/// 写入 → 重新解析：CFF-flavored OpenType 程序写到 `/FontFile3`
/// `/Subtype /OpenType`，字体子类型为 Type1，文本可提取。
#[test]
fn builder_opentype_cff_program_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontEncoding, FontProgram, PageSize, PdfBuilder};

    let program = sample_opentype_cff_sfnt();
    let font = Font::helvetica(12.0)
        .with_encoding(FontEncoding::named("WinAnsiEncoding"))
        .with_font_program(FontProgram::OpenType(program.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Open");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let embedded = reparsed
        .type1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("OpenType"));
    assert_eq!(embedded.data, program);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Open"));
}

/// 写入 → 重新解析：CMYK 图片 XObject 应声明 `/ColorSpace /DeviceCMYK`，
/// 流字节原样保留，页面内容流经 `Do` 引用该图片且文本仍可提取。
#[test]
fn builder_cmyk_image_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, PageSize, PdfBuilder};

    let pixels = vec![255, 0, 0, 0, 0, 255, 0, 0];
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceCMYK,
        bits: 8,
        data: pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "CMYK");
    builder.image(page, 72.0, 600.0, 100.0, 50.0, &img);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");

    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let Value::Ref(image_num, _) = xobjects.get("Im1").expect("Im1 resource") else {
        panic!("Im1 should be an indirect reference");
    };
    let image_object = reparsed
        .get_object(*image_num)
        .expect("get object")
        .expect("image object should exist");
    let Value::Stream(dict, _) = &image_object.value else {
        panic!("image should be a stream");
    };
    assert_eq!(dict.get("Subtype").and_then(Value::as_name), Some("Image"));
    assert_eq!(
        dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceCMYK")
    );
    assert_eq!(dict.get("Width").and_then(Value::as_integer), Some(2));
    assert_eq!(dict.get("Height").and_then(Value::as_integer), Some(1));
    assert_eq!(
        dict.get("BitsPerComponent").and_then(Value::as_integer),
        Some(8)
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&image_object.value)
            .expect("decode image"),
        pixels
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    assert!(content_text.contains("/Im1 Do"), "{content_text}");
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("CMYK"));
}

/// 写入 → 重新解析：带 `/Decode`、`/Interpolate` 与 `/SMask` 的图片应
/// 写出软蒙版流对象并在基图中引用；蒙版为同尺寸 DeviceGray 图片，
/// 像素与文本经解析器往返后保持可读。
#[test]
fn builder_soft_mask_image_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, PageSize, PdfBuilder};

    let mask_pixels = vec![0, 64, 128, 255];
    let mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: mask_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };
    let pixels = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: pixels.clone(),
        decode: Some(vec![0.0, 1.0, 1.0, 0.0, 0.0, 1.0]),
        interpolate: true,
        smask: Some(Box::new(mask)),
        image_mask: false,
        mask: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "IMG");
    builder.image(page, 72.0, 600.0, 100.0, 100.0, &img);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");

    let image_object = reparsed
        .get_object(xobjects.get("Im1").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("image object should exist");
    let Value::Stream(dict, _) = &image_object.value else {
        panic!("image should be a stream");
    };
    assert_eq!(dict.get("Subtype").and_then(Value::as_name), Some("Image"));
    assert_eq!(
        dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    let decode = dict
        .get("Decode")
        .and_then(Value::as_array)
        .expect("should carry Decode array");
    let decode_values: Vec<f64> = decode
        .0
        .iter()
        .map(|value| match value {
            Value::Float(f) => *f,
            Value::Integer(i) => *i as f64,
            other => panic!("Decode entry should be numeric, got {}", other.type_name()),
        })
        .collect();
    assert_eq!(decode_values, vec![0.0, 1.0, 1.0, 0.0, 0.0, 1.0]);
    assert!(
        matches!(dict.get("Interpolate"), Some(Value::Bool(true))),
        "应写出 /Interpolate true"
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&image_object.value)
            .expect("decode image"),
        pixels
    );

    let mask_num = dict
        .get("SMask")
        .and_then(Value::as_ref)
        .expect("image should reference an SMask")
        .0;
    let mask_object = reparsed
        .get_object(mask_num)
        .expect("get mask object")
        .expect("mask object should exist");
    let Value::Stream(mask_dict, _) = &mask_object.value else {
        panic!("mask should be a stream");
    };
    assert_eq!(
        mask_dict.get("Subtype").and_then(Value::as_name),
        Some("Image")
    );
    assert_eq!(
        mask_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceGray")
    );
    assert_eq!(mask_dict.get("Width").and_then(Value::as_integer), Some(2));
    assert_eq!(mask_dict.get("Height").and_then(Value::as_integer), Some(2));
    assert_eq!(
        mask_dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer),
        Some(8)
    );
    assert!(mask_dict.get("SMask").is_none(), "软蒙版不得再嵌套蒙版");
    assert_eq!(
        reparsed
            .decode_stream_bytes(&mask_object.value)
            .expect("decode mask"),
        mask_pixels
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    assert!(content_text.contains("/Im1 Do"), "{content_text}");
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("IMG"));
}

/// 写入 → 重新解析：Stencil 蒙版、显式蒙版引用与 Colour Key 蒙版
/// 应分别写出 `/ImageMask true`（无 ColorSpace）、`/Mask` 间接引用
/// （指向独立的 1 位蒙版流）与 `/Mask` 整数区间数组。
#[test]
fn builder_image_masks_roundtrip_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, ImageMask, PageSize, PdfBuilder};

    let stencil_pixels = vec![0b1010_1010];
    let stencil = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 8,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: stencil_pixels.clone(),
        decode: Some(vec![1.0, 0.0]),
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
    };
    let explicit_mask_pixels = vec![0b1010_1010, 0b0101_0101];
    let explicit_mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 4,
        height: 2,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: explicit_mask_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
    };
    let base_pixels = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let explicit = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: base_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: Some(ImageMask::explicit(explicit_mask)),
    };
    let keyed = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0u8, 10, 20, 30, 40, 50],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: Some(ImageMask::color_key(vec![0, 10, 20, 30, 40, 50])),
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "MASK");
    builder.image(page, 72.0, 600.0, 100.0, 100.0, &stencil);
    builder.image(page, 72.0, 480.0, 100.0, 100.0, &explicit);
    builder.image(page, 72.0, 360.0, 100.0, 100.0, &keyed);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let image_object = |name: &str| -> Object {
        let num = xobjects
            .get(name)
            .and_then(Value::as_ref)
            .unwrap_or_else(|| panic!("{name} should be a reference"))
            .0;
        reparsed
            .get_object(num)
            .expect("get object")
            .unwrap_or_else(|| panic!("{name} object should exist"))
    };
    let stream_dict = |name: &str, object: &Object| -> Dict {
        match &object.value {
            Value::Stream(dict, _) => dict.clone(),
            other => panic!("{name} should be a stream, got {}", other.type_name()),
        }
    };

    let stencil_object = image_object("Im1");
    let stencil_dict = stream_dict("Im1", &stencil_object);
    assert!(
        matches!(stencil_dict.get("ImageMask"), Some(Value::Bool(true))),
        "Stencil 蒙版应写出 /ImageMask true"
    );
    assert!(
        stencil_dict.get("ColorSpace").is_none(),
        "Stencil 蒙版不得带 /ColorSpace"
    );
    assert_eq!(
        stencil_dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer),
        Some(1)
    );
    let stencil_decode: Vec<f64> = stencil_dict
        .get("Decode")
        .and_then(Value::as_array)
        .expect("stencil decode")
        .0
        .iter()
        .map(|value| match value {
            Value::Float(f) => *f,
            Value::Integer(i) => *i as f64,
            other => panic!("Decode entry should be numeric, got {}", other.type_name()),
        })
        .collect();
    assert_eq!(stencil_decode, vec![1.0, 0.0]);
    assert_eq!(
        reparsed
            .decode_stream_bytes(&stencil_object.value)
            .expect("decode stencil"),
        stencil_pixels
    );

    let explicit_object = image_object("Im2");
    let explicit_dict = stream_dict("Im2", &explicit_object);
    assert_eq!(
        explicit_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    let mask_num = explicit_dict
        .get("Mask")
        .and_then(Value::as_ref)
        .expect("显式蒙版应写出 /Mask 间接引用")
        .0;
    let mask_object = reparsed
        .get_object(mask_num)
        .expect("get mask object")
        .expect("mask object should exist");
    let Value::Stream(mask_dict, _) = &mask_object.value else {
        panic!("mask should be a stream");
    };
    assert!(matches!(
        mask_dict.get("ImageMask"),
        Some(Value::Bool(true))
    ));
    assert!(
        mask_dict.get("ColorSpace").is_none(),
        "显式蒙版不得带 /ColorSpace"
    );
    assert_eq!(mask_dict.get("Width").and_then(Value::as_integer), Some(4));
    assert_eq!(mask_dict.get("Height").and_then(Value::as_integer), Some(2));
    assert_eq!(
        mask_dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&mask_object.value)
            .expect("decode mask"),
        explicit_mask_pixels
    );

    let keyed_object = image_object("Im3");
    let keyed_dict = stream_dict("Im3", &keyed_object);
    assert_eq!(
        keyed_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    let ranges: Vec<i64> = keyed_dict
        .get("Mask")
        .and_then(Value::as_array)
        .expect("Colour Key 蒙版应写出 /Mask 数组")
        .0
        .iter()
        .map(|value| value.as_integer().expect("range entry is an integer"))
        .collect();
    assert_eq!(ranges, vec![0, 10, 20, 30, 40, 50]);

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    for name in ["/Im1 Do", "/Im2 Do", "/Im3 Do"] {
        assert!(
            content_text.contains(name),
            "missing {name}: {content_text}"
        );
    }
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("MASK"));
}

/// 写入 → 重新解析：Lab 与 ICCBased（CalRGB 备用空间）图片应写出
/// CIE-Based 颜色空间数组、ICC 配置文件流，并保持像素与文本可提取。
#[test]
fn builder_cie_color_spaces_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        CalRGB, ColorSpace, Font, ICCBased, ImageData, Lab, PageSize, PdfBuilder,
    };

    let lab_pixels = vec![50, 128, 128];
    let lab_img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::Lab(Lab {
            white_point: [0.9505, 1.0, 1.089],
            black_point: Some([0.0, 0.0, 0.0]),
            range: Some([-128.0, 127.0, -128.0, 127.0]),
        }),
        bits: 8,
        data: lab_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };

    let profile: Vec<u8> = (0..128).map(|v| v as u8).collect();
    let icc_pixels = vec![10, 20, 30, 40, 50, 60];
    let icc_img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: profile.clone(),
            alternate: Some(Box::new(ColorSpace::CalRGB(CalRGB {
                white_point: [0.9505, 1.0, 1.089],
                black_point: None,
                gamma: Some([2.2, 2.2, 2.2]),
                matrix: None,
            }))),
            range: Some(vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0]),
        }),
        bits: 8,
        data: icc_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "CIE");
    builder.image(page, 72.0, 600.0, 50.0, 50.0, &lab_img);
    builder.image(page, 150.0, 600.0, 50.0, 50.0, &icc_img);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");

    let lab_object = reparsed
        .get_object(xobjects.get("Im1").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("Lab image object should exist");
    let Value::Stream(lab_dict, _) = &lab_object.value else {
        panic!("Lab image should be a stream");
    };
    let lab_space = lab_dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("Lab ColorSpace should be an array");
    assert_eq!(lab_space.0[0].as_name(), Some("Lab"));
    let lab_cs = lab_space.0[1].as_dict().expect("Lab parameters dict");
    let white_point = lab_cs
        .get("WhitePoint")
        .and_then(Value::as_array)
        .expect("Lab WhitePoint");
    assert_eq!(white_point.0.len(), 3);
    assert!(matches!(white_point.0[1], Value::Float(v) if v == 1.0));
    let range = lab_cs
        .get("Range")
        .and_then(Value::as_array)
        .expect("Lab Range");
    let range_values: Vec<i64> = range.0.iter().filter_map(Value::as_integer).collect();
    assert_eq!(range_values, vec![-128, 127, -128, 127]);
    assert_eq!(
        reparsed
            .decode_stream_bytes(&lab_object.value)
            .expect("decode Lab image"),
        lab_pixels
    );

    let icc_image_object = reparsed
        .get_object(xobjects.get("Im2").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("ICCBased image object should exist");
    let Value::Stream(icc_image_dict, _) = &icc_image_object.value else {
        panic!("ICCBased image should be a stream");
    };
    let icc_space = icc_image_dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("ICCBased ColorSpace should be an array");
    assert_eq!(icc_space.0[0].as_name(), Some("ICCBased"));
    assert_eq!(
        reparsed
            .decode_stream_bytes(&icc_image_object.value)
            .expect("decode ICC image"),
        icc_pixels
    );

    let icc_stream_num = icc_space.0[1].as_ref().expect("ICC profile reference").0;
    let icc_profile_object = reparsed
        .get_object(icc_stream_num)
        .expect("get ICC object")
        .expect("ICC profile stream should exist");
    let Value::Stream(icc_dict, _) = &icc_profile_object.value else {
        panic!("ICC profile should be a stream");
    };
    assert_eq!(icc_dict.get("N").and_then(Value::as_integer), Some(3));
    assert_eq!(
        reparsed
            .decode_stream_bytes(&icc_profile_object.value)
            .expect("decode ICC profile"),
        profile
    );
    let alternate = icc_dict
        .get("Alternate")
        .and_then(Value::as_array)
        .expect("ICCBased alternate should be an array");
    assert_eq!(alternate.0[0].as_name(), Some("CalRGB"));
    assert!(
        alternate.0[1]
            .as_dict()
            .and_then(|dict| dict.get("Gamma"))
            .and_then(Value::as_array)
            .is_some()
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("CIE"));
}

/// 写入 → 重新解析：Indexed 图片应写出 `[/Indexed base hival 调色板]`，
/// 调色板以十六进制串保留，像素与文本仍可读取。
#[test]
fn builder_indexed_image_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, Indexed, PageSize, PdfBuilder};

    let lookup = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11];
    let pixels = vec![0, 1];
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceRGB, 1, lookup.clone())),
        bits: 8,
        data: pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "IDX");
    builder.image(page, 72.0, 600.0, 100.0, 50.0, &img);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let image_object = reparsed
        .get_object(xobjects.get("Im1").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("indexed image object should exist");
    let Value::Stream(dict, _) = &image_object.value else {
        panic!("indexed image should be a stream");
    };
    let space = dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("Indexed ColorSpace should be an array");
    assert_eq!(space.0.len(), 4);
    assert_eq!(space.0[0].as_name(), Some("Indexed"));
    assert_eq!(space.0[1].as_name(), Some("DeviceRGB"));
    assert_eq!(space.0[2].as_integer(), Some(1));
    assert_eq!(
        space.0[3],
        Value::HexString("DEADBEEF0011".into()),
        "调色板应按字节顺序写成大写十六进制"
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&image_object.value)
            .expect("decode indexed image"),
        pixels
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    assert!(
        String::from_utf8_lossy(&content).contains("/Im1 Do"),
        "content should paint the indexed image"
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("IDX"));
}

/// 写入 → 重新解析：Separation 与 DeviceN 图片的色彩空间数组应带色料名、
/// 备用空间与 tint 变换函数（Type 2 内联、Type 0 独立流）；像素流可解码，
/// 文本仍可提取。
#[test]
fn builder_separation_and_devicen_images_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, DeviceN, ExponentialFunction, Font, Function, ImageData, PageSize, PdfBuilder,
        SampledFunction, Separation,
    };

    let separation = ImageData {
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
        data: vec![0x00, 0xFF],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };
    let devicen = ImageData {
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
                vec![2, 2],
                8,
                vec![0.0; 12],
            )),
        )),
        bits: 8,
        data: vec![0x10, 0x20, 0x30, 0x40],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "SPOT");
    builder.image(page, 72.0, 600.0, 100.0, 50.0, &separation);
    builder.image(page, 72.0, 500.0, 100.0, 50.0, &devicen);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let image_object = |name: &str| {
        let number = xobjects.get(name).and_then(Value::as_ref).unwrap().0;
        reparsed
            .get_object(number)
            .expect("get object")
            .expect("image object should exist")
    };

    let separation_object = image_object("Im1");
    let Value::Stream(separation_dict, _) = &separation_object.value else {
        panic!("separation image should be a stream");
    };
    let separation_space = separation_dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("Separation ColorSpace should be an array");
    assert_eq!(separation_space.0.len(), 4);
    assert_eq!(separation_space.0[0].as_name(), Some("Separation"));
    assert_eq!(separation_space.0[1].as_name(), Some("Orange"));
    assert_eq!(separation_space.0[2].as_name(), Some("DeviceCMYK"));
    let tint = separation_space.0[3]
        .as_dict()
        .expect("Type 2 tint transform should be an inline dictionary");
    assert_eq!(
        tint.get("FunctionType").and_then(Value::as_integer),
        Some(2)
    );
    let c1 = tint.get("C1").and_then(Value::as_array).expect("C1");
    assert_eq!(c1.0[1], Value::Float(0.4));
    assert_eq!(
        reparsed
            .decode_stream_bytes(&separation_object.value)
            .expect("decode separation image"),
        vec![0x00, 0xFF]
    );

    let devicen_object = image_object("Im2");
    let Value::Stream(devicen_dict, _) = &devicen_object.value else {
        panic!("DeviceN image should be a stream");
    };
    let devicen_space = devicen_dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("DeviceN ColorSpace should be an array");
    assert_eq!(devicen_space.0.len(), 4);
    assert_eq!(devicen_space.0[0].as_name(), Some("DeviceN"));
    let names = devicen_space.0[1]
        .as_array()
        .expect("DeviceN names should be an array");
    let names: Vec<&str> = names.0.iter().filter_map(Value::as_name).collect();
    assert_eq!(names, vec!["Orange", "Green"]);
    assert_eq!(devicen_space.0[2].as_name(), Some("DeviceRGB"));
    let (function_num, _) = devicen_space.0[3]
        .as_ref()
        .expect("Type 0 tint transform should be an indirect stream");
    let function_object = reparsed
        .get_object(function_num)
        .expect("get function object")
        .expect("function stream should exist");
    let Value::Stream(function_dict, _) = &function_object.value else {
        panic!("Type 0 function should be a stream");
    };
    assert_eq!(
        function_dict
            .get("FunctionType")
            .and_then(Value::as_integer),
        Some(0)
    );
    let size = function_dict
        .get("Size")
        .and_then(Value::as_array)
        .expect("Size");
    let size: Vec<i64> = size.0.iter().filter_map(Value::as_integer).collect();
    assert_eq!(size, vec![2, 2]);
    assert_eq!(
        reparsed
            .decode_stream_bytes(&function_object.value)
            .expect("decode sampled function")
            .len(),
        12,
        "2×2 采样点 × 3 个输出"
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&devicen_object.value)
            .expect("decode devicen image"),
        vec![0x10, 0x20, 0x30, 0x40]
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("SPOT"));
}

/// 写入 → 重新解析：渐变应写入页面 `/Shading` 资源，渐变字典带
/// `/ShadingType`、`/Coords`、`/Domain`、`/Extend` 与内联 Type 2 函数；
/// 着色图案写入 `/Pattern` 资源，`PatternType 2` 字典内联径向渐变；
/// 页面内容含 `sh`、`/Pattern cs` 与 `scn`，文本仍可提取。
#[test]
fn builder_shading_and_pattern_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AxialShading, ColorSpace, ExponentialFunction, Font, Function, PageSize, PdfBuilder,
        RadialShading, Shading, ShadingPattern,
    };

    let exponential = |c0: Vec<f64>, c1: Vec<f64>| {
        Function::Exponential(
            ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                .with_c0(c0)
                .with_c1(c1),
        )
    };
    let axial = Shading::Axial(
        AxialShading::new(
            ColorSpace::DeviceGray,
            [0.0, 0.0, 200.0, 0.0],
            exponential(vec![0.0], vec![1.0]),
        )
        .with_domain([0.0, 1.0])
        .with_extend([true, false]),
    );
    let radial = Shading::Radial(
        RadialShading::new(
            ColorSpace::DeviceRGB,
            [50.0, 50.0, 0.0, 50.0, 50.0, 80.0],
            exponential(vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0]),
        )
        .with_extend([true, true]),
    );
    let pattern = ShadingPattern::new(radial).with_matrix([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "GRAD");
    let axial_handle = builder.shading(&axial);
    builder.paint_shading(page, axial_handle);
    builder.paint_shading(page, axial_handle);
    let pattern_handle = builder.shading_pattern(&pattern);
    builder.fill_rect_with_pattern(page, pattern_handle, [72.0, 500.0, 120.0, 80.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page resources");
    let shadings = resources
        .get("Shading")
        .and_then(Value::as_dict)
        .expect("page should have Shading resources");
    let patterns = resources
        .get("Pattern")
        .and_then(Value::as_dict)
        .expect("page should have Pattern resources");

    let shading_num = shadings.get("Sh1").and_then(Value::as_ref).unwrap().0;
    let shading_object = reparsed
        .get_object(shading_num)
        .expect("get shading object")
        .expect("shading object should exist");
    let shading_dict = shading_object.value.as_dict().expect("shading dict");
    assert_eq!(
        shading_dict.get("ShadingType").and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        shading_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceGray")
    );
    let coords: Vec<Value> = shading_dict
        .get("Coords")
        .and_then(Value::as_array)
        .expect("Coords")
        .0
        .clone();
    assert_eq!(coords, [0.0, 0.0, 200.0, 0.0].map(Value::Float).to_vec());
    let domain = shading_dict
        .get("Domain")
        .and_then(Value::as_array)
        .expect("Domain");
    assert_eq!(domain.0, vec![Value::Float(0.0), Value::Float(1.0)]);
    let extend = shading_dict
        .get("Extend")
        .and_then(Value::as_array)
        .expect("Extend");
    assert_eq!(extend.0, vec![Value::Bool(true), Value::Bool(false)]);
    let axial_function = shading_dict
        .get("Function")
        .and_then(Value::as_dict)
        .expect("Type 2 function should be inline");
    assert_eq!(
        axial_function
            .get("FunctionType")
            .and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        axial_function
            .get("C0")
            .and_then(Value::as_array)
            .map(|a| a.0.clone()),
        Some(vec![Value::Float(0.0)])
    );
    assert_eq!(
        axial_function
            .get("C1")
            .and_then(Value::as_array)
            .map(|a| a.0.clone()),
        Some(vec![Value::Float(1.0)])
    );

    let pattern_num = patterns.get("P1").and_then(Value::as_ref).unwrap().0;
    let pattern_object = reparsed
        .get_object(pattern_num)
        .expect("get pattern object")
        .expect("pattern object should exist");
    let pattern_dict = pattern_object.value.as_dict().expect("pattern dict");
    assert_eq!(
        pattern_dict.get("Type").and_then(Value::as_name),
        Some("Pattern")
    );
    assert_eq!(
        pattern_dict.get("PatternType").and_then(Value::as_integer),
        Some(2)
    );
    let matrix = pattern_dict
        .get("Matrix")
        .and_then(Value::as_array)
        .expect("Matrix");
    assert_eq!(
        matrix.0,
        [1.0, 0.0, 0.0, 1.0, 10.0, 20.0].map(Value::Float).to_vec()
    );
    let inline = pattern_dict
        .get("Shading")
        .and_then(Value::as_dict)
        .expect("pattern shading should be inline");
    assert_eq!(
        inline.get("ShadingType").and_then(Value::as_integer),
        Some(3)
    );
    assert_eq!(
        inline.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    let radial_coords: Vec<Value> = inline
        .get("Coords")
        .and_then(Value::as_array)
        .expect("radial Coords")
        .0
        .clone();
    assert_eq!(
        radial_coords,
        [50.0, 50.0, 0.0, 50.0, 50.0, 80.0]
            .map(Value::Float)
            .to_vec()
    );
    let radial_extend = inline
        .get("Extend")
        .and_then(Value::as_array)
        .expect("Extend");
    assert_eq!(radial_extend.0, vec![Value::Bool(true), Value::Bool(true)]);
    let radial_function = inline
        .get("Function")
        .and_then(Value::as_dict)
        .expect("radial function");
    assert_eq!(
        radial_function
            .get("C0")
            .and_then(Value::as_array)
            .map(|a| a.0.clone()),
        Some(vec![
            Value::Float(1.0),
            Value::Float(0.0),
            Value::Float(0.0)
        ])
    );
    assert_eq!(
        radial_function
            .get("C1")
            .and_then(Value::as_array)
            .map(|a| a.0.clone()),
        Some(vec![
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(1.0)
        ])
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert_eq!(content.matches("/Sh1 sh").count(), 2, "{content}");
    assert!(content.contains("/Pattern cs"), "{content}");
    assert!(content.contains("/P1 scn"), "{content}");
    assert!(content.contains("72.0 500.0 120.0 80.0 re"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("GRAD"));
}

/// 写入 → 重新解析：所有渐变共有的 `/Background`、`/BBox` 与
/// `/AntiAlias` 应同时出现在直接渐变字典与网格流渐变字典中，页面仍以
/// `sh` 绘制且文本可提取。
#[test]
fn builder_shading_options_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AxialShading, ColorSpace, CoonsPatch, CoonsPatchShading, ExponentialFunction, Font,
        Function, PageSize, PdfBuilder, Shading, ShadingOptions,
    };

    let function = Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(vec![0.0])
            .with_c1(vec![1.0]),
    );
    let axial = Shading::Axial(
        AxialShading::new(ColorSpace::DeviceGray, [0.0, 0.0, 200.0, 0.0], function).with_options(
            ShadingOptions::new()
                .with_background([0.5])
                .with_bbox([10.0, 20.0, 200.0, 100.0])
                .with_anti_alias(true),
        ),
    );
    let coons_points = [
        [0.0, 0.0],
        [10.0, 0.0],
        [20.0, 0.0],
        [30.0, 0.0],
        [30.0, 10.0],
        [30.0, 20.0],
        [30.0, 30.0],
        [20.0, 30.0],
        [10.0, 30.0],
        [0.0, 30.0],
        [0.0, 20.0],
        [0.0, 10.0],
    ];
    let coons = CoonsPatchShading::new(
        ColorSpace::DeviceGray,
        vec![CoonsPatch::new(
            coons_points,
            [vec![0.0], vec![1.0], vec![0.0], vec![1.0]],
        )],
    )
    .with_bits(8, 8, 8)
    .with_options(
        ShadingOptions::new()
            .with_background([1.0])
            .with_bbox([0.0, 0.0, 30.0, 30.0])
            .with_anti_alias(false),
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "OPTS");
    let axial_handle = builder.shading(&axial);
    builder.paint_shading(page, axial_handle);
    let coons_handle = builder.shading(&Shading::Coons(coons));
    builder.paint_shading(page, coons_handle);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let shadings = page_object
        .value
        .as_dict()
        .and_then(|dict| dict.get("Resources"))
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("Shading"))
        .and_then(Value::as_dict)
        .expect("page should have Shading resources");
    let shading_dict = |name: &str| {
        let shading_num = shadings.get(name).and_then(Value::as_ref).unwrap().0;
        reparsed
            .get_object(shading_num)
            .expect("get shading object")
            .expect("shading object should exist")
            .value
            .as_dict()
            .expect("shading dictionary")
            .clone()
    };

    let direct = shading_dict("Sh1");
    assert_eq!(
        direct.get("ShadingType").and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        direct
            .get("Background")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![Value::Float(0.5)])
    );
    assert_eq!(
        direct
            .get("BBox")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some([10.0, 20.0, 200.0, 100.0].map(Value::Float).to_vec())
    );
    assert_eq!(direct.get("AntiAlias"), Some(&Value::Bool(true)));

    let mesh = shading_dict("Sh2");
    assert_eq!(mesh.get("ShadingType").and_then(Value::as_integer), Some(6));
    assert_eq!(
        mesh.get("Background")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![Value::Float(1.0)])
    );
    assert_eq!(
        mesh.get("BBox")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some([0.0, 0.0, 30.0, 30.0].map(Value::Float).to_vec())
    );
    assert_eq!(mesh.get("AntiAlias"), Some(&Value::Bool(false)));

    let Value::Ref(content_num, _) = page_object
        .value
        .as_dict()
        .unwrap()
        .get("Contents")
        .unwrap()
    else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/Sh1 sh"), "{content}");
    assert!(content.contains("/Sh2 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("OPTS"));
}

/// 写入 → 重新解析：Type 4/5 网格渐变携带 `/Function` 时，每个顶点只
/// 编码一个参数变量，`/Decode` 只保留一对颜色范围；Type 0 函数写成
/// 独立流对象，Type 2 函数内联；页面仍用 `sh` 绘制且文本可提取。
#[test]
fn builder_mesh_shading_function_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, ExponentialFunction, Font, FreeFormMeshShading, Function, LatticeMeshShading,
        MeshVertex, PageSize, PdfBuilder, SampledFunction, Shading,
    };

    let color_function = Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(vec![1.0, 0.0, 0.0])
            .with_c1(vec![0.0, 0.0, 1.0]),
    );
    let free_form = FreeFormMeshShading::new(
        ColorSpace::DeviceRGB,
        vec![
            MeshVertex::new(10.0, 20.0, vec![0.0]),
            MeshVertex::new(90.0, 20.0, vec![0.5]),
            MeshVertex::new(50.0, 80.0, vec![1.0]),
        ],
    )
    .with_function(color_function)
    .with_decode([0.0, 100.0, 0.0, 100.0])
    .with_bits(8, 8, 8);
    let sampled = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0],
        vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        vec![2],
        8,
        vec![0.0, 0.0, 0.0, 255.0, 255.0, 255.0],
    ));
    let lattice = LatticeMeshShading::new(
        ColorSpace::DeviceRGB,
        2,
        vec![
            MeshVertex::new(0.0, 0.0, vec![0.0]),
            MeshVertex::new(10.0, 0.0, vec![0.0]),
            MeshVertex::new(0.0, 10.0, vec![1.0]),
            MeshVertex::new(10.0, 10.0, vec![1.0]),
        ],
    )
    .with_function(sampled)
    .with_decode([0.0, 10.0, 0.0, 10.0])
    .with_bits(8, 8);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "FUNC");
    let free_form_handle = builder.shading(&Shading::FreeForm(free_form));
    builder.paint_shading(page, free_form_handle);
    let lattice_handle = builder.shading(&Shading::Lattice(lattice));
    builder.paint_shading(page, lattice_handle);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let shadings = page_object
        .value
        .as_dict()
        .and_then(|dict| dict.get("Resources"))
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("Shading"))
        .and_then(Value::as_dict)
        .expect("page should have Shading resources");
    let shading_object = |name: &str| {
        let shading_num = shadings.get(name).and_then(Value::as_ref).unwrap().0;
        reparsed
            .get_object(shading_num)
            .expect("get shading object")
            .expect("shading object should exist")
    };

    let free_form_object = shading_object("Sh1");
    let free_form_dict = free_form_object.value.as_dict().expect("shading dict");
    assert_eq!(
        free_form_dict
            .get("ShadingType")
            .and_then(Value::as_integer),
        Some(4)
    );
    assert_eq!(
        free_form_dict
            .get("Decode")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(
            [0.0, 100.0, 0.0, 100.0, 0.0, 1.0]
                .map(Value::Float)
                .to_vec()
        )
    );
    let inline_function = free_form_dict
        .get("Function")
        .and_then(Value::as_dict)
        .expect("Type 2 function should be inline");
    assert_eq!(
        inline_function
            .get("FunctionType")
            .and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        inline_function
            .get("C0")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![
            Value::Float(1.0),
            Value::Float(0.0),
            Value::Float(0.0)
        ])
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&free_form_object.value)
            .expect("decode type 4 payload"),
        vec![0, 26, 51, 0, 0, 230, 51, 128, 0, 128, 204, 255]
    );

    let lattice_object = shading_object("Sh2");
    let lattice_dict = lattice_object.value.as_dict().expect("shading dict");
    assert_eq!(
        lattice_dict.get("ShadingType").and_then(Value::as_integer),
        Some(5)
    );
    assert!(!lattice_dict.0.contains_key("BitsPerFlag"));
    let Value::Ref(function_num, _) = lattice_dict.get("Function").expect("Function") else {
        panic!("Type 0 function must be an indirect stream");
    };
    let function_object = reparsed
        .get_object(*function_num)
        .expect("get function object")
        .expect("function object should exist");
    let function_dict = function_object.value.as_dict().expect("function dict");
    assert_eq!(
        function_dict
            .get("FunctionType")
            .and_then(Value::as_integer),
        Some(0)
    );
    assert_eq!(
        function_dict
            .get("Size")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![Value::Integer(2)])
    );
    assert_eq!(
        function_dict
            .get("BitsPerSample")
            .and_then(Value::as_integer),
        Some(8)
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&function_object.value)
            .expect("decode sampled samples"),
        vec![0, 0, 0, 255, 255, 255]
    );

    let Value::Ref(content_num, _) = page_object
        .value
        .as_dict()
        .unwrap()
        .get("Contents")
        .unwrap()
    else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/Sh1 sh"), "{content}");
    assert!(content.contains("/Sh2 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("FUNC"));
}

/// 写入 → 重新解析：网格 `/Function` 可以是按颜色分量排列的函数数组；
/// 每个顶点仍只编码一个参数变量，数组内联三个 Type 2 函数。
#[test]
fn builder_mesh_function_array_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, ExponentialFunction, Font, FreeFormMeshShading, Function, MeshVertex, PageSize,
        PdfBuilder, Shading,
    };

    let component = |c0: f64, c1: f64| {
        Function::Exponential(
            ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                .with_c0(vec![c0])
                .with_c1(vec![c1]),
        )
    };
    let mesh = FreeFormMeshShading::new(
        ColorSpace::DeviceRGB,
        vec![
            MeshVertex::new(10.0, 20.0, vec![0.0]),
            MeshVertex::new(90.0, 20.0, vec![0.5]),
            MeshVertex::new(50.0, 80.0, vec![1.0]),
        ],
    )
    .with_component_functions(vec![
        component(0.0, 1.0),
        component(1.0, 0.0),
        component(0.0, 0.0),
    ])
    .with_decode([0.0, 100.0, 0.0, 100.0])
    .with_bits(8, 8, 8);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "AFUN");
    let handle = builder.shading(&Shading::FreeForm(mesh));
    builder.paint_shading(page, handle);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let shadings = page_object
        .value
        .as_dict()
        .and_then(|dict| dict.get("Resources"))
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("Shading"))
        .and_then(Value::as_dict)
        .expect("page should have Shading resources");
    let shading_num = shadings.get("Sh1").and_then(Value::as_ref).unwrap().0;
    let shading_object = reparsed
        .get_object(shading_num)
        .expect("get shading object")
        .expect("shading object should exist");
    let shading_dict = shading_object.value.as_dict().expect("shading dict");
    assert_eq!(
        shading_dict.get("ShadingType").and_then(Value::as_integer),
        Some(4)
    );
    assert_eq!(
        shading_dict
            .get("Decode")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(
            [0.0, 100.0, 0.0, 100.0, 0.0, 1.0]
                .map(Value::Float)
                .to_vec()
        )
    );
    let functions = shading_dict
        .get("Function")
        .and_then(Value::as_array)
        .expect("Function should be an array");
    assert_eq!(functions.0.len(), 3);
    for (value, (c0, c1)) in functions.0.iter().zip([(0.0, 1.0), (1.0, 0.0), (0.0, 0.0)]) {
        let function = value.as_dict().expect("component function dictionary");
        assert_eq!(
            function.get("FunctionType").and_then(Value::as_integer),
            Some(2)
        );
        assert_eq!(
            function
                .get("C0")
                .and_then(Value::as_array)
                .map(|array| array.0.clone()),
            Some(vec![Value::Float(c0)])
        );
        assert_eq!(
            function
                .get("C1")
                .and_then(Value::as_array)
                .map(|array| array.0.clone()),
            Some(vec![Value::Float(c1)])
        );
    }
    assert_eq!(
        reparsed
            .decode_stream_bytes(&shading_object.value)
            .expect("decode type 4 payload"),
        vec![0, 26, 51, 0, 0, 230, 51, 128, 0, 128, 204, 255],
        "参数变量编码应与单函数形式一致"
    );

    let Value::Ref(content_num, _) = page_object
        .value
        .as_dict()
        .unwrap()
        .get("Contents")
        .unwrap()
    else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/Sh1 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AFUN"));
}

/// 写入 → 重新解析：页级 `/ColorSpace` 命名资源应写出 CalRGB 与
/// Separation 数组，内容流用 `cs`/`CS` 切换空间、用分量操作符设色，
/// 文本仍可提取。
#[test]
fn builder_named_color_spaces_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        CalRGB, ColorSpace, ExponentialFunction, Font, Function, PageSize, PdfBuilder, Separation,
    };

    let mut cal = CalRGB::new([0.9505, 1.0, 1.089]);
    cal.gamma = Some([2.2, 2.2, 2.2]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let fill = builder.color_space(&ColorSpace::CalRGB(cal));
    let stroke = builder.color_space(&ColorSpace::Separation(Separation::new(
        "Spot",
        ColorSpace::DeviceGray,
        Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
    )));
    builder.set_fill_color_space(page, fill);
    builder.set_fill_color_components(page, &[0.2, 0.4, 0.6]);
    builder.set_stroke_color_space(page, stroke);
    builder.set_stroke_color_components(page, &[0.5]);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "NAMED");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let color_spaces = page_object
        .value
        .as_dict()
        .and_then(|dict| dict.get("Resources"))
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("ColorSpace"))
        .and_then(Value::as_dict)
        .expect("page should have ColorSpace resources");

    let cal_array = color_spaces
        .get("CS1")
        .and_then(Value::as_array)
        .expect("CS1 should be an array");
    assert_eq!(cal_array.0[0].as_name(), Some("CalRGB"));
    let cal_dict = cal_array.0[1].as_dict().expect("CalRGB dictionary");
    assert_eq!(
        cal_dict
            .get("WhitePoint")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![
            Value::Float(0.9505),
            Value::Float(1.0),
            Value::Float(1.089)
        ])
    );
    assert_eq!(
        cal_dict
            .get("Gamma")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![
            Value::Float(2.2),
            Value::Float(2.2),
            Value::Float(2.2)
        ])
    );

    let separation_array = color_spaces
        .get("CS2")
        .and_then(Value::as_array)
        .expect("CS2 should be an array");
    assert_eq!(separation_array.0[0].as_name(), Some("Separation"));
    assert_eq!(separation_array.0[1].as_name(), Some("Spot"));
    assert_eq!(separation_array.0[2].as_name(), Some("DeviceGray"));
    assert!(
        separation_array.0[3].as_dict().is_some(),
        "tint transform should be a function dictionary"
    );

    let Value::Ref(content_num, _) = page_object
        .value
        .as_dict()
        .unwrap()
        .get("Contents")
        .unwrap()
    else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/CS1 cs"), "{content}");
    assert!(content.contains("0.2 0.4 0.6 sc"), "{content}");
    assert!(content.contains("/CS2 CS"), "{content}");
    assert!(content.contains("0.5 SC"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("NAMED"));
}

/// 写入 → 重新解析：Type 4 自由三角网格渐变应写成流对象，字典给出
/// 位深、`/BitsPerFlag` 与 `/Decode`；顶点按 MSB-first 位流编码，
/// 解码后的字节与量化结果一致，页面用 `sh` 绘制且文本仍可提取。
#[test]
fn builder_free_form_mesh_shading_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, FreeFormMeshShading, MeshVertex, PageSize, PdfBuilder, Shading,
    };

    let mesh = FreeFormMeshShading::new(
        ColorSpace::DeviceRGB,
        vec![
            MeshVertex::new(10.0, 20.0, vec![0.0, 0.0, 0.0]),
            MeshVertex::new(90.0, 20.0, vec![1.0, 0.0, 0.0]),
            MeshVertex::new(50.0, 80.0, vec![0.0, 0.0, 1.0]),
        ],
    )
    .with_decode([0.0, 100.0, 0.0, 100.0])
    .with_bits(8, 8, 8);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "MESH");
    let handle = builder.shading(&Shading::FreeForm(mesh));
    builder.paint_shading(page, handle);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page resources");
    let shadings = resources
        .get("Shading")
        .and_then(Value::as_dict)
        .expect("page should have Shading resources");
    let shading_num = shadings.get("Sh1").and_then(Value::as_ref).unwrap().0;
    let shading_object = reparsed
        .get_object(shading_num)
        .expect("get shading object")
        .expect("shading object should exist");
    let shading_dict = shading_object
        .value
        .as_dict()
        .expect("mesh shading should be a stream dictionary");
    assert_eq!(
        shading_dict.get("ShadingType").and_then(Value::as_integer),
        Some(4)
    );
    assert_eq!(
        shading_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    assert_eq!(
        shading_dict
            .get("BitsPerCoordinate")
            .and_then(Value::as_integer),
        Some(8)
    );
    assert_eq!(
        shading_dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer),
        Some(8)
    );
    assert_eq!(
        shading_dict.get("BitsPerFlag").and_then(Value::as_integer),
        Some(8)
    );
    let decode: Vec<Value> = shading_dict
        .get("Decode")
        .and_then(Value::as_array)
        .expect("Decode")
        .0
        .clone();
    assert_eq!(
        decode,
        [0.0, 100.0, 0.0, 100.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]
            .map(Value::Float)
            .to_vec()
    );

    let payload = reparsed
        .decode_stream_bytes(&shading_object.value)
        .expect("decode mesh payload");
    assert_eq!(
        payload,
        vec![
            0, 26, 51, 0, 0, 0, // flag 0、x=10→26、y=20→51、黑色
            0, 230, 51, 255, 0, 0, // x=90→230、红色
            0, 128, 204, 0, 0, 255, // x=50→128、y=80→204、蓝色
        ]
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/Sh1 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("MESH"));
}

/// 写入 → 重新解析：Type 6（Coons）与 Type 7（张量积）网格渐变应写成
/// 流对象，包含 `/BitsPerFlag`、`/Decode` 与按 MSB-first 打包的网格片
/// 数据；页面两个 `/Shading` 资源分别由 `sh` 绘制，文本仍可提取。
#[test]
fn builder_patch_mesh_shading_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, CoonsPatch, CoonsPatchShading, Font, PageSize, PdfBuilder, Shading,
        TensorProductPatch, TensorProductPatchShading,
    };

    let coons_points = [
        [0.0, 0.0],
        [10.0, 0.0],
        [20.0, 0.0],
        [30.0, 0.0],
        [30.0, 10.0],
        [30.0, 20.0],
        [30.0, 30.0],
        [20.0, 30.0],
        [10.0, 30.0],
        [0.0, 30.0],
        [0.0, 20.0],
        [0.0, 10.0],
    ];
    let tensor_points = [
        [0.0, 0.0],
        [10.0, 0.0],
        [20.0, 0.0],
        [30.0, 0.0],
        [0.0, 10.0],
        [10.0, 10.0],
        [20.0, 10.0],
        [30.0, 10.0],
        [0.0, 20.0],
        [10.0, 20.0],
        [20.0, 20.0],
        [30.0, 20.0],
        [0.0, 30.0],
        [10.0, 30.0],
        [20.0, 30.0],
        [30.0, 30.0],
    ];
    let colors = || {
        [
            vec![0.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
        ]
    };
    let coons = CoonsPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![CoonsPatch::new(coons_points, colors())],
    )
    .with_bits(8, 8, 8);
    let tensor = TensorProductPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![TensorProductPatch::new(tensor_points, colors())],
    )
    .with_bits(8, 8, 8);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "PATCH");
    let coons_handle = builder.shading(&Shading::Coons(coons));
    let tensor_handle = builder.shading(&Shading::TensorProduct(tensor));
    builder.paint_shading(page, coons_handle);
    builder.paint_shading(page, tensor_handle);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let shadings = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("Shading"))
        .and_then(Value::as_dict)
        .expect("page should have Shading resources");

    for (name, expected_type, expected_len) in [("Sh1", 6, 37), ("Sh2", 7, 45)] {
        let shading_num = shadings.get(name).and_then(Value::as_ref).unwrap().0;
        let shading_object = reparsed
            .get_object(shading_num)
            .expect("get shading object")
            .expect("shading object should exist");
        let dict = shading_object
            .value
            .as_dict()
            .expect("patch shading should be a stream dictionary");
        assert_eq!(
            dict.get("ShadingType").and_then(Value::as_integer),
            Some(expected_type)
        );
        assert_eq!(dict.get("BitsPerFlag").and_then(Value::as_integer), Some(8));
        assert_eq!(
            dict.get("BitsPerCoordinate").and_then(Value::as_integer),
            Some(8)
        );
        assert_eq!(
            dict.get("BitsPerComponent").and_then(Value::as_integer),
            Some(8)
        );
        assert_eq!(
            dict.get("Length").and_then(Value::as_integer),
            Some(expected_len)
        );
        let decode = dict
            .get("Decode")
            .and_then(Value::as_array)
            .expect("Decode");
        assert_eq!(
            decode.0[..4].to_vec(),
            [0.0, 30.0, 0.0, 30.0].map(Value::Float).to_vec()
        );
        let payload = reparsed
            .decode_stream_bytes(&shading_object.value)
            .expect("decode patch payload");
        assert_eq!(payload.len(), expected_len as usize);
        assert_eq!(payload[0], 0, "每个网格片的标志位应为 0");
        assert!(payload.iter().any(|&byte| byte != 0), "网格数据不应全为零");
    }

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/Sh1 sh"), "{content}");
    assert!(content.contains("/Sh2 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("PATCH"));
}

/// 写入 → 重新解析：平铺图案应作为流对象写出 `/Type /Pattern`、
/// `/PatternType 1`、`/PaintType 1`、`/TilingType`、`/BBox`、`/XStep`、
/// `/YStep`、`/Resources` 与图块内容流；页面 `/Pattern` 资源以 `/P1`
/// 引用且重复填充复用同一资源，文本仍可提取。
#[test]
fn builder_tiling_pattern_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder, TilingPattern, TilingType};
    use pdf_engine::core::content::ContentStream;

    let mut tile = ContentStream::new();
    tile.set_fill_rgb(0.2, 0.4, 0.6)
        .rect(0.0, 0.0, 6.0, 6.0)
        .fill();
    let pattern = TilingPattern::new([0.0, 0.0, 12.0, 12.0], 12.0, 12.0, tile)
        .with_tiling_type(TilingType::ConstantSpacingAndSpeed)
        .with_matrix([1.0, 0.0, 0.0, 1.0, 3.0, 4.0]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "TILE");
    let handle = builder.tiling_pattern(&pattern);
    builder.fill_rect_with_pattern(page, handle, [72.0, 500.0, 120.0, 80.0]);
    builder.fill_rect_with_pattern(page, handle, [72.0, 300.0, 120.0, 80.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page resources");
    let patterns = resources
        .get("Pattern")
        .and_then(Value::as_dict)
        .expect("page should have Pattern resources");
    let pattern_num = patterns.get("P1").and_then(Value::as_ref).unwrap().0;
    assert!(
        patterns.get("P2").is_none(),
        "重复填充应复用 /P1: {patterns:?}"
    );

    let pattern_object = reparsed
        .get_object(pattern_num)
        .expect("get pattern object")
        .expect("pattern object should exist");
    let Value::Stream(pattern_dict, _) = &pattern_object.value else {
        panic!("tiling pattern should be a stream");
    };
    assert_eq!(
        pattern_dict.get("Type").and_then(Value::as_name),
        Some("Pattern")
    );
    assert_eq!(
        pattern_dict.get("PatternType").and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        pattern_dict.get("PaintType").and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        pattern_dict.get("TilingType").and_then(Value::as_integer),
        Some(3)
    );
    let bbox = pattern_dict
        .get("BBox")
        .and_then(Value::as_array)
        .expect("BBox");
    assert_eq!(bbox.0, [0.0, 0.0, 12.0, 12.0].map(Value::Float).to_vec());
    assert_eq!(pattern_dict.get("XStep"), Some(&Value::Float(12.0)));
    assert_eq!(pattern_dict.get("YStep"), Some(&Value::Float(12.0)));
    let matrix = pattern_dict
        .get("Matrix")
        .and_then(Value::as_array)
        .expect("Matrix");
    assert_eq!(
        matrix.0,
        [1.0, 0.0, 0.0, 1.0, 3.0, 4.0].map(Value::Float).to_vec()
    );
    let pattern_resources = pattern_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("pattern Resources");
    assert!(pattern_resources.0.is_empty(), "{pattern_resources:?}");

    let tile_content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&pattern_object.value)
            .expect("decode pattern stream"),
    )
    .into_owned();
    assert!(tile_content.contains("0.2 0.4 0.6 rg"), "{tile_content}");
    assert!(
        tile_content.contains("0.0 0.0 6.0 6.0 re"),
        "{tile_content}"
    );
    assert!(tile_content.contains("re\nf\n"), "{tile_content}");

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/Pattern cs"), "{content}");
    assert_eq!(content.matches("/P1 scn").count(), 2, "{content}");
    assert!(content.contains("72.0 500.0 120.0 80.0 re"), "{content}");
    assert!(content.contains("72.0 300.0 120.0 80.0 re"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("TILE"));
}

/// 写入 → 重新解析：无色平铺图案应写出 `/PaintType 2` 与 `/ColorSpace`，
/// 图块内容不含颜色操作符；页面内容以 `c1 … cn /P1 scn` 提供填充色，
/// 文本仍可提取。
#[test]
fn builder_uncolored_tiling_pattern_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, PageSize, PdfBuilder, TilingPattern};
    use pdf_engine::core::content::ContentStream;

    let mut tile = ContentStream::new();
    tile.rect(0.0, 0.0, 6.0, 6.0).fill();
    let pattern = TilingPattern::uncolored(
        [0.0, 0.0, 12.0, 12.0],
        12.0,
        12.0,
        ColorSpace::DeviceRGB,
        tile,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "SPOT");
    let handle = builder.tiling_pattern(&pattern);
    builder.fill_rect_with_uncolored_pattern(
        page,
        handle,
        &[0.2, 0.4, 0.6],
        [72.0, 500.0, 120.0, 80.0],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page resources");
    let patterns = resources
        .get("Pattern")
        .and_then(Value::as_dict)
        .expect("page should have Pattern resources");
    let pattern_num = patterns.get("P1").and_then(Value::as_ref).unwrap().0;

    let pattern_object = reparsed
        .get_object(pattern_num)
        .expect("get pattern object")
        .expect("pattern object should exist");
    let Value::Stream(pattern_dict, _) = &pattern_object.value else {
        panic!("tiling pattern should be a stream");
    };
    assert_eq!(
        pattern_dict.get("PaintType").and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        pattern_dict.get("PatternType").and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        pattern_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );

    let tile_content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&pattern_object.value)
            .expect("decode pattern stream"),
    )
    .into_owned();
    assert_eq!(
        tile_content, "0.0 0.0 6.0 6.0 re\nf\n",
        "无色图案的图块不应包含颜色操作符"
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/Pattern cs"), "{content}");
    assert!(content.contains("0.2 0.4 0.6 /P1 scn"), "{content}");
    assert!(content.contains("72.0 500.0 120.0 80.0 re"), "{content}");
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("SPOT"));
}

/// 写入 → 类型化读端模型：构建器写出的轴向渐变、自由形态网格、
/// 着色/无色平铺与着色图案应能经 `page_shading`/`page_pattern` 还原为
/// 坐标、域、Decode、位深、顶点、图案类型与图块内容字节。
#[test]
fn reader_shading_pattern_model_roundtrips_builder_output() {
    use pdf_engine::core::builder::{
        AxialShading, ColorSpace, ExponentialFunction, Font, FreeFormMeshShading, Function,
        MeshVertex, PageSize, PdfBuilder, RadialShading, Shading, ShadingPattern, TilingPattern,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::color_space::ColorSpaceModel;
    use pdf_engine::core::reader::shading::{PaintType, PatternModel, ShadingModel, TilingType};

    let exponential = |c0: Vec<f64>, c1: Vec<f64>| {
        Function::Exponential(
            ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                .with_c0(c0)
                .with_c1(c1),
        )
    };
    let axial = Shading::Axial(
        AxialShading::new(
            ColorSpace::DeviceGray,
            [0.0, 0.0, 200.0, 0.0],
            exponential(vec![0.0], vec![1.0]),
        )
        .with_domain([0.2, 0.8])
        .with_extend([true, false]),
    );
    let mesh = Shading::FreeForm(
        FreeFormMeshShading::new(
            ColorSpace::DeviceRGB,
            vec![
                MeshVertex::new(0.0, 0.0, vec![0.0, 0.0, 0.0]),
                MeshVertex::new(100.0, 0.0, vec![1.0, 0.0, 0.0]),
                MeshVertex::new(0.0, 100.0, vec![0.0, 1.0, 0.0]),
            ],
        )
        .with_decode([0.0, 100.0, 0.0, 100.0]),
    );
    let pattern = ShadingPattern::new(Shading::Radial(
        RadialShading::new(
            ColorSpace::DeviceRGB,
            [50.0, 50.0, 0.0, 50.0, 50.0, 80.0],
            exponential(vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0]),
        )
        .with_extend([true, true]),
    ))
    .with_matrix([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);

    let mut colored_tile = ContentStream::new();
    colored_tile
        .set_fill_rgb(0.2, 0.4, 0.6)
        .rect(0.0, 0.0, 6.0, 6.0)
        .fill();
    let colored = TilingPattern::new([0.0, 0.0, 12.0, 12.0], 12.0, 12.0, colored_tile);
    let mut spot_tile = ContentStream::new();
    spot_tile.rect(0.0, 0.0, 6.0, 6.0).fill();
    let uncolored = TilingPattern::uncolored(
        [0.0, 0.0, 12.0, 12.0],
        12.0,
        12.0,
        ColorSpace::DeviceRGB,
        spot_tile,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "D5");
    let axial_handle = builder.shading(&axial);
    builder.paint_shading(page, axial_handle);
    let mesh_handle = builder.shading(&mesh);
    builder.paint_shading(page, mesh_handle);
    let shading_pattern_handle = builder.shading_pattern(&pattern);
    builder.fill_rect_with_pattern(page, shading_pattern_handle, [72.0, 500.0, 120.0, 80.0]);
    let colored_handle = builder.tiling_pattern(&colored);
    builder.fill_rect_with_pattern(page, colored_handle, [72.0, 400.0, 120.0, 80.0]);
    let uncolored_handle = builder.tiling_pattern(&uncolored);
    builder.fill_rect_with_uncolored_pattern(
        page,
        uncolored_handle,
        &[0.2, 0.4, 0.6],
        [72.0, 300.0, 120.0, 80.0],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let axial_model = reparsed
        .page_shading(0, "Sh1")
        .expect("page_shading Sh1")
        .expect("Sh1 exists");
    let ShadingModel::Axial(a) = axial_model else {
        panic!("expected axial shading, got {axial_model:?}");
    };
    assert_eq!(a.coords, [0.0, 0.0, 200.0, 0.0]);
    assert_eq!(a.domain, vec![(0.2, 0.8)]);
    assert_eq!(a.extend, [true, false]);
    assert_eq!(a.color_space, ColorSpaceModel::DeviceGray);

    let mesh_model = reparsed
        .page_shading(0, "Sh2")
        .expect("page_shading Sh2")
        .expect("Sh2 exists");
    let ShadingModel::FreeFormMesh(m) = mesh_model else {
        panic!("expected free form mesh, got {mesh_model:?}");
    };
    assert_eq!(m.bits_per_coordinate, 16);
    assert_eq!(m.bits_per_components, 8);
    assert_eq!(m.bits_per_flag, 8);
    assert_eq!(
        m.decode,
        vec![
            (0.0, 100.0),
            (0.0, 100.0),
            (0.0, 1.0),
            (0.0, 1.0),
            (0.0, 1.0)
        ]
    );
    assert_eq!(m.vertices.len(), 3);
    assert_eq!(m.vertices[0].flag, 0);
    assert_eq!(m.vertices[1].x, 100.0);
    assert_eq!(m.vertices[1].components, vec![1.0, 0.0, 0.0]);
    assert_eq!(m.vertices[2].y, 100.0);
    assert_eq!(m.vertices[2].components, vec![0.0, 1.0, 0.0]);

    let shading_pattern = reparsed
        .page_pattern(0, "P1")
        .expect("page_pattern P1")
        .expect("P1 exists");
    let PatternModel::Shading(sp) = shading_pattern else {
        panic!("expected shading pattern, got {shading_pattern:?}");
    };
    assert_eq!(sp.matrix, Some([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]));
    let ShadingModel::Radial(r) = sp.shading else {
        panic!("expected radial shading inside pattern");
    };
    assert_eq!(r.coords, [50.0, 50.0, 0.0, 50.0, 50.0, 80.0]);
    assert_eq!(r.domain, vec![(0.0, 1.0)]);
    assert_eq!(r.extend, [true, true]);
    assert_eq!(r.color_space, ColorSpaceModel::DeviceRGB);

    let colored_pattern = reparsed
        .page_pattern(0, "P2")
        .expect("page_pattern P2")
        .expect("P2 exists");
    let PatternModel::Tiling(t) = colored_pattern else {
        panic!("expected tiling pattern, got {colored_pattern:?}");
    };
    assert_eq!(t.paint_type, PaintType::Colored);
    assert_eq!(t.tiling_type, TilingType::ConstantSpacing);
    assert_eq!(t.bbox, [0.0, 0.0, 12.0, 12.0]);
    assert_eq!((t.x_step, t.y_step), (12.0, 12.0));
    assert!(t.matrix.is_none());
    assert!(t.color_space.is_none());
    let tile_text = String::from_utf8(t.content).expect("tile content is utf-8");
    assert!(
        tile_text.contains("0.2 0.4 0.6"),
        "着色图块应保留颜色操作符: {tile_text}"
    );

    let uncolored_pattern = reparsed
        .page_pattern(0, "P3")
        .expect("page_pattern P3")
        .expect("P3 exists");
    let PatternModel::Tiling(t) = uncolored_pattern else {
        panic!("expected tiling pattern, got {uncolored_pattern:?}");
    };
    assert_eq!(t.paint_type, PaintType::Uncolored);
    assert_eq!(
        t.color_space,
        Some(ColorSpaceModel::DeviceRGB),
        "无色平铺应带回基色空间"
    );
    let spot_text = String::from_utf8(t.content).expect("spot tile is utf-8");
    assert!(
        !spot_text.contains("0.2 0.4 0.6"),
        "无色图块不应含颜色操作符: {spot_text}"
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("D5"));
}

/// 写入 → 重新解析：Form XObject 应写出 `/Subtype /Form` 字典与内容流，
/// 页面资源用 `/Fm1` 引用并由 `Do` 绘制；表单未写 `/Resources` 时继承
/// 页面资源，内部文本仍可提取。
#[test]
fn builder_form_xobject_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FormXObject, PageSize, PdfBuilder};
    use pdf_engine::core::content::ContentStream;

    let mut inner = ContentStream::new();
    inner
        .save_state()
        .set_fill_rgb(0.2, 0.4, 0.6)
        .rect(0.0, 0.0, 100.0, 50.0)
        .fill()
        .restore_state()
        .begin_text()
        .set_font("F1", 12.0)
        .move_next_line(10.0, 20.0)
        .show_text("Inner")
        .end_text();
    let form = FormXObject::new([0.0, 0.0, 100.0, 50.0], inner)
        .with_matrix([1.0, 0.0, 0.0, 1.0, 5.0, 5.0]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "FORM");
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [2.0, 0.0, 0.0, 2.0, 72.0, 600.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let form_object = reparsed
        .get_object(xobjects.get("Fm1").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("form object should exist");
    let Value::Stream(dict, _) = &form_object.value else {
        panic!("form should be a stream");
    };
    assert_eq!(dict.get("Subtype").and_then(Value::as_name), Some("Form"));
    assert_eq!(dict.get("FormType").and_then(Value::as_integer), Some(1));
    let bbox = dict
        .get("BBox")
        .and_then(Value::as_array)
        .expect("BBox should be an array");
    let bbox_values: Vec<i64> = bbox.0.iter().filter_map(Value::as_integer).collect();
    assert_eq!(bbox_values, vec![0, 0, 100, 50]);
    let matrix = dict
        .get("Matrix")
        .and_then(Value::as_array)
        .expect("Matrix should be an array");
    let matrix_values: Vec<i64> = matrix.0.iter().filter_map(Value::as_integer).collect();
    assert_eq!(matrix_values, vec![1, 0, 0, 1, 5, 5]);
    assert!(
        dict.get("Resources").is_none(),
        "表单未写 /Resources，应继承页面资源"
    );

    let stream_bytes = reparsed
        .decode_stream_bytes(&form_object.value)
        .expect("decode form stream");
    let stream_text = String::from_utf8_lossy(&stream_bytes);
    assert!(
        stream_text.contains("0.0 0.0 100.0 50.0 re"),
        "{stream_text}"
    );
    assert!(stream_text.contains("/F1 12.0 Tf"), "{stream_text}");
    assert!(stream_text.contains("(Inner) Tj"), "{stream_text}");

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    assert!(content_text.contains("/Fm1 Do"), "{content_text}");
    assert!(
        content_text.contains("2.0 0.0 0.0 2.0 72.0 600.0 cm"),
        "{content_text}"
    );

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("FORM"), "{extracted}");
    assert!(
        extracted.contains("Inner"),
        "表单内文本应经 Do 提取: {extracted}"
    );
}

/// 写入 → 重新解析：标记内容操作符（`BMC`/`BDC`/`EMC`/`MP`/`DP`）应写入
/// 表单内容流，属性名与内联字典原样保留，标记范围内的文本仍可提取。
#[test]
fn builder_marked_content_operators_roundtrip_through_parser() {
    use pdf_engine::core::builder::{Font, FormXObject, PageSize, PdfBuilder};
    use pdf_engine::core::content::{ContentStream, MarkedContentProperties};
    use pdf_engine::core::types::{Dict, Value};

    let properties = Dict::new()
        .insert("Lang", Value::String("en".into()))
        .insert("MCID", Value::Integer(0));
    let mut inner = ContentStream::new();
    inner
        .begin_marked_content("P")
        .begin_marked_content_properties("Span", MarkedContentProperties::Dict(properties))
        .begin_text()
        .set_font("F1", 12.0)
        .move_next_line(10.0, 20.0)
        .show_text("Marked")
        .end_text()
        .end_marked_content()
        .end_marked_content()
        .begin_marked_content_properties("OC", MarkedContentProperties::Name("Layer1".into()))
        .marked_content_point_properties("Artifact", MarkedContentProperties::Name("Props1".into()))
        .end_marked_content();
    let form = FormXObject::new([0.0, 0.0, 100.0, 50.0], inner);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "OUTER");
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [1.0, 0.0, 0.0, 1.0, 72.0, 600.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let form_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Subtype"))
                .and_then(Value::as_name)
                == Some("Form")
        })
        .expect("should find form object");
    let stream_text = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&form_object.value)
            .expect("decode form stream"),
    )
    .into_owned();
    for expected in [
        "/P BMC\n",
        "/Span << /Lang (en) /MCID 0 >> BDC\n",
        "EMC\n",
        "/OC /Layer1 BDC\n",
        "/Artifact /Props1 DP\n",
        "(Marked) Tj",
    ] {
        assert!(
            stream_text.contains(expected),
            "missing {expected:?} in form stream: {stream_text}"
        );
    }

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("OUTER"), "{extracted}");
    assert!(
        extracted.contains("Marked"),
        "标记内容范围内的文本应可提取: {extracted}"
    );
}

/// 写入 → 重新解析：可选内容组应写出 `/OCG`、页资源 `/Properties` 与
/// 目录 `/OCProperties`；`/OC name BDC ... EMC` 标记范围内，默认可见组
/// 的文本仍可提取、默认隐藏组的文本被过滤，组名称、`/Intent` 与
/// 默认可见性可经读端类型化还原。
#[test]
fn builder_optional_content_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, OptionalContentGroup, PageSize, PdfBuilder};
    use pdf_engine::core::reader::optional_content::BaseState;
    use pdf_engine::core::types::Value;

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let visible = builder.optional_content_group(&OptionalContentGroup::new("Visible Layer"));
    let hidden = builder.optional_content_group(
        &OptionalContentGroup::new("Hidden Layer")
            .with_visibility(false)
            .with_intent("View"),
    );
    builder.begin_optional_content(page, visible);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "VISIBLE");
    builder.end_optional_content(page);
    builder.begin_optional_content(page, hidden);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "HIDDEN");
    builder.end_optional_content(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "PLAIN");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let properties = resources
        .get("Properties")
        .and_then(Value::as_dict)
        .expect("page /Properties");
    let visible_object = properties
        .get("OC1")
        .and_then(Value::as_ref)
        .expect("OC1 reference")
        .0;
    let hidden_object = properties
        .get("OC2")
        .and_then(Value::as_ref)
        .expect("OC2 reference")
        .0;
    assert_ne!(visible_object, hidden_object);

    let parsed = reparsed
        .optional_content_properties()
        .expect("parse /OCProperties")
        .expect("expected /OCProperties");
    assert_eq!(parsed.groups.len(), 2);
    assert_eq!(parsed.groups[0].object, visible_object);
    assert_eq!(parsed.groups[0].name, "Visible Layer");
    assert!(parsed.groups[0].intent.is_empty());
    assert_eq!(parsed.groups[1].object, hidden_object);
    assert_eq!(parsed.groups[1].name, "Hidden Layer");
    assert_eq!(parsed.groups[1].intent, ["View"]);
    assert_eq!(parsed.is_visible(visible_object), Some(true));
    assert_eq!(parsed.is_visible(hidden_object), Some(false));
    let configuration = parsed.default.as_ref().expect("default configuration");
    assert_eq!(configuration.base_state, BaseState::On);
    assert!(configuration.on.contains(&visible_object));
    assert!(configuration.off.contains(&hidden_object));
    assert_eq!(configuration.order, [visible_object, hidden_object]);

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("/Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/OC /OC1 BDC\n"), "{content}");
    assert!(content.contains("/OC /OC2 BDC\n"), "{content}");
    assert_eq!(content.matches("EMC\n").count(), 2, "{content}");

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    for text in ["VISIBLE", "PLAIN"] {
        assert!(extracted.contains(text), "{text} missing: {extracted}");
    }
    assert!(
        !extracted.contains("HIDDEN"),
        "hidden layer should be filtered: {extracted}"
    );

    // C4：页标记内容还原出两个 /OC 区段，可见性分别跟随默认配置。
    let sections = reparsed.page_marked_content(0).expect("marked sections");
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].tag, "OC");
    assert_eq!(sections[0].visibility, Some(true));
    assert_eq!(sections[1].tag, "OC");
    assert_eq!(sections[1].visibility, Some(false));
}

/// 写入 → 重新解析：ExtGState `/TR` 应写出 Type 2 直接字典、Type 0 流对象
/// 与 Type 3 拼接函数（内联子函数），读端可再次构造并求值，文本提取不受影响。
#[test]
fn builder_transfer_functions_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ExponentialFunction, Font, Function, GraphicsState, PageSize, PdfBuilder, SampledFunction,
        StitchingFunction,
    };
    use pdf_engine::core::functions::Function as ReaderFunction;

    // Type 2：y = x²。
    let exponential = Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 2.0)
            .with_c0(vec![0.0])
            .with_c1(vec![1.0])
            .with_range(vec![0.0, 1.0]),
    );
    // Type 0：5 个 8 位样本，线性插值。
    let sampled = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0],
        vec![0.0, 1.0],
        vec![5],
        8,
        vec![0.0, 64.0, 128.0, 191.0, 255.0],
    ));
    // Type 3：前半段恒等，后半段放大 2 倍。
    let stitching = Function::Stitching(
        StitchingFunction::new(
            vec![0.0, 1.0],
            vec![
                Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
                Function::Exponential(
                    ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                        .with_c0(vec![0.0])
                        .with_c1(vec![2.0])
                        .with_range(vec![0.0, 2.0]),
                ),
            ],
            vec![0.5],
        )
        .with_encode(vec![0.0, 1.0, 0.0, 1.0]),
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    for function in [&exponential, &sampled, &stitching] {
        let handle = builder.function(function);
        builder.set_graphics_state(page, &GraphicsState::new().with_transfer_function(handle));
    }
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "TR");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let states = reparsed
        .ext_g_states(resources)
        .expect("parse /ExtGState resources");

    let exp = ReaderFunction::from_value(
        states
            .get("GS1")
            .expect("GS1")
            .transfer
            .as_ref()
            .expect("/TR"),
        None,
    )
    .expect("parse type 2 /TR");
    assert_eq!(exp.function_type(), 2);
    assert!((exp.eval(&[0.5]).expect("eval")[0] - 0.25).abs() < 1e-9);

    let transfer = states
        .get("GS2")
        .expect("GS2")
        .transfer
        .as_ref()
        .expect("/TR");
    assert!(
        matches!(transfer, Value::Stream(_, _)),
        "Type 0 /TR should resolve to a stream, got {transfer:?}"
    );
    let dict = transfer.as_dict().expect("function stream dict");
    let samples = reparsed
        .decode_stream_bytes(transfer)
        .expect("decode sample stream");
    let sampled_reader = ReaderFunction::from_dict(dict, Some(&samples)).expect("parse type 0 /TR");
    assert_eq!(sampled_reader.function_type(), 0);
    let interpolated = sampled_reader.eval(&[0.5]).expect("eval")[0];
    assert!(
        (interpolated - 128.0 / 255.0).abs() < 1e-9,
        "unexpected interpolation: {interpolated}"
    );
    assert_eq!(sampled_reader.eval(&[0.0]).expect("eval"), vec![0.0]);
    assert_eq!(sampled_reader.eval(&[1.0]).expect("eval"), vec![1.0]);

    let stitch = ReaderFunction::from_value(
        states
            .get("GS3")
            .expect("GS3")
            .transfer
            .as_ref()
            .expect("/TR"),
        None,
    )
    .expect("parse type 3 /TR");
    assert_eq!(stitch.function_type(), 3);
    // 段内输入先归一到 [0,1]：0.25 → 0.5（恒等），0.75 → 0.5 → 1.0（放大 2 倍）。
    assert!((stitch.eval(&[0.25]).expect("eval")[0] - 0.5).abs() < 1e-9);
    assert!((stitch.eval(&[0.75]).expect("eval")[0] - 1.0).abs() < 1e-9);

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("TR"), "{extracted}");
}

/// 写入 → 重新解析：ExtGState 的 `/BG`/`/BG2`/`/UCR`/`/UCR2` 应引用
/// 同一个四输入单输出采样函数流，读端把四个条目保留为同一原始值。
#[test]
fn builder_black_generation_functions_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        Font, Function, GraphicsState, PageSize, PdfBuilder, SampledFunction,
    };

    let function = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        vec![0.0, 1.0],
        vec![2, 2, 2, 2],
        8,
        (0..16)
            .map(|value| value as f64 * 17.0)
            .collect::<Vec<f64>>(),
    ));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let handle = builder.function(&function);
    builder.set_graphics_state(
        page,
        &GraphicsState::new()
            .with_black_generation(handle)
            .with_black_generation_2(handle)
            .with_undercolor_removal(handle)
            .with_undercolor_removal_2(handle),
    );
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "BG");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let states = reparsed
        .ext_g_states(resources)
        .expect("parse /ExtGState resources");
    let state = states.get("GS1").expect("GS1");

    let bg = state.black_generation.as_ref().expect("/BG");
    for (name, value) in [
        ("/BG2", state.black_generation2.as_ref()),
        ("/UCR", state.under_color_removal.as_ref()),
        ("/UCR2", state.under_color_removal2.as_ref()),
    ] {
        assert_eq!(
            value.expect(name),
            bg,
            "{name} should reference the same function"
        );
    }
    let dict = bg.as_dict().expect("function stream dictionary");
    assert_eq!(
        dict.get("FunctionType").and_then(Value::as_integer),
        Some(0)
    );
    assert_eq!(
        dict.get("Size")
            .and_then(Value::as_array)
            .map(|array| array.0.len()),
        Some(4)
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(bg)
            .expect("decode samples")
            .len(),
        16
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("BG"));
}

/// 写入 → 重新解析：Group XObject 的 `/Group` 字典应写出
/// `/Type /Group`、`/S /Transparency`、组颜色空间与 `/I`，表单仍可绘制
/// 并经 `Do` 提取内部文本。
#[test]
fn builder_group_xobject_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, FormXObject, GroupXObject, PageSize, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;

    let mut inner = ContentStream::new();
    inner
        .begin_text()
        .set_font("F1", 12.0)
        .move_next_line(10.0, 20.0)
        .show_text("Group")
        .end_text();
    let form = FormXObject::new([0.0, 0.0, 100.0, 50.0], inner).with_group(
        GroupXObject::transparency()
            .with_color_space(ColorSpace::DeviceRGB)
            .with_isolated(true),
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "GROUP");
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [1.0, 0.0, 0.0, 1.0, 72.0, 600.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let form_object = reparsed
        .get_object(xobjects.get("Fm1").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("form object should exist");
    let Value::Stream(dict, _) = &form_object.value else {
        panic!("form should be a stream");
    };
    let group = dict
        .get("Group")
        .and_then(Value::as_dict)
        .expect("form should carry a Group dictionary");
    assert_eq!(group.get("Type").and_then(Value::as_name), Some("Group"));
    assert_eq!(
        group.get("S").and_then(Value::as_name),
        Some("Transparency")
    );
    assert_eq!(group.get("CS").and_then(Value::as_name), Some("DeviceRGB"));
    assert!(
        matches!(group.get("I"), Some(Value::Bool(true))),
        "isolated 组应写 /I true"
    );
    assert!(group.get("K").is_none(), "未设置 knockout 时不应写 /K");

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("GROUP"), "{extracted}");
    assert!(
        extracted.contains("Group"),
        "组内文本应经 Do 提取: {extracted}"
    );
}

/// 写入 → 重新解析：页字典应携带 `/Group` 透明组字典，页面文本仍可提取。
#[test]
fn builder_page_group_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, GroupXObject, PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "PAGE");
    builder.set_page_group(
        page,
        &GroupXObject::transparency()
            .with_color_space(ColorSpace::DeviceRGB)
            .with_isolated(true)
            .with_knockout(true),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let group = page_dict
        .get("Group")
        .and_then(Value::as_dict)
        .expect("page should carry a Group dictionary");
    assert_eq!(group.get("Type").and_then(Value::as_name), Some("Group"));
    assert_eq!(
        group.get("S").and_then(Value::as_name),
        Some("Transparency")
    );
    assert_eq!(group.get("CS").and_then(Value::as_name), Some("DeviceRGB"));
    assert!(
        matches!(group.get("I"), Some(Value::Bool(true))),
        "isolated 页组应写 /I true"
    );
    assert!(
        matches!(group.get("K"), Some(Value::Bool(true))),
        "knockout 页组应写 /K true"
    );

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("PAGE"), "{extracted}");
}

/// 写入 → 重新解析：ExtGState `/SMask` 应被读端解析为类型化软蒙版，
/// `/G` 指向带 `/Group` 的 Form 流且不登记为页面 XObject 资源；
/// 页面文本仍可提取。
#[test]
fn builder_soft_mask_graphics_state_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, FormXObject, GraphicsState, GroupXObject, PageSize, PdfBuilder, SoftMask,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::graphics_state::SoftMaskSubtype;

    let mut mask_content = ContentStream::new();
    mask_content
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 50.0, 50.0)
        .fill();
    let mask_form = FormXObject::new([0.0, 0.0, 50.0, 50.0], mask_content)
        .with_group(GroupXObject::transparency().with_color_space(ColorSpace::DeviceGray));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let handle = builder.form_xobject(&mask_form);
    builder.set_graphics_state(
        page,
        &GraphicsState::new()
            .with_fill_alpha(0.5)
            .with_soft_mask(SoftMask::luminosity(handle).with_backdrop([0.0, 0.5, 1.0])),
    );
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Mask");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let states = reparsed
        .ext_g_states(resources)
        .expect("parse /ExtGState resources");
    let gs = states.get("GS1").expect("GS1 graphics state");
    let raw_mask = gs.soft_mask.as_ref().expect("GS1 should carry /SMask");
    let mask = reparsed.soft_mask(raw_mask).expect("typed soft mask");

    assert_eq!(mask.subtype, SoftMaskSubtype::Luminosity);
    assert_eq!(mask.backdrop, Some(vec![0.0, 0.5, 1.0]));
    let Value::Ref(group_num, _) = &mask.group else {
        panic!("/G should be an indirect reference");
    };
    let group_object = reparsed
        .get_object(*group_num)
        .expect("group lookup")
        .expect("group object");
    let Value::Stream(group_dict, _) = &group_object.value else {
        panic!("/G should reference a stream");
    };
    assert_eq!(
        group_dict.get("Subtype").and_then(Value::as_name),
        Some("Form")
    );
    assert_eq!(
        group_dict
            .get("Group")
            .and_then(Value::as_dict)
            .and_then(|group| group.get("S"))
            .and_then(Value::as_name),
        Some("Transparency")
    );
    assert!(
        !reparsed
            .decode_stream_bytes(&group_object.value)
            .expect("decode group stream")
            .is_empty(),
        "软蒙版表单应有内容流"
    );

    // 软蒙版表单不是页面绘制资源；页面只经 /GS1 gs 应用透明度。
    let page_xobjects = resources
        .get("XObject")
        .and_then(Value::as_dict)
        .map(|xobjects| xobjects.0.len())
        .unwrap_or(0);
    assert_eq!(page_xobjects, 0, "软蒙版表单不应登记为页面 XObject");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Mask"));
}

/// 写入 → 重新解析： 子集字体应写出 `TAG+Name` 形式的 BaseFont，
/// 读端剥离标签后仍能解析嵌入程序并提取文本。
#[test]
fn subset_font_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let sfnt = sample_sfnt_font();
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(sfnt.clone()))
        .subset();

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "TrueType");
    let base_font = font_dict
        .as_dict()
        .and_then(|dict| dict.get("BaseFont"))
        .and_then(Value::as_name)
        .expect("BaseFont");
    let (tag, name) = base_font.split_once('+').expect("应含子集标签前缀");
    assert_eq!(tag.len(), 6, "标签应为六个字母: {tag}");
    assert!(
        tag.bytes().all(|byte| byte.is_ascii_uppercase()),
        "标签应为大写 ASCII 字母: {tag}"
    );
    assert_eq!(name, "Helvetica");
    assert_eq!(strip_subset_tag(base_font), "Helvetica");

    let parsed = reparsed
        .truetype_font(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert_eq!(parsed.glyph_id_for_unicode(0x41).unwrap(), Some(2));
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Test"));
}

/// 写入 → 重新解析： `with_subset_glyphs` 应把嵌入的 TrueType 程序重写为
/// 只含请求字形的子集（字形数减少、cmap 重编号），/BaseFont 仍带确定性
/// 六字母标签，且页面文本仍可提取。
#[test]
fn builder_truetype_subset_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let sfnt = sample_sfnt_font();
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(sfnt.clone()))
        .with_subset_glyphs(&[2]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "TrueType");
    let parsed = reparsed
        .truetype_font(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert_eq!(parsed.glyph_count(), 2, "子集应只保留 gid 0 与 2");
    assert_eq!(
        parsed.glyph_id_for_unicode(0x41).expect("cmap lookup"),
        Some(1),
        "U+0041 应在子集中重编号为 gid 1"
    );

    let embedded = reparsed
        .truetype_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_ne!(embedded.data, sfnt, "嵌入程序应为重写后的子集");
    assert!(
        embedded.data.len() < sfnt.len(),
        "子集程序应短于原始程序: {} >= {}",
        embedded.data.len(),
        sfnt.len()
    );

    let base_font = font_dict
        .as_dict()
        .and_then(|dict| dict.get("BaseFont"))
        .and_then(Value::as_name)
        .expect("BaseFont");
    let (tag, name) = base_font.split_once('+').expect("应含子集标签前缀");
    assert_eq!(tag.len(), 6, "标签应为六个字母: {tag}");
    assert!(
        tag.bytes().all(|byte| byte.is_ascii_uppercase()),
        "标签应为大写 ASCII 字母: {tag}"
    );
    assert_eq!(name, "Helvetica");
    assert_eq!(strip_subset_tag(base_font), "Helvetica");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Test"));
}

/// 写入 → 重新解析： builder 嵌入的 CFF 程序应作为 `/FontFile3`
/// 保存，并带 `/Subtype /Type1C`。
#[test]
fn builder_embeds_cff_font_program_roundtrip() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let cff = vec![0x01, 0x00, 0x04, 0x05];
    let font = Font::helvetica(12.0).with_font_program(FontProgram::CFF(cff.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let embedded = reparsed
        .type1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("Type1C"));
    assert_eq!(embedded.data, cff);
}

/// 写入 → 重新解析：简单字体的 `/FontFile3` `/Subtype /Type1C` 应可解析为
/// name-keyed CFF，按 charset/String INDEX 还原字形名并给出轮廓。
#[test]
fn builder_embeds_name_keyed_cff_and_reader_resolves_glyph_names() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let cff = sample_name_keyed_cff_program();
    let font = Font::helvetica(12.0).with_font_program(FontProgram::CFF(cff));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "A");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let program = reparsed
        .type1c_program(&font_dict)
        .expect("should parse Type1C")
        .expect("should have Type1C program");
    assert!(!program.is_cid_keyed());
    assert_eq!(program.glyph_count(), 2);
    assert_eq!(program.glyph_name(0), Some(".notdef"));
    assert_eq!(program.glyph_name(1), Some("Alpha"));
    assert_eq!(program.glyph_for_name("Alpha"), Some(1));
    assert_eq!(program.glyph_for_name("missing"), None);
    assert_eq!(program.units_per_em(), 1000);
    assert_eq!(program.glyph_width(1), Some(600.0));

    let outline = program.outline(1).expect("glyph outline");
    assert_eq!(outline.width, 600.0);
    assert_eq!(
        outline.segments,
        vec![
            CffPathSegment::MoveTo { x: 100.0, y: 0.0 },
            CffPathSegment::LineTo { x: 300.0, y: 0.0 },
            CffPathSegment::ClosePath,
        ]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("A"));
}

/// 写入 → 重新解析： CIDFont 的系统信息、宽度与描述符（含 `/CIDSet`）
/// 应可从生成的文档还原。
#[test]
fn builder_cid_font_descriptor_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, CidWidth, FontDescriptor, PageSize, PdfBuilder,
    };

    let descriptor = FontDescriptor::new()
        .with_font_name("ExampleCIDFont")
        .with_family("Example Family")
        .with_ascent(800.0)
        .with_descent(-200.0)
        .with_flags(4)
        .with_cid_set(vec![0xA0, 0x01]);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        descriptor,
        12.0,
    )
    .with_widths(vec![
        CidWidth::Single(2, 500.0),
        CidWidth::Range(3, 5, 600.0),
    ]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[2, 3, 4]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    assert_eq!(parsed.system_info().registry, "Adobe");
    assert_eq!(parsed.system_info().ordering, "Identity");
    assert_eq!(parsed.width_for_cid(2), 500.0);
    assert_eq!(parsed.width_for_cid(3), 600.0);
    assert_eq!(parsed.width_for_cid(6), 1000.0);
    let metrics = parsed.descriptor();
    assert_eq!(metrics.ascent, Some(800.0));
    assert_eq!(metrics.descent, Some(-200.0));
    assert_eq!(metrics.cid_is_present(0), Some(true));
    assert_eq!(metrics.cid_is_present(1), Some(false));
    assert_eq!(metrics.cid_is_present(2), Some(true));
    assert_eq!(metrics.cid_is_present(15), Some(true));
}

/// 写入 → 重新解析：CIDFont 的 `/DW2` 与 `/W2` 垂直度量应能还原，
/// 未列出的 CID 按 `/DW2` 与水平宽度的一半回退。
#[test]
fn builder_cid_font_vertical_metrics_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, CidVerticalMetrics, CidWidth, FontDescriptor,
        PageSize, PdfBuilder,
    };
    use pdf_engine::core::reader::font::{DefaultVerticalMetrics, VerticalMetrics};

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_widths(vec![CidWidth::Single(3, 600.0)])
    .with_default_vertical_metrics(900.0, -800.0)
    .with_vertical_metrics(vec![
        CidVerticalMetrics::Single(1, -500.0, 250.0, 700.0),
        CidVerticalMetrics::Range(5, 6, -600.0, 300.0, 800.0),
    ]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[1]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    assert_eq!(
        parsed.default_vertical_metrics(),
        DefaultVerticalMetrics {
            v_y: 900.0,
            w1_y: -800.0
        }
    );
    assert_eq!(
        parsed.vertical_metrics_for_cid(1),
        VerticalMetrics {
            w1_y: -500.0,
            v1x: 250.0,
            v1y: 700.0
        }
    );
    assert_eq!(
        parsed.vertical_metrics_for_cid(6),
        VerticalMetrics {
            w1_y: -600.0,
            v1x: 300.0,
            v1y: 800.0
        }
    );
    // CID 3 无 /W2：v_x 取水平宽度的一半，其余取 /DW2。
    assert_eq!(
        parsed.vertical_metrics_for_cid(3),
        VerticalMetrics {
            w1_y: -800.0,
            v1x: 300.0,
            v1y: 900.0
        }
    );
}

/// 写入 → 重新解析：CIDFont 描述符的 `/Style /Panose`、`/Lang` 与 `/FD`
/// 字形类覆盖应原样保留。
#[test]
fn builder_cid_font_descriptor_style_lang_fd_roundtrip() {
    use std::collections::BTreeMap;

    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "Proportional".to_string(),
        FontDescriptor::new()
            .with_ascent(750.0)
            .with_descent(-250.0),
    );
    let descriptor = FontDescriptor::new()
        .with_style_panose([
            0x02, 0x0B, 0x05, 0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ])
        .with_lang("zh-CN")
        .with_fd(overrides);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        descriptor,
        12.0,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[1]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    let descriptor = parsed.descriptor();
    assert_eq!(
        descriptor.style_panose,
        Some([
            0x02, 0x0B, 0x05, 0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ])
    );
    assert_eq!(descriptor.lang.as_deref(), Some("zh-CN"));
    let fd = descriptor.fd.as_ref().expect("/FD");
    let proportional = fd.get("Proportional").expect("Proportional 覆盖");
    assert_eq!(proportional.get("Ascent"), Some(&Value::Float(750.0)));
    assert_eq!(proportional.get("Descent"), Some(&Value::Float(-250.0)));
}

/// 写入 → 重新解析： CIDFontType2 嵌入的 TrueType 程序应作为 `/FontFile2`
/// 保存，读端可还原原始字节并继续解析 CIDFont。
#[test]
fn builder_cid_font_type2_embeds_truetype_program_roundtrip() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };

    let sfnt = sample_sfnt_font();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(sfnt.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[2]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let embedded = reparsed
        .cid_font_type2_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_eq!(embedded.data, sfnt);

    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    assert_eq!(parsed.system_info().registry, "Adobe");
    assert_eq!(parsed.system_info().ordering, "Identity");
}

/// 写入 → 重新解析：CIDFontType2 的 TrueType 程序按 CID 子集化后，
/// `/CIDToGIDMap` 应按新 gid 重建，读端 CID→glyph 解析与文本提取保持一致。
#[test]
fn builder_cid_font_type2_subset_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };
    use pdf_engine::core::reader::font::TrueTypeFont;

    let sfnt = sample_sfnt_font();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(sfnt.clone()))
    .with_cid_to_gid_map(&[0, 1, 2])
    .with_subset_cids(&[2]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(page, index, 72.0, 700.0, &[(2, "A")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let base_font = descendant
        .as_dict()
        .and_then(|dict| dict.get("BaseFont"))
        .and_then(Value::as_name)
        .expect("BaseFont");
    assert!(
        base_font.contains('+'),
        "子集 BaseFont 应带标签前缀: {base_font}"
    );

    let embedded = reparsed
        .cid_font_type2_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_ne!(embedded.data, sfnt, "嵌入程序应为子集");
    assert!(embedded.data.len() < sfnt.len(), "子集程序应更短");
    let subset = TrueTypeFont::parse(embedded.data).expect("子集程序应可解析");
    assert_eq!(subset.glyph_count(), 2, "应只保留 glyph 0 与原 gid 2");

    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    assert_eq!(parsed.glyph_id_for_cid(2), Some(1), "CID 2 应指向新 gid 1");
    assert_eq!(parsed.glyph_id_for_cid(0), Some(0));
    assert_eq!(parsed.glyph_id_for_cid(1), Some(0), "未保留的 CID 应归零");
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("A"));
}

/// 写入 → 重新解析： CIDFontType0 嵌入的 CFF 程序应作为 `/FontFile3`
/// 保存，流子类型为 `/CIDFontType0C`。
#[test]
fn builder_cid_font_type0_embeds_cff_program_roundtrip() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };

    let cff = vec![0x01, 0x00, 0x04, 0x05];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(cff.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[2]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType0");
    let embedded = reparsed
        .cid_font_type0_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("CIDFontType0C"));
    assert_eq!(embedded.data, cff);

    let parsed = reparsed
        .cid_font_type0(&descendant)
        .expect("should parse CIDFontType0")
        .expect("should be CIDFontType0");
    let program = parsed.font_program().expect("should keep program");
    assert_eq!(program.subtype.as_deref(), Some("CIDFontType0C"));
    assert_eq!(program.data, cff);
}

/// 写入 → 重新解析 → 提取：CIDFontType0 嵌入 CFF-flavored OpenType
/// 程序时应写 `/FontFile3` `/Subtype /OpenType`，文本经 ToUnicode 还原。
#[test]
fn builder_cid_font_type0_opentype_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };

    let program = sample_opentype_cff_sfnt();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::OpenType(program.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(page, index, 72.0, 700.0, &[(7, "Hi")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType0");
    let embedded = reparsed
        .cid_font_type0_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("OpenType"));
    assert_eq!(embedded.data, program);

    let parsed = reparsed
        .cid_font_type0(&descendant)
        .expect("should parse CIDFontType0")
        .expect("should be CIDFontType0");
    let embedded = parsed.font_program().expect("should keep program");
    assert_eq!(embedded.subtype.as_deref(), Some("OpenType"));
    assert_eq!(embedded.data, program);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Hi"));
}

/// 写入 → 重新解析 → 提取：builder 为 CIDFont 写出的 `/ToUnicode`
/// CMap 应让读端把 CID 还原为 Unicode 文本。
#[test]
fn builder_cid_font_to_unicode_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(
        page,
        index,
        72.0,
        700.0,
        &[(1, "T"), (2, "é"), (3, "测"), (4, "😀")],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04])
        .expect("should decode");
    let text: Vec<&str> = decoded
        .iter()
        .map(|code| code.unicode.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(text, ["T", "é", "测", "😀"]);

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Té测😀"));
}

/// 写入 → 重新解析 → 提取：`/ToUnicode` 的 `Adobe-Identity-UCS` 基座
/// 应让未显式映射的 2 字节字符码按码值还原为 Unicode。
#[test]
fn builder_to_unicode_identity_base_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_to_unicode_usecmap("Adobe-Identity-UCS");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[0x0041, 0x0042]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x00, 0x41, 0x00, 0x42])
        .expect("should decode");
    let text: Vec<&str> = decoded
        .iter()
        .map(|code| code.unicode.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(text, ["A", "B"]);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
}

/// 写入 → 重新解析 → 提取：连续 CID 的 `/ToUnicode` 应以 `bfrange`
/// 写出，读端仍能把 CID 序列还原为文本。
#[test]
fn builder_cid_font_bfrange_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(
        page,
        index,
        72.0,
        700.0,
        &[(1, "A"), (2, "B"), (3, "C"), (4, "D"), (5, "E")],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let (cmap_num, _) = type0
        .as_dict()
        .and_then(|dict| dict.get("ToUnicode"))
        .and_then(Value::as_ref)
        .expect("Type 0 字体应引用 ToUnicode");
    let cmap = reparsed
        .get_object(cmap_num)
        .expect("ToUnicode 对象应可解析")
        .expect("ToUnicode 对象应存在");
    let cmap_bytes = reparsed
        .decode_stream_bytes(&cmap.value)
        .expect("ToUnicode 流应可解码");
    let cmap_text = String::from_utf8_lossy(&cmap_bytes);
    assert!(
        cmap_text.contains("<0001> <0005> <0041>"),
        "连续 CID 应以 bfrange 写出: {cmap_text}"
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("ABCDE"));
}

/// 写入 → 重新解析 → 提取：自定义嵌入 CMap（非 Identity）应把 2 字节
/// 字符码映射为 CID，ToUnicode 以字符码为键，提取器仍还原出原文。
#[test]
fn builder_custom_cid_encoding_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidEncoding, CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let encoding = CidEncoding::new(0)
        .mapping(0x0020, 1)
        .mapping(0x0021, 2)
        .mapping(0x0022, 3);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(
        page,
        index,
        72.0,
        700.0,
        &[(0x0020, "A"), (0x0021, "B"), (0x0022, "C")],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    assert!(
        matches!(
            type0.as_dict().and_then(|dict| dict.get("Encoding")),
            Some(Value::Ref(_, _))
        ),
        "自定义编码应以流引用写出"
    );
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    assert!(!font.is_vertical(), "WMode 0 应为水平书写");
    let decoded = font
        .decode_codes(&[0x00, 0x20, 0x00, 0x21, 0x00, 0x22])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [1, 2, 3],
        "字符码应经嵌入 CMap 映射为 CID"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("A"), Some("B"), Some("C")]
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("ABC"));
}

/// 写入 → 重新解析 → 提取：预定义 CMap（GB1 的 UCS2 变体 UniGB-UCS2-H）
/// 以名称引用作为 `/Encoding` 写出，Unicode 文本按码位写 2 字节字符码，
/// ToUnicode 以字符码为键，提取器仍还原出原文。
#[test]
fn builder_predefined_cmap_encoding_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "GB1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_predefined_encoding("UniGB-UCS2-H");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_predefined_unicode(page, index, 72.0, 700.0, "中文GB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    assert!(
        matches!(
            type0.as_dict().and_then(|dict| dict.get("Encoding")),
            Some(Value::Name(name)) if name == "UniGB-UCS2-H"
        ),
        "预定义 CMap 应以名称引用写出"
    );
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    assert!(!font.is_vertical(), "UniGB-UCS2-H 应为水平书写");
    let decoded = font
        .decode_codes(&[0x4E, 0x2D, 0x65, 0x87, 0x00, 0x47, 0x00, 0x42])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [0x4E2D, 0x6587, 0, 0],
        "≥ 0x0100 的字符码恒等映射为 CID，低位码映射为 CID 0（提取由 ToUnicode 驱动）"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("中"), Some("文"), Some("G"), Some("B")]
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("中文GB"));
}

/// 写入 → 重新解析 → 提取：builder 写出的 Type 3 字体字典应能被读端
/// 解析出矩阵、宽度与字符过程，文本提取仍还原出原文（9.6.5）。
#[test]
fn builder_type3_font_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder, Type3Font, Type3Glyph};

    let type3 = Type3Font::new()
        .glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"))
        .glyph(Type3Glyph::new(66, "B", 600.0, b"600 0 d0"));
    let font = Font::type3(12.0, type3);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "AB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type3");
    let parsed = reparsed
        .type3_font(&font_dict)
        .expect("should parse Type 3 font")
        .expect("should be Type 3");
    assert_eq!(parsed.font_matrix, [0.001, 0.0, 0.0, 0.001, 0.0, 0.0]);
    assert_eq!(parsed.font_bbox, [0.0, 0.0, 1000.0, 1000.0]);
    assert_eq!(parsed.first_char, 65);
    assert_eq!(parsed.widths, vec![500.0, 600.0]);
    assert_eq!(parsed.char_procs, vec!["A".to_string(), "B".to_string()]);
    assert_eq!(parsed.width_for(65), Some(500.0));
    assert_eq!(parsed.width_for(66), Some(600.0));
    assert_eq!(parsed.width_for(64), None);

    // /CharProcs 的每个条目应解析为含原始字符过程字节的流。
    let char_procs = font_dict
        .as_dict()
        .and_then(|dict| dict.get("CharProcs"))
        .and_then(Value::as_dict)
        .expect("/CharProcs");
    for (name, expected) in [("A", b"500 0 d0".as_slice()), ("B", b"600 0 d0".as_slice())] {
        let (proc_num, _) = char_procs
            .get(name)
            .and_then(Value::as_ref)
            .unwrap_or_else(|| panic!("缺少字符过程 {name}"));
        let proc_object = reparsed
            .get_object(proc_num)
            .expect("字符过程查找")
            .expect("字符过程对象");
        let bytes = reparsed
            .decode_stream_bytes(&proc_object.value)
            .expect("字符过程流应可解码");
        assert_eq!(bytes, expected, "{name} 的字符过程字节");
    }

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
}

/// 写入 → 重新解析：builder 写出的 Type 3 字形程序以 d0/d1 开头时，读端
/// 应能按字符码查询其声明的宽度与包围盒，且文本提取不受影响（9.6.5）。
#[test]
fn builder_type3_glyph_metrics_roundtrip_with_text_extraction() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder, Type3Font, Type3Glyph};

    // 手工构造含 d0/d1 前缀的 CharProcs 字节：A 用 d1 声明宽度与包围盒，
    // B 用 d0 仅声明宽度（与 /Widths 中的 600 故意不同，两者独立可查）。
    let type3 = Type3Font::new()
        .glyph(Type3Glyph::new(
            65,
            "A",
            500.0,
            b"500 0 10 20 490 680 d1 10 20 480 660 re f",
        ))
        .glyph(Type3Glyph::new(66, "B", 600.0, b"650 0 d0"));
    let font = Font::type3(12.0, type3);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "AB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type3");
    let parsed = reparsed
        .type3_font(&font_dict)
        .expect("should parse Type 3 font")
        .expect("should be Type 3");

    // d1：声明宽度 + 包围盒（glyph 空间）。
    let metrics_a = parsed
        .glyph_metrics(&reparsed, 65)
        .expect("glyph A metrics")
        .expect("glyph A should have metrics");
    assert_eq!(metrics_a.width, [500.0, 0.0]);
    assert_eq!(metrics_a.bbox, Some([10.0, 20.0, 490.0, 680.0]));

    // d0：仅声明宽度，无包围盒；与 /Widths 的 600 共存且互不干扰。
    let metrics_b = parsed
        .glyph_metrics(&reparsed, 66)
        .expect("glyph B metrics")
        .expect("glyph B should have metrics");
    assert_eq!(metrics_b.width, [650.0, 0.0]);
    assert_eq!(metrics_b.bbox, None);
    assert_eq!(parsed.width_for(66), Some(600.0));

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
}

/// 写入 → 重新解析 → 提取：Type 3 的 `/Name`、`/FontDescriptor`、
/// 字体级 `/Resources` 与 `/ToUnicode` 应完整往返；`/ToUnicode` 优先于
/// `/Encoding` 决定提取出的文本（9.6.5，Table 112）。
#[test]
fn builder_type3_metadata_and_resources_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        Font, FontDescriptor, PageSize, PdfBuilder, Type3Font, Type3Glyph,
    };

    let resources = Dict::new().insert(
        "XObject",
        Value::Dict(Dict::new().insert("Im1", Value::Integer(1))),
    );
    let type3 = Type3Font::new()
        .glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"))
        .with_name("SampleType3")
        .with_resources(Value::Dict(resources.clone()))
        .with_to_unicode(65, "Ω");
    let descriptor = FontDescriptor::new()
        .with_family("Sample")
        .with_ascent(800.0);
    let font = Font::type3(12.0, type3).with_descriptor(descriptor);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "A");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type3");
    let parsed = reparsed
        .type3_font(&font_dict)
        .expect("should parse Type 3 font")
        .expect("should be Type 3");

    assert_eq!(parsed.name.as_deref(), Some("SampleType3"));
    assert!(parsed.has_to_unicode, "应记录 /ToUnicode 条目");
    let descriptor = parsed
        .descriptor
        .as_ref()
        .expect("应解析出 /FontDescriptor");
    assert_eq!(descriptor.font_family.as_deref(), Some("Sample"));
    assert_eq!(descriptor.ascent, Some(800.0));
    assert_eq!(
        parsed.resources(&reparsed).expect("应解析 /Resources"),
        Some(resources)
    );
    assert_eq!(
        parsed
            .resource(&reparsed, "XObject", "Im1")
            .expect("资源查找"),
        Some(Value::Integer(1))
    );
    assert_eq!(
        parsed.resource(&reparsed, "XObject", "Missing").unwrap(),
        None
    );

    // /ToUnicode 映射优先于 /Encoding：字符码 65 提取为 Ω 而非 A。
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Ω"));
}

/// 合成一个最小 name-keyed CFF 程序：2 个字形（`.notdef` 与 `Alpha`）、
/// charset format 0（GID 1 → SID 391）以及提供 `Alpha` 的 String INDEX。
/// 所有 DICT 操作数与偏移用定长 longint 编码，偏移可单趟计算。
fn sample_name_keyed_cff_program() -> Vec<u8> {
    fn longint(value: i32) -> Vec<u8> {
        let mut out = vec![29];
        out.extend_from_slice(&value.to_be_bytes());
        out
    }
    fn shortint(value: i16) -> Vec<u8> {
        let mut out = vec![28];
        out.extend_from_slice(&value.to_be_bytes());
        out
    }
    fn index(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        if entries.is_empty() {
            return out;
        }
        let mut offsets = vec![1u32];
        let mut total = 1u32;
        for entry in entries {
            total += entry.len() as u32;
            offsets.push(total);
        }
        out.push(1); // offSize：本夹具的偏移都小于 0xFF
        for offset in offsets {
            out.push(offset as u8);
        }
        for entry in entries {
            out.extend_from_slice(entry);
        }
        out
    }

    let charstring = {
        let mut cs = shortint(600); // width 操作数（nominalWidthX 为 0）
        cs.extend(shortint(100));
        cs.extend(shortint(0));
        cs.push(21); // rmoveto
        cs.extend(shortint(200));
        cs.extend(shortint(0));
        cs.push(5); // rlineto
        cs.push(14); // endchar
        cs
    };
    let private = {
        let mut p = shortint(500);
        p.push(20); // defaultWidthX
        p.extend(shortint(0));
        p.push(21); // nominalWidthX
        p
    };
    let header = [1u8, 0, 4, 1];
    let name_index = index(&[b"NKTest".to_vec()]);
    let string_index = index(&[b"Alpha".to_vec()]);
    let gsubr_index = index(&[]);
    let charset = vec![0, 0x01, 0x87]; // format 0：GID 1 → SID 391
    let charstrings = index(&[vec![14], charstring]);

    let top_dict =
        |charset_off: i32, char_strings_off: i32, private_size: i32, private_off: i32| {
            let mut out = Vec::new();
            out.extend(longint(charset_off));
            out.push(15); // charset
            out.extend(longint(char_strings_off));
            out.push(17); // CharStrings
            out.extend(longint(private_size));
            out.extend(longint(private_off));
            out.push(18); // Private
            out
        };
    let placeholder = top_dict(0, 0, private.len() as i32, 0);
    let top_index_size = index(std::slice::from_ref(&placeholder)).len();
    let prefix =
        header.len() + name_index.len() + top_index_size + string_index.len() + gsubr_index.len();
    let charset_offset = prefix as i32;
    let private_offset = (prefix + charset.len()) as i32;
    let char_strings_offset = (prefix + charset.len() + private.len()) as i32;
    let top_dict = top_dict(
        charset_offset,
        char_strings_offset,
        private.len() as i32,
        private_offset,
    );
    assert_eq!(
        index(std::slice::from_ref(&top_dict)).len(),
        top_index_size,
        "Top DICT 长度不应随偏移取值变化"
    );

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    out.extend(name_index);
    out.extend(index(&[top_dict]));
    out.extend(string_index);
    out.extend(gsubr_index);
    out.extend(charset);
    out.extend(private);
    out.extend(charstrings);
    out
}

/// 合成一个最小 CID-keyed CFF 程序：3 个字形（CID 0/7/9）、单个 Font
/// DICT、charset 与 FDSelect 均为 format 0。所有 DICT 操作数用定长
/// longint 编码，偏移可单趟计算。
fn sample_cid_keyed_cff_program() -> Vec<u8> {
    fn longint(value: i32) -> Vec<u8> {
        let mut out = vec![29];
        out.extend_from_slice(&value.to_be_bytes());
        out
    }
    fn shortint(value: i16) -> Vec<u8> {
        let mut out = vec![28];
        out.extend_from_slice(&value.to_be_bytes());
        out
    }
    fn index(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        if entries.is_empty() {
            return out;
        }
        let mut offsets = vec![1u32];
        let mut total = 1u32;
        for entry in entries {
            total += entry.len() as u32;
            offsets.push(total);
        }
        out.push(1); // offSize：本夹具的偏移都小于 0xFF
        for offset in offsets {
            out.push(offset as u8);
        }
        for entry in entries {
            out.extend_from_slice(entry);
        }
        out
    }

    // charstring：宽度操作数 400 + rmoveto + rlineto + endchar。
    let charstring = {
        let mut cs = shortint(400);
        cs.extend(shortint(100));
        cs.extend(shortint(0));
        cs.push(21); // rmoveto
        cs.extend(shortint(200));
        cs.extend(shortint(0));
        cs.push(5); // rlineto
        cs.push(14); // endchar
        cs
    };
    let private = {
        let mut p = shortint(500);
        p.push(20); // defaultWidthX
        p.extend(shortint(0));
        p.push(21); // nominalWidthX
        p
    };
    let header = [1u8, 0, 4, 1];
    let name_index = index(&[b"B".to_vec()]);
    let string_index = index(&[]);
    let gsubr_index = index(&[]);
    let charset = vec![0, 0, 7, 0, 9]; // format 0：GID 1→CID 7，GID 2→CID 9
    let fd_select = vec![0, 0, 0, 0]; // format 0：全部 FD 0
    let charstrings = index(&[vec![14], charstring.clone(), charstring]);

    let top_dict = |charset_off: i32, fd_array_off: i32, fd_select_off: i32, cs_off: i32| {
        let mut out = Vec::new();
        for value in [391, 392, 0] {
            out.extend(longint(value));
        }
        out.extend([12, 30]); // ROS
        out.extend(longint(100));
        out.extend([12, 34]); // CIDCount
        out.extend(longint(charset_off));
        out.push(15); // charset
        out.extend(longint(fd_array_off));
        out.extend([12, 36]); // FDArray
        out.extend(longint(fd_select_off));
        out.extend([12, 37]); // FDSelect
        out.extend(longint(cs_off));
        out.push(17); // CharStrings
        out
    };
    let top_index_size = index(&[top_dict(0, 0, 0, 0)]).len();
    let fd_array_size = index(&[{
        let mut fd = longint(0);
        fd.extend(longint(0));
        fd.push(18); // Private
        fd
    }])
    .len();

    let prefix =
        header.len() + name_index.len() + top_index_size + string_index.len() + gsubr_index.len();
    let charset_off = prefix;
    let fd_select_off = charset_off + charset.len();
    let fd_array_off = fd_select_off + fd_select.len();
    let private_off = fd_array_off + fd_array_size;
    let cs_off = private_off + private.len();

    let fd_array = index(&[{
        let mut fd = longint(private.len() as i32);
        fd.extend(longint(private_off as i32));
        fd.push(18); // Private
        fd
    }]);
    assert_eq!(fd_array.len(), fd_array_size);

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    out.extend(name_index);
    out.extend(index(&[top_dict(
        charset_off as i32,
        fd_array_off as i32,
        fd_select_off as i32,
        cs_off as i32,
    )]));
    out.extend(string_index);
    out.extend(gsubr_index);
    out.extend(charset);
    out.extend(fd_select);
    out.extend(fd_array);
    out.extend(private);
    out.extend(charstrings);
    out
}

/// 写入 → 重新解析 → 提取：CIDFontType0 的 CFF 程序按 `with_subset_cids`
/// 裁剪后仍应完整往返——`/BaseFont` 带标签、`/FontFile3` 为可解析的子
/// 集、文本经 `/ToUnicode` 提取一致。
#[test]
fn builder_cid_font_type0_subset_roundtrips_through_reader() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };
    use pdf_engine::core::reader::font::RawCffFont;

    let cff = sample_cid_keyed_cff_program();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(cff.clone()))
    .with_subset_cids(&[9]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(page, index, 72.0, 700.0, &[(9, "乙")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    // Type 0 父字体与 CIDFont 后代的 /BaseFont 都带六字母标签前缀。
    let assert_tagged = |base_font: &str| {
        assert_eq!(strip_subset_tag(base_font), "ExampleCIDFont");
        assert_ne!(base_font, "ExampleCIDFont", "应带标签前缀");
        let (tag, rest) = base_font.split_at(7);
        assert!(
            tag[..6].bytes().all(|b| b.is_ascii_uppercase()),
            "{base_font}"
        );
        assert_eq!(tag.as_bytes()[6], b'+');
        assert_eq!(rest, "ExampleCIDFont");
    };
    let type0_dict = find_font_dict(&reparsed, "Type0");
    let type0 = reparsed
        .type0_font(&type0_dict)
        .expect("should parse Type0")
        .expect("should be Type0");
    assert_tagged(type0.base_font());

    let descendant = find_font_dict(&reparsed, "CIDFontType0");
    let cid = reparsed
        .cid_font_type0(&descendant)
        .expect("should parse CIDFontType0")
        .expect("should be CIDFontType0");
    assert_tagged(cid.base_font());

    // /FontFile3 已被裁剪：更短、仍为 CIDFontType0C 且可被 raw 层解析，
    // 只剩 CID 0 与 CID 9（GID 重编号为 0/1）。
    let embedded = reparsed
        .cid_font_type0_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("CIDFontType0C"));
    assert!(embedded.data.len() < cff.len(), "子集程序应更短");
    let parsed = RawCffFont::parse(embedded.data.clone()).expect("子集程序应可解析");
    assert!(parsed.is_cid_keyed());
    assert_eq!(parsed.glyph_count(), 2, "应只剩 CID 0 与 CID 9");
    assert_eq!(parsed.glyph_for_cid(9), Some(1));
    assert_eq!(parsed.glyph_for_cid(7), None);

    // 文本经 /ToUnicode 提取一致。
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("乙"));
}

/// 写入 → 重新解析 → 提取：1/2/3 字节混合字符码经嵌入 CMap 映射为 CID，
/// 读端按最长匹配解码，ToUnicode 键长度与字符码一致。
#[test]
fn builder_multibyte_cid_cmap_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidEncoding, CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let encoding = CidEncoding::new(0)
        .mapping_bytes(&[0x41], 1)
        .mapping(0xB1B2, 2)
        .mapping_bytes(&[0x01, 0x02, 0x03], 3);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_code_bytes_unicode(
        page,
        index,
        72.0,
        700.0,
        &[
            (&[0x41], "A"),
            (&[0xB1, 0xB2], "B"),
            (&[0x01, 0x02, 0x03], "C"),
        ],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x41, 0xB1, 0xB2, 0x01, 0x02, 0x03])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [1, 2, 3],
        "混合码长应最长匹配并映射为 CID"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.code.len())
            .collect::<Vec<_>>(),
        [1, 2, 3],
        "回读应保留字符码字节长度"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("A"), Some("B"), Some("C")]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("ABC"));
}

/// 写入 → 重新解析 → 提取：自定义 /CMapName 与 usecmap 基座（Identity-H）
/// 一同写出；本地映射覆盖基座，未覆盖码仍继承基座映射。
#[test]
fn builder_usecmap_and_custom_cmap_name_roundtrip() {
    use pdf_engine::core::builder::{
        CidEncoding, CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let encoding = CidEncoding::new(0)
        .with_cmap_name("Roundtrip-H")
        .with_usecmap("Identity-H")
        .mapping(0x0041, 9);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(page, index, 72.0, 700.0, &[(0x0041, "A"), (0x0043, "C")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let text = String::from_utf8_lossy(&buf);
    assert!(
        text.contains("/CMapName /Roundtrip-H def"),
        "应写自定义 CMapName"
    );
    assert!(text.contains("/Identity-H usecmap"), "应写 usecmap 基座");

    let reparsed = parse_buf(&buf);
    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x00, 0x41, 0x00, 0x43])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [9, 0x43],
        "本地映射覆盖基座，未覆盖码继承 Identity-H"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("A"), Some("C")]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AC"));
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

/// 写入 → 重新解析：builder 构造的 `/ExtGState` 资源应被读端完整解析，
/// 包括 `/D`、`/BM` 回退数组与 `/Font` 字体字典引用，同时不影响文本提取。
#[test]
fn builder_graphics_state_roundtrips_through_parser() {
    use pdf_engine::core::builder::{DashPattern, Font, GraphicsState, PageSize, PdfBuilder};
    use pdf_engine::core::reader::graphics_state::BlendMode;

    let font = Font::helvetica(12.0);
    let state = GraphicsState::new()
        .with_line_width(2.5)
        .with_line_cap(1)
        .with_line_join(2)
        .with_miter_limit(4.0)
        .with_dash_pattern(DashPattern::new(vec![2.0, 1.0], 0.5))
        .with_rendering_intent("RelativeColorimetric")
        .with_overprint(true)
        .with_nonstroking_overprint(false)
        .with_overprint_mode(1)
        .with_font(font.clone(), 18.0)
        .with_flatness(50.0)
        .with_smoothness(0.25)
        .with_stroke_adjustment(true)
        .with_blend_modes(&["Normal", "Compatible"])
        .with_stroke_alpha(0.8)
        .with_fill_alpha(0.6)
        .with_alpha_is_shape(true)
        .with_text_knockout(false);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.set_graphics_state(page, &state);
    builder.text(page, &font, 72.0, 700.0, "Hello");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let reparsed = parse_buf(&buf);
    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let states = reparsed
        .ext_g_states(resources)
        .expect("parse /ExtGState resources");
    let gs = states.get("GS1").expect("GS1 graphics state");

    assert_eq!(gs.line_width, Some(2.5));
    assert_eq!(gs.line_cap, Some(1));
    assert_eq!(gs.line_join, Some(2));
    assert_eq!(gs.miter_limit, Some(4.0));
    let dash = gs.dash_pattern.as_ref().expect("/D");
    assert_eq!(dash.array, vec![2.0, 1.0]);
    assert_eq!(dash.phase, 0.5);
    assert_eq!(gs.rendering_intent.as_deref(), Some("RelativeColorimetric"));
    assert_eq!(gs.overprint, Some(true));
    assert_eq!(gs.nonstroking_overprint, Some(false));
    assert_eq!(gs.overprint_mode, Some(1));
    assert_eq!(gs.flatness, Some(50.0));
    assert_eq!(gs.smoothness, Some(0.25));
    assert_eq!(gs.stroke_adjustment, Some(true));
    assert_eq!(
        gs.blend_mode,
        Some(BlendMode::Array(vec![
            "Normal".to_string(),
            "Compatible".to_string(),
        ]))
    );
    assert_eq!(gs.stroke_alpha, Some(0.8));
    assert_eq!(gs.fill_alpha, Some(0.6));
    assert_eq!(gs.alpha_is_shape, Some(true));
    assert_eq!(gs.text_knockout, Some(false));

    let font_entry = gs.font.as_ref().expect("/Font");
    assert_eq!(font_entry.size, 18.0);
    let num = font_entry.font.as_ref().expect("font reference").0;
    let font_object = reparsed
        .get_object(num)
        .expect("font lookup")
        .expect("font object");
    let font_dict = font_object.value.as_dict().expect("font dictionary");
    assert_eq!(font_dict.get("Type").and_then(Value::as_name), Some("Font"));

    // ISO 32000-1 Table A.1 无 ca/CA/BM 操作符：三者只能存在于
    // ExtGState 字典，内容流仅经 `/GS1 gs` 应用。
    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    assert!(content_text.contains("/GS1 gs\n"), "{content_text}");
    for inline in [" ca", " CA", " BM"] {
        assert!(
            !content_text.contains(inline),
            "内容流不得内联 {inline:?}: {content_text}"
        );
    }

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Hello"));
}

/// C5 读写模型统一往返：页面内容 → 读端 Op 模型 → `serialize_ops` →
/// 重新解析模型相等；再经 `to_content_op` 重建为写端 `ContentStream`
/// 表单写出，同样的文本仍可提取。
#[test]
fn content_model_roundtrip_preserves_text_extraction() {
    use pdf_engine::core::builder::{Font, FormXObject, PageSize, PdfBuilder};
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::content::{parse_content_stream, serialize_ops, to_content_op};

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Roundtrip");
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let ops = doc.page_content_ops(0).expect("page content ops");
    let rewritten = serialize_ops(&ops);
    let reparsed = parse_content_stream(&rewritten).expect("re-parse serialized content stream");
    assert_eq!(ops, reparsed, "模型往返应无损");
    assert_eq!(doc.extract_page_text(0).as_deref(), Some("Roundtrip"));

    // 模型 → 写端 Op → FormXObject 重建表单（继承页面资源）后文本仍可提取。
    let mut inner = ContentStream::new();
    for op in &ops {
        inner.push(to_content_op(op).expect("builder 生成的操作符应全部可映射"));
    }
    let form = FormXObject::new([0.0, 0.0, 842.0, 595.0], inner);
    let mut builder2 = PdfBuilder::new();
    let page2 = builder2.page(PageSize::A4);
    // 先在 page2 注册 /F1 资源，供模型引用的 Tf 名称解析。
    builder2.text(page2, &Font::helvetica(12.0), 72.0, 50.0, "Base");
    let handle = builder2.form_xobject(&form);
    builder2.draw_form(page2, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let mut buf2 = Vec::new();
    builder2.build(&mut buf2).expect("模型重建内容应可写出");
    let doc2 = parse_buf(&buf2);
    let text2 = doc2.extract_page_text(0).expect("重建后应有文本");
    assert!(text2.contains("Roundtrip"), "重建后提取文本: {text2:?}");
}

/// 写入 → 重新解析：启用 D4 压缩策略后，图片流应带 PNG Up 预测器地写成
/// FlateDecode，内容流写成无 DecodeParms 的 FlateDecode；读端解码字节与
/// 原始样本一致，文本仍可提取，且整体输出严格小于同内容明文版本。
#[test]
fn builder_flate_stream_compression_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, PageSize, PdfBuilder};
    use pdf_engine::core::writer::{ImagePredictor, StreamCompression};

    let mut pixels = Vec::with_capacity(16 * 4 * 3);
    for y in 0..4u8 {
        for x in 0..16u8 {
            pixels.extend_from_slice(&[x * 15, y * 60, (x + y) * 14]);
        }
    }
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 16,
        height: 4,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };

    let build = |compress: bool| -> Vec<u8> {
        let mut builder = PdfBuilder::new();
        if compress {
            builder.compress_streams(
                StreamCompression::flate().with_image_predictor(ImagePredictor::PngUp),
            );
        }
        let page = builder.page(PageSize::A4);
        builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "Flate");
        // 重复填充文本把内容流推到 ~450 B，确保 zlib 收益超过头部开销。
        builder.text(
            page,
            &Font::helvetica(9.0),
            72.0,
            700.0,
            &"padding ".repeat(40),
        );
        builder.image(page, 72.0, 600.0, 96.0, 24.0, &img);
        let mut buf = Vec::new();
        builder.build(&mut buf).expect("build");
        buf
    };

    let plain = build(false);
    let compressed = build(true);
    assert!(
        compressed.len() < plain.len(),
        "压缩输出应小于明文输出: plain {} B, compressed {} B",
        plain.len(),
        compressed.len()
    );
    let text = String::from_utf8_lossy(&compressed).into_owned();
    assert!(text.contains("/Filter /FlateDecode"), "{text}");
    assert!(text.contains("/Predictor 15"), "{text}");

    let reparsed = parse_buf(&compressed);
    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let Value::Ref(image_num, _) = xobjects.get("Im1").expect("Im1 resource") else {
        panic!("Im1 should be an indirect reference");
    };
    let image_object = reparsed
        .get_object(*image_num)
        .expect("get object")
        .expect("image object should exist");
    let Value::Stream(image_dict, _) = &image_object.value else {
        panic!("image should be a stream");
    };
    assert_eq!(
        image_dict.get("Filter").and_then(Value::as_name),
        Some("FlateDecode")
    );
    let parms = image_dict
        .get("DecodeParms")
        .and_then(Value::as_dict)
        .expect("image should carry DecodeParms");
    assert_eq!(parms.get("Predictor").and_then(Value::as_integer), Some(15));
    assert_eq!(parms.get("Colors").and_then(Value::as_integer), Some(3));
    assert_eq!(parms.get("Columns").and_then(Value::as_integer), Some(16));
    assert_eq!(
        reparsed
            .decode_stream_bytes(&image_object.value)
            .expect("decode image"),
        pixels
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let Value::Stream(content_dict, _) = &content_object.value else {
        panic!("content should be a stream");
    };
    assert_eq!(
        content_dict.get("Filter").and_then(Value::as_name),
        Some("FlateDecode")
    );
    assert!(
        content_dict.get("DecodeParms").is_none(),
        "非图像流不应带预测器参数"
    );
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    assert!(content_text.contains("/Im1 Do"), "{content_text}");
    let extracted = reparsed.extract_page_text(0).expect("compressed text");
    assert!(extracted.contains("Flate"), "提取文本: {extracted:?}");
}

/// D1 读端颜色空间模型：写端产出的 Indexed / Separation / DeviceN / ICCBased
/// 图片与页级命名颜色空间，经 `Document::color_space` 解析为类型化模型，
/// `decode_samples` 解码样本值正确（调色板展开、tint 函数求值、ICC 配置
/// 字节与 /N），非法空间返回类型化错误。
#[test]
fn reader_color_space_model_decodes_builder_images() {
    use pdf_engine::core::builder::{
        CalRGB, ColorSpace, DeviceN, ExponentialFunction, Font, Function, ICCBased, ImageData,
        Indexed, PageSize, PdfBuilder, SampledFunction, Separation,
    };
    use pdf_engine::core::reader::color_space::{ColorSpaceError, ColorSpaceModel};

    let assert_close = |samples: &[f64], expected: &[f64]| {
        assert_eq!(samples.len(), expected.len(), "样本个数不符: {samples:?}");
        for (actual, want) in samples.iter().zip(expected) {
            assert!((actual - want).abs() < 1e-6, "got {actual}, want {want}");
        }
    };

    let indexed_lookup = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11];
    let indexed = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::Indexed(Indexed::new(
            ColorSpace::DeviceRGB,
            1,
            indexed_lookup.clone(),
        )),
        bits: 8,
        data: vec![0, 1],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };
    let separation = ImageData {
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
        data: vec![0x00, 0xFF],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };
    let devicen = ImageData {
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
                vec![2, 2],
                8,
                vec![0.0; 12],
            )),
        )),
        bits: 8,
        data: vec![0x10, 0x20, 0x30, 0x40],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };
    let profile: Vec<u8> = (0..64u32).map(|v| v as u8).collect();
    let icc = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: profile.clone(),
            alternate: None,
            range: None,
        }),
        bits: 8,
        data: vec![10, 20, 30, 40, 50, 60],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "CS");
    builder.image(page, 72.0, 600.0, 60.0, 30.0, &indexed);
    builder.image(page, 72.0, 540.0, 60.0, 30.0, &separation);
    builder.image(page, 72.0, 480.0, 60.0, 30.0, &devicen);
    builder.image(page, 72.0, 420.0, 60.0, 30.0, &icc);
    let cal = ColorSpace::CalRGB(CalRGB {
        white_point: [0.9505, 1.0, 1.089],
        black_point: None,
        gamma: Some([2.2, 2.2, 2.2]),
        matrix: None,
    });
    let handle = builder.color_space(&cal);
    builder.set_fill_color_space(page, handle);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let image_object = |name: &str| {
        let number = xobjects.get(name).and_then(Value::as_ref).unwrap().0;
        reparsed
            .get_object(number)
            .expect("get object")
            .expect("image object should exist")
    };

    let decode_image = |name: &str| -> (ColorSpaceModel, Vec<f64>) {
        let object = image_object(name);
        let Value::Stream(dict, _) = &object.value else {
            panic!("{name} should be a stream");
        };
        let model = reparsed
            .color_space(dict.get("ColorSpace").expect("ColorSpace"))
            .expect("color space should parse");
        let bits = dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer)
            .expect("BitsPerComponent") as usize;
        let width = dict
            .get("Width")
            .and_then(Value::as_integer)
            .expect("Width") as usize;
        let raw = reparsed
            .decode_stream_bytes(&object.value)
            .expect("decode image bytes");
        let samples = model
            .decode_samples(bits, width, &raw)
            .expect("decode samples");
        (model, samples)
    };

    let (model, samples) = decode_image("Im1");
    let ColorSpaceModel::Indexed(indexed_space) = &model else {
        panic!("Im1 should be Indexed");
    };
    assert_eq!(indexed_space.hival, 1);
    assert_eq!(indexed_space.lookup, indexed_lookup);
    assert_eq!(
        *indexed_space.base,
        ColorSpaceModel::DeviceRGB,
        "基空间应解析为 DeviceRGB"
    );
    let expected: Vec<f64> = indexed_lookup.iter().map(|v| *v as f64 / 255.0).collect();
    assert_close(&samples, &expected);

    let (model, samples) = decode_image("Im2");
    let ColorSpaceModel::Separation(separation_space) = &model else {
        panic!("Im2 should be Separation");
    };
    assert_eq!(separation_space.colorant, "Orange");
    assert_eq!(*separation_space.alternate, ColorSpaceModel::DeviceCMYK);
    assert!(separation_space.tint.is_some(), "tint 函数应已构造");
    assert_close(&samples, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.4, 0.8, 0.0]);

    let (model, samples) = decode_image("Im3");
    let ColorSpaceModel::DeviceN(device_n) = &model else {
        panic!("Im3 should be DeviceN");
    };
    assert_eq!(device_n.colorants, vec!["Orange", "Green"]);
    assert_eq!(model.component_count(), Some(2));
    assert_close(&samples, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);

    let (model, samples) = decode_image("Im4");
    let ColorSpaceModel::IccBased(icc_space) = &model else {
        panic!("Im4 should be ICCBased");
    };
    assert_eq!(icc_space.n, 3);
    assert_eq!(icc_space.profile, profile, "应返回 ICC 配置文件字节");
    assert_eq!(
        *icc_space.alternate,
        ColorSpaceModel::DeviceRGB,
        "缺 /Alternate 时按 /N=3 取 DeviceRGB"
    );
    assert_eq!(icc_space.range, vec![(0.0, 1.0); 3]);
    let expected: Vec<f64> = [10, 20, 30, 40, 50, 60]
        .iter()
        .map(|v| *v as f64 / 255.0)
        .collect();
    assert_close(&samples, &expected);

    let named = reparsed
        .page_color_space(0, "CS1")
        .expect("named color space lookup")
        .expect("CS1 should exist");
    let ColorSpaceModel::CalRGB(cal_rgb) = named else {
        panic!("CS1 should be CalRGB");
    };
    assert_eq!(cal_rgb.white_point, vec![0.9505, 1.0, 1.089]);
    assert_eq!(cal_rgb.gamma, vec![2.2, 2.2, 2.2]);
    assert_eq!(
        cal_rgb.matrix,
        vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
    );
    assert!(
        reparsed
            .page_color_space(0, "Absent")
            .expect("lookup")
            .is_none()
    );
    assert!(matches!(
        reparsed.color_space(&Value::Name("NChannel".into())),
        Err(ColorSpaceError::Unsupported(_))
    ));
}

/// 写入 → 重新解析：PDF 1.6 透明度扩展（/TR2 字典、/HT 半色调包装字典、
/// `/SMask` 的 /TR、/SMaskInData、/Matte、页/表单透明组）应写出并被读端
/// 解析为类型化模型；页面文本仍可提取。
#[test]
fn builder_transparency_extensions_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, ExponentialFunction, Font, FormXObject, Function, GraphicsState, GroupXObject,
        Halftone, HalftoneDictionary, PageSize, PdfBuilder, SoftMask, Type1Halftone, Type2Halftone,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::graphics_state::{
        HalftoneModel, SoftMaskSubtype, Type1Halftone as RType1, Type2Halftone as RType2,
    };
    use pdf_engine::core::reader::group::GroupSubtype;

    let mut mask_content = ContentStream::new();
    mask_content
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 50.0, 50.0)
        .fill();
    let mask_form = FormXObject::new([0.0, 0.0, 50.0, 50.0], mask_content)
        .with_group(GroupXObject::transparency().with_color_space(ColorSpace::DeviceGray));

    let tr2 = Value::Dict(
        Dict::new()
            .insert("TransferType", Value::Name("HalftoneType".into()))
            .insert("TransferFunction0", Value::Name("identity".into())),
    );

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_6);
    let page = builder.page(PageSize::A4);
    let mask_handle = builder.form_xobject(&mask_form);
    let spot = builder.function(&Function::Exponential(ExponentialFunction::new(
        vec![0.0, 1.0],
        2.2,
    )));
    let halftone = HalftoneDictionary::new(1)
        .with_colorant(
            "C",
            Halftone::Type1(Type1Halftone {
                x_frequency: 65.0,
                angle: 15.0,
                spot_function: spot,
                transfer_function: None,
            }),
        )
        .with_colorant(
            "M",
            Halftone::Type2(Type2Halftone {
                frequency: 133.3,
                angle: 75.0,
                width: 0.5,
                uncompensated: true,
                spot_function: spot,
                transfer_function: None,
            }),
        );
    builder.set_graphics_state(
        page,
        &GraphicsState::new()
            .with_fill_alpha(0.5)
            .with_transfer_function_2(tr2.clone())
            .with_halftone(halftone)
            .with_soft_mask(
                SoftMask::luminosity(mask_handle)
                    .with_transfer_function(spot)
                    .with_smask_in_data(1)
                    .with_matte([0.2, 0.4, 0.6]),
            ),
    );
    builder.set_page_group(
        page,
        &GroupXObject::transparency()
            .with_color_space(ColorSpace::DeviceRGB)
            .with_isolated(true)
            .with_knockout(true),
    );
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "XTND");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("PDF 1.6 build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let states = reparsed
        .ext_g_states(resources)
        .expect("parse /ExtGState resources");
    let gs = states.get("GS1").expect("GS1 graphics state");

    assert_eq!(gs.transfer2, Some(tr2));

    let raw_halftone = gs.halftone.as_ref().expect("GS1 should carry /HT");
    let model = reparsed.halftone(raw_halftone).expect("typed halftone");
    match model {
        HalftoneModel::Composite(composite) => {
            assert_eq!(composite.default_type, Some(1));
            match composite.colorants.get("C") {
                Some(HalftoneModel::Type1(RType1 {
                    x_frequency,
                    angle,
                    transfer_function,
                    ..
                })) => {
                    assert_eq!(*x_frequency, 65.0);
                    assert_eq!(*angle, 15.0);
                    assert!(transfer_function.is_none());
                }
                other => panic!("colorant C should be typed, got {other:?}"),
            }
            match composite.colorants.get("M") {
                Some(HalftoneModel::Type2(RType2 {
                    frequency,
                    width,
                    uncompensated,
                    ..
                })) => {
                    assert!((frequency - 133.3).abs() < 1e-9);
                    assert_eq!(*width, 0.5);
                    assert!(*uncompensated);
                }
                other => panic!("colorant M should be typed, got {other:?}"),
            }
        }
        other => panic!("expected composite halftone, got {other:?}"),
    }

    let raw_mask = gs.soft_mask.as_ref().expect("GS1 should carry /SMask");
    let mask = reparsed.soft_mask(raw_mask).expect("typed soft mask");
    assert_eq!(mask.subtype, SoftMaskSubtype::Luminosity);
    assert_eq!(mask.smask_in_data, Some(1));
    assert_eq!(mask.matte, Some(vec![0.2, 0.4, 0.6]));
    let transfer = mask.transfer.as_ref().expect("/SMask /TR function");
    assert_eq!(
        transfer
            .as_dict()
            .and_then(|dict| dict.get("FunctionType"))
            .and_then(Value::as_integer),
        Some(2)
    );

    let group = reparsed
        .page_group(0)
        .expect("page group")
        .expect("page has /Group");
    assert_eq!(group.subtype, GroupSubtype::Transparency);
    assert!(group.isolated);
    assert_eq!(group.knockout, Some(true));
    assert!(matches!(
        group.color_space,
        Some(pdf_engine::core::reader::color_space::ColorSpaceModel::DeviceRGB)
    ));

    let Value::Stream(mask_dict, _) = reparsed
        .get_object(match &mask.group {
            Value::Ref(num, _) => *num,
            other => panic!("soft mask /G should stay a reference, got {other:?}"),
        })
        .expect("lookup")
        .expect("mask form object")
        .value
    else {
        panic!("/G references a stream");
    };
    let form_group = reparsed
        .group(mask_dict.get("Group").expect("form /Group"))
        .expect("typed form group");
    assert_eq!(form_group.subtype, GroupSubtype::Transparency);
    assert_eq!(
        form_group.color_space_raw,
        Some(Value::Name("DeviceGray".into()))
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("XTND"));
}

/// 严出校验：`/Matte` 需要 PDF 1.6，降级到 1.5 目标应报错且零字节。
#[test]
fn builder_soft_mask_matte_requires_pdf_1_6_target() {
    use pdf_engine::core::builder::{
        ColorSpace, FormXObject, GraphicsState, GroupXObject, PageSize, PdfBuilder, SoftMask,
    };
    use pdf_engine::core::content::ContentStream;

    let mut mask_content = ContentStream::new();
    mask_content
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 50.0, 50.0)
        .fill();
    let mask_form = FormXObject::new([0.0, 0.0, 50.0, 50.0], mask_content)
        .with_group(GroupXObject::transparency().with_color_space(ColorSpace::DeviceGray));

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    let page = builder.page(PageSize::A4);
    let mask_handle = builder.form_xobject(&mask_form);
    builder.set_graphics_state(
        page,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask_handle).with_matte([0.5])),
    );

    let mut buf = Vec::new();
    let error = builder
        .build(&mut buf)
        .expect_err("/Matte must not build at PDF 1.5");
    let message = error.to_string();
    assert!(
        message.contains("soft mask matte color") && message.contains("1.6"),
        "{message}"
    );
    assert!(buf.is_empty(), "构建失败时不应写出任何字节");
}

/// D2 读端图像模型：写端产出的 RGB、软蒙版、显式蒙版与颜色键蒙版图片
/// 经 `Document::page_image` 解析为 `ImageModel`；`samples()` 返回解码后的
/// 样本值，`/Mask` 区分为显式蒙版或颜色键范围；页面文本仍可提取。
#[test]
fn builder_images_roundtrip_through_image_model() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, ImageMask, PageSize, PdfBuilder};
    use pdf_engine::core::reader::color_space::ColorSpaceModel;
    use pdf_engine::core::reader::image::ImageMaskModel;

    let rgb = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        decode: None,
        interpolate: true,
        smask: None,
        image_mask: false,
        mask: None,
    };
    let alpha = ImageData {
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
    };
    let soft = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![9, 9, 9, 8, 8, 8, 7, 7, 7, 6, 6, 6],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    }
    .with_smask(alpha);
    let stencil = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 4,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0b1010_0000],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
    };
    let explicit = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    }
    .with_mask(ImageMask::explicit(stencil));
    let keyed = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0, 10, 20, 30, 40, 50],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
    }
    .with_mask(ImageMask::color_key(vec![0, 10, 20, 30, 40, 50]));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "IMG");
    builder.image(page, 72.0, 600.0, 100.0, 100.0, &rgb);
    builder.image(page, 72.0, 480.0, 100.0, 100.0, &soft);
    builder.image(page, 72.0, 360.0, 100.0, 100.0, &explicit);
    builder.image(page, 72.0, 240.0, 100.0, 100.0, &keyed);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let rgb_model = reparsed
        .page_image(0, "Im1")
        .expect("page image lookup")
        .expect("Im1 should exist");
    assert_eq!((rgb_model.width, rgb_model.height), (2, 2));
    assert_eq!(rgb_model.color_space, Some(ColorSpaceModel::DeviceRGB));
    assert_eq!(rgb_model.bits_per_component, 8);
    assert!(rgb_model.interpolate);
    assert_eq!(rgb_model.decode, None);
    assert!(rgb_model.smask.is_none() && rgb_model.mask.is_none());
    let samples = rgb_model.samples().expect("rgb samples");
    assert_eq!(samples.pixel(0, 0), Some(&[1.0, 0.0, 0.0][..]));
    assert_eq!(samples.pixel(1, 0), Some(&[0.0, 1.0, 0.0][..]));
    assert_eq!(samples.pixel(0, 1), Some(&[0.0, 0.0, 1.0][..]));
    assert_eq!(samples.pixel(1, 1), Some(&[1.0, 1.0, 1.0][..]));

    let soft_model = reparsed
        .page_image(0, "Im2")
        .expect("page image lookup")
        .expect("Im2 should exist");
    let smask = soft_model.smask.as_ref().expect("Im2 should have /SMask");
    assert_eq!(smask.color_space, Some(ColorSpaceModel::DeviceGray));
    assert_eq!(
        smask.samples().expect("smask samples").values,
        vec![0.0, 64.0 / 255.0, 128.0 / 255.0, 1.0]
    );

    let explicit_model = reparsed
        .page_image(0, "Im3")
        .expect("page image lookup")
        .expect("Im3 should exist");
    let Some(ImageMaskModel::Explicit(mask)) = explicit_model.mask else {
        panic!("expected an explicit mask, got {:?}", explicit_model.mask);
    };
    assert!(mask.image_mask);
    assert_eq!((mask.width, mask.height), (4, 1));
    assert_eq!(
        mask.samples().expect("mask samples").values,
        vec![1.0, 0.0, 1.0, 0.0]
    );

    let keyed_model = reparsed
        .page_image(0, "Im4")
        .expect("page image lookup")
        .expect("Im4 should exist");
    assert_eq!(
        keyed_model.mask,
        Some(ImageMaskModel::ColorKey(vec![0, 10, 20, 30, 40, 50]))
    );

    assert!(
        reparsed
            .page_image(0, "Im9")
            .expect("page image lookup")
            .is_none()
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("IMG"));
}

/// D7 写端图像补全：`/Intent`、`/Alternates`、`/Matte` 经写端组装后由
/// `page_image` 读回；替代图像解析为独立 `ImageModel` 且不占页面资源，
/// `/Matte` 落在 SMask 图像上并按基图分量数校验。
#[test]
fn builder_image_intent_alternates_and_matte_roundtrip() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, ImageData, ImageIntent, PageSize, PdfBuilder,
    };

    let alternate = ImageData {
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![10, 20, 30],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
    };
    let alpha = ImageData {
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![64, 128, 192, 255],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
    };
    let img = ImageData {
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![128; 12],
        decode: None,
        interpolate: false,
        intent: Some(ImageIntent::Perceptual),
        alternates: vec![alternate],
        matte: Some(vec![0.25, 0.5, 0.75]),
        smask: Some(Box::new(alpha)),
        image_mask: false,
        mask: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "EXTRA");
    builder.image(page, 72.0, 600.0, 100.0, 100.0, &img);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let model = reparsed
        .page_image(0, "Im1")
        .expect("page image lookup")
        .expect("Im1 should exist");
    assert_eq!(model.intent.as_deref(), Some("Perceptual"));
    assert_eq!(model.alternates.len(), 1, "one alternate image");
    assert_eq!(
        (model.alternates[0].width, model.alternates[0].height),
        (1, 1)
    );
    let alternate_samples = model.alternates[0].samples().expect("alternate samples");
    assert_eq!(
        alternate_samples.pixel(0, 0),
        Some(&[10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0][..])
    );

    let smask = model.smask.as_ref().expect("Im1 should have an SMask");
    assert_eq!(smask.matte, Some(vec![0.25, 0.5, 0.75]));
    assert_eq!(
        smask.samples().expect("smask samples").values,
        vec![64.0 / 255.0, 128.0 / 255.0, 192.0 / 255.0, 1.0]
    );

    assert!(
        reparsed
            .page_image(0, "Im2")
            .expect("page image lookup")
            .is_none(),
        "alternate and SMask must not be page resources"
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("EXTRA"));
}

#[test]
fn builder_form_own_resources_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, FormResources, FormXObject, ImageData, OptionalContentGroup, PageSize,
        PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;

    let image = ImageData {
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![40, 80, 120],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
    };
    let mut inner = ContentStream::new();
    inner
        .begin_text()
        .set_font("F1", 12.0)
        .move_next_line(10.0, 40.0)
        .show_text("OwnRes")
        .end_text();
    inner
        .save_state()
        .concat_matrix([20.0, 0.0, 0.0, 20.0, 10.0, 10.0])
        .draw_xobject("Im1")
        .restore_state();

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "PAGE");
    let group = builder.optional_content_group(&OptionalContentGroup::new("Form Layer"));
    let form = FormXObject::new([0.0, 0.0, 100.0, 50.0], inner)
        .with_resources(
            FormResources::new()
                .with_font(Font::helvetica(12.0))
                .with_image(image)
                .with_optional_content(group),
        )
        .with_optional_content(group);
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [1.0, 0.0, 0.0, 1.0, 72.0, 600.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let form_ref = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .and_then(|xobjects| xobjects.get("Fm1"))
        .and_then(Value::as_ref)
        .expect("page should reference /Fm1");
    let form_object = reparsed
        .get_object(form_ref.0)
        .expect("form lookup")
        .expect("form object");
    let Value::Stream(form_dict, _) = &form_object.value else {
        panic!("form should be a stream");
    };
    assert_eq!(
        form_dict.get("Subtype").and_then(Value::as_name),
        Some("Form")
    );

    // 表单自有资源：字体与图片子字典。
    let resources = form_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("form should write /Resources");
    assert!(
        resources
            .get("Font")
            .and_then(Value::as_dict)
            .and_then(|fonts| fonts.get("F1"))
            .and_then(Value::as_ref)
            .is_some(),
        "form /Font/F1: {resources:?}"
    );
    assert!(
        resources
            .get("Properties")
            .and_then(Value::as_dict)
            .and_then(|properties| properties.get("OC1"))
            .and_then(Value::as_ref)
            .is_some(),
        "form /Properties/OC1: {resources:?}"
    );
    let image_ref = resources
        .get("XObject")
        .and_then(Value::as_dict)
        .and_then(|xobjects| xobjects.get("Im1"))
        .cloned()
        .expect("form image /Im1");
    let image_model = reparsed.image(&image_ref).expect("parse form image");
    assert_eq!((image_model.width, image_model.height), (1, 1));
    assert_eq!(
        image_model.samples().expect("image samples").pixel(0, 0),
        Some(&[40.0 / 255.0, 80.0 / 255.0, 120.0 / 255.0][..])
    );

    // 表单 /OC 引用同一 OCG，且目录 /OCProperties 包含该组。
    let Value::Ref(form_group, _) = form_dict.get("OC").expect("form /OC") else {
        panic!("form /OC should be an indirect reference");
    };
    assert_eq!(
        resources
            .get("Properties")
            .and_then(Value::as_dict)
            .and_then(|properties| properties.get("OC1"))
            .and_then(Value::as_ref)
            .map(|(num, _)| num),
        Some(*form_group),
        "form /OC and /Properties/OC1 should reference the same OCG"
    );
    let oc_properties = reparsed
        .optional_content_properties()
        .expect("parse /OCProperties")
        .expect("expected /OCProperties");
    assert_eq!(
        oc_properties
            .group(*form_group)
            .map(|group| group.name.as_str()),
        Some("Form Layer")
    );

    // 文本提取经表单自有资源解析字体（页面与表单文字都可提取）。
    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("PAGE"), "{extracted}");
    assert!(extracted.contains("OwnRes"), "{extracted}");
}

/// B7：位置片段 API 与 builder 写出的文本坐标一致，且平铺文本输出不变。
#[test]
fn builder_text_fragments_report_positions() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};
    use pdf_engine::core::reader::text::TextFragment;

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let font = Font::helvetica(12.0);
    builder.text(page, &font, 72.0, 700.0, "First");
    builder.text(page, &font, 72.0, 680.0, "Second");
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    assert_eq!(
        reparsed
            .extract_page_text_fragments(0)
            .expect("page fragments"),
        vec![
            TextFragment {
                text: "First".into(),
                x: 72.0,
                y: 700.0,
            },
            TextFragment {
                text: "Second".into(),
                x: 72.0,
                y: 680.0,
            },
        ]
    );
    assert_eq!(
        reparsed.extract_page_text(0).as_deref(),
        Some("First\nSecond")
    );
}
