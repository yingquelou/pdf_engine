//! 3D artwork 写端模型（ISO 32000-1 13.6，PDF 1.6）。
//!
//! [`ThreeDArtwork`] 经
//! [`PdfBuilder::add_three_d_artwork`](crate::core::builder::PdfBuilder::add_three_d_artwork)
//! 注册为句柄，`build` 时写成独立间接流对象（Table 300）；3D 注解
//! （Table 298）以 `/3DD` 引用该流，并可附带 `/3DV` `/3DA` `/3DI` `/3DB`。
//! 视图（Table 304）、投影（Table 305）、背景（Table 306）与激活字典
//! （Table 299）的取值模型与读端逐项对应，缺省条目不物化。
//!
//! 取值枚举直接复用读端类型，保证名字拼写与往返一致；各结构体只保留
//! 写端需要的构建方法。U3D 字节不解码，`/RM` `/LS` `/SA` `/NA` 原样透传。

mod activation;
mod artwork;
mod background;
mod projection;
mod view;

pub use activation::ThreeDActivation;
pub use artwork::ThreeDArtwork;
pub use background::ThreeDBackground;
pub use projection::ThreeDProjection;
pub use view::{ThreeDView, ThreeDViewSelection};

pub use crate::core::reader::three_d::{
    ThreeDActivationPolicy, ThreeDArtworkState, ThreeDBackgroundType, ThreeDClipping,
    ThreeDDeactivationPolicy, ThreeDMatrixSource, ThreeDProjectionBinding, ThreeDProjectionScale,
    ThreeDProjectionType, ThreeDStreamFormat,
};
