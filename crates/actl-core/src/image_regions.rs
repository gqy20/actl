//! image-regions:盲区几何提议器(确定性像素扫描,零语义)。
//! 自绘控件(UIA 不暴露缩略图/视频封面/头像)的像素仍在窗口表面——本模块
//! 在灰度网格上做方差阈值 + 8 连通域,提议「图像类内容」的候选矩形:
//! bbox + density(胞密度)+ motion(双帧差,>0 即动画内容)。只管几何,
//! 不懂内容——语义判读是 decide 视觉咨询的事,精确确认是 find-image
//! 模板的事;三者组成「提议→确认→判读」链。
//!
//! 与 template_match 同层的纯像素算术(单测主场,合成图钉死行为):
//! 纯色背景 → 零提议;纹理方块 → bbox/density 精确复原;静止 → motion 0。
use crate::template_match::Gray;

/// 扫描参数(全部有界,越界由 uia 胶合层 PROTOCOL 拒绝)。
#[derive(Clone, Copy, Debug)]
pub struct ScanParams {
    /// 胞方差阈值(灰度 0-255 上的总体方差;默认 800——文字行/照片远超,
    /// 纯色背景与淡渐变为 0 附近)
    pub var_threshold: f64,
    /// 连通域最小胞数(默认 25 ≈ 50×50 px 等效;滤单胞边缘噪声)
    pub min_cells: usize,
    /// 胞密度下限(域内胞数/包围盒胞数;0 = 不过滤)
    pub density_min: f64,
    /// 最多返回的提议数(按扫描序 = 自上而下阅读序截断)
    pub limit: usize,
}

impl Default for ScanParams {
    fn default() -> Self {
        ScanParams {
            var_threshold: 800.0,
            min_cells: 25,
            density_min: 0.0,
            limit: 64,
        }
    }
}

/// 网格边长(物理像素)。10px 对 150% 缩放屏的 160px 缩略图 = 16×16 胞,
/// 对 80px 头像 = 8×8 胞;固定值不过度配置。
pub const CELL: usize = 10;

/// 胞内采样步长(4×4=16 样本/胞;步内取样而非全胞,开销降 6 倍方差稳定)。
const SAMPLE_STEP: usize = 3;

/// 一个候选矩形(坐标为相对扫描面左上角的物理像素;uia 层加窗口原点换算屏幕)。
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// 域内高方差胞数 / 包围盒胞数(1.0 = 实心;0.5 上下 = L 形/边缘残迹)
    pub density: f64,
    /// 域内高方差胞数
    pub cells: u32,
    /// 双帧平均绝对灰度差(0 = 静止;>0 = 动画内容——视频/GIF 定性依据)。
    /// motion 扫描关闭时恒 0。
    pub motion: f64,
}

/// 方差网格:每胞灰度总体方差(采样近似)。返回 (cells_w, cells_h, variances)。
fn variance_grid(g: &Gray) -> (usize, usize, Vec<f64>) {
    let cw = g.w / CELL;
    let ch = g.h / CELL;
    let mut vars = vec![0.0f64; cw * ch];
    for cy in 0..ch {
        for cx in 0..cw {
            let mut sum = 0f64;
            let mut sq = 0f64;
            let mut n = 0f64;
            let mut dy = 0;
            while dy < CELL {
                let mut dx = 0;
                while dx < CELL {
                    let v = g.data[(cy * CELL + dy) * g.w + cx * CELL + dx] as f64;
                    sum += v;
                    sq += v * v;
                    n += 1.0;
                    dx += SAMPLE_STEP;
                }
                dy += SAMPLE_STEP;
            }
            let mean = sum / n;
            // 总体方差 E[x²]-E[x]²
            vars[cy * cw + cx] = (sq / n - mean * mean).max(0.0);
        }
    }
    (cw, ch, vars)
}

/// 8 连通域标记(迭代栈,免递归深度);返回每胞的域号(-1 = 非高方差)。
fn components(cw: usize, ch: usize, vars: &[f64], threshold: f64) -> Vec<i32> {
    let mut label = vec![-1i32; cw * ch];
    let mut next = 0i32;
    for start in 0..(cw * ch) {
        if label[start] != -1 || vars[start] <= threshold {
            continue;
        }
        let mut stack = vec![start];
        label[start] = next;
        while let Some(idx) = stack.pop() {
            let x = idx % cw;
            let y = idx / cw;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= cw as i32 || ny >= ch as i32 {
                        continue;
                    }
                    let j = ny as usize * cw + nx as usize;
                    if label[j] == -1 && vars[j] > threshold {
                        label[j] = next;
                        stack.push(j);
                    }
                }
            }
        }
        next += 1;
    }
    label
}

/// 域聚合器:包围盒(胞坐标,含端点)+胞数。
#[derive(Default)]
struct Acc {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    cells: u32,
}

impl Acc {
    fn first(x: i32, y: i32) -> Self {
        Acc {
            x0: x,
            y0: y,
            x1: x,
            y1: y,
            cells: 1,
        }
    }
    fn add(&mut self, x: i32, y: i32) {
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x);
        self.y1 = self.y1.max(y);
        self.cells += 1;
    }
}

/// 主扫描:灰度面 → 候选矩形(阅读序)。motion_frame 传 Some(第二帧灰度)
/// 时逐域计算双帧差(尺寸必须一致)。
pub fn scan(g: &Gray, params: &ScanParams, motion_frame: Option<&Gray>) -> Vec<Region> {
    if g.w < CELL || g.h < CELL {
        return Vec::new();
    }
    let (cw, ch, vars) = variance_grid(g);
    let label = components(cw, ch, &vars, params.var_threshold);
    // 聚合各域
    let mut boxes: Vec<Option<Acc>> = Vec::new();
    for (idx, l) in label.iter().enumerate() {
        if *l < 0 {
            continue;
        }
        while boxes.len() <= *l as usize {
            boxes.push(None);
        }
        let x = (idx % cw) as i32;
        let y = (idx / cw) as i32;
        let slot = &mut boxes[*l as usize];
        match slot {
            None => *slot = Some(Acc::first(x, y)),
            Some(acc) => acc.add(x, y),
        }
    }
    let mut out: Vec<Region> = boxes
        .into_iter()
        .flatten()
        .filter_map(|acc| {
            if (acc.cells as usize) < params.min_cells {
                return None;
            }
            let w = acc.x1 - acc.x0 + 1;
            let h = acc.y1 - acc.y0 + 1;
            let density = acc.cells as f64 / (w * h) as f64;
            if density < params.density_min {
                return None;
            }
            Some(Region {
                x: acc.x0 * CELL as i32,
                y: acc.y0 * CELL as i32,
                w: w * CELL as i32,
                h: h * CELL as i32,
                density: (density * 1000.0).round() / 1000.0,
                cells: acc.cells,
                motion: 0.0,
            })
        })
        .collect();
    out.sort_by_key(|r| (r.y, r.x));
    out.truncate(params.limit);
    if let Some(m) = motion_frame.filter(|m| m.w == g.w && m.h == g.h) {
        for r in &mut out {
            r.motion = region_motion(g, m, r);
        }
    }
    out
}

/// 域内双帧平均绝对灰度差(每 2px 采样;3 位小数)。
fn region_motion(a: &Gray, b: &Gray, r: &Region) -> f64 {
    let x0 = r.x.max(0) as usize;
    let y0 = r.y.max(0) as usize;
    let x1 = (x0 + r.w as usize).min(a.w);
    let y1 = (y0 + r.h as usize).min(a.h);
    let mut sum = 0f64;
    let mut n = 0f64;
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            sum += (a.data[y * a.w + x] as i32 - b.data[y * b.w + x] as i32).abs() as f64;
            n += 1.0;
            x += 2;
        }
        y += 2;
    }
    if n == 0.0 {
        return 0.0;
    }
    (sum / n * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成灰度面:纯色背景 + 指定矩形填充噪声纹理(伪随机,确定性)。
    fn synth(w: usize, h: usize, blocks: &[(usize, usize, usize, usize)]) -> Gray {
        let mut data = vec![40u8; w * h];
        for b in blocks {
            for y in b.1..(b.1 + b.3) {
                for x in b.0..(b.0 + b.2) {
                    // LCG 伪随机纹理(方差远超 800)
                    let v = ((x * 73 + y * 151 + 97) % 251) as u8;
                    data[y * w + x] = v;
                }
            }
        }
        Gray { w, h, data }
    }

    #[test]
    fn flat_surface_proposes_nothing() {
        let g = synth(400, 300, &[]);
        let out = scan(&g, &ScanParams::default(), None);
        assert!(out.is_empty(), "flat background must yield zero regions");
    }

    #[test]
    fn textured_square_is_proposed_with_exact_bbox() {
        // 160×160 块 @ (100,100) → bbox 精确到胞对齐(100..260 → x=100,w=160)
        let g = synth(600, 500, &[(100, 100, 160, 160)]);
        let out = scan(&g, &ScanParams::default(), None);
        assert_eq!(out.len(), 1, "one textured block = one region");
        let r = &out[0];
        assert_eq!((r.x, r.y, r.w, r.h), (100, 100, 160, 160));
        assert!(
            r.density > 0.95,
            "solid block density ~1.0, got {}",
            r.density
        );
        assert_eq!(r.motion, 0.0);
    }

    #[test]
    fn sparse_glyph_rows_are_proposed_and_ordered() {
        // 两条 8px 高的「文字行」:8px < CELL=10 采不满 → 用 14px 高
        let g = synth(500, 400, &[(40, 60, 300, 14), (40, 200, 200, 14)]);
        let out = scan(&g, &ScanParams::default(), None);
        assert_eq!(out.len(), 2);
        assert!(out[0].y < out[1].y, "reading order (top first)");
        assert!(
            out.iter().all(|r| r.density > 0.9),
            "thin full-width rows are dense"
        );
    }

    #[test]
    fn min_cells_and_density_filters_apply() {
        let g = synth(500, 400, &[(50, 50, 160, 160)]);
        // 阈值抬到域上方 → 零提议
        let p = ScanParams {
            min_cells: 500,
            ..ScanParams::default()
        };
        assert!(scan(&g, &p, None).is_empty());
        // 密度门槛 1.0:仅实心块通过
        let p = ScanParams {
            density_min: 1.0,
            ..ScanParams::default()
        };
        assert_eq!(scan(&g, &p, None).len(), 1);
    }

    #[test]
    fn motion_detects_frame_delta() {
        let a = synth(400, 300, &[(60, 60, 160, 160)]);
        let mut b = synth(400, 300, &[(60, 60, 160, 160)]);
        // 平移纹理相位 → 域内内容变化
        for y in 60..220 {
            for x in 60..220 {
                b.data[y * 400 + x] = ((x * 79 + y * 157 + 31) % 251) as u8;
            }
        }
        let out = scan(&a, &ScanParams::default(), Some(&b));
        assert_eq!(out.len(), 1);
        assert!(
            out[0].motion > 5.0,
            "changed block must report motion, got {}",
            out[0].motion
        );
        // 静止帧:同图自比 → 0
        let out = scan(&a, &ScanParams::default(), Some(&a));
        assert_eq!(out[0].motion, 0.0);
    }

    #[test]
    fn limit_truncates_in_reading_order() {
        let blocks: Vec<(usize, usize, usize, usize)> = (0..6)
            .map(|i| (30 + (i % 3) * 160, 30 + (i / 3) * 160, 100, 100))
            .collect();
        let g = synth(600, 500, &blocks);
        let p = ScanParams {
            limit: 3,
            ..ScanParams::default()
        };
        let out = scan(&g, &p, None);
        assert_eq!(out.len(), 3);
        // 阅读序:先上排(左→右),再下排
        assert!(out.windows(2).all(|w| (w[0].y, w[0].x) <= (w[1].y, w[1].x)));
    }
}
