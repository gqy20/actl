//! Scoped cancellation for workflow waits; never interrupts an in-flight COM call.
use crate::{CtlError, ErrorCode};
use std::{cell::RefCell, marker::PhantomData, path::PathBuf, rc::Rc};

thread_local! {
    static PAUSE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}
pub struct Scope(Option<PathBuf>, PhantomData<Rc<()>>);
impl Scope {
    pub fn enter(path: PathBuf) -> Self {
        Self(PAUSE.with(|p| p.replace(Some(path))), PhantomData)
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        PAUSE.with(|p| p.replace(self.0.take()));
    }
}
pub fn check() -> Result<(), CtlError> {
    crate::stop::check_current()?;
    PAUSE.with(|p| {
        if let Some(path) = p.borrow().as_ref()
            && path.exists()
        {
            return Err(CtlError::new(
                ErrorCode::Aborted,
                "workflow wait interrupted",
            ));
        }
        Ok(())
    })
}
/// 暂停请求是否待处理(仅流程 Scope 内有效;普通 CLI 调用恒 false)。
/// 注入批间等非等待场景的观察点用,错误包装由调用方决定。
pub fn pause_pending() -> bool {
    PAUSE.with(|p| p.borrow().as_ref().is_some_and(|path| path.exists()))
}
