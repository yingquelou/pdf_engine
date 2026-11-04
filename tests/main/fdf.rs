//! FDF 读端集成测试（12.7.7）：手写字节缓冲 → 真实 Parser →
//! `Document::fdf()` 类型化读模型。
//!
//! 写端目前不产出 FDF 根（不在本车道范围），故 fixture 按 12.7.7.2 的
//! 文件结构（%FDF-1.2 头 + 对象体 + 经典 xref + trailer + startxref）
//! 直接拼装字节，经 `support::parse_buf` 走完整解析管线回读。

use super::*;

mod embedded;
/// ASCII85 载荷门禁（7.4.3）的写出→重解析往返与零字节拒绝。
mod gate;
mod javascript;
/// Table 248–251 的 `/Pages`/`/Annots` 类型化视图写出→重解析往返与
/// 写端装配前校验的零字节拒绝。
mod pages;
mod write;

/// 追加一段文本到缓冲（避免闭包与长度读取的借用冲突）。
fn push(buf: &mut Vec<u8>, chunk: &str) {
    buf.extend_from_slice(chunk.as_bytes());
}

/// 拼一个 FDF 字段树：person（Kids：直接对象 name、间接对象 address，
/// address 再带一个空 `/T` 子项）与 agree（/Opt、/RV）。
fn fdf_fixture_bytes() -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    push(&mut buf, "%FDF-1.2\n");

    let mut offsets = [0usize; 10];
    offsets[1] = buf.len();
    push(
        &mut buf,
        "1 0 obj\n<< /FDF 2 0 R /Version /1.4 >>\nendobj\n",
    );
    offsets[2] = buf.len();
    push(
        &mut buf,
        "2 0 obj\n<< /F (tax-return.pdf) /Status (Imported OK) /Encoding /WinAnsiEncoding /Fields [5 0 R 6 0 R] /Pages [ 42 0 R ] /Annots 8 0 R >>\nendobj\n",
    );
    offsets[5] = buf.len();
    push(
        &mut buf,
        "5 0 obj\n<< /T (person) /Kids [ << /T (name) /V (Ada) /Foo (raw-extra) /Ff 4 >> 9 0 R ] >>\nendobj\n",
    );
    offsets[9] = buf.len();
    push(
        &mut buf,
        "9 0 obj\n<< /T (address) /Kids [ << /T () /V (Blankville) >> ] >>\nendobj\n",
    );
    offsets[6] = buf.len();
    push(
        &mut buf,
        "6 0 obj\n<< /T (agree) /V (Yes) /Opt [ (Yes) (No) ] /RV <FEFF0052> >>\nendobj\n",
    );
    offsets[8] = buf.len();
    push(
        &mut buf,
        "8 0 obj\n[ << /Type /Annot /Subtype /Text /Page 0 /Contents (hi) >> ]\nendobj\n",
    );

    let xref_at = buf.len();
    push(&mut buf, "xref\n0 10\n");
    push(&mut buf, "0000000000 65535 f \n");
    // 对象号 1..=9：3、4、7 未使用，登记为自由条目。
    let entry = |num: usize| -> String {
        if offsets[num] == 0 {
            "0000000000 65535 f \n".to_string()
        } else {
            format!("{:010} 00000 n \n", offsets[num])
        }
    };
    for num in 1..10 {
        push(&mut buf, &entry(num));
    }
    push(&mut buf, "trailer\n<< /Root 1 0 R /Size 10 >>\n");
    push(&mut buf, &format!("startxref\n{xref_at}\n%%EOF\n"));
    buf
}

#[test]
fn handbuilt_fdf_file_exposes_typed_root_and_field_tree() {
    let document = parse_buf(&fdf_fixture_bytes());
    let fdf = document
        .fdf()
        .expect("FDF 解析不应报错")
        .expect("该缓冲应被识别为 FDF");
    // FDF 目录（Table 242）：/Version 名字条目
    assert_eq!(fdf.catalog_version.as_deref(), Some("1.4"));
    // FDF 字典（Table 243）：/F /Status /Encoding 类型化，/Pages /Annots 原值保留
    assert_eq!(
        fdf.fdf.source_file,
        Some(Value::String("tax-return.pdf".to_string()))
    );
    assert_eq!(fdf.fdf.status.as_deref(), Some("Imported OK"));
    assert_eq!(fdf.fdf.encoding.as_deref(), Some("WinAnsiEncoding"));
    assert_eq!(
        fdf.fdf.pages,
        Some(Value::Array(Array(vec![Value::Ref(42, 0)])))
    );
    assert_eq!(fdf.fdf.annots, Some(Value::Ref(8, 0)));
    assert!(
        fdf.fdf.fields_pages_conflict(),
        "/Fields 与 /Pages 同现应被诊断为冲突（Table 243 shall not）"
    );
    // 字段树（Table 246）：全限定名 + 值 + 标志 + 原值保留
    let names: Vec<&str> = fdf
        .all_fields()
        .iter()
        .map(|field| field.fully_qualified_name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "person",
            "person.name",
            "person.address",
            "person.address",
            "agree"
        ]
    );
    let name_field = &fdf.all_fields()[1];
    assert_eq!(name_field.value, Some(Value::String("Ada".to_string())));
    assert_eq!(name_field.field_flags, Some(4));
    assert_eq!(
        name_field.dict.get("Foo"),
        Some(&Value::String("raw-extra".to_string())),
        "未建模条目必须原样保留在字段字典里"
    );
    let agree = fdf.field("agree").expect("按全限定名查找");
    assert_eq!(
        agree.options,
        Some(Value::Array(Array(vec![
            Value::String("Yes".to_string()),
            Value::String("No".to_string()),
        ])))
    );
    assert_eq!(
        agree.rich_value,
        Some(Value::HexString("FEFF0052".to_string()))
    );
}

#[test]
fn fdf_field_values_listing_roundtrips_through_real_parser() {
    let document = parse_buf(&fdf_fixture_bytes());
    let listing = document.fdf_field_values().expect("枚举不应报错");
    assert_eq!(
        listing,
        vec![
            ("person".to_string(), None),
            (
                "person.name".to_string(),
                Some(Value::String("Ada".to_string()))
            ),
            ("person.address".to_string(), None),
            (
                "person.address".to_string(),
                Some(Value::String("Blankville".to_string()))
            ),
            ("agree".to_string(), Some(Value::String("Yes".to_string()))),
        ]
    );
}

#[test]
fn regular_pdf_file_is_not_detected_as_fdf() {
    // support::sample_doc() 是 /Type /Catalog 根：fdf() 应安静返回 None。
    let document = sample_doc();
    assert_eq!(document.fdf().expect("普通 PDF 不应报 FDF 错误"), None);
    assert_eq!(document.fdf_field_values().expect("空枚举"), Vec::new());
}
