//! 颜色空间值构造（Table 93 系列）：设备空间写名字，CIE-Based 写内联字典，
//! ICCBased 额外分配配置文件流对象，Separation/DeviceN 写出备用空间与 tint 变换函数。

use crate::core::builder::ColorSpace;
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
use crate::core::writer::WriteError;

use super::utils::{hex_bytes, real_array, write_function};

fn named_space(name: &str, dict: Dict) -> Value {
    Value::Array(Array(vec![Value::Name(name.into()), Value::Dict(dict)]))
}

/// 图片 `/ColorSpace`：设备空间写名字，CIE-Based 写 `[名称 字典]`，
/// ICCBased 额外分配配置文件流对象；Separation/DeviceN 写出备用空间
/// 与 tint 变换函数（Type 0/4 流函数分配独立对象）。
pub(crate) fn color_space_value(
    space: &ColorSpace,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Value, WriteError> {
    Ok(match space {
        ColorSpace::DeviceGray | ColorSpace::DeviceRGB | ColorSpace::DeviceCMYK => {
            Value::Name(space.device_name().expect("device color space").into())
        }
        ColorSpace::CalGray(cal) => {
            let mut dict = Dict::new().insert("WhitePoint", real_array(&cal.white_point));
            if let Some(black_point) = cal.black_point {
                dict = dict.insert("BlackPoint", real_array(&black_point));
            }
            if let Some(gamma) = cal.gamma {
                dict = dict.insert("Gamma", Value::Float(gamma));
            }
            named_space("CalGray", dict)
        }
        ColorSpace::CalRGB(cal) => {
            let mut dict = Dict::new().insert("WhitePoint", real_array(&cal.white_point));
            if let Some(black_point) = cal.black_point {
                dict = dict.insert("BlackPoint", real_array(&black_point));
            }
            if let Some(gamma) = cal.gamma {
                dict = dict.insert("Gamma", real_array(&gamma));
            }
            if let Some(matrix) = cal.matrix {
                dict = dict.insert("Matrix", real_array(&matrix));
            }
            named_space("CalRGB", dict)
        }
        ColorSpace::Lab(lab) => {
            let mut dict = Dict::new().insert("WhitePoint", real_array(&lab.white_point));
            if let Some(black_point) = lab.black_point {
                dict = dict.insert("BlackPoint", real_array(&black_point));
            }
            if let Some(range) = lab.range {
                dict = dict.insert("Range", real_array(&range));
            }
            named_space("Lab", dict)
        }
        ColorSpace::ICCBased(icc) => {
            let icc_num = *num;
            *num += 1;
            let mut dict = Dict::new().insert("N", Value::Integer(i64::from(icc.n)));
            if let Some(alternate) = &icc.alternate {
                dict = dict.insert("Alternate", color_space_value(alternate, doc, num)?);
            }
            if let Some(range) = &icc.range {
                dict = dict.insert("Range", real_array(range));
            }
            doc.set_object(Object::new(
                icc_num,
                0,
                Value::Stream(dict, StreamData::Owned(icc.data.clone())),
            ));
            Value::Array(Array(vec![
                Value::Name("ICCBased".into()),
                Value::Ref(icc_num, 0),
            ]))
        }
        ColorSpace::Indexed(indexed) => Value::Array(Array(vec![
            Value::Name("Indexed".into()),
            color_space_value(&indexed.base, doc, num)?,
            Value::Integer(i64::from(indexed.hival)),
            Value::HexString(hex_bytes(&indexed.lookup)),
        ])),
        ColorSpace::Separation(separation) => Value::Array(Array(vec![
            Value::Name("Separation".into()),
            Value::Name(separation.colorant.clone()),
            color_space_value(&separation.alternate, doc, num)?,
            write_function(doc, &separation.tint_transform, num)?,
        ])),
        ColorSpace::DeviceN(devicen) => {
            let names = Array(
                devicen
                    .colorants
                    .iter()
                    .map(|colorant| Value::Name(colorant.clone()))
                    .collect(),
            );
            Value::Array(Array(vec![
                Value::Name("DeviceN".into()),
                Value::Array(names),
                color_space_value(&devicen.alternate, doc, num)?,
                write_function(doc, &devicen.tint_transform, num)?,
            ]))
        }
    })
}
