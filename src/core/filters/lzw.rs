//! `LZWDecode`：9–12 位变长码、CLEAR/EOD、KwKwK 与 `/EarlyChange`。

use crate::core::types::Dict;

use super::lzw_backend::WeezlLzwBackend;
use super::predictor::apply_predictor;
use super::{DecodeError, Result, StreamFilter};

/// `LZWDecode`：LZW 解码，随后应用 Predictor。
pub(super) struct LzwFilter;

impl StreamFilter for LzwFilter {
    fn name(&self) -> &'static str {
        "LZWDecode"
    }

    fn decode(&self, data: &[u8], params: Option<&Dict>) -> Result<Vec<u8>> {
        let decoded = lzw_decode(data, params)?;
        apply_predictor(decoded, params)
    }
}

/// LZW 位流与字典解码后端。
///
/// 只经字节、`/EarlyChange` 布尔语义与字符串错误交互，第三方码流库的类型
/// 不越过此边界；换库或改回本地实现只需替换 `lzw_backend.rs` 中的实现。
pub(super) trait LzwBackend {
    /// `early_change` 为 `true` 表示 `/EarlyChange 1`（码长提前一个码切换，
    /// 与 TIFF 口径一致）；`false` 为标准口径。
    fn decode(&self, data: &[u8], early_change: bool) -> std::result::Result<Vec<u8>, String>;
}

/// 读取 `/EarlyChange`（仅 LZWDecode）：缺省 1；规范只定义 0 与 1，
/// 其他值（含非整数）视为参数错误。
fn lzw_early_change(params: Option<&Dict>) -> Result<usize> {
    let Some(value) = params.and_then(|d| d.get("EarlyChange")) else {
        return Ok(1);
    };
    match value.as_integer() {
        Some(0) => Ok(0),
        Some(1) => Ok(1),
        _ => Err(DecodeError::new("LZWDecode: /EarlyChange must be 0 or 1")),
    }
}

/// `LZWDecode`（ISO 32000-1 7.4.4）：9–12 位变长码、CLEAR/EOD、KwKwK
/// 特例与 `/EarlyChange`。
///
/// 位流与字典解码交给后端适配层（当前为 `weezl`）：`/EarlyChange 1`（缺省）
/// 按 TIFF 口径在表长 511 时切到 10 位（之后 1023、2047），`/EarlyChange 0`
/// 在 512 时切换；12 位封顶，表满 4096 后停止新增但继续解码。规范要求 EOD，
/// 但流被截断时后端返回已解出的前缀（宽容，与库的「宽进」策略一致），绝不
/// panic。
fn lzw_decode(data: &[u8], params: Option<&Dict>) -> Result<Vec<u8>> {
    let early_change = lzw_early_change(params)?;
    lzw_decode_with(data, early_change == 1, &WeezlLzwBackend)
}

/// 经后端 trait 解码，便于以替身实现验证 PDF 语义层只依赖抽象。
fn lzw_decode_with<B: LzwBackend>(data: &[u8], early_change: bool, backend: &B) -> Result<Vec<u8>> {
    backend.decode(data, early_change).map_err(DecodeError::new)
}

#[cfg(test)]
mod tests {
    use super::super::decode_stream;
    use super::*;
    use crate::core::types::Value;

    fn dict_with(filter: Value, parms: Option<Value>) -> Dict {
        let mut d = Dict::new().insert("Filter", filter);
        if let Some(p) = parms {
            d = d.insert("DecodeParms", p);
        }
        d
    }

    /// 按 `(码值, 码宽)` 序列做 MSB-first 位打包，供 LZW 测试构造码流。
    fn pack_lzw_codes(codes: &[(u32, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut acc = 0u32;
        let mut pending = 0u32;
        for &(code, width) in codes {
            acc = (acc << width) | code;
            pending += width;
            while pending >= 8 {
                pending -= 8;
                out.push((acc >> pending) as u8);
            }
        }
        if pending > 0 {
            out.push(((acc << (8 - pending)) & 0xff) as u8);
        }
        out
    }

    /// 规范 7.4.4.2 Example 2：`80 0B 60 50 22 0C 0C 85 01`
    /// 解码为 `-----A---B`；全名与缩写 `LZW` 走同一实现。
    #[test]
    fn lzw_decodes_spec_example() {
        let bytes = [0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01];
        let expected = b"-----A---B";
        for name in ["LZWDecode", "LZW"] {
            let d = dict_with(Value::Name(name.into()), None);
            assert_eq!(
                decode_stream(&d, &bytes).unwrap(),
                expected,
                "filter {name}"
            );
        }
    }

    /// CLEAR 重置字典：随后重新以字面量开始可正常解码；引用 CLEAR 之前的
    /// 旧码（258）必须报错，不得读到残留表项。
    #[test]
    fn lzw_clear_resets_table() {
        let d = dict_with(Value::Name("LZWDecode".into()), None);
        // 65 66 → 新增 258=AB；CLEAR；67 68。
        let codes = [
            (256, 9),
            (65, 9),
            (66, 9),
            (256, 9),
            (67, 9),
            (68, 9),
            (257, 9),
        ];
        assert_eq!(decode_stream(&d, &pack_lzw_codes(&codes)).unwrap(), b"ABCD");

        let stale = [(256, 9), (65, 9), (66, 9), (256, 9), (258, 9), (257, 9)];
        let e = decode_stream(&d, &pack_lzw_codes(&stale)).unwrap_err();
        assert!(e.to_string().contains("LZWDecode"), "got: {e}");
    }

    /// `/EarlyChange` 控制码宽切换点：1（缺省）在表长 511 时切 10 位，
    /// 0 在 512 时切换；CLEAR 后码宽复位为 9。
    #[test]
    fn lzw_early_change_switches_code_width() {
        let literals: Vec<u8> = (0..260).map(|i| (i % 256) as u8).collect();
        let build = |early: usize| {
            let mut codes: Vec<(u32, u32)> = vec![(256, 9)];
            for (i, &b) in literals.iter().enumerate() {
                let width = if early == 1 {
                    if i < 254 { 9 } else { 10 }
                } else if i < 255 {
                    9
                } else {
                    10
                };
                codes.push((b as u32, width));
            }
            // 以当前 10 位发 CLEAR，再以复位后的 9 位发一个字面量与 EOD。
            codes.push((256, 10));
            codes.push((65, 9));
            codes.push((257, 9));
            pack_lzw_codes(&codes)
        };

        let mut expected = literals.clone();
        expected.push(b'A');
        for early in [0, 1] {
            let parms = Dict::new().insert("EarlyChange", Value::Integer(early));
            let d = dict_with(Value::Name("LZWDecode".into()), Some(Value::Dict(parms)));
            assert_eq!(
                decode_stream(&d, &build(early as usize)).unwrap(),
                expected,
                "EarlyChange={early}"
            );
        }
        // 缺省 EarlyChange 等价于 1。
        let d = dict_with(Value::Name("LZWDecode".into()), None);
        assert_eq!(decode_stream(&d, &build(1)).unwrap(), expected);
    }

    /// LZW 解码结果再经 Predictor（PNG Up）还原：验证先 LZW、后预测器的顺序。
    #[test]
    fn lzw_applies_predictor() {
        let raw: Vec<u8> = vec![10, 20, 30, 40, 5, 250, 1, 2];
        let mut enc = Vec::new();
        for r in 0..2 {
            for c in 0..4 {
                let cur = raw[r * 4 + c];
                let up = if r == 0 { 0 } else { raw[(r - 1) * 4 + c] };
                enc.push(cur.wrapping_sub(up));
            }
        }
        let mut codes: Vec<(u32, u32)> = vec![(256, 9)];
        codes.extend(enc.iter().map(|&b| (b as u32, 9)));
        codes.push((257, 9));

        let parms = Dict::new()
            .insert("Predictor", Value::Integer(12))
            .insert("Columns", Value::Integer(4));
        let d = dict_with(Value::Name("LZWDecode".into()), Some(Value::Dict(parms)));
        assert_eq!(decode_stream(&d, &pack_lzw_codes(&codes)).unwrap(), raw);
    }

    /// 码值超出解码端表长（且不是合法的 KwKwK 情形）→ Err，不 panic。
    #[test]
    fn lzw_rejects_invalid_code() {
        let d = dict_with(Value::Name("LZWDecode".into()), None);
        let codes = [(256, 9), (45, 9), (300, 9)];
        let e = decode_stream(&d, &pack_lzw_codes(&codes)).unwrap_err();
        assert!(e.to_string().contains("outside table"), "got: {e}");
    }

    /// CLEAR 之后紧跟 KwKwK 形态的码（等于表长）但无前驱 → Err，不 panic。
    #[test]
    fn lzw_rejects_kwkwk_without_predecessor() {
        let d = dict_with(Value::Name("LZWDecode".into()), None);
        let codes = [(256, 9), (258, 9)];
        let e = decode_stream(&d, &pack_lzw_codes(&codes)).unwrap_err();
        assert!(e.to_string().contains("LZWDecode"), "got: {e}");
    }

    /// `/EarlyChange` 只接受 0/1；其他值报参数错误。
    #[test]
    fn lzw_rejects_invalid_early_change() {
        let parms = Dict::new().insert("EarlyChange", Value::Integer(2));
        let d = dict_with(Value::Name("LZWDecode".into()), Some(Value::Dict(parms)));
        let codes = [(256, 9), (45, 9), (257, 9)];
        let e = decode_stream(&d, &pack_lzw_codes(&codes)).unwrap_err();
        assert!(e.to_string().contains("EarlyChange"), "got: {e}");
    }

    /// 流缺少 EOD（截断）：返回已解出的前缀，不报错、不 panic。
    #[test]
    fn lzw_stops_at_truncated_stream() {
        let d = dict_with(Value::Name("LZWDecode".into()), None);
        let codes = [(256, 9), (45, 9), (65, 9)]; // 无 EOD
        assert_eq!(decode_stream(&d, &pack_lzw_codes(&codes)).unwrap(), b"-A");
    }

    /// 码流总位数不是 8 的倍数：末字节的填充位被忽略，EOD 前的数据完整解出。
    #[test]
    fn lzw_ignores_non_byte_aligned_tail() {
        let d = dict_with(Value::Name("LZWDecode".into()), None);
        let codes = [(256, 9), (65, 9), (66, 9), (257, 9)]; // 36 位 = 4.5 字节
        assert_eq!(decode_stream(&d, &pack_lzw_codes(&codes)).unwrap(), b"AB");
    }

    /// 替身后端：验证 PDF 语义层只依赖 `LzwBackend`，且 `/EarlyChange`
    /// 语义被原样传给后端（换库或改回本地实现只需替换适配文件）。
    struct FakeBackend;

    impl LzwBackend for FakeBackend {
        fn decode(&self, _data: &[u8], early_change: bool) -> std::result::Result<Vec<u8>, String> {
            Ok(if early_change {
                b"early".to_vec()
            } else {
                b"standard".to_vec()
            })
        }
    }

    #[test]
    fn lzw_backend_is_replaceable() {
        assert_eq!(lzw_decode_with(&[0], true, &FakeBackend).unwrap(), b"early");
        assert_eq!(
            lzw_decode_with(&[0], false, &FakeBackend).unwrap(),
            b"standard"
        );
    }
}
