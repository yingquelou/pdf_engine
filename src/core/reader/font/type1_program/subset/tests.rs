use super::super::super::FontError;
use super::*;

/// A minimal `.notdef` or `B`: `0 0 hsbw endchar`.
const FLAT_CHARSTRING: &[u8] = &[139, 139, 13, 14];
/// `A`: `100 10 hsbw 20 30 rmoveto 40 0 rlineto 0 callsubr closepath
/// endchar`, where subroutine 0 returns immediately.
const A_CHARSTRING: &[u8] = &[239, 149, 13, 159, 169, 21, 179, 139, 5, 139, 10, 9, 14];
/// A `seac` composite: `0 0 hsbw 0 10 20 66 67 seac`, whose components are
/// the StandardEncoding glyphs `B` (base) and `C` (accent).
const SEAC_CHARSTRING: &[u8] = &[139, 139, 13, 139, 149, 159, 205, 206, 12, 6];
/// A nested `seac` composite: base `D` (code 68) and accent `E` (69).
const NESTED_SEAC_CHARSTRING: &[u8] = &[139, 139, 13, 139, 149, 159, 207, 208, 12, 6];

/// Encrypt bytes at the given Type 1 key (4330 for segments, 55665 for
/// `eexec`). Copied locally because the parser's test helper is private to
/// its own test module.
fn encrypt_with_key(plaintext: &[u8], key: u16) -> Vec<u8> {
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
    encrypt_with_key(&padded, 4330)
}

fn push_segment(output: &mut Vec<u8>, prefix: &str, charstring: &[u8], len_iv: i32, suffix: &str) {
    let data = segment(charstring, len_iv);
    output.extend_from_slice(format!("{prefix}{} RD ", data.len()).as_bytes());
    output.extend_from_slice(&data);
    output.extend_from_slice(suffix.as_bytes());
}

/// Build a complete Type 1 program with two subrs, the given CharStrings,
/// and a `lenIV` of `len_iv` (`-1` leaves segments unencrypted).
fn program(len_iv: i32, charstrings: &[(&str, &[u8])]) -> Vec<u8> {
    let mut private = Vec::new();
    private.extend_from_slice(b"dup /Private 9 dict dup begin\n");
    private.extend_from_slice(b"/RD {string currentfile exch readstring pop} executeonly def\n");
    private.extend_from_slice(b"/ND {noaccess def} executeonly def\n");
    private.extend_from_slice(b"/NP {noaccess put} executeonly def\n");
    private.extend_from_slice(format!("/lenIV {len_iv} def\n").as_bytes());
    private.extend_from_slice(b"/BlueValues [-20 0 500 520] def\n");
    private.extend_from_slice(b"/Subrs 2 array\n");
    push_segment(&mut private, "dup 0 ", &[139, 11], len_iv, " NP\n");
    push_segment(&mut private, "dup 1 ", &[139, 11], len_iv, " NP\n");
    private.extend_from_slice(b"def\n");
    private.extend_from_slice(
        format!(
            "2 index /CharStrings {} dict dup begin\n",
            charstrings.len()
        )
        .as_bytes(),
    );
    for (name, charstring) in charstrings {
        push_segment(
            &mut private,
            &format!("/{name} "),
            charstring,
            len_iv,
            " ND\n",
        );
    }
    private.extend_from_slice(b"end\nend\nreadonly put\nnoaccess put\n");
    private.extend_from_slice(b"dup /FontName get exch definefont pop\n");
    private.extend_from_slice(b"mark currentfile closefile\n");

    let mut plaintext = vec![0x5Au8; 4];
    plaintext.extend_from_slice(&private);
    let eexec = encrypt_with_key(&plaintext, 55665);

    let mut program = Vec::new();
    program.extend_from_slice(b"%!PS-AdobeFont-1.0: SubsetSans 001.000\n");
    program.extend_from_slice(b"11 dict begin\n");
    program.extend_from_slice(b"/FontInfo 10 dict dup begin\n");
    program.extend_from_slice(b"/FamilyName (Subset Sans) def\n");
    program.extend_from_slice(b"/FullName (Subset Sans Regular) def\n");
    program.extend_from_slice(b"end readonly def\n");
    program.extend_from_slice(b"/FontName /SubsetSans def\n");
    program.extend_from_slice(b"/FontType 1 def\n");
    program.extend_from_slice(b"/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n");
    program.extend_from_slice(b"/FontBBox [0 -200 1000 900] readonly def\n");
    program.extend_from_slice(b"currentdict end\ncurrentfile eexec\r\n");
    program.extend_from_slice(&eexec);
    program.extend_from_slice(&[b'0'; ZERO_GUARD]);
    program.extend_from_slice(CLEARTOMARK);
    program
}

fn sample(len_iv: i32) -> Vec<u8> {
    program(
        len_iv,
        &[
            (".notdef", FLAT_CHARSTRING),
            ("A", A_CHARSTRING),
            ("B", FLAT_CHARSTRING),
        ],
    )
}

#[test]
fn subset_keeps_notdef_and_requested_glyph() {
    let data = sample(4);
    let original = Type1FontProgram::parse(&data).unwrap();
    let subset = subset_type1(&data, &["A"]).unwrap();
    let parsed = Type1FontProgram::parse(&subset).unwrap();

    let names: Vec<&str> = parsed.charstrings.keys().map(String::as_str).collect();
    assert_eq!(names, [".notdef", "A"]);
    assert!(!parsed.charstrings.contains_key("B"));
    assert_eq!(parsed.charstrings.get("A"), original.charstrings.get("A"));
    assert_eq!(
        parsed.charstrings.get(".notdef"),
        original.charstrings.get(".notdef")
    );
    assert_eq!(parsed.subrs, original.subrs);
    assert_eq!(parsed.len_iv, 4);
    assert_eq!(parsed.glyph("A").unwrap(), original.glyph("A").unwrap());
}

#[test]
fn subset_preserves_cleartext_header() {
    let data = sample(4);
    let original = Type1FontProgram::parse(&data).unwrap();
    let subset = subset_type1(&data, &["A"]).unwrap();
    let parsed = Type1FontProgram::parse(&subset).unwrap();

    assert_eq!(parsed.header.font_name.as_deref(), Some("SubsetSans"));
    assert_eq!(parsed.header.family_name.as_deref(), Some("Subset Sans"));
    assert_eq!(parsed.header.font_bbox, Some([0.0, -200.0, 1000.0, 900.0]));
    assert_eq!(parsed.header, original.header);

    let eexec_end = data
        .windows(b"eexec".len())
        .position(|window| window == b"eexec")
        .unwrap()
        + b"eexec".len();
    assert_eq!(&subset[..eexec_end], &data[..eexec_end]);
}

#[test]
fn subset_preserves_private_dictionary_entries() {
    let data = sample(4);
    let subset = subset_type1(&data, &["A"]).unwrap();
    let layout = locate(&subset).unwrap();
    let plaintext = String::from_utf8_lossy(&layout.plaintext);

    assert!(plaintext.contains("/BlueValues [-20 0 500 520] def"));
    assert!(plaintext.contains("/Subrs 2 array"));
    assert!(plaintext.contains("/lenIV 4 def"));
    assert!(plaintext.contains("/A "));
    assert!(!plaintext.contains("/B "));
}

#[test]
fn rejects_unknown_glyph() {
    let data = sample(4);
    let error = subset_type1(&data, &["Z"]).unwrap_err();
    assert!(matches!(&error, FontError::Invalid(_)));
    assert!(error.to_string().contains('Z'), "{error}");
}

#[test]
fn rejects_program_with_empty_charstrings() {
    let data = program(4, &[]);
    let error = subset_type1(&data, &["A"]).unwrap_err();
    assert!(matches!(&error, FontError::Invalid(_)));
    assert!(error.to_string().contains("CharStrings"), "{error}");
}

#[test]
fn empty_request_keeps_only_notdef() {
    let data = sample(4);
    let subset = subset_type1(&data, &[]).unwrap();
    let parsed = Type1FontProgram::parse(&subset).unwrap();

    let names: Vec<&str> = parsed.charstrings.keys().map(String::as_str).collect();
    assert_eq!(names, [".notdef"]);
    assert!(parsed.glyph(".notdef").unwrap().is_some());
}

#[test]
fn subsets_seac_component_closure() {
    let data = program(
        4,
        &[
            (".notdef", FLAT_CHARSTRING),
            ("A", SEAC_CHARSTRING),
            ("B", FLAT_CHARSTRING),
            ("C", FLAT_CHARSTRING),
        ],
    );
    let original = Type1FontProgram::parse(&data).unwrap();
    let subset = subset_type1(&data, &["A"]).unwrap();
    let parsed = Type1FontProgram::parse(&subset).unwrap();

    let names: Vec<&str> = parsed.charstrings.keys().map(String::as_str).collect();
    assert_eq!(names, [".notdef", "A", "B", "C"]);
    for name in [".notdef", "A", "B", "C"] {
        assert_eq!(
            parsed.charstrings.get(name),
            original.charstrings.get(name),
            "charstring for `{name}` changed"
        );
    }
}

#[test]
fn rejects_seac_component_missing_from_charstrings() {
    let data = program(
        4,
        &[
            (".notdef", FLAT_CHARSTRING),
            // The base `B` (code 66) is absent; the accent `C` is present.
            ("A", SEAC_CHARSTRING),
            ("C", FLAT_CHARSTRING),
        ],
    );
    let error = subset_type1(&data, &["A"]).unwrap_err();
    assert!(matches!(&error, FontError::Invalid(_)));
    assert!(error.to_string().contains('B'), "{error}");
    assert!(error.to_string().contains("seac"), "{error}");
}

#[test]
fn dropped_seac_glyph_does_not_require_components() {
    let data = program(
        4,
        &[
            (".notdef", FLAT_CHARSTRING),
            // `A` is dropped; its base `B` is absent from /CharStrings.
            ("A", SEAC_CHARSTRING),
            ("D", FLAT_CHARSTRING),
        ],
    );
    let subset = subset_type1(&data, &["D"]).unwrap();
    let parsed = Type1FontProgram::parse(&subset).unwrap();

    let names: Vec<&str> = parsed.charstrings.keys().map(String::as_str).collect();
    assert_eq!(names, [".notdef", "D"]);
}

#[test]
fn subsets_nested_seac_component_closure() {
    let data = program(
        4,
        &[
            (".notdef", FLAT_CHARSTRING),
            ("A", SEAC_CHARSTRING),
            ("B", NESTED_SEAC_CHARSTRING),
            ("C", FLAT_CHARSTRING),
            ("D", FLAT_CHARSTRING),
            ("E", FLAT_CHARSTRING),
        ],
    );
    let original = Type1FontProgram::parse(&data).unwrap();
    let subset = subset_type1(&data, &["A"]).unwrap();
    let parsed = Type1FontProgram::parse(&subset).unwrap();

    let names: Vec<&str> = parsed.charstrings.keys().map(String::as_str).collect();
    assert_eq!(names, [".notdef", "A", "B", "C", "D", "E"]);
    for name in [".notdef", "A", "B", "C", "D", "E"] {
        assert_eq!(
            parsed.charstrings.get(name),
            original.charstrings.get(name),
            "charstring for `{name}` changed"
        );
    }
}

#[test]
fn subset_output_is_smaller_than_source() {
    let data = sample(4);
    let subset = subset_type1(&data, &["A"]).unwrap();
    assert!(
        subset.len() < data.len(),
        "subset length {} is not smaller than source length {}",
        subset.len(),
        data.len()
    );
}

#[test]
fn subsets_unencrypted_len_iv_minus_one() {
    let data = sample(-1);
    let original = Type1FontProgram::parse(&data).unwrap();
    assert_eq!(original.len_iv, -1);

    let subset = subset_type1(&data, &["A"]).unwrap();
    let parsed = Type1FontProgram::parse(&subset).unwrap();
    assert_eq!(parsed.len_iv, -1);
    assert_eq!(parsed.glyph("A").unwrap(), original.glyph("A").unwrap());
    assert!(!parsed.charstrings.contains_key("B"));
    assert_eq!(parsed.subrs, original.subrs);
}

#[test]
fn malformed_inputs_return_errors_without_panicking() {
    let data = sample(4);
    let guard_start = data
        .windows(ZERO_GUARD)
        .position(|window| window.iter().all(|&byte| byte == b'0'))
        .unwrap();
    let mut unguarded = data.clone();
    unguarded.truncate(guard_start);

    let cases = [
        Vec::new(),
        b"%!PS-AdobeFont-1.0: Broken\n".to_vec(),
        b"currentfile eexec\n".to_vec(),
        b"currentfile eexec\n\x00\x01\x02".to_vec(),
        unguarded,
        data[..data.len() / 2].to_vec(),
        data[..data.len().saturating_sub(600)].to_vec(),
    ];
    for case in cases {
        match subset_type1(&case, &["A"]) {
            Err(FontError::Invalid(message)) => assert!(!message.is_empty()),
            Err(error) => panic!("expected FontError::Invalid, got {error:?}"),
            Ok(_) => panic!("malformed input was accepted"),
        }
    }
}
