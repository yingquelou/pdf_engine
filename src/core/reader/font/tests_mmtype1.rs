//! Multiple Master（MMType1）读端单元测试：子类型识别、Table 111 条目解析、
//! 类型化错误，以及既有 Type 1 / Type 0 路径不受影响。

use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

use super::cff::{fixtures, name_keyed_fixtures};
use super::{FontError, FontProgramSource, MultipleMasterFont, PredefinedEncoding};

fn descriptor(entries: Vec<(&str, Value)>) -> Value {
    let mut dict = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

fn widths(first: i64, last: i64) -> Value {
    let mut array = Array::new();
    for code in first..=last {
        array = array.push(Value::Integer(100 + code));
    }
    Value::Array(array)
}

/// A conforming MM instance dictionary: Table 111 entries plus the §9.6.2.3
/// `/BaseFont` naming rule (LOW LINEs stand in for SPACEs).
fn mm_font_dict() -> Dict {
    Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name("MMType1".into()))
        .insert("BaseFont", Value::Name("MinionMM_366_465_11_".into()))
        .insert("FirstChar", Value::Integer(32))
        .insert("LastChar", Value::Integer(34))
        .insert("Widths", widths(32, 34))
        .insert(
            "FontDescriptor",
            descriptor(vec![
                ("MissingWidth", Value::Integer(500)),
                ("Ascent", Value::Integer(750)),
            ]),
        )
}

/// Drop dictionary keys, keeping everything else intact.
fn omit(dict: Dict, keys: &[&str]) -> Dict {
    Dict(
        dict.0
            .into_iter()
            .filter(|(key, _)| !keys.contains(&key.as_str()))
            .collect(),
    )
}

fn parse(document: &Document, dict: &Dict) -> MultipleMasterFont {
    document
        .mmtype1_font(&Value::Dict(dict.clone()))
        .unwrap()
        .expect("MMType1 dictionary")
}

#[test]
fn mmtype1_font_parses_table_111_entries_for_an_mm_instance() {
    let to_unicode = Value::Stream(Dict::new(), StreamData::Owned(b"/CMapType 1 def".to_vec()));
    let dict = mm_font_dict()
        .insert("Name", Value::Name("F1".into()))
        .insert("Encoding", Value::Name("MacRomanEncoding".into()))
        .insert("ToUnicode", to_unicode);

    let font = parse(&Document::new(), &dict);
    assert_eq!(font.base_font, "MinionMM_366_465_11_");
    assert_eq!(font.name.as_deref(), Some("F1"));
    assert_eq!(font.first_char, 32);
    assert_eq!(font.widths, vec![132.0, 133.0, 134.0]);
    assert_eq!(font.descriptor.missing_width, Some(500.0));
    assert_eq!(font.descriptor.ascent, Some(750.0));
    assert!(font.has_to_unicode);
    assert_eq!(
        font.encoding.clone().expect("/Encoding").base_encoding(),
        PredefinedEncoding::MacRoman
    );
    assert!(font.entries().is_empty());
}

#[test]
fn mmtype1_font_returns_none_for_other_font_subtypes() {
    let document = Document::new();
    for subtype in ["Type1", "TrueType", "Type3", "Type0", "CIDFontType0"] {
        let dict = mm_font_dict().insert("Subtype", Value::Name(subtype.into()));
        assert_eq!(
            document.mmtype1_font(&Value::Dict(dict)).unwrap(),
            None,
            "/Subtype /{subtype} is not an MMType1 dictionary"
        );
    }

    let not_a_dict = Value::Array(Array::new());
    assert!(matches!(
        document.mmtype1_font(&not_a_dict),
        Err(FontError::Invalid(_))
    ));
}

#[test]
fn mmtype1_font_requires_widths_and_descriptor_without_the_standard_14_exemption() {
    let document = Document::new();

    let error = document
        .mmtype1_font(&Value::Dict(omit(
            mm_font_dict(),
            &["FirstChar", "LastChar", "Widths"],
        )))
        .expect_err("MMType1 needs explicit widths");
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("/FirstChar")),
        "{error}"
    );

    let error = document
        .mmtype1_font(&Value::Dict(omit(mm_font_dict(), &["FontDescriptor"])))
        .expect_err("MMType1 needs a descriptor");
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("/FontDescriptor")),
        "{error}"
    );

    // 标准 14 名称在 Type 1 路径可省略度量；MM 路径不套用该豁免。
    let standard_named = omit(
        mm_font_dict(),
        &["FirstChar", "LastChar", "Widths", "FontDescriptor"],
    )
    .insert("BaseFont", Value::Name("Helvetica".into()));
    assert!(
        document
            .mmtype1_font(&Value::Dict(standard_named.clone()))
            .is_err()
    );
    assert_eq!(
        document.type1_font(&Value::Dict(standard_named)).unwrap(),
        None,
        "the /Type1 path is not consulted for /MMType1"
    );
}

#[test]
fn mmtype1_font_reports_malformed_entries_as_typed_errors() {
    let document = Document::new();
    let cases = [
        (
            "BaseFont",
            Value::Integer(7),
            "missing or invalid /BaseFont",
        ),
        (
            "Name",
            Value::Integer(7),
            "/Name in MMType1 font is not a name",
        ),
        (
            "ToUnicode",
            Value::Name("NotAStream".into()),
            "/ToUnicode in MMType1 font is not a stream",
        ),
    ];
    for (key, value, expected) in cases {
        let error = document
            .mmtype1_font(&Value::Dict(mm_font_dict().insert(key, value)))
            .expect_err("/{key} has the wrong type");
        assert!(
            matches!(&error, FontError::Invalid(message) if message.contains(expected)),
            "{error}"
        );
    }

    let error = document
        .mmtype1_font(&Value::Dict(
            mm_font_dict().insert("FontDescriptor", Value::Array(Array::new())),
        ))
        .expect_err("/FontDescriptor must be a dictionary");
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("/FontDescriptor is not a dictionary")),
        "{error}"
    );

    let error = document
        .mmtype1_font(&Value::Dict(
            mm_font_dict().insert("Widths", widths(32, 33)),
        ))
        .expect_err("/Widths must cover FirstChar..LastChar");
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("expected 3")),
        "{error}"
    );

    assert!(
        document
            .mmtype1_font(&Value::Dict(
                mm_font_dict().insert("Encoding", Value::Name("BogusEncoding".into()))
            ))
            .is_err()
    );

    let non_numeric = Value::Array(
        Array::new()
            .push(Value::Integer(1))
            .push(Value::Integer(2))
            .push(Value::Name("wide".into())),
    );
    assert!(
        document
            .mmtype1_font(&Value::Dict(mm_font_dict().insert("Widths", non_numeric)))
            .is_err()
    );

    let dangling = mm_font_dict().insert("FontDescriptor", Value::Ref(404, 0));
    assert!(matches!(
        document.mmtype1_font(&Value::Dict(dangling)),
        Err(FontError::Invalid(_))
    ));
}

#[test]
fn mmtype1_font_keeps_unmodelled_entries_raw_and_resolves_the_indirect_descriptor() {
    let document = Document::new();
    document.set_object(Object::new(
        6,
        0,
        descriptor(vec![("MissingWidth", Value::Integer(250))]),
    ));
    let dict = mm_font_dict()
        .insert("FontDescriptor", Value::Ref(6, 0))
        .insert("InstanceTag", Value::Integer(3))
        // 悬空引用属于未建模条目：原样保存而不解引用，因此不报错。
        .insert("MMPrivate", Value::Ref(99, 0));

    let font = parse(&document, &dict);
    assert_eq!(font.descriptor.missing_width, Some(250.0));
    assert_eq!(font.entry("InstanceTag"), Some(&Value::Integer(3)));
    assert_eq!(font.entry("MMPrivate"), Some(&Value::Ref(99, 0)));
    assert_eq!(font.entry("BaseFont"), None);
    assert_eq!(font.entries().len(), 2);
    let keys: Vec<&String> = font.entries().keys().collect();
    assert_eq!(keys, vec!["InstanceTag", "MMPrivate"]);
}

#[test]
fn mmtype1_font_width_lookup_falls_back_to_missing_width() {
    let font = parse(&Document::new(), &mm_font_dict());
    assert_eq!(font.width_for(32), Some(132.0));
    assert_eq!(font.width_for(34), Some(134.0));
    assert_eq!(font.width_for(31), Some(500.0));
    assert_eq!(font.width_for(200), Some(500.0));
}

#[test]
fn mmtype1_font_keeps_base_font_verbatim_and_restores_instance_spaces() {
    let font = parse(&Document::new(), &mm_font_dict());
    assert_eq!(font.base_font, "MinionMM_366_465_11_");
    assert_eq!(font.instance_name_with_spaces(), "MinionMM 366 465 11 ");
}

#[test]
fn mmtype1_font_encoding_reads_differences_and_the_base_font_default() {
    let document = Document::new();
    let differences = Array::new()
        .push(Value::Integer(65))
        .push(Value::Name("f_i".into()));
    let encoding = Value::Dict(
        Dict::new()
            .insert("BaseEncoding", Value::Name("WinAnsiEncoding".into()))
            .insert("Differences", Value::Array(differences)),
    );
    let font = parse(&document, &mm_font_dict().insert("Encoding", encoding));
    let encoding = font.encoding.clone().expect("/Encoding");
    assert_eq!(encoding.base_encoding(), PredefinedEncoding::WinAnsi);
    assert_eq!(encoding.glyph_name_for(65).as_deref(), Some("f_i"));
    assert_eq!(encoding.glyph_name_for(200).as_deref(), None);

    let symbolic = mm_font_dict().insert("BaseFont", Value::Name("Symbol".into()));
    let font = parse(&document, &symbolic);
    assert_eq!(
        font.encoding
            .clone()
            .expect("default encoding")
            .base_encoding(),
        PredefinedEncoding::Symbol
    );
}

#[test]
fn type1_and_truetype_paths_are_unaffected_by_mmtype1_dictionaries() {
    let document = Document::new();
    let dict = mm_font_dict().insert(
        "FontDescriptor",
        descriptor(vec![(
            "FontFile3",
            Value::Stream(
                Dict::new().insert("Subtype", Value::Name("Type1C".into())),
                StreamData::Owned(fixtures::name_keyed_cff()),
            ),
        )]),
    );
    let font = Value::Dict(dict);

    assert_eq!(document.type1_font(&font).unwrap(), None);
    assert_eq!(document.type1_font_program(&font).unwrap(), None);
    assert!(document.type1c_program(&font).unwrap().is_none());
    assert_eq!(document.type1_program(&font).unwrap(), None);
    assert_eq!(document.truetype_font_program(&font).unwrap(), None);
    assert!(document.mmtype1_font(&font).unwrap().is_some());
}

#[test]
fn mmtype1_font_program_reads_fontfile_and_fontfile3_snapshots() {
    let document = Document::new();

    let font_file = Value::Dict(mm_font_dict().insert(
        "FontDescriptor",
        descriptor(vec![(
            "FontFile",
            Value::Stream(
                Dict::new(),
                StreamData::Owned(b"%!PS-AdobeFont-1.0: MinionMM 001.000".to_vec()),
            ),
        )]),
    ));
    let program = document
        .mmtype1_font_program(&font_file)
        .unwrap()
        .expect("/FontFile snapshot");
    assert_eq!(program.source, FontProgramSource::FontFile);
    assert_eq!(program.subtype, None);
    assert!(program.data.starts_with(b"%!PS-AdobeFont"));

    let font_file3 = Value::Dict(mm_font_dict().insert(
        "FontDescriptor",
        descriptor(vec![(
            "FontFile3",
            Value::Stream(
                Dict::new().insert("Subtype", Value::Name("Type1C".into())),
                StreamData::Owned(fixtures::name_keyed_cff()),
            ),
        )]),
    ));
    let program = document
        .mmtype1_font_program(&font_file3)
        .unwrap()
        .expect("/FontFile3 snapshot");
    assert_eq!(program.source, FontProgramSource::FontFile3);
    assert_eq!(program.subtype.as_deref(), Some("Type1C"));
    assert_eq!(program.data, fixtures::name_keyed_cff());

    assert_eq!(
        document
            .mmtype1_font_program(&Value::Dict(mm_font_dict()))
            .unwrap(),
        None,
        "a descriptor without /FontFile* has no embedded program"
    );
}

#[test]
fn mmtype1_type1_program_parses_the_fontfile_snapshot() {
    let document = Document::new();
    let font = Value::Dict(mm_font_dict().insert(
        "FontDescriptor",
        descriptor(vec![(
            "FontFile",
            Value::Stream(Dict::new(), StreamData::Owned(snapshot_program())),
        )]),
    ));

    let program = document
        .mmtype1_type1_program(&font)
        .unwrap()
        .expect("§9.6.2.3: the embedded MM instance program is an ordinary Type 1 program");
    assert_eq!(program.header.font_name.as_deref(), Some("ExampleSans"));
    assert_eq!(program.len_iv, 4);
    let names: Vec<&String> = program.charstrings.keys().collect();
    assert_eq!(names, vec![".notdef", "A", "B"]);
    let decrypted_a: &[u8] = &[0x8b, 0x0e, 0x0c];
    assert_eq!(
        program.charstrings.get("A").map(|bytes| bytes.as_slice()),
        Some(decrypted_a),
        "charstrings are decrypted with /lenIV 4"
    );
    assert_eq!(program.subrs.len(), 2);
}

#[test]
fn mmtype1_type1_program_rejects_fontfile3_and_broken_snapshots() {
    let document = Document::new();

    let cff_font = Value::Dict(mm_font_dict().insert(
        "FontDescriptor",
        descriptor(vec![(
            "FontFile3",
            Value::Stream(
                Dict::new().insert("Subtype", Value::Name("Type1C".into())),
                StreamData::Owned(fixtures::name_keyed_cff()),
            ),
        )]),
    ));
    let error = document.mmtype1_type1_program(&cff_font).unwrap_err();
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("/FontFile3")),
        "{error}"
    );

    let malformed = Value::Dict(mm_font_dict().insert(
        "FontDescriptor",
        descriptor(vec![(
            "FontFile",
            Value::Stream(
                Dict::new(),
                StreamData::Owned(b"not a type1 program".to_vec()),
            ),
        )]),
    ));
    assert!(matches!(
        document.mmtype1_type1_program(&malformed),
        Err(FontError::Invalid(_))
    ));

    let wrong_key = Value::Dict(mm_font_dict().insert(
        "FontDescriptor",
        descriptor(vec![(
            "FontFile2",
            Value::Stream(Dict::new(), StreamData::Owned(vec![0; 16])),
        )]),
    ));
    let error = document.mmtype1_font_program(&wrong_key).unwrap_err();
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("/FontFile2")),
        "{error}"
    );
}

#[test]
fn mmtype1_type1c_program_reads_and_validates_the_cff_snapshot() {
    let document = Document::new();
    let with_stream = |data: Vec<u8>, subtype: &str| {
        Value::Dict(mm_font_dict().insert(
            "FontDescriptor",
            descriptor(vec![(
                "FontFile3",
                Value::Stream(
                    Dict::new().insert("Subtype", Value::Name(subtype.into())),
                    StreamData::Owned(data),
                ),
            )]),
        ))
    };

    let cff = with_stream(
        name_keyed_fixtures::name_keyed_cff_custom_charset(0),
        "Type1C",
    );
    let program = document
        .mmtype1_type1c_program(&cff)
        .unwrap()
        .expect("name-keyed CFF snapshot");
    assert!(!program.is_cid_keyed());

    let cid_keyed = with_stream(fixtures::cid_keyed_cff(), "Type1C");
    assert!(matches!(
        document.mmtype1_type1c_program(&cid_keyed),
        Err(FontError::Invalid(_))
    ));

    let wrong_subtype = with_stream(fixtures::cid_keyed_cff(), "OpenType");
    let error = document.mmtype1_type1c_program(&wrong_subtype).unwrap_err();
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("Type1C")),
        "{error}"
    );

    let malformed = with_stream(vec![1, 0, 4, 9], "Type1C");
    assert!(matches!(
        document.mmtype1_type1c_program(&malformed),
        Err(FontError::Invalid(_))
    ));

    let font_file = Value::Dict(mm_font_dict().insert(
        "FontDescriptor",
        descriptor(vec![(
            "FontFile",
            Value::Stream(Dict::new(), StreamData::Owned(b"%!PS-AdobeFont".to_vec())),
        )]),
    ));
    assert!(
        document
            .mmtype1_type1c_program(&font_file)
            .unwrap()
            .is_none(),
        "/FontFile snapshots are not CFF programs"
    );
}

#[test]
fn type0_font_reports_an_mmtype1_descendant_because_only_cidfonts_are_allowed() {
    let document = Document::new();
    let type0 = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("Type0".into()))
            .insert("BaseFont", Value::Name("MinionMM".into()))
            .insert("Encoding", Value::Name("Identity-H".into()))
            .insert(
                "DescendantFonts",
                Value::Array(Array::new().push(Value::Dict(mm_font_dict()))),
            ),
    );
    let error = document.type0_font(&type0).unwrap_err();
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("CIDFont")),
        "{error}"
    );
    assert_eq!(document.mmtype1_font(&type0).unwrap(), None);
}

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

fn push_segment(output: &mut Vec<u8>, prefix: &str, charstring: &[u8], suffix: &str) {
    let data = segment(charstring, 4);
    output.extend_from_slice(format!("{prefix}{} RD ", data.len()).as_bytes());
    output.extend_from_slice(&data);
    output.extend_from_slice(suffix.as_bytes());
}

/// A complete Type 1 program used as the MM instance snapshot (`/FontFile`),
/// per §9.6.2.3's requirement that embedded MM programs are ordinary Type 1.
fn snapshot_program() -> Vec<u8> {
    let mut private = Vec::new();
    private.extend_from_slice(
        b"dup /Private 8 dict dup begin
/RD {string currentfile exch readstring pop} executeonly def
/ND {noaccess def} executeonly def
/NP {noaccess put} executeonly def
/password 5839 def
/lenIV 4 def
/Subrs 2 array
",
    );
    push_segment(&mut private, "dup 0 ", b"\x8b\x0c", " NP\n");
    push_segment(&mut private, "dup 1 ", b"\x8b\x0e\x0c", " NP\n");
    private.extend_from_slice(b"def\n2 index /CharStrings 3 dict dup begin\n");
    push_segment(&mut private, "/.notdef ", b"\x8b\x8b\x0c", " ND\n");
    push_segment(&mut private, "/A ", b"\x8b\x0e\x0c", " ND\n");
    push_segment(&mut private, "/B ", b"\x8b\x10\x0c", " ND\n");
    private.extend_from_slice(b"end\nend\nreadonly put\nnoaccess put\n");
    private.extend_from_slice(b"dup /FontName get exch definefont pop\n");
    private.extend_from_slice(b"mark currentfile closefile\n");

    let mut eexec_plain = vec![0x5Au8; 4];
    eexec_plain.extend_from_slice(&private);

    let mut program = Vec::new();
    program.extend_from_slice(
        b"%!PS-AdobeFont-1.0: ExampleSans 001.000
11 dict begin
/FontName /ExampleSans def
/PaintType 0 def
/FontType 1 def
currentdict end
currentfile eexec
",
    );
    program.extend_from_slice(&encrypt(&eexec_plain, 55665));
    program.extend_from_slice(&[b'0'; 512]);
    program.extend_from_slice(b"\ncleartomark\n");
    program
}
