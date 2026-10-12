//! 确定性模板匹配(传统 RPA 图色定位的 actl 形态)——纯像素算术,无语义
//! 视觉(PRD §1.3 非目标修订:"core 不做视觉"排除的是 OCR/识别/VLM 类
//! **语义视觉**;ZNCC 像素相关是确定性运算,与校验和同类,与"确定性、快、
//! 零模型成本"的定位一致)。
//!
//! 算法:金字塔粗搜(4× 降采样,积分图 O(1) 归一化 + 逐位相关)→ 候选
//! 全分辨率精修(±2·factor 窗内直接 ZNCC)→ 非极大值抑制(重叠 >50% 保
//! 最高分)。诚实语义:score < threshold 一律不报(不降级猜测);均匀模板
//! (std=0)ZNCC 无定义,`degenerate` 如实报告由调用方拒绝。

/// 灰度图(BGRA → 0.299R+0.587G+0.114B 定点近似)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

pub fn to_gray(bgra: &[u8], w: usize, h: usize) -> Option<Gray> {
    if bgra.len() < w * h * 4 {
        return None;
    }
    let mut data = Vec::with_capacity(w * h);
    for i in 0..(w * h) {
        let o = i * 4;
        // BGRA 序:B=[o] G=[o+1] R=[o+2]
        let g =
            ((bgra[o + 2] as u32 * 77 + bgra[o + 1] as u32 * 150 + bgra[o] as u32 * 29) >> 8) as u8;
        data.push(g);
    }
    Some(Gray { w, h, data })
}

/// 盒式降采样(factor ≥1;1 = 原样克隆)。
pub fn downsample(g: &Gray, factor: usize) -> Gray {
    if factor <= 1 {
        return g.clone();
    }
    let w = g.w / factor;
    let h = g.h / factor;
    let mut data = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0u32;
            let mut n = 0u32;
            for dy in 0..factor {
                for dx in 0..factor {
                    sum += g.data[(y * factor + dy) * g.w + x * factor + dx] as u32;
                    n += 1;
                }
            }
            data[y * w + x] = (sum / n) as u8;
        }
    }
    Gray { w, h, data }
}

/// 盒式缩放(多尺度搜索用;最近邻不够稳,盒平均抗锯齿)。
pub fn resize(g: &Gray, nw: usize, nh: usize) -> Gray {
    if nw == g.w && nh == g.h {
        return g.clone();
    }
    let mut data = vec![0u8; nw * nh];
    for y in 0..nh {
        let y0 = y * g.h / nh;
        let y1 = ((y + 1) * g.h / nh).max(y0 + 1).min(g.h);
        for x in 0..nw {
            let x0 = x * g.w / nw;
            let x1 = ((x + 1) * g.w / nw).max(x0 + 1).min(g.w);
            let mut sum = 0u32;
            let mut n = 0u32;
            for yy in y0..y1 {
                for xx in x0..x1 {
                    sum += g.data[yy * g.w + xx] as u32;
                    n += 1;
                }
            }
            data[y * nw + x] = (sum / n.max(1)) as u8;
        }
    }
    Gray { w: nw, h: nh, data }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Match {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub score: f32,
}

/// 模板退化判定:灰度标准差≈0(纯色块)时 ZNCC 分母为零,匹配无定义。
pub fn degenerate(needle: &Gray) -> bool {
    variance(needle) < 1.0
}

fn mean_var(g: &Gray) -> (f64, f64) {
    let n = (g.w * g.h) as f64;
    let sum: u64 = g.data.iter().map(|&v| v as u64).sum();
    let sumsq: u64 = g.data.iter().map(|&v| (v as u64) * (v as u64)).sum();
    let mean = sum as f64 / n;
    let var = sumsq as f64 / n - mean * mean;
    (mean, var.max(0.0))
}

fn variance(g: &Gray) -> f64 {
    mean_var(g).1
}

/// 全图 ZNCC 扫描(粗搜层用):积分图 O(1) 归一化 + 逐位相关 O(n)。
/// 返回 score ≥ threshold 的全部位置(未抑制)。
fn scan(hay: &Gray, needle: &Gray, threshold: f32) -> Vec<Match> {
    let (tw, th) = (needle.w, needle.h);
    if tw == 0 || th == 0 || hay.w < tw || hay.h < th {
        return Vec::new();
    }
    let (t_mean, t_var) = mean_var(needle);
    let t_norm = (t_var * (tw * th) as f64).sqrt();
    if t_norm < 1e-6 {
        return Vec::new(); // 均匀模板:无定义,如实空
    }
    // 模板零均值化(相关时省去逐位减法)
    let t_zero: Vec<f32> = needle
        .data
        .iter()
        .map(|&v| v as f32 - t_mean as f32)
        .collect();

    // 灰度积分图(sum/sumsq,u64;4× 降采样层尺寸下无溢出风险)
    let (hw, hh) = (hay.w, hay.h);
    let mut isum = vec![0u64; (hw + 1) * (hh + 1)];
    let mut isq = vec![0u64; (hw + 1) * (hh + 1)];
    for y in 0..hh {
        let mut row_sum = 0u64;
        let mut row_sq = 0u64;
        for x in 0..hw {
            let v = hay.data[y * hw + x] as u64;
            row_sum += v;
            row_sq += v * v;
            isum[(y + 1) * (hw + 1) + x + 1] = isum[y * (hw + 1) + x + 1] + row_sum;
            isq[(y + 1) * (hw + 1) + x + 1] = isq[y * (hw + 1) + x + 1] + row_sq;
        }
    }
    let rect = |img: &[u64], x: usize, y: usize| -> u64 {
        img[(y + th) * (hw + 1) + x + tw] + img[y * (hw + 1) + x]
            - img[(y + th) * (hw + 1) + x]
            - img[y * (hw + 1) + x + tw]
    };
    let n = (tw * th) as f64;
    let mut out = Vec::new();
    for y in 0..=(hh - th) {
        for x in 0..=(hw - tw) {
            let s = rect(&isum, x, y) as f64;
            let q = rect(&isq, x, y) as f64;
            let h_mean = s / n;
            let h_var = (q / n - h_mean * h_mean).max(0.0);
            let h_norm = (h_var * n).sqrt();
            if h_norm < 1e-6 {
                continue; // 均匀窗:相关无定义
            }
            // Σ H·T'
            let mut cross = 0.0f64;
            for ty in 0..th {
                let hrow = (y + ty) * hw + x;
                let trow = ty * tw;
                for tx in 0..tw {
                    cross += hay.data[hrow + tx] as f64 * t_zero[trow + tx] as f64;
                }
            }
            let score = (cross / (h_norm * t_norm)) as f32;
            if score >= threshold {
                out.push(Match {
                    x,
                    y,
                    w: tw,
                    h: th,
                    score,
                });
            }
        }
    }
    out
}

/// 直接 ZNCC(精修层用,单位置)。
fn zncc_at(hay: &Gray, needle: &Gray, x: usize, y: usize) -> f32 {
    let (tw, th) = (needle.w, needle.h);
    if x + tw > hay.w || y + th > hay.h {
        return -1.0;
    }
    let (t_mean, t_var) = mean_var(needle);
    let t_norm = (t_var * (tw * th) as f64).sqrt();
    if t_norm < 1e-6 {
        return -1.0;
    }
    let n = (tw * th) as f64;
    let mut s = 0.0f64;
    let mut q = 0.0f64;
    let mut cross = 0.0f64;
    for ty in 0..th {
        for tx in 0..tw {
            let hv = hay.data[(y + ty) * hay.w + x + tx] as f64;
            let tv = needle.data[ty * tw + tx] as f64;
            s += hv;
            q += hv * hv;
            cross += hv * (tv - t_mean);
        }
    }
    let h_mean = s / n;
    let h_var = (q / n - h_mean * h_mean).max(0.0);
    let h_norm = (h_var * n).sqrt();
    if h_norm < 1e-6 {
        return -1.0;
    }
    (cross / (h_norm * t_norm)) as f32
}

/// 非极大值抑制:按分数降序,重叠面积 >50%(较小者)的后续匹配丢弃。
pub fn suppress(mut matches: Vec<Match>) -> Vec<Match> {
    matches.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept: Vec<Match> = Vec::new();
    for m in &matches {
        let overlaps = kept.iter().any(|k: &Match| {
            let ix = k.x.max(m.x)..(k.x + k.w).min(m.x + m.w);
            let iy = k.y.max(m.y)..(k.y + k.h).min(m.y + m.h);
            if ix.is_empty() || iy.is_empty() {
                return false;
            }
            let inter = (ix.end - ix.start) * (iy.end - iy.start);
            let smaller = (k.w * k.h).min(m.w * m.h);
            smaller > 0 && inter * 2 > smaller
        });
        if !overlaps {
            kept.push(*m);
        }
    }
    kept
}

/// 单尺度查找:粗搜(4× 金字塔)→ 全分辨率精修 → NMS。
/// 坐标为 haystack 像素坐标;score ≥ threshold 才入选。
pub fn find(hay: &Gray, needle: &Gray, threshold: f32) -> Vec<Match> {
    if needle.w == 0 || needle.h == 0 || hay.w < needle.w || hay.h < needle.h {
        return Vec::new();
    }
    if degenerate(needle) {
        return Vec::new();
    }
    const FACTOR: usize = 4;
    let (cw, ch) = (hay.w / FACTOR, hay.h / FACTOR);
    let (nw, nh) = (needle.w / FACTOR, needle.h / FACTOR);
    let coarse = if cw >= 8 && ch >= 8 && nw >= 3 && nh >= 3 {
        let hay_c = downsample(hay, FACTOR);
        let needle_c = downsample(needle, FACTOR);
        // 粗层用稍宽阈值捞候选,精修层按真阈值裁决
        scan(&hay_c, &needle_c, (threshold - 0.08).clamp(-1.0, 1.0))
    } else {
        Vec::new()
    };
    let mut refined: Vec<Match> = Vec::new();
    if coarse.is_empty() {
        // 粗层无候选(模板过小或降采样丢特征):全分辨率直扫兜底(小窗可承受)
        if hay.w * hay.h <= 640 * 480 {
            return suppress(scan(hay, needle, threshold));
        }
        return refined;
    }
    let pad = FACTOR * 2;
    for c in coarse {
        let cx = c.x * FACTOR;
        let cy = c.y * FACTOR;
        let x0 = cx.saturating_sub(pad);
        let y0 = cy.saturating_sub(pad);
        let x1 = (cx + pad).min(hay.w.saturating_sub(needle.w));
        let y1 = (cy + pad).min(hay.h.saturating_sub(needle.h));
        let mut best: Option<Match> = None;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let score = zncc_at(hay, needle, x, y);
                if score >= threshold && best.is_none_or(|b: Match| score > b.score) {
                    best = Some(Match {
                        x,
                        y,
                        w: needle.w,
                        h: needle.h,
                        score,
                    });
                }
            }
        }
        if let Some(m) = best {
            refined.push(m);
        }
    }
    suppress(refined)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(w: usize, h: usize, base: u8) -> Gray {
        Gray {
            w,
            h,
            data: vec![base; w * h],
        }
    }

    fn patch(g: &mut Gray, x: usize, y: usize, p: &Gray) {
        for dy in 0..p.h {
            for dx in 0..p.w {
                g.data[(y + dy) * g.w + x + dx] = p.data[dy * p.w + dx];
            }
        }
    }

    fn distinctive() -> Gray {
        // 8×8 棋盘+渐变:高方差,非退化
        let mut g = canvas(8, 8, 0);
        for y in 0..8 {
            for x in 0..8 {
                g.data[y * 8 + x] = if (x / 2 + y / 2) % 2 == 0 { 230 } else { 40 };
            }
        }
        g
    }

    #[test]
    fn gray_conversion_uses_luma_weights() {
        // 纯白 BGRA(255,255,255) → 255;纯黑 → 0;纯红(B=0,G=0,R=255) → 77·255>>8 = 76
        let px = [255u8, 255, 255, 255, 0, 0, 0, 255, 0, 0, 255, 255];
        let g = to_gray(&px, 3, 1).unwrap();
        assert_eq!(g.data[0], 255);
        assert_eq!(g.data[1], 0);
        assert_eq!(g.data[2], 76);
    }

    #[test]
    fn finds_exact_patch_with_high_score() {
        let mut hay = canvas(64, 48, 128);
        let needle = distinctive();
        patch(&mut hay, 21, 13, &needle);
        let matches = find(&hay, &needle, 0.8);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!((matches[0].x, matches[0].y), (21, 13));
        assert!(matches[0].score > 0.95, "score={}", matches[0].score);
    }

    #[test]
    fn no_match_below_threshold_is_honest_empty() {
        let hay = canvas(64, 48, 128); // 纯灰背景,无模板
        let needle = distinctive();
        assert!(find(&hay, &needle, 0.8).is_empty());
    }

    #[test]
    fn two_patches_two_matches() {
        let mut hay = canvas(120, 60, 128);
        let needle = distinctive();
        patch(&mut hay, 10, 10, &needle);
        patch(&mut hay, 80, 30, &needle);
        let matches = find(&hay, &needle, 0.8);
        assert_eq!(matches.len(), 2, "{matches:?}");
        let mut pts: Vec<_> = matches.iter().map(|m| (m.x, m.y)).collect();
        pts.sort();
        assert_eq!(pts, vec![(10, 10), (80, 30)]);
    }

    #[test]
    fn degenerate_needle_reports_and_yields_nothing() {
        let uniform = canvas(8, 8, 200);
        assert!(degenerate(&uniform));
        let hay = canvas(32, 32, 100);
        assert!(find(&hay, &uniform, 0.5).is_empty());
        assert!(!degenerate(&distinctive()));
    }

    #[test]
    fn needle_larger_than_haystack_is_empty_not_panic() {
        let hay = canvas(4, 4, 0);
        let needle = distinctive();
        assert!(find(&hay, &needle, 0.5).is_empty());
    }

    #[test]
    fn downsample_and_resize_preserve_coarse_structure() {
        let g = distinctive();
        let d = downsample(&g, 2);
        assert_eq!((d.w, d.h), (4, 4));
        let r = resize(&g, 16, 16);
        assert_eq!((r.w, r.h), (16, 16));
        // 缩放回原尺寸后仍可匹配(多尺度搜索的基础)
        let back = resize(&r, 8, 8);
        let mut hay = canvas(40, 40, 128);
        patch(&mut hay, 5, 7, &g);
        let matches = find(&hay, &back, 0.75);
        assert!(!matches.is_empty());
    }

    #[test]
    fn suppress_keeps_highest_of_overlapping() {
        let a = Match {
            x: 10,
            y: 10,
            w: 8,
            h: 8,
            score: 0.9,
        };
        let b = Match {
            x: 12,
            y: 11,
            w: 8,
            h: 8,
            score: 0.95,
        };
        let c = Match {
            x: 60,
            y: 60,
            w: 8,
            h: 8,
            score: 0.85,
        };
        let kept = suppress(vec![a, b, c]);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].score, 0.95);
    }
}
