//! 7.4.1 / Table 5 集成：`/Filter` 与 `/DecodeParms` 写成**间接引用**的流对象，
//! 走「写出 → 重新解析 → 解码取内容」全链路。
//!
//! 覆盖四种合法形态（引用→Name、引用→Name 数组、引用→参数字典、引用→参数
//! 字典数组且数组元素本身也是引用）与两种失败形态（引用指向字典、引用成环）：
//! 合法形态的解码结果与 Table 5 例子的直接对象写法一致；失败形态保留原值、
//! 产出 `7.4.1` 诊断且**原始字节不丢**（解码明确失败）。
//!
//! 图像读端（切片 149）复用同一规范化器：`/Filter [ /AHx 2 0 R ]` 形态的
//! 图像 XObject 经写出→重解析后，元数据、解码数据与 `samples()` 与全直接
//! 写法一致，见 `image_xobject_with_indirect_filter_array_roundtrips_like_inline_form`。

use super::*;

/// 全白 + 两行 4 白 4 黑的 Group 4 条带（`filters` 主题同一份样本）。
const CCITT_DATA: &[u8] = &[0x9B, 0x78, 0x00, 0x80, 0x08];

/// 上一条带的期望解码结果（8 列 × 3 行，位展平为 3 字节）。
const CCITT_PLAIN: [u8; 3] = [0xFF, 0xF0, 0xF0];

/// 一个声明给定条目、正文为 [`CCITT_DATA`] 的流对象本体。
fn ccitt_stream(num: i64, entries: &str) -> Vec<u8> {
    let mut body = format!(
        "{num} 0 obj\n<< {entries} /Length {} >>\nstream\n",
        CCITT_DATA.len()
    )
    .into_bytes();
    body.extend_from_slice(CCITT_DATA);
    body.extend_from_slice(b"\nendstream\nendobj\n");
    body
}

/// 按顺序写出对象并生成经典 xref 表 + trailer（偏移实测，`/Root 1 0 R`）。
fn assemble(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut buf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for body in objects {
        offsets.push(buf.len());
        buf.extend_from_slice(body);
    }
    let xref_offset = buf.len();
    buf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    buf
}

/// 混合六种形态的 PDF：
/// - 对象 4：`/Filter 2 0 R`（指向 Name）+ `/DecodeParms 3 0 R`（指向字典）；
/// - 对象 6：`/Filter 5 0 R`（指向 Name 数组）；
/// - 对象 8：`/DecodeParms 7 0 R`（指向字典数组，元素 `[ 3 0 R ]` 仍是引用）；
/// - 对象 10：`/Filter 9 0 R`（指向字典——非法形态）；
/// - 对象 12：`/Filter 11 0 R`（对象 11 自身即 `11 0 R`——环）。
fn indirect_filter_pdf() -> Vec<u8> {
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        b"2 0 obj\n/CCITTFaxDecode\nendobj\n".to_vec(),
        b"3 0 obj\n<< /K -1 /Columns 8 /Rows 3 >>\nendobj\n".to_vec(),
        ccitt_stream(4, "/Filter 2 0 R /DecodeParms 3 0 R"),
        b"5 0 obj\n[ /CCITTFaxDecode ]\nendobj\n".to_vec(),
        ccitt_stream(6, "/Filter 5 0 R /DecodeParms 3 0 R"),
        b"7 0 obj\n[ 3 0 R ]\nendobj\n".to_vec(),
        ccitt_stream(8, "/Filter /CCITTFaxDecode /DecodeParms 7 0 R"),
        b"9 0 obj\n<< /Columns 8 >>\nendobj\n".to_vec(),
        ccitt_stream(10, "/Filter 9 0 R"),
        b"11 0 obj\n11 0 R\nendobj\n".to_vec(),
        ccitt_stream(12, "/Filter 11 0 R"),
    ];
    assemble(&objects)
}

/// 取指定对象的流值。
fn stream_value(doc: &Document, num: i64) -> Value {
    doc.get_object(num)
        .unwrap_or_else(|error| panic!("get_object({num}) 失败：{error}"))
        .unwrap_or_else(|| panic!("对象 {num} 缺失"))
        .value
}

/// 断言三个合法形态的流都能解出同一份明文。
fn assert_valid_forms_decode(doc: &Document, stage: &str) {
    for num in [4, 6, 8] {
        let value = stream_value(doc, num);
        assert!(
            matches!(value, Value::Stream(_, _)),
            "{stage}: 对象 {num} 不是流"
        );
        assert_eq!(
            doc.decode_stream_bytes(&value)
                .unwrap_or_else(|error| panic!("{stage}: 对象 {num} 解码失败：{error}")),
            CCITT_PLAIN,
            "{stage}: 对象 {num} 的间接引用形态解码结果不符"
        );
    }
    assert!(
        !doc.warnings()
            .iter()
            .any(|warning| warning.contains("7.4.1")),
        "{stage}: 合法形态不应产生诊断：{:?}",
        doc.warnings()
    );
}

/// 解析（写出前）与重解析（写出后）都走一遍全链路。
#[test]
fn indirect_filter_and_parms_forms_roundtrip() {
    let bytes = indirect_filter_pdf();
    let doc = parse_buf(&bytes);
    assert_valid_forms_decode(&doc, "解析后");

    let mut out = Vec::new();
    core::writer::DocumentWriter::new()
        .build(&doc, &mut out)
        .expect("写出应成功（间接引用形态的流对象不得被丢弃）");
    let reparsed = parse_buf(&out);
    assert_valid_forms_decode(&reparsed, "写出重读后");
}

/// 失败形态（引用指向字典）：保留原值 + 诊断 + 解码失败，原始字节不丢。
#[test]
fn indirect_filter_to_illegal_target_is_diagnosed_and_preserved() {
    for (stage, doc) in [
        ("解析后", parse_buf(&indirect_filter_pdf())),
        ("写出重读后", {
            let doc = parse_buf(&indirect_filter_pdf());
            let mut out = Vec::new();
            core::writer::DocumentWriter::new()
                .build(&doc, &mut out)
                .expect("违例文档也要能写出");
            parse_buf(&out)
        }),
    ] {
        let value = stream_value(&doc, 10);
        let error = match doc.decode_stream_bytes(&value) {
            Err(error) => error,
            Ok(bytes) => panic!("{stage}: 引用指向字典必须解码失败，却得到 {bytes:?}"),
        };
        assert!(error.contains("/Filter"), "{stage}: got {error}");
        assert_eq!(
            doc.stream_bytes(&value).as_deref(),
            Some(CCITT_DATA),
            "{stage}: 原始流字节不得丢失"
        );
        assert!(
            doc.warnings()
                .iter()
                .any(|warning| warning.contains("7.4.1") && warning.contains("resolves to dict")),
            "{stage}: 诊断缺失：{:?}",
            doc.warnings()
        );
        // 原值仍是引用（未被静默改写）。
        assert_eq!(
            value.as_dict().expect("stream dict").get("Filter"),
            Some(&Value::Ref(9, 0)),
            "{stage}: 非法形态必须保留原值"
        );
    }
}

/// 失败形态（引用成环）：诊断 + 解码失败，且绝不无限递归。
#[test]
fn cyclic_indirect_filter_is_diagnosed() {
    let doc = parse_buf(&indirect_filter_pdf());
    let value = stream_value(&doc, 12);
    assert!(
        doc.decode_stream_bytes(&value).is_err(),
        "环引用不得解码成功"
    );
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| warning.contains("7.4.1") && warning.contains("cyclic")),
        "环诊断缺失：{:?}",
        doc.warnings()
    );
    assert_eq!(doc.stream_bytes(&value).as_deref(), Some(CCITT_DATA));
}

/// Table 5 的 shall 个数核对同样落到 `warnings`（多过滤器配单参数字典）。
#[test]
fn decode_parms_count_mismatch_is_reported_while_parsing_stream() {
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        b"2 0 obj\n<< /K -1 /Columns 8 /Rows 3 >>\nendobj\n".to_vec(),
        ccitt_stream(
            3,
            "/Filter [ /ASCIIHexDecode /CCITTFaxDecode ] /DecodeParms 2 0 R",
        ),
    ];
    let doc = parse_buf(&assemble(&objects));
    let value = stream_value(&doc, 3);
    assert!(doc.decode_stream_bytes(&value).is_err());
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| warning.contains("one entry for each filter")),
        "shall 诊断缺失：{:?}",
        doc.warnings()
    );
}

/// 2×1 DeviceGray 8-bit 图像流：数据为 ASCIIHex→RunLength 两级编码
/// （`0100FF80>` → 字节 01 00 FF 80 → 字面段 [00 FF] + EOD），
/// 期望样本 [0.0, 1.0]。`/Filter` 取调用方给定的字面形态。
fn image_stream(num: i64, filter: &str) -> Vec<u8> {
    let data: &[u8] = b"0100FF80>";
    let mut body = format!(
        "{num} 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 1 \
         /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter {filter} /Length {} >>\nstream\n",
        data.len()
    )
    .into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream\nendobj\n");
    body
}

/// 图像读端复用 7.4.1 规范化器（切片 149）：`/Filter [ /AHx 2 0 R ]` 这类
/// **数组元素为间接引用**的图像 XObject，写出→重解析后，规范名列表、解码
/// 数据与 `samples()` 检索与全直接写法完全一致；引用指向缺失对象则诊断 +
/// 明确失败（不静默误解码）。
#[test]
fn image_xobject_with_indirect_filter_array_roundtrips_like_inline_form() {
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        b"2 0 obj\n/RunLengthDecode\nendobj\n".to_vec(),
        image_stream(3, "[ /AHx 2 0 R ]"), // 缺口形态：数组元素是引用
        image_stream(4, "[ /AHx /RunLengthDecode ]"), // 全直接基准
        image_stream(5, "[ /AHx 99 0 R ]"), // 引用指向缺失对象
    ];
    let bytes = assemble(&objects);
    let stages = [
        ("解析后", parse_buf(&bytes)),
        ("写出重读后", {
            let doc = parse_buf(&bytes);
            let mut out = Vec::new();
            core::writer::DocumentWriter::new()
                .build(&doc, &mut out)
                .expect("间接引用 /Filter 数组的图像流不得被写出丢弃");
            parse_buf(&out)
        }),
    ];
    for (stage, doc) in stages {
        let indirect = doc.image(&stream_value(&doc, 3)).unwrap_or_else(|error| {
            panic!("{stage}: 元素引用形态的 /Filter 数组应可解析：{error}")
        });
        let inline = doc
            .image(&stream_value(&doc, 4))
            .expect("全直接基准应可解析");
        assert_eq!(
            indirect.filters,
            vec!["ASCIIHexDecode".to_string(), "RunLengthDecode".to_string()],
            "{stage}: 规范名列表"
        );
        assert_eq!(indirect.filters, inline.filters, "{stage}: 过滤器列表");
        assert_eq!(indirect.width, 2, "{stage}: 元数据 /Width");
        assert_eq!(indirect.height, 1, "{stage}: 元数据 /Height");
        assert_eq!(indirect.data, inline.data, "{stage}: 解码数据");
        let samples = indirect
            .samples()
            .unwrap_or_else(|error| panic!("{stage}: 样本读取失败：{error}"));
        assert_eq!(samples.values, vec![0.0, 1.0], "{stage}: 像素样本");
        assert_eq!(
            samples.values,
            inline.samples().expect("基准样本").values,
            "{stage}: 与全直接写法样本不一致"
        );
        assert!(
            !doc.warnings()
                .iter()
                .any(|warning| warning.contains("7.4.1")),
            "{stage}: 合法形态不应产生诊断：{:?}",
            doc.warnings()
        );

        // 负面：引用指向缺失对象 → 诊断 + 失败，绝不静默按剩余过滤器解码。
        let error = doc
            .image(&stream_value(&doc, 5))
            .expect_err("{stage}: 缺失对象的引用元素必须失败而非误解码");
        assert!(
            error
                .to_string()
                .contains("/Filter array has a non-name element"),
            "{stage}: got {error}"
        );
        assert!(
            doc.warnings()
                .iter()
                .any(|warning| { warning.contains("7.4.1") && warning.contains("absent object") }),
            "{stage}: absent 诊断缺失：{:?}",
            doc.warnings()
        );
    }
}

// ── 7.6.5：加密流上显式 `/Crypt` 的筛选器条目写成间接引用 ───────────────
//
// 写端不产出 `/CF` crypt 字典，故按 `standard/tests` 的密钥派生先例手工构造
// R4/AESV2 加密样本（解析 → 解密全链路，不经写出重读）。

/// 三个 crypt 流共用的明文（先 ASCIIHex 编码再 AES 加密，故 `/Crypt` 之后
/// 还有一个真实的链上筛选器）。
const CRYPT_REF_PLAIN: &[u8] = b"indirect crypt filter payload";

/// 7.6.3.1 Table 22 的 32 字节口令填充表。
const CRYPT_REF_PADDING: [u8; 32] = [
    0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01, 0x08,
    0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53, 0x69, 0x7a,
];

fn crypt_ref_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn crypt_ref_rc4_16(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U16>::new_from_slice(key).unwrap();
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

/// AES-128-CBC + PKCS#5 填充，IV 前置（AESV2 的流加密形态）。
fn crypt_ref_aes(plaintext: &[u8], key: &[u8], iv: [u8; 16]) -> Vec<u8> {
    let padding = 16 - plaintext.len() % 16;
    let mut data = plaintext.to_vec();
    data.extend(std::iter::repeat_n(padding as u8, padding));
    let cipher = Aes128::new_from_slice(key).unwrap();
    let mut previous = iv;
    for chunk in data.chunks_mut(16) {
        for (byte, prior) in chunk.iter_mut().zip(previous) {
            *byte ^= prior;
        }
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
        previous.copy_from_slice(chunk);
    }
    [iv.as_slice(), data.as_slice()].concat()
}

/// R4（`/V 4 /R 4 /Length 128`、`EncryptMetadata false`、`/P -4`）的文件密钥与
/// `/O` `/U` 条目（Algorithm 2 与 Algorithm 4/5 的手工实现）。
fn crypt_ref_r4_entries(id: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let pad = |password: &[u8]| {
        let mut result = password[..password.len().min(32)].to_vec();
        result.extend_from_slice(&CRYPT_REF_PADDING[..32 - result.len()]);
        result
    };
    let user_password = pad(b"user-password");
    let owner_password = pad(b"owner-password");
    let mut owner_digest = Md5::digest(&owner_password).to_vec();
    for _ in 0..50 {
        owner_digest = Md5::digest(&owner_digest[..16]).to_vec();
    }
    let mut owner_entry = crypt_ref_rc4_16(&owner_digest[..16], &user_password);
    for iteration in 1..20u8 {
        let key: Vec<u8> = owner_digest[..16]
            .iter()
            .map(|byte| byte ^ iteration)
            .collect();
        owner_entry = crypt_ref_rc4_16(&key, &owner_entry);
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
    let mut user_material = CRYPT_REF_PADDING.to_vec();
    user_material.extend_from_slice(id);
    let digest = Md5::digest(user_material);
    let mut user_entry = crypt_ref_rc4_16(&file_key[..16], &digest);
    for iteration in 1..20u8 {
        let key: Vec<u8> = file_key[..16].iter().map(|byte| byte ^ iteration).collect();
        user_entry = crypt_ref_rc4_16(&key, &user_entry);
    }
    user_entry.extend_from_slice(&[0; 16]);
    (file_key, owner_entry, user_entry)
}

/// Algorithm 1：对象密钥 = MD5(文件密钥 + 3 字节对象号 + 2 字节代号 + `sAlT`)。
fn crypt_ref_object_key(file_key: &[u8], num: i64, generation: i64) -> Vec<u8> {
    let mut material = file_key[..16].to_vec();
    material.extend_from_slice(&(num as u32).to_le_bytes()[..3]);
    material.extend_from_slice(&(generation as u16).to_le_bytes());
    material.extend_from_slice(b"sAlT");
    Md5::digest(material)[..16].to_vec()
}

/// 写出给定编号的对象（编号缺口写自由条目）+ 带 `/Encrypt` 与 `/ID` 的 trailer。
fn assemble_encrypted(objects: &[(i64, Vec<u8>)], encrypt_num: i64, id: &[u8]) -> Vec<u8> {
    let highest = objects.iter().map(|(num, _)| *num).max().unwrap_or(1);
    let mut buf = b"%PDF-1.5\n".to_vec();
    let mut offsets = vec![0usize; highest as usize + 1];
    for (num, body) in objects {
        offsets[*num as usize] = buf.len();
        buf.extend_from_slice(body);
    }
    let xref_offset = buf.len();
    buf.extend_from_slice(format!("xref\n0 {}\n", highest + 1).as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \n");
    for num in 1..=highest {
        let line = if offsets[num as usize] == 0 {
            "0000000000 65535 f \n".to_string()
        } else {
            format!("{:010} 00000 n \n", offsets[num as usize])
        };
        buf.extend_from_slice(line.as_bytes());
    }
    let id_hex = crypt_ref_hex(id);
    let trailer = format!(
        "trailer\n<< /Size {} /Root 1 0 R /Encrypt {encrypt_num} 0 R /ID [<{id_hex}> <{id_hex}>] >>\nstartxref\n{xref_offset}\n%%EOF\n",
        highest + 1
    );
    buf.extend_from_slice(trailer.as_bytes());
    buf
}

/// 一个 `/Crypt` 之后接 `/ASCIIHexDecode` 的加密流对象。
fn crypt_ref_stream(num: i64, entries: &str, file_key: &[u8]) -> Vec<u8> {
    let payload = format!("{}>", crypt_ref_hex(CRYPT_REF_PLAIN)).into_bytes();
    let encrypted = crypt_ref_aes(
        &payload,
        &crypt_ref_object_key(file_key, num, 0),
        [0x41; 16],
    );
    let head = format!(
        "{num} 0 obj\n<< {entries} /Length {} >>\nstream\n",
        encrypted.len()
    );
    let mut body = head.into_bytes();
    body.extend_from_slice(&encrypted);
    body.extend_from_slice(b"\nendstream\nendobj\n");
    body
}

/// R4/AESV2 样本：
/// - 对象 2：`/Filter 6 0 R`（→ `[ 5 0 R /ASCIIHexDecode ]`，元素 5 即 Name `/Crypt`）
///   ＋ `/DecodeParms 7 0 R`（→ `[ 8 0 R null ]`，元素 8 即 `<< /Name /StdCF >>`）；
/// - 对象 3：同一筛选器的**直接对象**写法（对照）；
/// - 对象 9：`/DecodeParms 10 0 R` 指向缺失对象（失败形态，对象 10 不在 xref 里）。
fn crypt_filter_reference_pdf() -> Vec<u8> {
    let id = b"pdf-r4-crypt-ref-test";
    let (file_key, owner_entry, user_entry) = crypt_ref_r4_entries(id);
    let entries = "/Filter [ /Crypt /ASCIIHexDecode ] /DecodeParms [ << /Name /StdCF >> null ]";
    let encrypt = format!(
        "4 0 obj\n<< /Filter /Standard /V 4 /R 4 /Length 128 /O <{}> /U <{}> /P -4 /EncryptMetadata false /CF << /StdCF << /CFM /AESV2 /Length 16 >> >> /StmF /StdCF /StrF /StdCF >>\nendobj\n",
        crypt_ref_hex(&owner_entry),
        crypt_ref_hex(&user_entry)
    );
    let objects: Vec<(i64, Vec<u8>)> = vec![
        (1, b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec()),
        (
            2,
            crypt_ref_stream(2, "/Filter 6 0 R /DecodeParms 7 0 R", &file_key),
        ),
        (3, crypt_ref_stream(3, entries, &file_key)),
        (4, encrypt.into_bytes()),
        (5, b"5 0 obj\n/Crypt\nendobj\n".to_vec()),
        (6, b"6 0 obj\n[ 5 0 R /ASCIIHexDecode ]\nendobj\n".to_vec()),
        (7, b"7 0 obj\n[ 8 0 R null ]\nendobj\n".to_vec()),
        (8, b"8 0 obj\n<< /Name /StdCF >>\nendobj\n".to_vec()),
        (
            9,
            crypt_ref_stream(9, "/Filter /Crypt /DecodeParms 10 0 R", &file_key),
        ),
    ];
    assemble_encrypted(&objects, 4, id)
}

/// 加密流上 `/Crypt` 的筛选器条目写成间接引用时，选中的 crypt filter 与直接
/// 对象写法**给出同一份明文**；引用指向缺失对象时明确报错、不返回任何字节。
#[test]
fn encrypted_stream_with_reference_crypt_filter_entries_decrypts_like_the_direct_form() {
    let bytes = crypt_filter_reference_pdf();
    let doc = parse_buf_with_password(&bytes, b"user-password");
    let indirect = stream_value(&doc, 2);
    let direct = stream_value(&doc, 3);

    // 解密后的原始字节仍是 ASCIIHex 载荷（密钥取自 /CF 的 StdCF，而非 Identity）。
    let payload = format!("{}>", crypt_ref_hex(CRYPT_REF_PLAIN)).into_bytes();
    assert_eq!(
        doc.try_stream_bytes(&indirect)
            .expect("引用形态的 /Crypt 条目应选中 StdCF 并解密"),
        payload
    );
    assert_eq!(
        doc.decode_stream_bytes(&indirect)
            .expect("引用形态的完整筛选器链应解码"),
        CRYPT_REF_PLAIN
    );
    // 与直接对象写法逐字节一致。
    assert_eq!(
        doc.decode_stream_bytes(&direct).unwrap(),
        doc.decode_stream_bytes(&indirect).unwrap()
    );

    // 失败形态：参数字典引用指向缺失对象 → 明确报错，不静默解密成乱码。
    let broken = stream_value(&doc, 9);
    let error = doc
        .try_stream_bytes(&broken)
        .expect_err("缺失的 /DecodeParms 引用必须报错");
    assert!(
        error.contains("/DecodeParms") && error.contains("absent object"),
        "错误应给出键名与解引用诊断：{error}"
    );
}
