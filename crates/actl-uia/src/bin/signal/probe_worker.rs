//! A single background repair probe; UI polling never waits for filesystem/CLI work.
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

pub(super) struct Job<T> {
    receiver: mpsc::Receiver<T>,
}
impl<T: Send + 'static> Job<T> {
    pub fn start(work: impl FnOnce() -> T + Send + 'static) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("signal-repair-probe".into())
            .spawn(move || {
                let result = work();
                let _ = sender.send(result);
            })?;
        Ok(Self { receiver })
    }
    pub fn poll(&self) -> Result<Option<T>, mpsc::TryRecvError> {
        match self.receiver.try_recv() {
            Ok(value) => Ok(Some(value)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

pub(super) struct Worker<K, T> {
    pending: Option<(K, Job<T>)>,
    next: Option<Instant>,
    latest: Option<(K, Instant, T)>,
    /// 当前退避间隔(键切换时重置为 base;每完成一次探测翻倍,封顶 CAP)
    interval_now: Duration,
}
/// 退避封顶:10 分钟放弃窗口内 ~13 次探测(旧固定 5s = 120 次——每次都
/// 冷启一个 actl.exe 跑 UIA 探测,机器高载时与真工作抢 UI 线程,handoff-02
/// 实证暂停 run 被探测了 200s+/40 次)。首个探测照旧立即派发(用户正看
/// 面板);结果新鲜度窗口随当前退避缩放——标签持续显示不闪断。
const BACKOFF_CAP: Duration = Duration::from_secs(60);
impl<K, T> Default for Worker<K, T> {
    fn default() -> Self {
        Self {
            pending: None,
            next: None,
            latest: None,
            interval_now: Duration::ZERO,
        }
    }
}
impl<K: Clone + Eq, T: Clone + Send + 'static> Worker<K, T> {
    pub fn poll(
        &mut self,
        key: K,
        interval: Duration,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> std::io::Result<Option<T>> {
        // 键切换(新暂停 run / 换目标):退避重置——新目标首轮探测恢复即时
        if !self.interval_now.is_zero()
            && self.latest.as_ref().is_none_or(|(old, _, _)| *old != key)
            && self.pending.as_ref().is_none_or(|(old, _)| *old != key)
        {
            self.interval_now = Duration::ZERO;
        }
        if let Some((_, job)) = &self.pending {
            match job.poll() {
                Ok(Some(result)) => {
                    if let Some((key, _)) = self.pending.take() {
                        self.latest = Some((key, Instant::now(), result));
                        // 退避推进:完成越多次数,下次间隔越长(5→10→20→40→60 封顶)
                        self.interval_now = if self.interval_now.is_zero() {
                            interval
                        } else {
                            (self.interval_now * 2).min(BACKOFF_CAP)
                        };
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    self.pending = None;
                    self.latest = None;
                    return Err(std::io::Error::other("repair probe worker disconnected"));
                }
            }
        }
        let now = Instant::now();
        if self.pending.is_none() && self.next.is_none_or(|next| now >= next) {
            let next_interval = if self.interval_now.is_zero() {
                interval
            } else {
                self.interval_now
            };
            self.next = Some(now + next_interval);
            match Job::start(work) {
                Ok(job) => self.pending = Some((key.clone(), job)),
                Err(error) => {
                    self.latest = None;
                    return Err(error);
                }
            }
        }
        let fresh = self.interval_now.max(interval) * 2;
        Ok(self
            .latest
            .as_ref()
            .filter(|(old, at, _)| *old == key && now.duration_since(*at) <= fresh)
            .map(|(_, _, result)| result.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocked_probe_does_not_block_poll_or_spawn_duplicate() {
        let (started, ready) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let mut worker = Worker::<u8, u8>::default();
        assert_eq!(
            worker
                .poll(1, Duration::from_secs(5), move || {
                    started.send(()).unwrap();
                    wait.recv().unwrap();
                    7
                })
                .unwrap(),
            None
        );
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            worker
                .poll(1, Duration::ZERO, || panic!("overlapping probe"))
                .unwrap(),
            None
        );
        assert_eq!(
            worker
                .poll(2, Duration::ZERO, || panic!(
                    "key change must not spawn overlap"
                ))
                .unwrap(),
            None
        );
        release.send(()).unwrap();
    }
    #[test]
    fn stale_target_and_expired_result_are_not_returned() {
        let interval = Duration::from_secs(5);
        let mut worker = Worker::<u8, u8> {
            next: Some(Instant::now() + interval),
            latest: Some((1, Instant::now(), 7)),
            ..Default::default()
        };
        assert_eq!(worker.poll(1, interval, || panic!()).unwrap(), Some(7));
        assert_eq!(worker.poll(2, interval, || panic!()).unwrap(), None);
        worker.latest = Some((1, Instant::now() - interval * 3, 7));
        assert_eq!(worker.poll(1, interval, || panic!()).unwrap(), None);
    }
    #[test]
    fn disconnected_worker_invalidates_previous_result() {
        let (tx, rx) = mpsc::channel();
        drop(tx);
        let mut worker = Worker {
            pending: Some((1, Job { receiver: rx })),
            latest: Some((1, Instant::now(), 7)),
            next: None,
            interval_now: Duration::ZERO,
        };
        assert!(worker.poll(1, Duration::from_secs(5), || 8).is_err());
        assert!(worker.latest.is_none());
    }
    /// 退避序列:完成次数推进间隔翻倍(5→10→…封顶 60s),键切换重置回 base
    /// ——10 分钟窗口内 ~13 次探测(旧固定 5s = 120 次冷启)。
    /// settle 用轮询等待完成(固定 sleep 在并行测试负载下会丢完成事件)。
    #[test]
    fn probe_interval_backs_off_and_resets_on_key_change() {
        let interval = Duration::from_secs(5);
        let mut worker = Worker::<u8, u8>::default();
        // 完成当前在飞的 job:轮询到 latest 时间戳前进为止(有界防死循环);
        // 必须携带当前 key——异键轮询会触发键切换重置,毁掉退避断言。
        let settle_complete = |worker: &mut Worker<u8, u8>, key: u8| {
            let before = worker.latest.as_ref().map(|(_, at, _)| *at);
            for _ in 0..500 {
                worker.poll(key, interval, || panic!("not due")).unwrap();
                let advanced = worker
                    .latest
                    .as_ref()
                    .is_some_and(|(k, at, _)| *k == key && before.is_none_or(|b| *at > b));
                if advanced {
                    return;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            panic!("probe job never completed");
        };
        worker.poll(1, interval, || 7).unwrap(); // 首个探测立即派发
        settle_complete(&mut worker, 1);
        assert_eq!(worker.interval_now, interval);
        worker.next = None; // 时间前进到下一轮
        worker.poll(1, interval, || 8).unwrap();
        settle_complete(&mut worker, 1);
        assert_eq!(worker.interval_now, interval * 2); // 翻倍
        worker.next = None;
        worker.poll(2, interval, || 9).unwrap(); // 键切换
        settle_complete(&mut worker, 2);
        assert_eq!(worker.interval_now, interval); // 重置回 base
        // 封顶序列:从 base 连续完成,10→20→40→80 封顶 60→60
        for expected in [
            interval * 2,
            interval * 4,
            interval * 8,
            BACKOFF_CAP,
            BACKOFF_CAP,
        ] {
            worker.next = None;
            worker.poll(2, interval, || 1).unwrap();
            settle_complete(&mut worker, 2);
            assert_eq!(worker.interval_now, expected);
        }
    }
}
