//! 11.3.5 标准混合模式分类（Table 136/137）测试。

use super::*;
use crate::core::reader::graphics_state::StandardBlendMode;

#[test]
fn standard_blend_modes_cover_table_136_and_137() {
    let separable = [
        "Normal",
        "Compatible",
        "Multiply",
        "Screen",
        "Overlay",
        "Darken",
        "Lighten",
        "ColorDodge",
        "ColorBurn",
        "HardLight",
        "SoftLight",
        "Difference",
        "Exclusion",
    ];
    let nonseparable = ["Hue", "Saturation", "Color", "Luminosity"];
    for name in separable {
        let mode = StandardBlendMode::from_name(name)
            .unwrap_or_else(|| panic!("{name} should be a standard blend mode"));
        assert_eq!(mode.as_str(), name);
        assert!(mode.is_separable(), "{name} is a Table 136 mode");
    }
    for name in nonseparable {
        let mode = StandardBlendMode::from_name(name)
            .unwrap_or_else(|| panic!("{name} should be a standard blend mode"));
        assert_eq!(mode.as_str(), name);
        assert!(!mode.is_separable(), "{name} is a Table 137 mode");
    }
    assert_eq!(StandardBlendMode::from_name("normal"), None);
    assert_eq!(StandardBlendMode::from_name("CustomMode"), None);
}

#[test]
fn blend_mode_classification_preserves_names_and_order() {
    let single = BlendMode::Name("Multiply".into());
    assert_eq!(single.names(), ["Multiply"]);
    assert_eq!(
        single.standard_modes(),
        vec![Some(StandardBlendMode::Multiply)]
    );

    let fallback = BlendMode::Array(vec!["CustomMode".into(), "Hue".into(), "Screen".into()]);
    assert_eq!(fallback.names(), ["CustomMode", "Hue", "Screen"]);
    assert_eq!(
        fallback.standard_modes(),
        vec![
            None,
            Some(StandardBlendMode::Hue),
            Some(StandardBlendMode::Screen)
        ]
    );
}
