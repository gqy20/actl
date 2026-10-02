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
}
impl<K, T> Default for Worker<K, T> {
    fn default() -> Self {
        Self {
            pending: None,
            next: None,
            latest: None,
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
        if let Some((_, job)) = &self.pending {
            match job.poll() {
                Ok(Some(result)) => {
                    if let Some((key, _)) = self.pending.take() {
                        self.latest = Some((key, Instant::now(), result));
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
            self.next = Some(now + interval);
            match Job::start(work) {
                Ok(job) => self.pending = Some((key.clone(), job)),
                Err(error) => {
                    self.latest = None;
                    return Err(error);
                }
            }
        }
        Ok(self
            .latest
            .as_ref()
            .filter(|(old, at, _)| *old == key && now.duration_since(*at) <= interval * 2)
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
        };
        assert!(worker.poll(1, Duration::from_secs(5), || 8).is_err());
        assert!(worker.latest.is_none());
    }
}
