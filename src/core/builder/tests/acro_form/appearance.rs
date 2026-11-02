//! Widget 外观写出：`/AP` 的 `/N`、`/D` 状态子字典指向表单 XObject 池中同一个
//! 间接对象（同一句柄复用同一外观流），以及 `/AS` 选中态的写出与回读。

use super::super::*;
use crate::core::content::ContentStream;

#[test]
fn widget_appearance_references_pool_form_streams() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let handle = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ));
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::checkbox("subscribe", [72.0, 560.0, 92.0, 580.0])
            .with_appearance(handle)
            .with_appearance_down(handle)
            .with_selected_appearance_state("Yes"),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let parsed = doc.acro_form().expect("read").expect("form present");
    let field = &parsed.fields[0];
    let appearance = field
        .appearance
        .as_ref()
        .and_then(|value| value.as_dict())
        .expect("/AP dictionary");
    let (normal_num, _) = appearance
        .get("N")
        .and_then(|value| value.as_ref())
        .expect("/N");
    let (down_num, _) = appearance
        .get("D")
        .and_then(|value| value.as_ref())
        .expect("/D");
    assert_eq!(normal_num, down_num, "same form handle is shared");
    assert_eq!(field.appearance_state.as_deref(), Some("Yes"));
    let object = doc.get_object(normal_num).expect("lookup").expect("exists");
    match object.value {
        Value::Stream(ref dict, _) => {
            assert_eq!(dict.get("Type"), Some(&Value::Name("XObject".into())));
            assert_eq!(dict.get("Subtype"), Some(&Value::Name("Form".into())));
        }
        other => panic!("expected Form XObject stream, got {other:?}"),
    }
}
