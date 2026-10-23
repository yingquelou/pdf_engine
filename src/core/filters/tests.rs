//! `filters` 根模块的调度/扩展测试。

use super::*;
use crate::core::types::{Array, Value};

/// 构造带指定 `/Filter` + `/DecodeParms` 的流字典。
fn dict_with(filter: Value, parms: Option<Value>) -> Dict {
    let mut d = Dict::new().insert("Filter", filter);
    if let Some(p) = parms {
        d = d.insert("DecodeParms", p);
    }
    d
}

#[test]
fn no_filter_returns_input() {
    let d = Dict::new();
    assert_eq!(decode_stream(&d, b"raw bytes").unwrap(), b"raw bytes");
}

#[test]
fn external_file_specification_is_not_a_filter() {
    let d = Dict::new().insert(
        "F",
        Value::Dict(Dict::new().insert("F", Value::String("external.bin".into()))),
    );
    assert_eq!(decode_stream(&d, b"raw bytes").unwrap(), b"raw bytes");
}

/// array-of-filters：[ASCII85Decode, FlateDecode] 依次解码。
#[test]
fn multiple_filters_decode_in_sequence() {
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    // 原始数据 → flate 压缩 → ascii85 编码
    let raw = b"<< /Marker /Seq >>";
    let mut enc = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(raw).unwrap();
    let z = enc.finish().unwrap();
    // 手工 ascii85 编码（无需外部库）
    let a85 = encode_ascii85(&z);
    let d = dict_with(
        Value::Array(
            Array::new()
                .push(Value::Name("ASCII85Decode".into()))
                .push(Value::Name("FlateDecode".into())),
        ),
        None,
    );
    assert_eq!(decode_stream(&d, &a85).unwrap(), raw);
}

/// test 用：把字节编码为 ASCII85（含 `~>` 结束标记）。
fn encode_ascii85(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in data.chunks(4) {
        let mut group = [0u8; 4];
        group[..chunk.len()].copy_from_slice(chunk);
        let v = u32::from_be_bytes(group);
        let mut digits = [0u8; 5];
        let mut x = v;
        for slot in digits.iter_mut().rev() {
            *slot = (x % 85) as u8 + b'!';
            x /= 85;
        }
        out.extend_from_slice(&digits[..chunk.len() + 1]);
    }
    out.extend_from_slice(b"~>");
    out
}

/// 未支持的过滤器返回 Err（不 panic）。
#[test]
fn unsupported_filter_errors() {
    let d = dict_with(Value::Name("JBIG2Decode".into()), None);
    let e = decode_stream(&d, b"\x00\x01junk").unwrap_err();
    assert!(e.to_string().contains("JBIG2Decode"), "got: {e}");
}

/// 自定义 [`StreamFilter`] 可注册并参与调度；内建过滤器仍可用，
/// 未注册的过滤器仍报不支持。证明过滤器实现与调度解耦、可扩展。
#[test]
fn custom_filter_can_be_registered() {
    struct UpperFilter;
    impl StreamFilter for UpperFilter {
        fn name(&self) -> &'static str {
            "TestUpper"
        }

        fn decode(&self, data: &[u8], _params: Option<&Dict>) -> Result<Vec<u8>> {
            Ok(data.iter().map(|b| b.to_ascii_uppercase()).collect())
        }
    }

    let mut filters = builtin_filters().to_vec();
    filters.push(&UpperFilter);

    let custom = dict_with(Value::Name("TestUpper".into()), None);
    assert_eq!(
        decode_stream_with(&filters, &custom, b"abc").unwrap(),
        b"ABC"
    );

    let builtin = dict_with(Value::Name("ASCIIHexDecode".into()), None);
    assert_eq!(
        decode_stream_with(&filters, &builtin, b"4142>").unwrap(),
        b"AB"
    );

    let unknown = dict_with(Value::Name("JBIG2Decode".into()), None);
    let e = decode_stream_with(&filters, &unknown, b"x").unwrap_err();
    assert!(e.to_string().contains("JBIG2Decode"), "got: {e}");
}

/// CCITTFaxDecode 经 `StreamFilter` 调度（全名与缩写 CCF）。
/// 样本为 libtiff 生成的 8×3 Group 4 条带：全白 + 两行 4 白 4 黑。
#[test]
fn ccitt_filter_decodes_group4_and_alias() {
    let data = [0x9Bu8, 0x78, 0x00, 0x80, 0x08];
    for name in ["CCITTFaxDecode", "CCF"] {
        let parms = Dict::new()
            .insert("K", Value::Integer(-1))
            .insert("Columns", Value::Integer(8))
            .insert("Rows", Value::Integer(3));
        let d = dict_with(Value::Name(name.into()), Some(Value::Dict(parms)));
        assert_eq!(
            decode_stream(&d, &data).unwrap(),
            vec![0xFF, 0xF0, 0xF0],
            "filter {name}"
        );
    }
}

/// `/BlackIs1 true`：输出位取反。
#[test]
fn ccitt_filter_honors_black_is_1() {
    let data = [0x9Bu8, 0x78, 0x00, 0x80, 0x08];
    let parms = Dict::new()
        .insert("K", Value::Integer(-1))
        .insert("Columns", Value::Integer(8))
        .insert("Rows", Value::Integer(3))
        .insert("BlackIs1", Value::Bool(true));
    let d = dict_with(
        Value::Name("CCITTFaxDecode".into()),
        Some(Value::Dict(parms)),
    );
    assert_eq!(decode_stream(&d, &data).unwrap(), vec![0x00, 0x0F, 0x0F]);
}

/// 非法 `/Columns` → DecodeError（不 panic）。
#[test]
fn ccitt_filter_rejects_invalid_columns() {
    let parms = Dict::new().insert("Columns", Value::Integer(0));
    let d = dict_with(
        Value::Name("CCITTFaxDecode".into()),
        Some(Value::Dict(parms)),
    );
    let e = decode_stream(&d, b"\x00").unwrap_err();
    assert!(e.to_string().contains("Columns"), "got: {e}");
}
