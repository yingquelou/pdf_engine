//! 图像构建期校验：按行对齐的样本字节数计算，以及颜色空间、样本长度、
//! `/Decode`、软蒙版、图像蒙版、颜色键蒙版、`/Matte` 与替代图像的合法性。

use super::color_space::ColorSpace;
use super::image_data::{ImageData, ImageMask};

impl ImageData {
    /// 按行对齐计算的样本字节数；维度或位深非法时返回 `None`。
    pub(crate) fn expected_data_len(&self) -> Option<usize> {
        if self.width == 0 || self.height == 0 || !matches!(self.bits, 1 | 2 | 4 | 8 | 16) {
            return None;
        }
        let row_bits = (self.width as usize)
            .checked_mul(self.color_space.channels())?
            .checked_mul(self.bits as usize)?;
        row_bits.div_ceil(8).checked_mul(self.height as usize)
    }

    /// 校验颜色空间、样本长度、`/Decode`、软蒙版与图像蒙版；
    /// 非法时返回可读原因。
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.color_space
            .validate()
            .map_err(|message| format!("color space: {message}"))?;
        if self.image_mask {
            if !matches!(self.color_space, ColorSpace::DeviceGray) {
                return Err("image mask color space must be DeviceGray".into());
            }
            if self.bits != 1 {
                return Err(format!(
                    "image mask requires BitsPerComponent 1, got {}",
                    self.bits
                ));
            }
            if self.smask.is_some() {
                return Err("image mask must not have an SMask".into());
            }
            if self.mask.is_some() {
                return Err("image mask must not have a Mask entry".into());
            }
            if self.intent.is_some() {
                return Err("image mask must not have an Intent entry".into());
            }
            if self.matte.is_some() {
                return Err("image mask must not have a Matte entry".into());
            }
            if !self.alternates.is_empty() {
                return Err("image mask must not have Alternates".into());
            }
        }
        let expected = self.expected_data_len().ok_or_else(|| {
            format!(
                "invalid dimensions {}x{} or bits per component {}",
                self.width, self.height, self.bits
            )
        })?;
        if self.data.len() != expected {
            return Err(format!(
                "data length {} does not match expected {expected} for {} {}x{} at {} bits",
                self.data.len(),
                self.color_space.name(),
                self.width,
                self.height,
                self.bits
            ));
        }
        if let Some(decode) = &self.decode {
            let expected = self.color_space.channels() * 2;
            if decode.len() != expected {
                return Err(format!(
                    "Decode must contain {expected} numbers for a {}-component image, got {}",
                    self.color_space.channels(),
                    decode.len()
                ));
            }
            if decode.iter().any(|v| !v.is_finite()) {
                return Err("Decode must contain finite numbers".into());
            }
            if self.image_mask
                && !(decode.len() == 2
                    && ((decode[0] == 0.0 && decode[1] == 1.0)
                        || (decode[0] == 1.0 && decode[1] == 0.0)))
            {
                return Err("image mask Decode must be [0 1] or [1 0]".into());
            }
        }
        if let Some(smask) = &self.smask {
            if smask.width != self.width || smask.height != self.height {
                return Err(format!(
                    "SMask dimensions {}x{} do not match image {}x{}",
                    smask.width, smask.height, self.width, self.height
                ));
            }
            if !matches!(smask.color_space, ColorSpace::DeviceGray) {
                return Err("SMask color space must be DeviceGray".into());
            }
            if smask.smask.is_some() {
                return Err("SMask must not contain another SMask".into());
            }
            smask
                .validate()
                .map_err(|message| format!("SMask: {message}"))?;
        }
        match &self.mask {
            None => {}
            Some(ImageMask::Explicit(mask)) => {
                if !mask.image_mask {
                    return Err("Mask image must be an image mask (ImageMask true)".into());
                }
                mask.validate()
                    .map_err(|message| format!("Mask: {message}"))?;
            }
            Some(ImageMask::ColorKey(ranges)) => {
                let components = self.color_space.channels();
                let expected = components * 2;
                if ranges.len() != expected {
                    return Err(format!(
                        "color key Mask must contain {expected} integers for a \
                         {components}-component image, got {}",
                        ranges.len()
                    ));
                }
                let limit = (1i64 << self.bits) - 1;
                for pair in ranges.as_chunks::<2>().0 {
                    if pair[0] < 0 || pair[0] > pair[1] || pair[1] > limit {
                        return Err(format!(
                            "color key Mask ranges must satisfy 0 <= min <= max <= {limit}"
                        ));
                    }
                }
            }
        }
        if let Some(matte) = &self.matte {
            if self.smask.is_none() {
                return Err("Matte requires an SMask".into());
            }
            let components = self.color_space.channels();
            if matte.len() != components {
                return Err(format!(
                    "Matte must contain {components} numbers for a \
                     {components}-component image, got {}",
                    matte.len()
                ));
            }
            if matte.iter().any(|value| !value.is_finite()) {
                return Err("Matte must contain finite numbers".into());
            }
        }
        let mut default_for_printing = 0;
        for (index, alternate) in self.alternates.iter().enumerate() {
            let image = &alternate.image;
            if image.image_mask {
                return Err(format!("alternate image {index} must not be an image mask"));
            }
            if !image.alternates.is_empty() {
                return Err(format!(
                    "alternate image {index} must not contain nested Alternates"
                ));
            }
            if image.width > self.width || image.height > self.height {
                return Err(format!(
                    "alternate image {index} dimensions {}x{} exceed the base image {}x{}",
                    image.width, image.height, self.width, self.height
                ));
            }
            if alternate.default_for_printing {
                default_for_printing += 1;
            }
            image
                .validate()
                .map_err(|message| format!("alternate image {index}: {message}"))?;
        }
        // Table 91：「At most one alternate for a given base image shall be
        // so designated.」
        if default_for_printing > 1 {
            return Err(
                "at most one alternate image may set DefaultForPrinting".into(),
            );
        }
        Ok(())
    }
}
