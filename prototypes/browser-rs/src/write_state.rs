//! 写入闸门:写入开始(派发前)即进入 uncertain,核验通过前禁止任何新写入;
//! 语义与 Python 原型 adapter.py 一致,是 docs/72 uncertain 持久化设计的内存前身。

use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub enum WriteState {
    Idle,
    Uncertain { attempt: u64 },
}

pub const WRITE_REFUSED_UNCERTAIN: &str = "WRITE_REFUSED_UNCERTAIN";

pub struct WriteGate {
    state: Mutex<WriteState>,
    attempts: u64,
}

impl WriteGate {
    pub fn new() -> Self {
        Self { state: Mutex::new(WriteState::Idle), attempts: 0 }
    }

    /// 派发前调用:置 uncertain 并返回本次尝试号;已处于未核验的 uncertain 时拒绝。
    pub fn begin(&mut self) -> Result<u64, String> {
        let mut st = self.state.lock().unwrap();
        if let WriteState::Uncertain { attempt } = *st {
            return Err(format!(
                "{WRITE_REFUSED_UNCERTAIN}: write attempt #{attempt} is unverified; run a read-only recheck before any new write"
            ));
        }
        self.attempts += 1;
        *st = WriteState::Uncertain { attempt: self.attempts };
        Ok(self.attempts)
    }

    /// 仅核验通过且尝试号匹配时回到 Idle。
    pub fn confirm(&self, attempt: u64) {
        let mut st = self.state.lock().unwrap();
        if let WriteState::Uncertain { attempt: a } = *st {
            if a == attempt {
                *st = WriteState::Idle;
            }
        }
    }

    pub fn state(&self) -> WriteState {
        self.state.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_enters_uncertain_and_refuses_second_write() {
        let mut gate = WriteGate::new();
        let a1 = gate.begin().unwrap();
        assert_eq!(gate.state(), WriteState::Uncertain { attempt: a1 });
        let refused = gate.begin().unwrap_err();
        assert!(refused.starts_with(WRITE_REFUSED_UNCERTAIN));
    }

    #[test]
    fn confirm_clears_only_its_own_attempt() {
        let mut gate = WriteGate::new();
        let a1 = gate.begin().unwrap();
        gate.confirm(a1 + 999); // 过期尝试号不得误清
        assert_eq!(gate.state(), WriteState::Uncertain { attempt: a1 });
        gate.confirm(a1);
        assert_eq!(gate.state(), WriteState::Idle);
        gate.begin().unwrap(); // 清除后可再次写入
    }
}
