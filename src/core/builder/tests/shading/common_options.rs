//! 所有渐变共有的 `/Background`、`/BBox` 与 `/AntiAlias` 公共条目：
//! 直接字典与网格/网格片流字典的写出，以及缺省省略。

use super::super::*;
use super::support::{axial_shading, coons_patch_shading, exponential, render_with_shading};

/// 所有渐变共有的 `/Background`、`/BBox` 与 `/AntiAlias` 写入直接字典。
#[test]
fn shading_options_write_common_entries_to_direct_dictionary() {
    let shading = Shading::Axial(
        AxialShading::new(
            ColorSpace::DeviceRGB,
            [0.0, 0.0, 100.0, 0.0],
            exponential(vec![0.0, 0.0, 0.0], vec![1.0, 1.0, 1.0]),
        )
        .with_options(
            ShadingOptions::new()
                .with_background([0.25, 0.5, 0.75])
                .with_bbox([10.0, 20.0, 110.0, 70.0])
                .with_anti_alias(true),
        ),
    );
    let s = render_with_shading(shading);

    assert!(s.contains("/Background [0.25 0.5 0.75]"), "{s}");
    assert!(s.contains("/BBox [10.0 20.0 110.0 70.0]"), "{s}");
    assert!(s.contains("/AntiAlias true"), "{s}");
}

#[test]
fn shading_options_are_omitted_when_unset() {
    let s = render_with_shading(axial_shading());

    assert!(!s.contains("/Background"), "{s}");
    assert!(!s.contains("/BBox"), "{s}");
    assert!(!s.contains("/AntiAlias"), "{s}");
}

/// 网格渐变（Type 5）与网格片渐变（Type 6）的流字典同样携带公共条目。
#[test]
fn shading_options_apply_to_mesh_stream_dictionaries() {
    let options = ShadingOptions::new()
        .with_background([0.0, 0.0, 0.0])
        .with_bbox([0.0, 0.0, 100.0, 100.0])
        .with_anti_alias(false);
    let lattice = Shading::Lattice(
        LatticeMeshShading::new(
            ColorSpace::DeviceRGB,
            2,
            vec![
                MeshVertex::new(0.0, 0.0, vec![0.0, 0.0, 0.0]),
                MeshVertex::new(10.0, 0.0, vec![1.0, 0.0, 0.0]),
                MeshVertex::new(0.0, 10.0, vec![0.0, 1.0, 0.0]),
                MeshVertex::new(10.0, 10.0, vec![0.0, 0.0, 1.0]),
            ],
        )
        .with_bits(8, 8)
        .with_options(options.clone()),
    );
    let s = render_with_shading(lattice);
    assert!(s.contains("/ShadingType 5"), "{s}");
    assert!(s.contains("/Background [0.0 0.0 0.0]"), "{s}");
    assert!(s.contains("/BBox [0.0 0.0 100.0 100.0]"), "{s}");
    assert!(s.contains("/AntiAlias false"), "{s}");

    let coons = Shading::Coons(coons_patch_shading().with_options(options));
    let s = render_with_shading(coons);
    assert!(s.contains("/ShadingType 6"), "{s}");
    assert!(s.contains("/Background [0.0 0.0 0.0]"), "{s}");
    assert!(s.contains("/BBox [0.0 0.0 100.0 100.0]"), "{s}");
    assert!(s.contains("/AntiAlias false"), "{s}");
}
