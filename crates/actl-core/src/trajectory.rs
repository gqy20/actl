//! 人类拖拽轨迹生成器(纯函数,零依赖):滑块类验证解题的执行面整形。
//!
//! 行为形态学依据 slider-captcha-lab 的真人逐帧分析与「分类器当诊断仪」
//! 闭环(公开方法论):峰值速度出现在行程 ~70-75%(匀速直线是机器指纹)、
//! 事件密度 20-60、末端过冲回拉、y 轴线性臂漂移 ±2px、偶发中途回犹豫。
//! 生成器把这些形态做成**种子确定的参数化轨迹**——同 seed 同轨迹(可测、
//! 可回放);消费方是物理轨(mouse::drag_timed)与 CDP 轨(browser-drag),
//! 两侧共享同一时间轴(dt 由消费方按事件间隔睡眠,急停逐事件核验)。
//!
//! 诚实边界:形态参数来自公开行为研究的人类统计近似,不是对特定风控
//! 引擎的规避调参——成功率以真机验收为准,失败如实上报不重试轰炸。

/// 定时轨迹点:绝对物理/视口坐标 + 相对轨迹起点的累计毫秒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedPoint {
    pub x: i32,
    pub y: i32,
    pub t_ms: u32,
}

/// 事件数硬上限(与 mouse::drag_path 的 32768 批次上限同族但按人类
/// 事件密度收紧:轨迹生成面就不会超过人类统计区间)。
const MAX_EVENTS: usize = 60;

/// xorshift64*(确定性,跨平台稳定):种子驱动全部抖动。
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// [0, 1) 均匀。
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// [lo, hi] 整数均匀。
    fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.unit() * (hi - lo + 1) as f64).floor() as i32
    }
}

/// 行程主导轴与总距离(拖拽通常近水平,纵向给臂漂移)。
fn axis(start: (i32, i32), end: (i32, i32)) -> (f64, f64, f64) {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let dist = (dx as f64).hypot(dy as f64);
    (dx as f64, dy as f64, dist)
}

/// 速度剖面积分得到 s(t)∈[0,1](行程比例):分段线性峰值速度在 ~0.72。
/// 剖面(归一化速度):t∈[0,0.12] 0.35→1.0(快起);[0.12,0.72] 1.0;
/// [0.72,0.9] 1.0→0.12(减速);[0.9,0.95] 0.12→0.5(过冲推进);末端回落。
fn velocity_profile(t: f64) -> f64 {
    if t < 0.12 {
        0.35 + (t / 0.12) * 0.65
    } else if t < 0.72 {
        1.0
    } else if t < 0.9 {
        1.0 - ((t - 0.72) / 0.18) * 0.88
    } else {
        0.12
    }
}

/// 位置比例 s(t):速度剖面积分后归一化(梯形数值积分,步长 1/512)。
fn travel_at(t: f64) -> f64 {
    const STEPS: usize = 512;
    let clamp = t.clamp(0.0, 1.0);
    let idx = (clamp * STEPS as f64) as usize;
    let mut acc = 0.0f64;
    for i in 0..idx {
        let a = velocity_profile(i as f64 / STEPS as f64);
        let b = velocity_profile((i + 1) as f64 / STEPS as f64);
        acc += (a + b) / 2.0 / STEPS as f64;
    }
    // 归一化:全程积分
    let total = travel_total();
    acc / total
}

fn travel_total() -> f64 {
    const STEPS: usize = 512;
    let mut acc = 0.0f64;
    for i in 0..STEPS {
        let a = velocity_profile(i as f64 / STEPS as f64);
        let b = velocity_profile((i + 1) as f64 / STEPS as f64);
        acc += (a + b) / 2.0 / STEPS as f64;
    }
    acc
}

/// 生成人类形态拖拽轨迹(种子确定)。终点精确落位(解题的正确性要求),
/// 形态只作用于路径与时间轴。事件数与时长随距离缩放并夹在人类区间。
pub fn human_drag(start: (i32, i32), end: (i32, i32), seed: u64) -> Vec<TimedPoint> {
    let (dx, dy, dist) = axis(start, end);
    if dist < 1.0 {
        return vec![TimedPoint {
            x: end.0,
            y: end.1,
            t_ms: 0,
        }];
    }
    // xorshift64* 零种子退化态换非零常数;不做 |1(偶数种子会被并入相邻
    // 奇数——42|1==43|1,同轨迹)。
    let mut rng = Rng(if seed == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        seed
    });

    // 时长:人类滑块 ~0.4-1.6s,随距离缩放
    let duration_ms = (380.0 + dist * 2.4 + rng.int(-80, 120) as f64)
        .clamp(320.0, 1600.0)
        .round() as u32;
    // 事件数:人类密度 20-60,随距离缩放
    let events = ((dist / 14.0).ceil() as usize + 12).clamp(18, MAX_EVENTS);
    // 过冲:2-9px,随距离温和缩放,方向同行程
    let overshoot = ((dist * 0.02).ceil() as i32).clamp(2, 9) + rng.int(0, 3);
    let (sx, sy) = (dx / dist, dy / dist);
    let (ux, uy) = (sx, sy);

    // 采样时间点:非均匀——人类事件在快段密、慢段疏。对 [0,0.94] 均匀
    // 取 N-4 个 + 末段过冲/回拉 3-4 个专属点;中途回犹豫按种子概率插入
    // 一次 60-160ms 停顿(时间轴空隙,无位移)。
    let hesitation_at = if rng.unit() < 0.65 {
        Some(rng.int(40, 70) as f64 / 100.0)
    } else {
        None
    };
    let hesitation_ms = rng.int(60, 160) as u32;

    let mut out: Vec<TimedPoint> = Vec::with_capacity(events + 2);
    let main_events = events.saturating_sub(4).max(10);
    for i in 0..main_events {
        let t = i as f64 / (main_events - 1) as f64; // [0,1) 到 0.94 主行程
        let t_main = t * 0.94;
        let s = travel_at(t_main);
        let progress = s * 0.985; // 主行程留给末端 1.5% 收口
        // y 轴:线性插值 + 臂漂移 ±2px
        let drift = (rng.unit() - 0.5) * 4.0;
        let x = start.0 as f64 + dx * progress;
        let y = start.1 as f64 + dy * progress + drift * sx.abs();
        let mut t_ms = (t_main * duration_ms as f64).round() as u32;
        if let Some(h) = hesitation_at
            && t_main > h
        {
            t_ms += hesitation_ms;
        }
        out.push(TimedPoint {
            x: x.round() as i32,
            y: y.round() as i32,
            t_ms,
        });
    }
    // 过冲(0.94→0.97)与回拉(0.97→1.0):终点精确落位。末段时间轴基于
    // 主段**实际**最后时刻(犹豫平移已传导)——否则与主段撞车被去重吞掉。
    let base_ms = out.last().map(|point| point.t_ms).unwrap_or(0);
    let ox = end.0 as f64 + ux * overshoot as f64;
    let oy = end.1 as f64 + uy * overshoot as f64;
    let oy = oy + (rng.unit() - 0.5) * 2.0;
    out.push(TimedPoint {
        x: ox.round() as i32,
        y: oy.round() as i32,
        t_ms: base_ms + (0.03 * duration_ms as f64).round() as u32,
    });
    out.push(TimedPoint {
        x: end.0,
        y: end.1,
        t_ms: base_ms + (0.06 * duration_ms as f64).round() as u32 + rng.int(0, 40) as u32,
    });
    // 时间轴单调化(数值边界下的防御)与去重
    let mut last_ms = 0u32;
    for point in &mut out {
        if point.t_ms < last_ms {
            point.t_ms = last_ms;
        } else {
            last_ms = point.t_ms;
        }
    }
    let mut dedup: Vec<TimedPoint> = Vec::with_capacity(out.len());
    for point in out {
        if dedup
            .last()
            .is_none_or(|last: &TimedPoint| last.t_ms < point.t_ms)
        {
            dedup.push(point);
        }
    }
    dedup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_seed() {
        let a = human_drag((100, 200), (420, 205), 42);
        let b = human_drag((100, 200), (420, 205), 42);
        assert_eq!(a, b);
        let c = human_drag((100, 200), (420, 205), 43);
        assert_ne!(a, c, "different seeds differ");
    }

    #[test]
    fn endpoint_exact_and_time_monotonic() {
        for seed in [1u64, 7, 99, 12345] {
            let path = human_drag((50, 60), (390, 64), seed);
            let last = path.last().unwrap();
            assert_eq!((last.x, last.y), (390, 64), "seed {seed}");
            assert!(path.windows(2).all(|w| w[0].t_ms <= w[1].t_ms));
            assert_eq!(path[0].t_ms, 0);
        }
    }

    #[test]
    fn event_count_and_duration_stay_human() {
        let path = human_drag((0, 0), (300, 0), 5);
        assert!(
            path.len() >= 15 && path.len() <= MAX_EVENTS,
            "{}",
            path.len()
        );
        let last = path.last().unwrap();
        assert!(
            last.t_ms >= 300 && last.t_ms <= 1700,
            "duration {}ms",
            last.t_ms
        );
    }

    #[test]
    fn y_drift_bounded_and_overshoot_then_return() {
        let path = human_drag((0, 100), (280, 100), 11);
        for point in &path {
            assert!(
                (point.y - 100).abs() <= 3,
                "arm drift bound: y={} in {:?}",
                point.y,
                path.iter().map(|p| p.y).collect::<Vec<_>>()
            );
        }
        // 至少存在一次越过终点的过冲(正方向行程)
        assert!(
            path.iter().any(|p| p.x > 280),
            "overshoot present: {:?}",
            path.iter().map(|p| p.x).collect::<Vec<_>>()
        );
        // 主行程大体单调推进(允许过冲回拉;首 80% 事件 x 不倒退超过 3px)
        let head: Vec<i32> = path.iter().take(path.len() - 2).map(|p| p.x).collect();
        for pair in head.windows(2) {
            assert!(pair[1] >= pair[0] - 3, "no large retreat in main travel");
        }
    }

    #[test]
    fn zero_distance_degenerates_to_single_point() {
        let path = human_drag((10, 10), (10, 10), 3);
        assert_eq!(path.len(), 1);
        assert_eq!((path[0].x, path[0].y), (10, 10));
    }
}
