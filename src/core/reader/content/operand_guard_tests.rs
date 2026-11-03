//! 畸形操作数个数的越界守卫回归（不可信输入不得 panic）。
//!
//! 内容流是外部输入，算子可能少给操作数；每个位置访问都必须落到既有的
//! [`ContentStreamError`] 变体上，而不是 `index out of bounds` panic。

use super::{ContentStreamError, parse_content_stream};

#[test]
fn tf_rejects_missing_size_operand() {
    let error = parse_content_stream(b"BT /F1 Tf (x) Tj ET").unwrap_err();
    assert_eq!(
        error,
        ContentStreamError::InvalidOperandCount {
            operator: "Tf".into(),
            expected: "2".into(),
            actual: 1,
        }
    );
}

#[test]
fn tf_rejects_missing_font_operand() {
    let error = parse_content_stream(b"Tf").unwrap_err();
    assert_eq!(
        error,
        ContentStreamError::InvalidOperandCount {
            operator: "Tf".into(),
            expected: "1".into(),
            actual: 0,
        }
    );
}

/// 单操作数算子缺操作数 → `InvalidOperandCount { expected: "1", actual: 0 }`。
///
/// 这些算子曾经直接下标取 `operands[0]`，畸形内容流会越界 panic。
#[test]
fn single_operand_operators_reject_missing_operand() {
    let operators = [
        "w", "J", "j", "M", "ri", "i", "gs", "Tc", "Tw", "Tz", "TL", "Tf", "Tr", "Ts", "Tj", "'",
        "\"", "CS", "cs", "G", "g", "sh", "Do", "MP", "DP", "BMC", "BDC",
    ];
    for operator in operators {
        let error = parse_content_stream(operator.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("operator {operator} without operands must not parse"));
        assert_eq!(
            error,
            ContentStreamError::InvalidOperandCount {
                operator: operator.into(),
                expected: "1".into(),
                actual: 0,
            },
            "operator {operator} 缺操作数应返回越界守卫错误"
        );
    }
}

/// `"` 的三个操作数逐个缺失时报出对应的位置，而不是越界 panic。
#[test]
fn quote_operator_rejects_each_missing_operand() {
    for (input, expected, actual) in [
        ("\"".to_string(), "1", 0),
        ("1 \"".to_string(), "2", 1),
        ("1 2 \"".to_string(), "3", 2),
    ] {
        let error = parse_content_stream(input.as_bytes()).unwrap_err();
        assert_eq!(
            error,
            ContentStreamError::InvalidOperandCount {
                operator: "\"".into(),
                expected: expected.into(),
                actual,
            },
            "input: {input}"
        );
    }
}

/// `DP`/`BDC` 有 tag 但缺属性操作数时沿用既有的「at least 2」计数错误。
#[test]
fn marked_content_operators_reject_missing_properties_operand() {
    for operator in ["DP", "BDC"] {
        let input = format!("/Span {operator}");
        let error = parse_content_stream(input.as_bytes()).unwrap_err();
        assert_eq!(
            error,
            ContentStreamError::InvalidOperandCount {
                operator: operator.into(),
                expected: "at least 2".into(),
                actual: 1,
            },
            "input: {input}"
        );
    }
}

/// 定长数值算子缺操作数时返回计数错误（此前由 `expect_numbers_exact` 守卫，
/// 这里一并锁住行为，防止回归）。
#[test]
fn fixed_arity_numeric_operators_reject_truncated_forms() {
    let cases = [
        ("cm", 6, "1 0 0 1"),
        ("m", 2, "10"),
        ("l", 2, "10"),
        ("c", 6, "1 2 3 4 5"),
        ("v", 4, "1 2 3"),
        ("y", 4, "1 2 3"),
        ("re", 4, "0 0 1"),
        ("Td", 2, "1"),
        ("TD", 2, "1"),
        ("Tm", 6, "1 0 0 1 2"),
        ("d0", 2, "1"),
        ("d1", 6, "0 0 1 1 1"),
        ("RG", 3, "1 0"),
        ("rg", 3, "1 0"),
        ("K", 4, "0 0 0"),
        ("k", 4, "0 0 0"),
    ];
    for (operator, arity, truncated) in cases {
        let input = format!("{truncated} {operator}");
        let error = parse_content_stream(input.as_bytes()).unwrap_err();
        assert_eq!(
            error,
            ContentStreamError::InvalidOperandCount {
                operator: operator.into(),
                expected: arity.to_string(),
                actual: truncated.split(' ').count(),
            },
            "input: {input}"
        );
    }
}

/// 系统性截断扫描：把每个算子的操作数从 0 逐个删到 n-1，全部必须是
/// 类型化错误、零 panic；完整形态仍正常解析（合法输入行为不变）。
#[test]
fn truncated_operand_forms_never_panic() {
    let cases: &[(&str, &[&str])] = &[
        ("Tf", &["/F1", "12"]),
        ("Tj", &["(x)"]),
        ("TJ", &["[(x) -1]"]),
        ("Td", &["1", "2"]),
        ("TD", &["1", "2"]),
        ("Tm", &["1", "0", "0", "1", "2", "3"]),
        ("Tz", &["100"]),
        ("Tc", &["1"]),
        ("Tw", &["1"]),
        ("TL", &["1"]),
        ("Ts", &["1"]),
        ("Tr", &["0"]),
        ("'", &["(x)"]),
        ("\"", &["1", "2", "(x)"]),
        ("cm", &["1", "0", "0", "1", "2", "3"]),
        ("gs", &["/GS0"]),
        ("sh", &["/Sh0"]),
        ("re", &["0", "0", "1", "1"]),
        ("w", &["1"]),
        ("J", &["0"]),
        ("j", &["0"]),
        ("M", &["4"]),
        ("ri", &["/RelativeColorimetric"]),
        ("i", &["1"]),
        ("CS", &["/DeviceRGB"]),
        ("cs", &["/DeviceRGB"]),
        ("SC", &["1", "0", "0"]),
        ("sc", &["1", "0", "0"]),
        ("scn", &["1", "0", "0", "/P0"]),
        ("SCN", &["1", "0", "0", "/P0"]),
        ("G", &["0.5"]),
        ("g", &["0.5"]),
        ("RG", &["1", "0", "0"]),
        ("rg", &["0", "1", "0"]),
        ("K", &["0", "0", "0", "1"]),
        ("k", &["0", "0", "0", "1"]),
        ("m", &["1", "2"]),
        ("l", &["3", "4"]),
        ("c", &["1", "2", "3", "4", "5", "6"]),
        ("v", &["1", "2", "3", "4"]),
        ("y", &["1", "2", "3", "4"]),
        ("d", &["[3 2]", "0"]),
        ("d0", &["1", "2"]),
        ("d1", &["0", "0", "1", "1", "1", "2"]),
        ("Do", &["/Im0"]),
        ("MP", &["/P"]),
        ("DP", &["/P", "<</MCID 0>>"]),
        ("BMC", &["/P"]),
        ("BDC", &["/P", "<</MCID 0>>"]),
    ];
    for (operator, operands) in cases {
        let full = format!("{} {operator}", operands.join(" "));
        assert!(
            parse_content_stream(full.as_bytes()).is_ok(),
            "完整形态不应报错: {full}"
        );
        for taken in 0..operands.len() {
            // 变长分量算子（SC/sc/SCN/scn）少给操作数仍是合法形态，
            // `d` 的空数组同理；它们只要求不 panic，由专项用例锁定。
            if matches!(*operator, "SC" | "sc" | "SCN" | "scn") || (taken == 0 && *operator == "d")
            {
                let lenient = format!("{} {operator}", operands[..taken].join(" "));
                let lenient = lenient.trim_start().to_string();
                // Ok / Err 均可，本行只保证不越界 panic。
                let _ = parse_content_stream(lenient.as_bytes());
                continue;
            }
            let truncated = format!("{} {operator}", operands[..taken].join(" "));
            let truncated = truncated.trim_start().to_string();
            let error = parse_content_stream(truncated.as_bytes())
                .err()
                .unwrap_or_else(|| panic!("truncated form must not parse: {truncated}"));
            assert!(
                matches!(
                    error,
                    ContentStreamError::InvalidOperandCount { .. }
                        | ContentStreamError::InvalidOperandValue { .. }
                ),
                "truncated form {truncated} 返回了意外错误: {error:?}"
            );
        }
    }
}

/// 变长颜色算子：任意个数都不得 panic（少给分量是既有宽松行为）。
#[test]
fn variadic_color_operators_accept_any_operand_count() {
    for operator in ["SC", "sc", "SCN", "scn"] {
        for taken in 0..=3 {
            let operands = ["1", "0", "0"];
            let input = format!("{} {operator}", operands[..taken].join(" "));
            let input = input.trim_start().to_string();
            let ops = parse_content_stream(input.as_bytes())
                .unwrap_or_else(|error| panic!("input {input} 应可解析: {error}"));
            assert_eq!(ops.len(), 1, "input: {input}");
        }
    }

    // 名字形态的 SCN/scn 少给分量同样不 panic。
    for operator in ["SCN", "scn"] {
        let ops = parse_content_stream(format!("/P0 {operator}").as_bytes())
            .unwrap_or_else(|error| panic!("{operator} 仅有图案名应可解析: {error}"));
        assert_eq!(ops.len(), 1);
    }
}
