//! Type 1–3 渐变构造：函数式（轴向参数域为二维矩形）、轴向与径向渐变。

use crate::core::builder::types::{ColorSpace, Function};

use super::options::ShadingOptions;
use super::validate::{ensure_finite, validate_color_function, validate_domain};

/// Type 1 函数式渐变：以二维域上的函数直接给出颜色（`/ShadingType 1`）。
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionShading {
    pub color_space: ColorSpace,
    /// `[xmin xmax ymin ymax]`。
    pub domain: [f64; 4],
    /// 从渐变空间到用户空间的变换矩阵。
    pub matrix: Option<[f64; 6]>,
    pub function: Function,
    pub options: ShadingOptions,
}

impl FunctionShading {
    pub fn new(color_space: ColorSpace, domain: [f64; 4], function: Function) -> Self {
        Self {
            color_space,
            domain,
            matrix: None,
            function,
            options: ShadingOptions::new(),
        }
    }

    pub fn with_matrix(mut self, matrix: [f64; 6]) -> Self {
        self.matrix = Some(matrix);
        self
    }

    pub fn with_options(mut self, options: ShadingOptions) -> Self {
        self.options = options;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        ensure_finite(&self.domain, "type 1 shading Domain")?;
        if self.domain[0] >= self.domain[1] || self.domain[2] >= self.domain[3] {
            return Err("type 1 shading Domain minimum must be less than maximum".into());
        }
        if let Some(matrix) = self.matrix {
            ensure_finite(&matrix, "type 1 shading Matrix")?;
        }
        validate_color_function("type 1 shading", &self.color_space, &self.function, 2)?;
        self.options.validate(&self.color_space)
    }
}

/// Type 2 轴向渐变：沿两点连线插值（`/ShadingType 2`）。
#[derive(Debug, Clone, PartialEq)]
pub struct AxialShading {
    pub color_space: ColorSpace,
    /// `[x0 y0 x1 y1]`。
    pub coords: [f64; 4],
    /// 参数变量域；省略时按规范默认 `[0 1]`。
    pub domain: Option<[f64; 2]>,
    pub function: Function,
    /// 两端点之外的延伸行为；省略时默认 `[false false]`。
    pub extend: Option<[bool; 2]>,
    pub options: ShadingOptions,
}

impl AxialShading {
    pub fn new(color_space: ColorSpace, coords: [f64; 4], function: Function) -> Self {
        Self {
            color_space,
            coords,
            domain: None,
            function,
            extend: None,
            options: ShadingOptions::new(),
        }
    }

    pub fn with_domain(mut self, domain: [f64; 2]) -> Self {
        self.domain = Some(domain);
        self
    }

    pub fn with_extend(mut self, extend: [bool; 2]) -> Self {
        self.extend = Some(extend);
        self
    }

    pub fn with_options(mut self, options: ShadingOptions) -> Self {
        self.options = options;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        ensure_finite(&self.coords, "type 2 shading Coords")?;
        if let Some(domain) = self.domain {
            validate_domain(domain, "type 2 shading Domain")?;
        }
        validate_color_function("type 2 shading", &self.color_space, &self.function, 1)?;
        self.options.validate(&self.color_space)
    }
}

/// Type 3 径向渐变：两圆之间的径向插值（`/ShadingType 3`）。
#[derive(Debug, Clone, PartialEq)]
pub struct RadialShading {
    pub color_space: ColorSpace,
    /// `[x0 y0 r0 x1 y1 r1]`，半径不得为负。
    pub coords: [f64; 6],
    /// 参数变量域；省略时按规范默认 `[0 1]`。
    pub domain: Option<[f64; 2]>,
    pub function: Function,
    /// 两圆之外的延伸行为；省略时默认 `[false false]`。
    pub extend: Option<[bool; 2]>,
    pub options: ShadingOptions,
}

impl RadialShading {
    pub fn new(color_space: ColorSpace, coords: [f64; 6], function: Function) -> Self {
        Self {
            color_space,
            coords,
            domain: None,
            function,
            extend: None,
            options: ShadingOptions::new(),
        }
    }

    pub fn with_domain(mut self, domain: [f64; 2]) -> Self {
        self.domain = Some(domain);
        self
    }

    pub fn with_extend(mut self, extend: [bool; 2]) -> Self {
        self.extend = Some(extend);
        self
    }

    pub fn with_options(mut self, options: ShadingOptions) -> Self {
        self.options = options;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        ensure_finite(&self.coords, "type 3 shading Coords")?;
        if self.coords[2] < 0.0 || self.coords[5] < 0.0 {
            return Err("type 3 shading radii must not be negative".into());
        }
        if let Some(domain) = self.domain {
            validate_domain(domain, "type 3 shading Domain")?;
        }
        validate_color_function("type 3 shading", &self.color_space, &self.function, 1)?;
        self.options.validate(&self.color_space)
    }
}
