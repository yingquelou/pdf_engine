//! 边界形态测试：下标越界、非 MCR 子项、无结构树文档。

use super::support::*;
use crate::core::reader::structure::StructureError;
use crate::core::types::{Dict, Document, Trailer, Value};

#[test]
fn marked_content_parent_requires_valid_kid_and_mcr_shape() {
    let doc = positive_document(0, keyed_stream());
    // 子项不是 MCR（整数 MCID）不是错误，只是不在本通道。
    let integer_kid = document_with(
        vec![(PAGE, page(Dict::new()))],
        vec![(ELEMENT, figure(Some(PAGE), vec![Value::Integer(0)]))],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        Vec::new(),
    );
    assert_eq!(integer_kid.marked_content_parent(0, 0), Ok(None));

    for (element, kid) in [(9, 0), (0, 9)] {
        let error = doc
            .marked_content_parent(element, kid)
            .expect_err("越界下标应报错");
        assert!(matches!(error, StructureError::Malformed(_)), "{error:?}");
    }

    // 没有结构树的文档：既没有父元素也没有发现，读取阶段不报错。
    let mut plain = Document::new();
    insert(
        &plain,
        CATALOG,
        Value::Dict(Dict::new().insert("Type", name("Catalog"))),
    );
    plain.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    assert_eq!(plain.marked_content_parent(0, 0), Ok(None));
    assert!(plain.stream_mcr_diagnostic().expect("no tree").is_empty());
}
