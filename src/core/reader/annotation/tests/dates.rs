//! 注解日期条目（`/M`、`/CreationDate`）的类型化解析（7.9.4）。

use crate::core::reader::annotation::*;
use crate::core::types::Value;

use super::support::*;

#[test]
fn annotation_dates_parse_to_pdf_date() {
    let dict = base_annotation("Stamp")
        .insert("M", Value::String("D:20261029120000+08'00'".into()))
        .insert("CreationDate", Value::String("20261029".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("annotation");
    let modified = annotation.modified_date_parsed().expect("/M parses");
    assert_eq!(modified.year(), 2026);
    assert_eq!(modified.month(), Some(10));
    assert_eq!(modified.day(), Some(29));
    assert_eq!(modified.hour(), Some(12));
    assert_eq!(modified.minute(), Some(0));
    assert_eq!(modified.second(), Some(0));
    assert_eq!(modified.to_pdf_string(), "D:20261029120000+08'00'");
    // 缺 `D:` 前缀不是规范日期串：原始值保留，类型化解析为 None。
    assert!(annotation.creation_date.is_some());
    assert_eq!(annotation.creation_date_parsed(), None);
}
