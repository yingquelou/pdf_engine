//! 页面矩形模型（7.9.5，任意对角顺序构造时归一化为左下/右上角）。

/// 页面矩形（`[llx lly urx ury]`）。
///
/// 规范 7.9.5 允许矩形数组以任意对角线顺序给出坐标，构造时统一按
/// min/max 归一化为左下/右上角。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rectangle {
    /// 左下角 x。
    pub llx: f64,
    /// 左下角 y。
    pub lly: f64,
    /// 右上角 x。
    pub urx: f64,
    /// 右上角 y。
    pub ury: f64,
}

impl Rectangle {
    /// 按任意对角顺序构造矩形（坐标自动归一化）。
    pub fn new(llx: f64, lly: f64, urx: f64, ury: f64) -> Self {
        Self {
            llx: llx.min(urx),
            lly: lly.min(ury),
            urx: llx.max(urx),
            ury: lly.max(ury),
        }
    }

    /// 宽度（非负）。
    pub fn width(&self) -> f64 {
        self.urx - self.llx
    }

    /// 高度（非负）。
    pub fn height(&self) -> f64 {
        self.ury - self.lly
    }

    /// 两矩形交集。
    ///
    /// 无重叠时返回退化的零宽/零高矩形：下界钳制在交集上界（最近边界）
    /// 上，保证结果仍在两矩形范围内且 `width()` / `height()` 非负。
    pub fn intersect(self, other: Rectangle) -> Rectangle {
        let urx = self.urx.min(other.urx);
        let ury = self.ury.min(other.ury);
        let llx = self.llx.max(other.llx).min(urx);
        let lly = self.lly.max(other.lly).min(ury);
        Rectangle { llx, lly, urx, ury }
    }
}
