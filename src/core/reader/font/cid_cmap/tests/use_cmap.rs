//! CMap 流字典 `/UseCMap` 基座的单元测试（9.7.5.3，Table 120）。
//!
//! Table 120 的取值是「预定义 CMap 的名字，或一个含 CMap 的流」。这组测试
//! 覆盖两种形态、直接流与间接引用、链式嵌套、环、深度上限、悬空引用与损坏
//! 基座，并固定两条不变量：
//!
//! - 本地映射覆盖继承映射；
//! - 基座缺失、悬空、损坏、超深或成环时一律降级为「仅本地映射」，绝不把
//!   此前可读的字体变成报错，也绝不合成映射数据。

use crate::core::reader::font::Type0Font;
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

use super::super::use_cmap::MAX_USE_CMAP_DEPTH;
use super::{CidCMap, ToUnicodeMap};

/// Table 120 形态的 CMap 流字典：`/Type`、`/CMapName` 与 `/UseCMap`。
fn cmap_dict(use_cmap: Value) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("CMap".into()))
        .insert("CMapName", Value::Name("Test-UseCMap".into()))
        .insert("UseCMap", use_cmap)
}

/// 无 `/UseCMap` 的 CMap 流字典（基座链的末端）。
fn plain_cmap_dict() -> Dict {
    Dict::new().insert("Type", Value::Name("CMap".into()))
}

fn cmap_stream(dict: Dict, body: &str) -> Value {
    Value::Stream(dict, StreamData::Owned(body.as_bytes().to_vec()))
}

/// 声明 2 字节码空间的 CMap 流体；`local` 只给出与基座不同的映射。
fn cid_body(local: &str) -> String {
    format!("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n{local}")
}

/// 只含一条 `cidchar` 映射的 CMap 流体。
fn cid_char_body(code: &str, cid: u16) -> String {
    cid_body(&format!("1 begincidchar\n<{code}> {cid}\nendcidchar"))
}

/// 只有码空间、没有任何本地映射的 CMap 流体。
const CID_BODY_ONLY_CODESPACE: &str = "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange";

/// ToUnicode CMap 流体（`beginbfchar`）；不带码空间时继承基座全部映射。
fn bf_body(pairs: &[(&str, &str)]) -> String {
    let mut body = format!("{} beginbfchar\n", pairs.len());
    for (code, unicode) in pairs {
        body.push_str(&format!("<{code}> <{unicode}>\n"));
    }
    body.push_str("endbfchar");
    body
}

/// Type 0 字体字典：`/Encoding` 与可选 `/ToUnicode` 由调用方给出。
fn type0_font(encoding: Value, to_unicode: Option<Value>) -> Value {
    let descendant = Value::Dict(
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
            .insert("CIDToGIDMap", Value::Name("Identity".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(Dict::new().insert("Type", Value::Name("FontDescriptor".into()))),
            ),
    );
    let mut dict = Dict::new()
        .insert("Subtype", Value::Name("Type0".into()))
        .insert("BaseFont", Value::Name("ExampleComposite".into()))
        .insert("Encoding", encoding)
        .insert(
            "DescendantFonts",
            Value::Array(Array::new().push(descendant)),
        );
    if let Some(to_unicode) = to_unicode {
        dict = dict.insert("ToUnicode", to_unicode);
    }
    Value::Dict(dict)
}

/// 4 号对象是字体，其余对象按号预置；`Document::set_object` 取 `&self`。
fn document_with_font(font: Value, bases: &[(i64, Value)]) -> Document {
    let document = Document::new();
    document.set_object(Object::new(4, 0, font));
    for (number, value) in bases {
        document.set_object(Object::new(*number, 0, value.clone()));
    }
    document
}

/// `/Encoding` 为直接流且流字典带 `/UseCMap`（Table 120）的字体文档。
fn encoding_document(use_cmap: Value, local_body: &str, bases: &[(i64, Value)]) -> Document {
    let encoding = cmap_stream(cmap_dict(use_cmap), local_body);
    document_with_font(type0_font(encoding, None), bases)
}

/// `/Encoding` 为 `/Identity-H`、`/ToUnicode` 由调用方给出的字体文档。
fn to_unicode_document(to_unicode: Value, bases: &[(i64, Value)]) -> Document {
    document_with_font(
        type0_font(Value::Name("Identity-H".into()), Some(to_unicode)),
        bases,
    )
}

fn font(document: &Document) -> Type0Font {
    document
        .type0_font(&Value::Ref(4, 0))
        .expect("基座不可解析时字体仍应可读")
        .expect("应为 Type 0 字体")
}

fn cids(document: &Document, input: &[u8]) -> Vec<u16> {
    font(document)
        .decode_codes(input)
        .expect("字符码应可解码")
        .into_iter()
        .map(|code| code.cid)
        .collect()
}

fn unicodes(document: &Document, input: &[u8]) -> Vec<Option<String>> {
    font(document)
        .decode_codes(input)
        .expect("字符码应可解码")
        .into_iter()
        .map(|code| code.unicode)
        .collect()
}

#[test]
fn stream_dict_use_cmap_supplies_inherited_cid_mappings() {
    // 基座（20 号流）独有 <0041>；本地只声明 <0043>（Table 120：只给差异）。
    let bases = [(
        20,
        cmap_stream(plain_cmap_dict(), &cid_char_body("0041", 7)),
    )];
    let document = encoding_document(Value::Ref(20, 0), &cid_char_body("0043", 9), &bases);
    assert_eq!(
        cids(&document, &[0, 0x41, 0, 0x43]),
        vec![7, 9],
        "只在基座里声明的码必须取到继承 CID"
    );
}

#[test]
fn direct_stream_use_cmap_base_is_parsed_without_an_object_number() {
    // `/UseCMap` 也可以是直接内联流（非间接引用）。
    let base = cmap_stream(plain_cmap_dict(), &cid_char_body("0041", 7));
    let document = encoding_document(base, &cid_char_body("0043", 9), &[]);
    assert_eq!(cids(&document, &[0, 0x41, 0, 0x43]), vec![7, 9]);
}

#[test]
fn name_use_cmap_base_resolves_through_the_predefined_roster() {
    // 名字形态走既有名册（Identity-H/V + predefined_cmaps），不引入新数据。
    let document = encoding_document(
        Value::Name("Identity-V".into()),
        CID_BODY_ONLY_CODESPACE,
        &[],
    );
    assert!(font(&document).is_vertical(), "/WMode 应从字典基座继承");
    assert_eq!(cids(&document, &[0, 0x41]), vec![0x41]);
}

#[test]
fn local_mappings_override_the_use_cmap_base() {
    let bases = [(
        20,
        cmap_stream(
            plain_cmap_dict(),
            &cid_body("2 begincidchar\n<0041> 1\n<0042> 2\nendcidchar"),
        ),
    )];
    let document = encoding_document(Value::Ref(20, 0), &cid_char_body("0041", 9), &bases);
    assert_eq!(
        cids(&document, &[0, 0x41, 0, 0x42]),
        vec![9, 2],
        "同码冲突时本地映射优先，未冲突的基座映射保留"
    );
}

#[test]
fn use_cmap_stream_chain_is_resolved_transitively() {
    // 本地 → 20 → 21；三段的映射都必须可见。
    let bases = [
        (
            20,
            cmap_stream(cmap_dict(Value::Ref(21, 0)), &cid_char_body("0043", 3)),
        ),
        (
            21,
            cmap_stream(plain_cmap_dict(), &cid_char_body("0044", 4)),
        ),
    ];
    let document = encoding_document(Value::Ref(20, 0), &cid_char_body("0041", 1), &bases);
    assert_eq!(
        cids(&document, &[0, 0x41, 0, 0x43, 0, 0x44]),
        vec![1, 3, 4],
        "多层流字典基座应逐层继承"
    );
}

#[test]
fn use_cmap_cycle_terminates_and_keeps_reachable_mappings() {
    // 20 ↔ 21 成环：链必须终止，且环上可达的映射仍然可用。
    let bases = [
        (
            20,
            cmap_stream(cmap_dict(Value::Ref(21, 0)), &cid_char_body("0041", 1)),
        ),
        (
            21,
            cmap_stream(cmap_dict(Value::Ref(20, 0)), &cid_char_body("0042", 2)),
        ),
    ];
    let document = encoding_document(Value::Ref(20, 0), &cid_char_body("0043", 3), &bases);
    assert_eq!(cids(&document, &[0, 0x41, 0, 0x42, 0, 0x43]), vec![1, 2, 3]);
}

#[test]
fn use_cmap_self_reference_degrades_to_the_referencing_local_mappings() {
    // 20 号自引用：进入一次后即被判为重访，20 自身的映射仍作为基座可见。
    let bases = [(
        20,
        cmap_stream(cmap_dict(Value::Ref(20, 0)), &cid_char_body("0041", 1)),
    )];
    let document = encoding_document(Value::Ref(20, 0), &cid_char_body("0043", 3), &bases);
    assert_eq!(cids(&document, &[0, 0x41, 0, 0x43]), vec![1, 3]);
}

/// 构造 `length` 层基座链：100 号被本地 `/Encoding` 引用，每层 `/UseCMap`
/// 指向下一层，最深层给出 `<0042>` 的映射。
fn chained_bases(length: usize) -> Vec<(i64, Value)> {
    (0..length)
        .map(|index| {
            let number = 100 + index as i64;
            let last = index + 1 == length;
            let dict = if last {
                plain_cmap_dict()
            } else {
                cmap_dict(Value::Ref(number + 1, 0))
            };
            let body = if last {
                cid_char_body("0042", 7)
            } else {
                CID_BODY_ONLY_CODESPACE.to_owned()
            };
            (number, cmap_stream(dict, &body))
        })
        .collect()
}

#[test]
fn use_cmap_chain_stops_at_the_depth_cap() {
    // 恰好 MAX 层：最深层的映射仍可见。
    let bases = chained_bases(MAX_USE_CMAP_DEPTH);
    let document = encoding_document(Value::Ref(100, 0), &cid_char_body("0041", 5), &bases);
    assert_eq!(cids(&document, &[0, 0x41, 0, 0x42]), vec![5, 7]);

    // 多一层：只有最远的那段基座被裁掉，本地映射完整可用（不报错）。
    let bases = chained_bases(MAX_USE_CMAP_DEPTH + 1);
    let document = encoding_document(Value::Ref(100, 0), &cid_char_body("0041", 5), &bases);
    assert_eq!(
        cids(&document, &[0, 0x41, 0, 0x42]),
        vec![5, 0],
        "深度上限之外不得继续展开"
    );
}

#[test]
fn dangling_use_cmap_reference_keeps_local_mappings_usable() {
    let document = encoding_document(Value::Ref(999, 0), &cid_char_body("0041", 5), &[]);
    assert_eq!(cids(&document, &[0, 0x41]), vec![5]);
}

#[test]
fn wrong_typed_and_malformed_use_cmap_bases_degrade() {
    // Table 120 只允许 name 或 stream；其余形态按「基座不可解析」降级。
    for use_cmap in [
        Value::Integer(7),
        Value::String("1 begincodespacerange".into()),
        Value::Array(Array::new().push(Value::Integer(1))),
    ] {
        let document = encoding_document(use_cmap.clone(), &cid_char_body("0041", 5), &[]);
        assert_eq!(
            cids(&document, &[0, 0x41]),
            vec![5],
            "Table 120 之外的 /UseCMap 形态应按无基座降级：{use_cmap:?}"
        );
    }

    // 基座流存在但内容不是 CMap：同样只退回本地映射。
    let bases = [(20, cmap_stream(plain_cmap_dict(), "this is not a CMap"))];
    let document = encoding_document(Value::Ref(20, 0), &cid_char_body("0041", 5), &bases);
    assert_eq!(cids(&document, &[0, 0x41]), vec![5]);
}

#[test]
fn dict_use_cmap_wins_over_an_unresolvable_in_stream_base_name() {
    // 规则 (a)：两者指向同一 CMap；字典形态唯一能承载内嵌基座，故优先使用。
    let bases = [(
        20,
        cmap_stream(plain_cmap_dict(), &cid_char_body("0041", 7)),
    )];
    let local = cid_body("/NoSuchRegistryBase usecmap 1 begincidchar\n<0043> 9\nendcidchar");
    let document = encoding_document(Value::Ref(20, 0), &local, &bases);
    assert_eq!(
        cids(&document, &[0, 0x41, 0, 0x43]),
        vec![7, 9],
        "流内未知基座名不应遮蔽可解析的 /UseCMap 流"
    );
}

#[test]
fn dictionary_base_skips_the_in_stream_cid_resolver() {
    let base = CidCMap::parse(
        b"1 begincodespacerange <0000> <FFFF> endcodespacerange 1 begincidchar <0041> 7 endcidchar",
    )
    .unwrap();
    let source = b"/Identity-H usecmap 1 begincodespacerange <0000> <FFFF> endcodespacerange 1 begincidchar <0043> 9 endcidchar";
    let cmap = CidCMap::parse_with_base(source, Some(base), |_| {
        panic!("字典基座存在时不应再查询内置名册")
    })
    .unwrap();
    assert_eq!(
        cmap.decode(&[0, 0x41, 0, 0x43]).unwrap(),
        [(vec![0, 0x41], 7), (vec![0, 0x43], 9)]
    );
}

#[test]
fn to_unicode_stream_use_cmap_inherits_and_overrides() {
    let to_unicode = cmap_stream(cmap_dict(Value::Ref(31, 0)), &bf_body(&[("0041", "0050")]));
    let bases = [
        (30, to_unicode),
        (
            31,
            cmap_stream(
                plain_cmap_dict(),
                &bf_body(&[("0041", "004D"), ("0042", "004E")]),
            ),
        ),
    ];
    let document = to_unicode_document(Value::Ref(30, 0), &bases);
    assert_eq!(
        unicodes(&document, &[0, 0x41, 0, 0x42, 0, 0x43]),
        vec![Some("P".into()), Some("N".into()), None],
        "基座映射补齐、本地覆盖，未声明的码不得凭空合成"
    );
}

#[test]
fn to_unicode_name_use_cmap_uses_the_identity_roster() {
    // 名字形态只允许内置名册里的基座（此处 Adobe-Identity-UCS），不发明数据。
    let body = cid_body(&bf_body(&[("0041", "0058")]));
    let to_unicode = cmap_stream(cmap_dict(Value::Name("Adobe-Identity-UCS".into())), &body);
    let document = to_unicode_document(to_unicode, &[]);
    assert_eq!(
        unicodes(&document, &[0, 0x41, 0, 0x42]),
        vec![Some("X".into()), Some("B".into())],
        "未在本地映射的 2 字节码应按恒等基座取值"
    );
}

#[test]
fn to_unicode_unknown_name_base_degrades_to_local_mappings() {
    let to_unicode = cmap_stream(
        cmap_dict(Value::Name("Adobe-SomeThirdParty-UCS".into())),
        &bf_body(&[("0041", "0058")]),
    );
    let document = to_unicode_document(to_unicode, &[]);
    assert_eq!(
        unicodes(&document, &[0, 0x41, 0, 0x42]),
        vec![Some("X".into()), None],
        "名册外的基座名不得合成 Unicode"
    );
}

#[test]
fn to_unicode_cycle_and_dangling_bases_stay_readable() {
    // 自环：链终止后本流映射仍完整可用。
    let looping = cmap_stream(cmap_dict(Value::Ref(40, 0)), &bf_body(&[("0041", "0058")]));
    let document = to_unicode_document(Value::Ref(40, 0), &[(40, looping)]);
    assert_eq!(
        unicodes(&document, &[0, 0x41, 0, 0x42]),
        vec![Some("X".into()), None]
    );

    // 悬空引用：按无基座处理。
    let dangling = cmap_stream(cmap_dict(Value::Ref(999, 0)), &bf_body(&[("0041", "0058")]));
    let document = to_unicode_document(dangling, &[]);
    assert_eq!(
        unicodes(&document, &[0, 0x41]),
        vec![Some("X".into())],
        "坏基座不得让整个 /ToUnicode 失效"
    );
}

#[test]
fn to_unicode_dictionary_base_skips_the_in_stream_resolver() {
    let base = ToUnicodeMap::parse(b"1 beginbfchar <0041> <004D> endbfchar").unwrap();
    let map = ToUnicodeMap::parse_with_base(
        b"/Adobe-Identity-UCS usecmap 1 beginbfchar <0042> <0058> endbfchar",
        Some(base),
        |_| panic!("字典基座存在时不应再查询内置名册"),
    )
    .expect("字典基座应可合并");
    assert_eq!(map.unicode_for(&[0, 0x41]), Some("M"));
    assert_eq!(map.unicode_for(&[0, 0x42]), Some("X"));
    assert_eq!(
        map.resolve_unicode(&[0, 0x43]),
        None,
        "字典基座不是内置恒等基座时不得按码值合成"
    );
}

#[test]
fn simple_font_to_unicode_inherits_a_use_cmap_stream_base() {
    // 简单字体同样只走这一条 /UseCMap 通道；1 字节码。
    let font_dict = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSimple".into()))
            .insert("ToUnicode", Value::Ref(40, 0)),
    );
    let to_unicode = cmap_stream(cmap_dict(Value::Ref(41, 0)), &bf_body(&[("41", "0058")]));
    let base = cmap_stream(
        plain_cmap_dict(),
        &bf_body(&[("41", "0041"), ("42", "0042")]),
    );
    let document = document_with_font(font_dict, &[(40, to_unicode), (41, base)]);
    let map = document
        .simple_font_to_unicode(&Value::Ref(4, 0))
        .expect("简单字体 /ToUnicode 应可解析");
    assert_eq!(map.unicode_for(0x41), Some("X"), "本地映射覆盖基座");
    assert_eq!(map.unicode_for(0x42), Some("B"), "基座映射应被继承");
    assert_eq!(map.unicode_for(0x43), None, "未声明的码不得合成");
}
