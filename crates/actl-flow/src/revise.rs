//! flow-revise:计划版本化修订(运行时治理第二轮,docs/06 §7.7)。
//!
//! 设计立场:修订不是"改运行中的计划",而是**追加一个版本事件**——旧计划
//! 字节归档永不覆盖,revisions/index.json 只追加,run 历史含完整版本谱系。
//! 冻结计划的四不变量全部保持:结果溯源(StepRecord.plan_version)、无静默
//! 变更(显式命令+diff+message)、写步不盲改重放(规格变更的完成写步必须
//! --accept-rerun 点名)、重试先重立前置(续走 flow-resume 的
//! freshness/resume_check 路径)。

use crate::{CtlError, Plan, engine::Run};
use actl_core::ErrorCode;
use actl_core::state::unix_ms;
use std::path::Path;

/// 步骤级 diff(按 id 对齐;规格相等 = 规范化 JSON 序列一致)。
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiffSummary {
    pub unchanged: Vec<String>,
    pub modified: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// 一次修订的版本事件(revisions/index.json 的条目)。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Revision {
    /// 本次修订后的新版本号(从 1 起,首次修订 = 2)。
    pub version: u32,
    /// 新计划的字节指纹(FNV 族与 source.json 同算法要求,由调用方计算)。
    pub fingerprint: String,
    pub message: String,
    pub diff: DiffSummary,
    pub revised_ms: u64,
}

fn spec_fingerprint(step: &crate::Step) -> String {
    // 步骤规格的规范化序列:结构体字段序稳定,serde 输出确定性可比。
    let (mut h1, mut h2) = (0xcbf2_9ce4_8422_2325u64, 0x9e37_79b9_7f4a_7c15u64);
    for byte in serde_json::to_vec(step).unwrap_or_default() {
        h1 = (h1 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        h2 = (h2 ^ u64::from(byte).rotate_left(7)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h1:016x}{h2:016x}")
}

pub fn diff_plans(old: &Plan, new: &Plan) -> DiffSummary {
    let mut diff = DiffSummary::default();
    for (old_step, new_step) in old.steps.iter().zip(new.steps.iter()) {
        // 同位同名才算"未变"(顺序变化也算规格变化——重放序是规格的一部分)
        if old_step.id == new_step.id && spec_fingerprint(old_step) == spec_fingerprint(new_step) {
            diff.unchanged.push(new_step.id.clone());
        } else {
            diff.modified
                .push(format!("{}→{}", old_step.id, new_step.id));
        }
    }
    for step in new.steps.iter().skip(old.steps.len()) {
        diff.added.push(step.id.clone());
    }
    for step in old.steps.iter().skip(new.steps.len()) {
        diff.removed.push(step.id.clone());
    }
    diff
}

/// 修订对 run 记录的处置结果(供信封与测试断言)。
#[derive(Debug)]
pub struct RevisionOutcome {
    pub revision: Revision,
    /// 需要显式 --accept-rerun 的写步(为空 = 政策通过)。
    pub rerun_required: Vec<String>,
}

/// 计算修订政策并改写 run 记录(不落盘;落盘由调用方按
/// 归档旧计划 → 写新计划 → 写状态 → 追加索引 的顺序执行)。
pub fn apply_to_run(
    old: &Plan,
    new: &Plan,
    run: &mut Run,
    is_write: impl Fn(&crate::Step) -> bool,
    accepted: &[String],
    message: &str,
    fingerprint: &str,
) -> Result<RevisionOutcome, CtlError> {
    let diff = diff_plans(old, new);
    // 步骤记录按新计划重建:同位同名且规格未变 → 保留原记录(含结果与
    // plan_version 溯源);其余 → 重置 Pending。规格已变的完成写步必须
    // 被 --accept-rerun 点名,否则 PROTOCOL 拒绝(fail-closed)。
    let mut records = Vec::with_capacity(new.steps.len());
    let mut rerun_required = Vec::new();
    for (index, new_step) in new.steps.iter().enumerate() {
        let keep = old.steps.get(index).is_some_and(|old_step| {
            old_step.id == new_step.id && spec_fingerprint(old_step) == spec_fingerprint(new_step)
        });
        if keep {
            records.push(run.steps[index].clone());
            continue;
        }
        // 原位改名的写步且已完成:重放副作用需显式授权。异位替换
        // (remove+add)不在此列——旧记录随删除步归档,新步天然 Pending。
        let needs_accept = old.steps.get(index).is_some_and(|old_step| {
            old_step.id == new_step.id
                && is_write(old_step)
                && matches!(
                    run.steps.get(index).map(|record| record.status),
                    Some(crate::engine::StepStatus::Completed)
                )
        });
        if needs_accept && !accepted.contains(&new_step.id) {
            rerun_required.push(new_step.id.clone());
            records.push(run.steps[index].clone());
            continue;
        }
        records.push(crate::engine::StepRecord {
            id: new_step.id.clone(),
            status: crate::engine::StepStatus::Pending,
            attempts: 0,
            checkpoint_passed: false,
            verified: false,
            error_code: None,
            error_evidence: None,
            attempt_errors: Vec::new(),
            outputs: Default::default(),
            outputs_captured_ms: None,
            observation_retries: Vec::new(),
            branch: None,
            recovery: None,
            plan_version: None,
        });
    }
    if !rerun_required.is_empty() {
        return Err(CtlError::with_evidence(
            ErrorCode::Protocol,
            "completed write steps changed by this revision need explicit --accept-rerun",
            serde_json::json!({"steps": rerun_required}),
        ));
    }
    run.steps = records;
    run.plan_version += 1;
    let revision = Revision {
        version: run.plan_version,
        fingerprint: fingerprint.to_string(),
        message: message.to_string(),
        diff,
        revised_ms: unix_ms(),
    };
    Ok(RevisionOutcome {
        revision,
        rerun_required,
    })
}

/// 修订持久化:归档旧计划字节 → 写新计划 → 写状态 → 追加版本索引。
/// 崩窗窗口说明:归档先于任何覆盖;新计划与新状态之间若崩溃,load 会以
/// "inconsistent flow state" 拒绝,重跑修订可恢复(诚实失败优于半状态)。
pub fn persist(
    dir: &Path,
    old_plan_bytes: &[u8],
    new_plan: &Plan,
    run: &Run,
    revision: &Revision,
) -> Result<(), CtlError> {
    let revisions = dir.join("revisions");
    std::fs::create_dir_all(&revisions).map_err(|e| CtlError::internal(e.to_string()))?;
    std::fs::write(
        revisions.join(format!("v{}.plan.json", revision.version - 1)),
        old_plan_bytes,
    )
    .map_err(|e| CtlError::internal(e.to_string()))?;
    std::fs::write(
        dir.join("plan.json"),
        serde_json::to_vec(new_plan).unwrap_or_default(),
    )
    .map_err(|e| CtlError::internal(e.to_string()))?;
    std::fs::write(
        dir.join("state.json"),
        serde_json::to_vec(run).unwrap_or_default(),
    )
    .map_err(|e| CtlError::internal(e.to_string()))?;
    let index_path = revisions.join("index.json");
    let mut index: Vec<Revision> = std::fs::read(&index_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    index.push(revision.clone());
    std::fs::write(index_path, serde_json::to_vec(&index).unwrap_or_default())
        .map_err(|e| CtlError::internal(e.to_string()))
}

/// 读取版本谱系(只读,state 展示用);无修订 = 空表。
pub fn lineage(dir: &Path) -> Vec<Revision> {
    std::fs::read(dir.join("revisions").join("index.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// 新计划字节的 FNV-1a 双通道指纹(与 flow.rs 模板指纹同算法族)。
pub fn plan_fingerprint(bytes: &[u8]) -> String {
    let (mut h1, mut h2) = (0xcbf2_9ce4_8422_2325u64, 0x9e37_79b9_7f4a_7c15u64);
    for byte in bytes {
        h1 = (h1 ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        h2 = (h2 ^ u64::from(*byte).rotate_left(7)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h1:016x}{h2:016x}")
}
