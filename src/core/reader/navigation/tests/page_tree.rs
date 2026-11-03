use super::*;

#[test]
fn page_tree_index_maps_indirect_pages_and_detects_cycles() {
    let doc = document_with_pages(3, Dict::new());
    assert_eq!(doc.page_count().unwrap(), 3);
    let index = page_tree_index(&doc).unwrap();
    assert_eq!(index.len(), 3);
    assert_eq!(index.index_of(PAGE_BASE, 0), Some(0));
    assert_eq!(index.index_of(PAGE_BASE + 2, 0), Some(2));
    assert_eq!(index.index_of(99, 0), None);

    let mut cyclic = Document::new();
    insert(
        &cyclic,
        1,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES_OBJECT, 0)),
        ),
    );
    insert(
        &cyclic,
        2,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGES_OBJECT, 0)])),
        ),
    );
    cyclic.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    let error = page_tree_index(&cyclic).unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error}");
}
