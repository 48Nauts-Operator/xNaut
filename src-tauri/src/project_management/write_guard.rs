//! XNAUT-442: one cooperative OS lease for PM writers and maintenance.
//! The filename retains XNAUT-472 compatibility. This is not a lock that raw
//! `git gc --prune=now` understands; external maintenance must cooperate.
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
thread_local! {
    static HELD: RefCell<HashSet<PathBuf>> = RefCell::new(HashSet::new());
    #[cfg(test)]
    static TEST_HOLD: RefCell<Option<Option<String>>> = const { RefCell::new(None) };
    static OWNER: Cell<bool> = const { Cell::new(false) };
}
/// Only native owner command handlers call this, around synchronous work. Never
/// hold the attribution over await, and never derive it from request JSON.
pub(crate) fn owner_action<T>(f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            OWNER.with(|v| v.set(self.0));
        }
    }
    let _restore = Restore(OWNER.with(|v| v.replace(true)));
    f()
}
pub(crate) fn common_dir(repo: &Path) -> Result<PathBuf, String> {
    let common = run_git(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    std::fs::canonicalize(common).map_err(|e| format!("cannot resolve control repository: {e}"))
}
pub(crate) struct ControlWriteLease {
    held: Option<(PathBuf, std::fs::File)>,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl ControlWriteLease {
    pub(crate) fn acquire(repo: &Path) -> Result<Self, String> {
        Self::acquire_with(repo, || {
            if OWNER.with(Cell::get) {
                None
            } else {
                automatic_hold()
            }
        })
    }
    fn acquire_with(repo: &Path, hold: impl FnOnce() -> Option<String>) -> Result<Self, String> {
        let common = common_dir(repo)?;
        // Nested record_mutation/release writes are in the already admitted
        // transaction. Rechecking a switched flag after writing JSON would
        // strand an uncommitted half-write.
        if HELD.with(|v| v.borrow().contains(&common)) {
            return Ok(Self {
                held: None,
                _thread: std::marker::PhantomData,
            });
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(common.join("xnaut-ticket-update.lock"))
            .map_err(|e| format!("cannot open control repository lease: {e}"))?;
        match file.try_lock() {
            Ok(()) => {},
            Err(std::fs::TryLockError::WouldBlock) => return Err("another process is writing or maintaining this control repository; retry after it finishes".into()),
            Err(std::fs::TryLockError::Error(e)) => return Err(format!("cannot lock control repository: {e}")),
        }
        if let Some(reason) = hold() {
            let _ = file.unlock();
            return Err(reason);
        }
        HELD.with(|v| v.borrow_mut().insert(common.clone()));
        Ok(Self {
            held: Some((common, file)),
            _thread: std::marker::PhantomData,
        })
    }
}
impl Drop for ControlWriteLease {
    fn drop(&mut self) {
        if let Some((common, file)) = self.held.take() {
            HELD.with(|v| v.borrow_mut().remove(&common));
            let _ = file.unlock();
        }
    }
}

fn automatic_hold() -> Option<String> {
    #[cfg(test)]
    if let Some(hold) = TEST_HOLD.with(|v| v.borrow().clone()) {
        return hold;
    }
    crate::switches::automatic_pm_write_hold_strict()
}
#[cfg(test)]
pub(super) fn with_hold<T>(hold: Option<&str>, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Option<String>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_HOLD.with(|v| *v.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(TEST_HOLD.with(|v| v.replace(Some(hold.map(str::to_owned)))));
    f()
}
