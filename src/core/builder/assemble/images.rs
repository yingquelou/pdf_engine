//! 图像 XObject 流对象：对象号分配、基图与软蒙版/显式蒙版/替代图像的写出，
//! 以及页面缩略图（12.3.4 `/Thumb`）的取值校验。

use crate::core::builder::{ColorSpace, ImageData, ImageMask};
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, StreamData, Value};
use crate::core::writer::WriteError;

use super::colorspace::color_space_value;
use super::opi;
use super::utils::real_array;

/// 12.3.4：页面缩略图须为常规图像，颜色空间限于 DeviceGray、
/// DeviceRGB 或基于二者的 Indexed。
pub(super) fn validate_thumbnail(img: &ImageData) -> Result<(), String> {
    if img.image_mask {
        return Err("thumbnail must not be an image mask".into());
    }
    let supported = match &img.color_space {
        ColorSpace::DeviceGray | ColorSpace::DeviceRGB => true,
        ColorSpace::Indexed(indexed) => matches!(
            indexed.base.as_ref(),
            ColorSpace::DeviceGray | ColorSpace::DeviceRGB
        ),
        _ => false,
    };
    if !supported {
        return Err(
            "thumbnail color space must be DeviceGray, DeviceRGB or Indexed based on them".into(),
        );
    }
    Ok(())
}

/// 图像及其从属对象（软蒙版、显式蒙版、替代图像）的对象号。
pub(super) struct ImageObjectNums {
    pub(super) base: i64,
    pub(super) smask: Option<i64>,
    pub(super) mask: Option<i64>,
    pub(super) alternates: Vec<ImageObjectNums>,
}

/// 按「基图 → SMask → Mask → 替代图像」顺序分配对象号。
pub(super) fn allocate_image_objects(img: &ImageData, num: &mut i64) -> ImageObjectNums {
    let base = *num;
    *num += 1;
    let smask = img.smask.as_ref().map(|_| {
        let mask_num = *num;
        *num += 1;
        mask_num
    });
    let mask = if matches!(img.mask, Some(ImageMask::Explicit(_))) {
        let mask_num = *num;
        *num += 1;
        Some(mask_num)
    } else {
        None
    };
    let alternates = img
        .alternates
        .iter()
        .map(|alternate| allocate_image_objects(alternate, num))
        .collect();
    ImageObjectNums {
        base,
        smask,
        mask,
        alternates,
    }
}

/// 写出图像 XObject 及其软蒙版、显式蒙版与替代图像。
///
/// 替代图像以 `<< /Image n 0 R >>` 字典数组挂在基图 `/Alternates`
/// 下（Table 90），自身再作为普通图像流对象写出。
pub(super) fn write_image(
    doc: &mut Document,
    img: &ImageData,
    nums: &ImageObjectNums,
    num: &mut i64,
) -> Result<(), WriteError> {
    img.validate()
        .map_err(|message| WriteError::InvalidObject(format!("image XObject: {message}")))?;
    let mut stream_dict = if img.image_mask {
        image_mask_dict(img)
    } else {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("XObject".into()))
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(img.width as i64))
            .insert("Height", Value::Integer(img.height as i64))
            .insert("ColorSpace", color_space_value(&img.color_space, doc, num)?)
            .insert("BitsPerComponent", Value::Integer(img.bits as i64));
        if let Some(decode) = &img.decode {
            dict = dict.insert("Decode", real_array(decode));
        }
        if img.interpolate {
            dict = dict.insert("Interpolate", Value::Bool(true));
        }
        if let Some(intent) = img.intent {
            dict = dict.insert("Intent", Value::Name(intent.name().into()));
            doc.features.borrow_mut().insert(PdfFeature::ImageIntent);
        }
        if !nums.alternates.is_empty() {
            let alternates: Vec<Value> = nums
                .alternates
                .iter()
                .map(|alternate| {
                    Value::Dict(Dict::new().insert("Image", Value::Ref(alternate.base, 0)))
                })
                .collect();
            dict = dict.insert("Alternates", Value::Array(Array(alternates)));
            doc.features
                .borrow_mut()
                .insert(PdfFeature::AlternateImages);
        }
        dict
    };
    stream_dict = opi::insert_opi(doc, stream_dict, &img.opi);
    if let Some(smask_num) = nums.smask {
        stream_dict = stream_dict.insert("SMask", Value::Ref(smask_num, 0));
    }
    match &img.mask {
        None => {}
        Some(ImageMask::Explicit(_)) => {
            if let Some(mask_num) = nums.mask {
                stream_dict = stream_dict.insert("Mask", Value::Ref(mask_num, 0));
            }
            doc.features.borrow_mut().insert(PdfFeature::ImageMaskEntry);
        }
        Some(ImageMask::ColorKey(ranges)) => {
            let array = Array(ranges.iter().map(|value| Value::Integer(*value)).collect());
            stream_dict = stream_dict.insert("Mask", Value::Array(array));
            doc.features.borrow_mut().insert(PdfFeature::ImageMaskEntry);
        }
    }
    doc.set_object(Object::new(
        nums.base,
        0,
        Value::Stream(stream_dict, StreamData::Owned(img.data.clone())),
    ));

    if let (Some(smask_num), Some(smask)) = (nums.smask, &img.smask) {
        let mut mask_dict = Dict::new()
            .insert("Type", Value::Name("XObject".into()))
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(smask.width as i64))
            .insert("Height", Value::Integer(smask.height as i64))
            .insert("ColorSpace", Value::Name("DeviceGray".into()))
            .insert("BitsPerComponent", Value::Integer(smask.bits as i64));
        if let Some(decode) = &smask.decode {
            mask_dict = mask_dict.insert("Decode", real_array(decode));
        }
        if smask.interpolate {
            mask_dict = mask_dict.insert("Interpolate", Value::Bool(true));
        }
        // Table 89：`/Matte` 属于软蒙版图像，分量数按基图颜色空间解释。
        if let Some(matte) = &img.matte {
            mask_dict = mask_dict.insert("Matte", real_array(matte));
            doc.features.borrow_mut().insert(PdfFeature::SoftMaskMatte);
        }
        mask_dict = opi::insert_opi(doc, mask_dict, &smask.opi);
        doc.set_object(Object::new(
            smask_num,
            0,
            Value::Stream(mask_dict, StreamData::Owned(smask.data.clone())),
        ));
        doc.features.borrow_mut().insert(PdfFeature::Transparency);
    }

    if let (Some(mask_num), Some(ImageMask::Explicit(mask))) = (nums.mask, &img.mask) {
        let mask_dict = opi::insert_opi(doc, image_mask_dict(mask), &mask.opi);
        doc.set_object(Object::new(
            mask_num,
            0,
            Value::Stream(mask_dict, StreamData::Owned(mask.data.clone())),
        ));
    }

    for (alternate, alternate_nums) in img.alternates.iter().zip(&nums.alternates) {
        write_image(doc, alternate, alternate_nums, num)?;
    }
    Ok(())
}

/// 图像蒙版（`/ImageMask true`）的流字典：不写 `/ColorSpace`，
/// 位深固定为 1。
fn image_mask_dict(mask: &ImageData) -> Dict {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("XObject".into()))
        .insert("Subtype", Value::Name("Image".into()))
        .insert("ImageMask", Value::Bool(true))
        .insert("Width", Value::Integer(mask.width as i64))
        .insert("Height", Value::Integer(mask.height as i64))
        .insert("BitsPerComponent", Value::Integer(1));
    if let Some(decode) = &mask.decode {
        dict = dict.insert("Decode", real_array(decode));
    }
    if mask.interpolate {
        dict = dict.insert("Interpolate", Value::Bool(true));
    }
    dict
}
