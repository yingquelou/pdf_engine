//! 对象号分配（一）：页面字典/内容流槽位与字体、图片、缩略图、CIDFont、
//! ExtGState 的跨页去重池，以及软蒙版 `/G` 引用的表单存在性校验。

use crate::core::builder::{CidFont, Font, GraphicsState, ImageData, PdfBuilder};
use crate::core::writer::WriteError;

use super::images::{ImageObjectNums, allocate_image_objects, validate_thumbnail};
use super::plan::PoolPlan;

impl PdfBuilder {
    /// 为页面与各类去重资源池分配对象号；分配顺序与拆分前一致，故对象号
    /// 逐字节不变。软蒙版 `/G` 的存在性校验同样在此阶段完成。
    pub(super) fn allocate_pool_objects(&self, num: &mut i64) -> Result<PoolPlan, WriteError> {
        // 第一遍：为每页分配 Page 字典号 + 内容流号，并为字体/图片分配号。
        // 但字体/图片对象号要等遍历各页后才能确定连续段，故先扫一遍。
        // 这里采用「按页交织」分配：每页的 Page、Content 紧邻，
        // 字体和图片对象在所有页处理完后统一分配——但 PDF 对象号
        // 无需连续，只要引用正确即可。为简单与可断言，采用：
        //   3.. : 每页先 Page 再 Content（共 2 * N 个）
        //   之后 : 字体对象
        //   之后 : 图片对象
        let mut page_slots: Vec<(i64, i64)> = Vec::with_capacity(self.pages.len());
        for _ in 0..self.pages.len() {
            let pn = *num;
            *num += 1;
            let cn = *num;
            *num += 1;
            page_slots.push((pn, cn));
        }

        // 字体去重池：跨页合并同一 (name,size,encoding) 字体，复用同一对象号；
        // §9.6.2.3 的 MMType1 标记也须一致，否则 MM 实例会与同名 Type 1 字体
        // 挤占同一字体对象（`/Subtype` 只跟随先注册者）。
        let mut font_pool: Vec<(Font, i64)> = Vec::new();
        // 每页用到的字体在页内是 1-based 索引；映射到全局对象号。
        // 页内字体列表顺序即 text() 调用首次出现顺序；同一对象号在
        // font_pool 中按首次出现分配。
        let mut page_font_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.fonts.len());
            for f in &p.fonts {
                let pos = font_pool.iter().position(|(g, _)| f.same_dedup_key(g));
                let obj_num = match pos {
                    Some(i) => font_pool[i].1,
                    None => {
                        let n = *num;
                        *num += 1;
                        font_pool.push((f.clone(), n));
                        n
                    }
                };
                v.push(obj_num);
            }
            page_font_nums.push(v);
        }

        // 图片去重池：按内容完全相等合并（同像素即同一 XObject）。
        // 软蒙版、显式蒙版与替代图像各分配独立流对象号。
        let mut image_pool: Vec<(ImageData, ImageObjectNums)> = Vec::new();
        let mut page_image_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.images.len());
            for img in &p.images {
                let pos = image_pool.iter().position(|(g, _)| g == img);
                let obj_num = match pos {
                    Some(i) => image_pool[i].1.base,
                    None => {
                        let nums = allocate_image_objects(img, num);
                        let base = nums.base;
                        image_pool.push((img.clone(), nums));
                        base
                    }
                };
                v.push(obj_num);
            }
            page_image_nums.push(v);
        }

        // 页面缩略图（12.3.4 `/Thumb`）：与页面绘制图像共用去重池，
        // 但只作为页面字典条目引用，不进入内容资源字典。
        let mut page_thumbnail_nums: Vec<Option<i64>> = Vec::with_capacity(self.pages.len());
        for (index, page) in self.pages.iter().enumerate() {
            let Some(thumbnail) = &page.thumbnail else {
                page_thumbnail_nums.push(None);
                continue;
            };
            validate_thumbnail(thumbnail).map_err(|message| {
                WriteError::InvalidObject(format!("page {index} /Thumb: {message}"))
            })?;
            let obj_num = match image_pool.iter().position(|(g, _)| g == thumbnail) {
                Some(i) => image_pool[i].1.base,
                None => {
                    let nums = allocate_image_objects(thumbnail, num);
                    let base = nums.base;
                    image_pool.push((thumbnail.clone(), nums));
                    base
                }
            };
            page_thumbnail_nums.push(Some(obj_num));
        }

        // CIDFont 去重池：跨页合并同一 CidFont，复用同一 Type 0 父对象号。
        let mut cid_font_pool: Vec<(CidFont, i64)> = Vec::new();
        let mut page_cid_font_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.cid_fonts.len());
            for f in &p.cid_fonts {
                let pos = cid_font_pool.iter().position(|(g, _)| g == f);
                let obj_num = match pos {
                    Some(i) => cid_font_pool[i].1,
                    None => {
                        let n = *num;
                        *num += 1;
                        cid_font_pool.push((f.clone(), n));
                        n
                    }
                };
                v.push(obj_num);
            }
            page_cid_font_nums.push(v);
        }

        // ExtGState 去重池：跨页合并相同参数配置，复用同一对象号。
        let mut gs_pool: Vec<(GraphicsState, i64)> = Vec::new();
        let mut page_gs_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.graphics_states.len());
            for state in &p.graphics_states {
                let pos = gs_pool.iter().position(|(g, _)| g == state);
                let obj_num = match pos {
                    Some(i) => gs_pool[i].1,
                    None => {
                        let n = *num;
                        *num += 1;
                        gs_pool.push((state.clone(), n));
                        n
                    }
                };
                v.push(obj_num);
            }
            page_gs_nums.push(v);
        }

        // ExtGState 软蒙版的 /G 必须引用已注册且带 /Group 的 Form XObject。
        for (state, _) in &gs_pool {
            let Some(soft_mask) = &state.soft_mask else {
                continue;
            };
            let form = self.forms.get(soft_mask.group).ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "invalid graphics state: /SMask /G references unknown form handle {}",
                    soft_mask.group
                ))
            })?;
            if form.group.is_none() {
                return Err(WriteError::InvalidObject(
                    "invalid graphics state: /SMask /G must reference a transparency group Form XObject"
                        .into(),
                ));
            }
        }
        Ok(PoolPlan {
            page_slots,
            font_pool,
            page_font_nums,
            image_pool,
            page_image_nums,
            page_thumbnail_nums,
            cid_font_pool,
            page_cid_font_nums,
            gs_pool,
            page_gs_nums,
        })
    }
}
