//! Affine transform used by the extractor (PDF row-vector convention).

/// Affine transform in PDF's row-vector convention (`[x y 1] × M`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    pub(super) const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub(super) fn from_values(values: [f64; 6]) -> Self {
        Self {
            a: values[0],
            b: values[1],
            c: values[2],
            d: values[3],
            e: values[4],
            f: values[5],
        }
    }

    pub(super) fn translate(tx: f64, ty: f64) -> Self {
        Self {
            e: tx,
            f: ty,
            ..Self::IDENTITY
        }
    }

    /// Concatenation: applies `self` first, then `other`.
    pub(super) fn concat(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    pub(super) fn transform(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Vertical scaling of the linear part: the length of the image of the
    /// text-space y axis, `hypot(c, d)`.
    ///
    /// A unit of height in text space becomes this many units of the matrix's
    /// target space, so it is the factor that turns a font size (text space
    /// units, 9.3.4) into a typeface height (14.8.2.4.3 Table 332, `FontSize`
    /// "derived from the a, b, c, and d fields of the current text matrix").
    /// Pure rotation yields `1.0`, so rotation alone never changes the height.
    pub(super) fn vertical_scale(self) -> f64 {
        self.c.hypot(self.d)
    }
}
