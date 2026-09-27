//! Scoped cancellation for workflow waits; never interrupts an in-flight COM call.
use crate::{CtlError, ErrorCode, state::SignalPaths};
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
    PAUSE.with(|p| {
        if let Some(path) = p.borrow().as_ref()
            && (path.exists() || SignalPaths::default().stop_requested())
        {
            return Err(CtlError::new(
                ErrorCode::Aborted,
                "workflow wait interrupted",
            ));
        }
        Ok(())
    })
}
