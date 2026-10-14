//! verify-visual:像素对比断言核(纯像素算术,单测主场)。
//! 双帧灰度对比——mean absolute diff + 变化率(变化胞占比)+ 变化区域
//! 连通域 bbox。判据语义:diff 与 changed_ratio 双通道(阈值 0 = 该通道
//! 不判),changed>0 即「有变化」、changed==0 即「无变化」——调用方按
//! 断言方向(verify-visual 的 --changed / --unchanged)取用,本核只算不判。
//! 与 image_regions 同层共享 10px 胞网格(变化区域 = 差值胞的 8 连通域,
//! 阅读序);尺寸不一致 = PROTOCOL(胶合层拒,调用方两图必须同源同坐标系)。
use crate::image_regions::{CELL, Region};

/// 双帧对比结果。
pub struct VisualDiff {
    /// 平均绝对灰度差(0-255;全图逐像素,2px 采样降开销)
    pub mean_abs_diff: f64,
    /// 变化率:变化胞(cells)占总胞数的比例(0-1)
    pub changed_ratio: f64,
    /// 变化区域(阅读序;空 = 无变化)
    pub regions: Vec<Region>,
    /// 参与对比的像素数(采样后)
    pub samples: u64,
    /// 胞总数(网格维度)
    pub cells_total: u32,
}

/// 胞级变化阈值:单胞平均绝对差 ≥ 此值算「变化胞」。默认 12(灰度 4.7%
/// ——抗 PNG 有损压缩/亚像素渲染噪声;朋友圈时间戳重绘实证 1-5 级噪声
/// 不触发、内容大改 60+ 触发,12 是分离带中点)。
pub const CELL_DIFF_THRESHOLD: u16 = 12;

/// 对比两张同尺寸灰度图。返回 None 当尺寸不一致(胶合层转 PROTOCOL)。
pub fn diff(
    a: &crate::template_match::Gray,
    b: &crate::template_match::Gray,
) -> Option<VisualDiff> {
    if a.w != b.w || a.h != b.h {
        return None;
    }
    let mut sum: u64 = 0;
    let mut samples: u64 = 0;
    // 2px 步采样(4× 开销降;均值统计稳定)
    let mut y = 0;
    while y < a.h {
        let mut x = 0;
        while x < a.w {
            sum += (a.data[y * a.w + x] as i32 - b.data[y * a.w + x] as i32).unsigned_abs() as u64;
            samples += 1;
            x += 2;
        }
        y += 2;
    }
    let mean_abs_diff = if samples == 0 {
        0.0
    } else {
        sum as f64 / samples as f64
    };

    // 胞网格:每胞平均绝对差(全胞内采样),超阈值为变化胞
    let cw = a.w / CELL;
    let ch = a.h / CELL;
    let mut cell_diffs = vec![0u32; cw * ch];
    let mut changed_cells = 0u32;
    for cy in 0..ch {
        for cx in 0..cw {
            let mut csum: u64 = 0;
            let mut n: u64 = 0;
            let mut dy = 0;
            while dy < CELL {
                let mut dx = 0;
                while dx < CELL {
                    let (x, y) = (cx * CELL + dx, cy * CELL + dy);
                    csum += (a.data[y * a.w + x] as i32 - b.data[y * a.w + x] as i32).unsigned_abs()
                        as u64;
                    n += 1;
                    dx += 2;
                }
                dy += 2;
            }
            let avg = (csum / n.max(1)) as u16;
            cell_diffs[cy * cw + cx] = avg as u32;
            if u32::from(avg) >= CELL_DIFF_THRESHOLD as u32 {
                changed_cells += 1;
            }
        }
    }
    let cells_total = (cw * ch) as u32;
    let changed_ratio = if cells_total == 0 {
        0.0
    } else {
        changed_cells as f64 / cells_total as f64
    };

    // 变化区域:变化胞的 8 连通域(复用 image_regions 的邻域语义;手写小版
    // 避免跨模块私有依赖——components() 吃 f64 方差,这里吃 u32 差值)
    let mut label = vec![-1i32; cw * ch];
    let mut regions: Vec<Region> = Vec::new();
    let mut next = 0i32;
    for start in 0..(cw * ch) {
        if label[start] != -1 || cell_diffs[start] < CELL_DIFF_THRESHOLD as u32 {
            continue;
        }
        let mut stack = vec![start];
        label[start] = next;
        let (mut x0, mut y0, mut x1, mut y1, mut cells) =
            (start % cw, start / cw, start % cw, start / cw, 1u32);
        while let Some(idx) = stack.pop() {
            let x = idx % cw;
            let y = idx / cw;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= cw as i32 || ny >= ch as i32 {
                        continue;
                    }
                    let j = ny as usize * cw + nx as usize;
                    if label[j] == -1 && cell_diffs[j] >= CELL_DIFF_THRESHOLD as u32 {
                        label[j] = next;
                        stack.push(j);
                        x0 = x0.min(j % cw);
                        y0 = y0.min(j / cw);
                        x1 = x1.max(j % cw);
                        y1 = y1.max(j / cw);
                        cells += 1;
                    }
                }
            }
        }
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        regions.push(Region {
            x: x0 as i32 * CELL as i32,
            y: y0 as i32 * CELL as i32,
            w: bw as i32 * CELL as i32,
            h: bh as i32 * CELL as i32,
            density: (cells as f64 / (bw * bh) as f64 * 1000.0).round() / 1000.0,
            cells,
            motion: 0.0,
        });
        next += 1;
    }
    regions.sort_by_key(|r| (r.y, r.x));

    Some(VisualDiff {
        mean_abs_diff,
        changed_ratio,
        regions,
        samples,
        cells_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template_match::Gray;

    fn gray(w: usize, h: usize, fill: u8) -> Gray {
        Gray {
            w,
            h,
            data: vec![fill; w * h],
        }
    }

    #[test]
    fn identical_frames_report_zero_everything() {
        let a = gray(200, 200, 100);
        let d = diff(&a, &a).unwrap();
        assert_eq!(d.mean_abs_diff, 0.0);
        assert_eq!(d.changed_ratio, 0.0);
        assert!(d.regions.is_empty());
    }

    #[test]
    fn size_mismatch_is_none() {
        let a = gray(200, 200, 100);
        let b = gray(100, 200, 100);
        assert!(diff(&a, &b).is_none());
    }

    #[test]
    fn changed_block_is_located_with_bbox_and_ratio() {
        // 200×200 平底 + (40,60) 处 60×60 块整体 +80 灰度
        let mut b = gray(200, 200, 100);
        for y in 60..120 {
            for x in 40..100 {
                b.data[y * 200 + x] = 180;
            }
        }
        let d = diff(&gray(200, 200, 100), &b).unwrap();
        assert!(d.mean_abs_diff > 0.0);
        assert!(d.changed_ratio > 0.0);
        assert_eq!(d.regions.len(), 1, "one contiguous change");
        let r = &d.regions[0];
        // bbox 胞对齐:块 (40,60)-(100,120) → 胞 (4,6)-(10,12) → px 40,60,60,60
        assert_eq!((r.x, r.y, r.w, r.h), (40, 60, 60, 60));
        assert!(r.density > 0.9);
    }

    #[test]
    fn subthreshold_noise_below_cell_threshold_is_unchanged() {
        // +5 灰度噪声(< 阈值 12):unchanged——抗压缩噪声语义
        let a = gray(200, 200, 100);
        let mut b = gray(200, 200, 105);
        for (i, v) in b.data.iter_mut().enumerate() {
            *v = 100 + ((i % 7) as u8).min(5); // 5 级以内噪声
        }
        let d = diff(&a, &b).unwrap();
        assert!(d.mean_abs_diff < 5.0);
        assert_eq!(
            d.changed_ratio, 0.0,
            "noise under cell threshold reports unchanged"
        );
        assert!(d.regions.is_empty());
    }
}
