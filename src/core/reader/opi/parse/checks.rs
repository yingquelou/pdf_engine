use crate::core::reader::opi::{MAX_OPI_ARRAY, OpiError, limit_exceeded, malformed};

/// Table 369 `/Position`：`ulx-llx = urx-lrx` 且 `uly-lly = ury-lry`。
pub(super) fn check_position(position: &[f64; 8], context: &str) -> Result<(), OpiError> {
    const TOLERANCE: f64 = 1e-6;
    let top_width = position[2] - position[0];
    let bottom_width = position[4] - position[6];
    let left_height = position[3] - position[1];
    let right_height = position[5] - position[7];
    if (top_width - bottom_width).abs() > TOLERANCE
        || (left_height - right_height).abs() > TOLERANCE
    {
        return Err(malformed(format!(
            "{context} /Position corners must form a rectangle \
             (ulx-llx = urx-lrx and uly-lly = ury-lry)"
        )));
    }
    Ok(())
}

/// Table 370：`/Size` 与 `/CropRect` 成对出现，且裁切框落在代理尺寸内。
pub(super) fn check_size_and_crop(
    size: Option<[f64; 2]>,
    crop: Option<[f64; 4]>,
    context: &str,
) -> Result<(), OpiError> {
    match (size, crop) {
        (None, None) => Ok(()),
        (Some(_), None) | (None, Some(_)) => Err(malformed(format!(
            "{context} /Size and /CropRect must both be present or both be absent"
        ))),
        (Some(size), Some(crop)) => {
            if size[0] <= 0.0 || size[1] <= 0.0 {
                return Err(malformed(format!(
                    "{context} /Size must be positive, got [{} {}]",
                    size[0], size[1]
                )));
            }
            if crop[0] < 0.0 || crop[1] < 0.0 || crop[0] >= crop[2] || crop[1] >= crop[3] {
                return Err(malformed(format!(
                    "{context} /CropRect must satisfy 0 <= left < right and 0 <= top < bottom"
                )));
            }
            if crop[2] > size[0] || crop[3] > size[1] {
                return Err(malformed(format!(
                    "{context} /CropRect must lie within /Size [{}, {}]",
                    size[0], size[1]
                )));
            }
            Ok(())
        }
    }
}

pub(super) fn gray_map_len(bits: i64) -> Result<usize, OpiError> {
    if !(0..=16).contains(&bits) {
        return Err(malformed(format!(
            "OPI 1.3 /ImageType bits per sample must be 0–16, got {bits}"
        )));
    }
    let len = 1usize << bits;
    if len > MAX_OPI_ARRAY {
        return Err(limit_exceeded("OPI 1.3 /GrayMap"));
    }
    Ok(len)
}
