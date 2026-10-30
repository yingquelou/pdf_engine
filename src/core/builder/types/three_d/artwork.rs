//! 3D 流字典（Table 300）的构建态。
//!
//! 每个 artwork 写成独立间接流对象：`/Type /3D`、必填 `/Subtype`，以及
//! 非缺省的 `/VA` 视图数组、`/DV` 默认视图与 `/Resources`。U3D 字节原样
//! 写入，不附加过滤器。`/OnInstantiate` 是 JavaScript 流，经
//! [`ThreeDArtwork::with_on_instantiate`] 声明后写成独立间接流对象并由流
//! 字典引用。
//!
//! 默认注解 `/3DD` 直接引用 3D 流；启用
//! [`ThreeDArtwork::with_reference_dictionary`] 后改由 Table 303 引用字典
//! 承载，多个 3D 注解引用同一字典时共享一个运行实例（13.6.3.3）。

use super::super::validate_name_value;
use super::{ThreeDView, ThreeDViewSelection};
use crate::core::reader::three_d::ThreeDStreamFormat;
use crate::core::types::{Array, Dict, StreamData, Value};
use std::collections::BTreeSet;

/// 3D artwork（Table 300）的构建态（尚未分配对象号）。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDArtwork {
    data: Vec<u8>,
    format: ThreeDStreamFormat,
    views: Vec<ThreeDView>,
    default_view: Option<ThreeDViewSelection>,
    resources: Option<Dict>,
    reference: bool,
    on_instantiate: Option<Vec<u8>>,
}

impl ThreeDArtwork {
    /// 新建 U3D artwork；`data` 为 3D 流字节。
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        Self {
            data: data.into(),
            format: ThreeDStreamFormat::U3d,
            views: Vec::new(),
            default_view: None,
            resources: None,
            reference: false,
            on_instantiate: None,
        }
    }

    /// 数据格式 `/Subtype`；缺省 `U3D`。
    pub fn with_format(mut self, format: ThreeDStreamFormat) -> Self {
        self.format = format;
        self
    }

    /// 追加一个预设视图 `/VA`。
    pub fn with_view(mut self, view: ThreeDView) -> Self {
        self.views.push(view);
        self
    }

    /// 替换全部预设视图 `/VA`。
    pub fn with_views(mut self, views: Vec<ThreeDView>) -> Self {
        self.views = views;
        self
    }

    /// 默认视图 `/DV`；未声明且 `/VA` 非空时呈现端取第一项。
    pub fn with_default_view(mut self, view: ThreeDViewSelection) -> Self {
        self.default_view = Some(view);
        self
    }

    /// 资源名称树 `/Resources`（原样写出）。
    pub fn with_resources(mut self, resources: Dict) -> Self {
        self.resources = Some(resources);
        self
    }

    /// 以 3D 引用字典（Table 303）包装本 artwork。
    ///
    /// 注解 `/3DD` 改指引用字典，引用同一字典的 3D 注解共享一个运行实例；
    /// 未启用时注解直接引用 3D 流，各自实例化（13.6.3.3）。
    pub fn with_reference_dictionary(mut self) -> Self {
        self.reference = true;
        self
    }

    /// 实例化时执行的 JavaScript `/OnInstantiate`（Table 300）。
    ///
    /// 脚本写成独立间接流对象（仅含 `/Length`），由 3D 流字典引用。
    pub fn with_on_instantiate(mut self, script: impl Into<Vec<u8>>) -> Self {
        self.on_instantiate = Some(script.into());
        self
    }

    /// 视图数组（注解 `/3DV` 的下标与按名选择在此校验）。
    pub(crate) fn views(&self) -> &[ThreeDView] {
        &self.views
    }

    /// 是否需要写出 3D 引用字典（Table 303）。
    pub(crate) fn wants_reference_dictionary(&self) -> bool {
        self.reference
    }

    /// `/OnInstantiate` JavaScript 源字节。
    pub(crate) fn on_instantiate(&self) -> Option<&[u8]> {
        self.on_instantiate.as_deref()
    }

    /// 校验 `/Subtype`、视图集合与默认视图。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let ThreeDStreamFormat::Other(name) = &self.format {
            validate_name_value(name, "3D stream /Subtype")?;
        }
        let mut external = BTreeSet::new();
        let mut internal = BTreeSet::new();
        for (index, view) in self.views.iter().enumerate() {
            view.validate()
                .map_err(|message| format!("3D view {index}: {message}"))?;
            if !external.insert(view.external_name()) {
                return Err(format!(
                    "3D view /XN {:?} is registered twice",
                    view.external_name()
                ));
            }
            if let Some(name) = view.internal_name()
                && !internal.insert(name)
            {
                return Err(format!("3D view /IN {name:?} is registered twice"));
            }
        }
        if let Some(view) = &self.default_view {
            view.validate(&self.views, "3D stream /DV")?;
        }
        Ok(())
    }

    /// 写成 3D 流对象值；`/OnInstantiate` 为独立流对象号时写成间接引用，
    /// `/Length` 与数据由 writer 原样输出。
    pub(crate) fn to_stream_value(&self, on_instantiate: Option<i64>) -> Value {
        let mut dict = Dict::new().insert("Type", Value::Name("3D".into())).insert(
            "Subtype",
            Value::Name(match &self.format {
                ThreeDStreamFormat::U3d => "U3D".into(),
                ThreeDStreamFormat::Other(name) => name.clone(),
            }),
        );
        if !self.views.is_empty() {
            let views = self
                .views
                .iter()
                .map(|view| Value::Dict(view.to_dict()))
                .collect();
            dict = dict.insert("VA", Value::Array(Array(views)));
        }
        if let Some(default) = &self.default_view {
            let value = default
                .to_value(&self.views, "3D stream /DV")
                .expect("3D artwork is validated before serialization");
            dict = dict.insert("DV", value);
        }
        if let Some(resources) = &self.resources {
            dict = dict.insert("Resources", Value::Dict(resources.clone()));
        }
        if let Some(number) = on_instantiate {
            dict = dict.insert("OnInstantiate", Value::Ref(number, 0));
        }
        dict = dict.insert("Length", Value::Integer(self.data.len() as i64));
        Value::Stream(dict, StreamData::Owned(self.data.clone()))
    }
}
