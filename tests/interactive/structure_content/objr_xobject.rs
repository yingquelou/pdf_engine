//! 14.7.4.3 OBJR 对象引用（XObject 目标形态）的写出 → 解析 → 回读往返。

use super::*;

#[test]
fn builder_xobject_object_reference_roundtrip_links_struct_parent() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, FormXObject, ImageData, PageSize, PdfBuilder, StructureElement,
        StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::structure::{ParentTreeEntry, StructureKid};
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Tagged line");
    builder.end_structure_content(page);
    builder.image(
        page,
        72.0,
        620.0,
        20.0,
        20.0,
        &ImageData {
            width: 2,
            height: 2,
            color_space: ColorSpace::DeviceRGB,
            bits: 8,
            data: vec![0x40; 12],
            decode: None,
            interpolate: false,
            intent: None,
            alternates: Vec::new(),
            oc: None,
            metadata: None,
            matte: None,
            smask: None,
            image_mask: false,
            mask: None,
            opi: None,
        },
    );
    let form = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ));
    builder.draw_form(page, form, [1.0, 0.0, 0.0, 1.0, 72.0, 560.0]);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("Figure").with_image_xobject_reference(page, 0))
                .with_element(
                    StructureElement::new("Logo").with_form_xobject_reference(form, Some(page)),
                ),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with XObject OBJR structure content");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/Type /OBJR",
        "/StructParent 1",
        "/StructParent 2",
        "/StructParents 0",
        "/ParentTreeNextKey 3",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    // arena 前序：0 = Document、1 = P、2 = Figure（图像）、3 = Logo（表单）。
    let page_ref = match &tree.elements()[1].kids[..] {
        [StructureKid::MarkedContent(mcr)] => mcr.page.expect("MCR 带页引用"),
        other => panic!("P 元素应只有一个 MCR 子项，实际 {other:?}"),
    };
    let image_ref = objr_page_xobject_ref(&doc, page_ref.0, "Im1");
    let form_ref = objr_page_xobject_ref(&doc, page_ref.0, "Fm1");

    let mut objrs = Vec::new();
    for index in [2, 3] {
        match &tree.elements()[index].kids[..] {
            [StructureKid::ObjectReference(objr)] => objrs.push(objr.clone()),
            other => panic!("元素 {index} 应只有一个 OBJR 子项，实际 {other:?}"),
        }
    }
    let (image_objr, form_objr) = (&objrs[0], &objrs[1]);
    assert_eq!(
        image_objr.object, image_ref,
        "Figure 的 `/Obj` 指向页资源 `/Im1` 的图像 XObject"
    );
    assert_eq!(
        form_objr.object, form_ref,
        "Logo 的 `/Obj` 指向页资源 `/Fm1` 的表单 XObject"
    );
    for objr in &objrs {
        assert_eq!(objr.page, Some(page_ref), "XObject 目标的 `/Pg` 即渲染页");
        assert_eq!(
            objr.dict.get("Type"),
            Some(&Value::Name("OBJR".into())),
            "OBJR 字典带 /Type /OBJR"
        );
    }

    // 父树键：页面键 0 之后按目标对象号升序分配 1、2（Table 326）。
    let keys = [
        (image_ref, image_objr, 2, "Image"),
        (form_ref, form_objr, 3, "Form"),
    ];
    let mut ordered = keys.to_vec();
    ordered.sort_by_key(|(reference, _, _, _)| reference.0);
    for (rank, (reference, _, element_index, expected_subtype)) in ordered.iter().enumerate() {
        let key = rank as i64 + 1;
        let dict = objr_object_dict(&doc, reference.0);
        assert_eq!(
            dict.get("Type").and_then(Value::as_name),
            Some("XObject"),
            "目标 {reference:?} 应是 XObject"
        );
        assert_eq!(
            dict.get("Subtype").and_then(Value::as_name),
            Some(*expected_subtype),
            "目标 {reference:?} 的 /Subtype 应是 {expected_subtype}"
        );
        assert_eq!(
            dict.get("StructParent").and_then(Value::as_integer),
            Some(key),
            "XObject 流字典追加父树键 {key}"
        );
        assert!(
            dict.get("StructParents").is_none(),
            "同一对象不得同时带 /StructParents（Table 326）: {dict:?}"
        );
        match tree.parent_tree_entry(key) {
            Some(ParentTreeEntry::Object(found)) => {
                assert_eq!(
                    *found, *element_index,
                    "父树键 {key} 应指向元素 {element_index}"
                )
            }
            other => panic!("父树键 {key} 应是单元条目，实际 {other:?}"),
        }
        assert_eq!(tree.object_parent(key), Some(*element_index));
        assert_eq!(tree.mcid_parent(key, 0), None, "单元条目不参与 MCID 查找");
    }

    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0, "唯一带标记内容的页面仍是父树键 0");
    assert_eq!(parents.parents, vec![Some(1)]);
    assert_eq!(
        tree.mcid_parent(0, 0),
        Some(1),
        "页面 MCID 归属不受 OBJR 影响"
    );
    assert_eq!(
        tree.root().parent_tree_next_key,
        Some(3),
        "/ParentTreeNextKey 覆盖页面键与两个 OBJR 键"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("Tagged line"),
        "XObject 的 OBJR 内容项不影响页文本提取"
    );
}
