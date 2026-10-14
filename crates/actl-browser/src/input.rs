//! browser-drag:CDP Input.dispatchMouseEvent 事件序列(深水层拖拽动词)。
//! 语义与 UIA 轨 drag 对齐:**写操作**——停止代次核验前置 + 动作卡
//! (browser-drag,InputEffects 如实申报为空——CDP 注入不经物理键鼠,
//! 与既有 browser-click 同口径)。坐标口径:视口 CSS 像素(与
//! element_box/挑战解题循环同系,无 DPI 换算面)。
//!
//! 轨迹:默认直线插值(机器形态,如实);`--human` 走 actl-core 轨迹
//! 整形器(种子确定,速度剖面/过冲回拉/臂漂移——滑块类验证的行为面)。
//! 时间轴按事件间隔在 CDP 侧睡眠(tokio sleep,骨干 runtime 内)。

use std::time::Duration;

use actl_core::CtlError;
use actl_core::trajectory::{TimedPoint, human_drag};
use serde_json::{Value, json};

use crate::bridge::{self, eval_json};
use crate::ops::{OP_DEFAULT_TIMEOUT_MS, declare_action, dur, validate_target_shape};
use crate::session::{self, Paths};

pub struct DragSpec<'a> {
    pub session_id: &'a str,
    /// 起点/终点(视口 CSS px)。
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub human: bool,
    /// 人类轨迹种子(默认时间戳;确定性回放传显式值)。
    pub seed: Option<u64>,
    pub target: Option<&'a str>,
    pub timeout_ms: Option<u64>,
}

fn validate_point(label: &str, point: (f64, f64)) -> Result<(), CtlError> {
    if !point.0.is_finite()
        || !point.1.is_finite()
        || !(0.0..=16384.0).contains(&point.0)
        || !(0.0..=16384.0).contains(&point.1)
    {
        return Err(CtlError::protocol(format!(
            "{label} expects finite viewport CSS pixels within 0..=16384, got ({}, {})",
            point.0, point.1
        )));
    }
    Ok(())
}

pub fn drag(paths: &Paths, spec: DragSpec<'_>) -> Result<Value, CtlError> {
    validate_point("--from", spec.from)?;
    validate_point("--to", spec.to)?;
    if let Some(target) = spec.target {
        validate_target_shape(target)?;
    }
    // 停止代次核验:与桌面写操作同纪律(急停压过一切写通道)
    actl_core::stop::check_current()?;
    let timeout = dur(spec.timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, spec.session_id)?;
    let lane = bridge::open_lane(&record, spec.target, timeout)?;
    let mut action = declare_action("browser-drag", &format!("cdp:{}", spec.session_id));
    action.executing();

    // 轨迹生成(CLI 侧纯计算):human=轨迹整形器;默认=直线插值
    let start = (spec.from.0.round() as i32, spec.from.1.round() as i32);
    let end = (spec.to.0.round() as i32, spec.to.1.round() as i32);
    let points: Vec<TimedPoint> = if spec.human {
        let seed = spec
            .seed
            .unwrap_or_else(|| actl_core::state::unix_ms() as u64);
        human_drag(start, end, seed)
    } else {
        straight_line(start, end)
    };
    if points.is_empty() {
        return Err(CtlError::protocol("drag trajectory is empty"));
    }
    let events = points.len();
    let total_ms = points.last().map(|point| point.t_ms).unwrap_or(0);

    // 派发:mousePressed@起点 → mouseMoved×轨迹(按时间轴睡眠)→
    // mouseReleased@终点。轨迹时长 + 事件往返余量共用药洞预算。
    let budget = Duration::from_millis(
        timeout.as_millis() as u64 + total_ms as u64 + (events as u64 * 30) + 2000,
    );
    let dispatch = {
        let page = lane.page.clone();
        let from = spec.from;
        let to = spec.to;
        async move {
            use chromiumoxide::cdp::browser_protocol::input::{
                DispatchMouseEventParams, DispatchMouseEventType, MouseButton,
            };
            let build = |kind: DispatchMouseEventType,
                         x: f64,
                         y: f64,
                         count: Option<u32>|
             -> Result<DispatchMouseEventParams, String> {
                let mut builder = DispatchMouseEventParams::builder()
                    .x(x)
                    .y(y)
                    .button(MouseButton::Left)
                    .r#type(kind);
                if let Some(count) = count {
                    builder = builder.click_count(count);
                }
                builder.build().map_err(|e| format!("params: {e}"))
            };
            page.execute(build(
                DispatchMouseEventType::MouseMoved,
                from.0,
                from.1,
                None,
            )?)
            .await
            .map_err(|e| format!("move to start: {e}"))?;
            page.execute(build(
                DispatchMouseEventType::MousePressed,
                from.0,
                from.1,
                Some(1),
            )?)
            .await
            .map_err(|e| format!("press: {e}"))?;
            let mut last_ms = 0u32;
            for point in &points {
                let wait = point.t_ms.saturating_sub(last_ms);
                last_ms = point.t_ms;
                if wait > 0 {
                    tokio::time::sleep(Duration::from_millis(wait as u64)).await;
                }
                page.execute(build(
                    DispatchMouseEventType::MouseMoved,
                    point.x as f64,
                    point.y as f64,
                    Some(1),
                )?)
                .await
                .map_err(|e| format!("move: {e}"))?;
            }
            page.execute(build(
                DispatchMouseEventType::MouseReleased,
                to.0,
                to.1,
                Some(1),
            )?)
            .await
            .map_err(|e| format!("release: {e}"))?;
            Ok::<(), String>(())
        }
    };
    let result = bridge::block_on("drag-dispatch", dispatch, budget)
        .and_then(|inner| inner.map_err(|msg| CtlError::internal(format!("browser-drag: {msg}"))));
    result?;

    // 写后状态读回(与 click 同款轻量核验:导航事实而非效果断言——拖拽的
    // 业务效果归调用方谓词,如 browser-verify --challenge/--css)
    let state = eval_json(
        &lane.page,
        "(function(){return JSON.stringify({url:location.href})})()",
        None,
        timeout,
    )?;
    action.delivered();
    let mut payload = json!({
        "session_id": spec.session_id,
        "dragged": true,
        "from": {"x": spec.from.0, "y": spec.from.1},
        "to": {"x": spec.to.0, "y": spec.to.1},
        "human": spec.human,
        "events": events,
        "trajectory_ms": total_ms,
        "url": state.get("url").and_then(Value::as_str).unwrap_or_default(),
    });
    if spec.human {
        // 种子回显(确定性回放;缺省种子=时间戳,回放需调用方显式传值)
        payload["seed"] = json!(spec.seed);
    }
    Ok(payload)
}

/// 直线插值(默认机器形态):8px 步长,事件间隔 10ms(匀速,如实——
/// 行为面整形是 --human 的显式选项)。
fn straight_line(start: (i32, i32), end: (i32, i32)) -> Vec<TimedPoint> {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let dist = (dx as f64).hypot(dy as f64).ceil() as i32;
    let steps = (dist / 8).max(1);
    let mut out = Vec::with_capacity(steps as usize + 1);
    for step in 0..=steps {
        let t = step as f64 / steps as f64;
        out.push(TimedPoint {
            x: (start.0 as f64 + dx as f64 * t).round() as i32,
            y: (start.1 as f64 + dy as f64 * t).round() as i32,
            t_ms: (t * dist as f64 * 10.0).round() as u32,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_line_covers_endpoints_and_timing() {
        let points = straight_line((0, 0), (100, 0));
        assert_eq!(points.first().unwrap().x, 0);
        assert_eq!(points.last().unwrap().x, 100);
        assert!(points.windows(2).all(|w| w[0].t_ms <= w[1].t_ms));
        assert_eq!(points.last().unwrap().t_ms, 1000, "10ms per 8px step");
    }

    #[test]
    fn zero_span_straight_line_is_two_points() {
        let points = straight_line((5, 5), (5, 5));
        assert_eq!(points.len(), 2);
        assert_eq!(points[1].t_ms, 0);
    }
}
