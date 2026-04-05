//! Display sleep inhibitor for the native frontend.
//!
//! Prevents the OS from dimming or sleeping the display while the emulator is
//! actively running. Uses the `nosleep` crate for cross-platform support.
//!
//! The state machine is tested via a mock backend so tests are deterministic
//! and don't depend on platform services (D-Bus, IOKit, etc.).

use nosleep::{NoSleep, NoSleepType};

/// Abstraction over the platform sleep-prevention API.
trait SleepBackend {
    fn start(&mut self) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
}

/// Real backend that delegates to the `nosleep` crate.
struct NosleepBackend {
    inner: NoSleep,
}

impl SleepBackend for NosleepBackend {
    fn start(&mut self) -> Result<(), String> {
        self.inner
            .start(NoSleepType::PreventUserIdleDisplaySleep)
            .map_err(|e| format!("{e:?}"))
    }

    fn stop(&mut self) -> Result<(), String> {
        self.inner.stop().map_err(|e| format!("{e:?}"))
    }
}

/// Tracks whether display sleep is currently inhibited.
///
/// Wraps the platform-specific sleep prevention mechanism and provides
/// idempotent `activate()` / `deactivate()` transitions. Stops retrying
/// after the first failure to avoid log spam on headless/CI systems.
pub struct SleepInhibitor {
    backend: Box<dyn SleepBackend>,
    active: bool,
    permanently_failed: bool,
}

impl SleepInhibitor {
    /// Creates a new sleep inhibitor (initially inactive).
    pub fn new() -> Result<Self, String> {
        let inner =
            NoSleep::new().map_err(|e| format!("failed to create sleep inhibitor: {e:?}"))?;
        Ok(Self {
            backend: Box::new(NosleepBackend { inner }),
            active: false,
            permanently_failed: false,
        })
    }

    #[cfg(test)]
    fn with_backend(backend: Box<dyn SleepBackend>) -> Self {
        Self {
            backend,
            active: false,
            permanently_failed: false,
        }
    }

    /// Inhibits display sleep if not already inhibited.
    ///
    /// After the first failure, further attempts are suppressed to avoid
    /// spamming logs on headless or unsupported systems.
    pub fn activate(&mut self) {
        if self.active || self.permanently_failed {
            return;
        }
        if let Err(e) = self.backend.start() {
            crate::debugging::log_info(format!(
                "Failed to inhibit display sleep (will not retry): {e}"
            ));
            self.permanently_failed = true;
            return;
        }
        self.active = true;
    }

    /// Releases display sleep inhibition if currently active.
    ///
    /// Only clears the active state on successful stop. On failure the
    /// inhibitor remains marked active so `Drop` will retry cleanup.
    pub fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        if let Err(e) = self.backend.stop() {
            crate::debugging::log_info(format!("Failed to release display sleep inhibition: {e}"));
            return;
        }
        self.active = false;
    }
}

impl Drop for SleepInhibitor {
    fn drop(&mut self) {
        if self.active {
            // Best-effort cleanup; ignore errors since we're dropping.
            let _ = self.backend.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Mock backend that records calls and can be configured to fail.
    struct MockBackend {
        start_results: Vec<Result<(), String>>,
        stop_results: Vec<Result<(), String>>,
        call_log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl MockBackend {
        fn new(
            start_results: Vec<Result<(), String>>,
            stop_results: Vec<Result<(), String>>,
            call_log: Rc<RefCell<Vec<&'static str>>>,
        ) -> Self {
            Self {
                start_results,
                stop_results,
                call_log,
            }
        }
    }

    impl SleepBackend for MockBackend {
        fn start(&mut self) -> Result<(), String> {
            self.call_log.borrow_mut().push("start");
            if self.start_results.is_empty() {
                Ok(())
            } else {
                self.start_results.remove(0)
            }
        }

        fn stop(&mut self) -> Result<(), String> {
            self.call_log.borrow_mut().push("stop");
            if self.stop_results.is_empty() {
                Ok(())
            } else {
                self.stop_results.remove(0)
            }
        }
    }

    fn make_inhibitor(
        start_results: Vec<Result<(), String>>,
        stop_results: Vec<Result<(), String>>,
    ) -> (SleepInhibitor, Rc<RefCell<Vec<&'static str>>>) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let backend = MockBackend::new(start_results, stop_results, Rc::clone(&log));
        (SleepInhibitor::with_backend(Box::new(backend)), log)
    }

    #[test]
    fn new_inhibitor_is_not_active() {
        let (inhibitor, _) = make_inhibitor(vec![], vec![]);
        assert!(!inhibitor.active);
    }

    #[test]
    fn activate_calls_backend_start_and_sets_active() {
        let (mut inhibitor, log) = make_inhibitor(vec![Ok(())], vec![]);
        inhibitor.activate();
        assert!(inhibitor.active);
        assert_eq!(*log.borrow(), vec!["start"]);
    }

    #[test]
    fn deactivate_calls_backend_stop_and_clears_active() {
        let (mut inhibitor, log) = make_inhibitor(vec![Ok(())], vec![Ok(())]);
        inhibitor.activate();
        inhibitor.deactivate();
        assert!(!inhibitor.active);
        assert_eq!(*log.borrow(), vec!["start", "stop"]);
    }

    #[test]
    fn activate_is_idempotent() {
        let (mut inhibitor, log) = make_inhibitor(vec![Ok(()), Ok(())], vec![]);
        inhibitor.activate();
        inhibitor.activate();
        assert!(inhibitor.active);
        // Backend start should only be called once.
        assert_eq!(*log.borrow(), vec!["start"]);
    }

    #[test]
    fn deactivate_when_inactive_is_noop() {
        let (mut inhibitor, log) = make_inhibitor(vec![], vec![]);
        inhibitor.deactivate();
        assert!(!inhibitor.active);
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn activate_deactivate_cycle() {
        let (mut inhibitor, log) = make_inhibitor(vec![Ok(()), Ok(())], vec![Ok(()), Ok(())]);
        inhibitor.activate();
        assert!(inhibitor.active);
        inhibitor.deactivate();
        assert!(!inhibitor.active);
        inhibitor.activate();
        assert!(inhibitor.active);
        assert_eq!(*log.borrow(), vec!["start", "stop", "start"]);
    }

    #[test]
    fn activate_failure_sets_permanently_failed() {
        let (mut inhibitor, log) = make_inhibitor(vec![Err("no D-Bus".into())], vec![]);
        inhibitor.activate();
        assert!(!inhibitor.active);
        assert!(inhibitor.permanently_failed);
        // Second call should be suppressed entirely.
        inhibitor.activate();
        assert_eq!(*log.borrow(), vec!["start"]);
    }

    #[test]
    fn deactivate_failure_keeps_active_state() {
        let (mut inhibitor, log) = make_inhibitor(vec![Ok(())], vec![Err("stop failed".into())]);
        inhibitor.activate();
        inhibitor.deactivate();
        // active should stay true because stop() failed.
        assert!(inhibitor.active);
        assert_eq!(*log.borrow(), vec!["start", "stop"]);
    }

    #[test]
    fn drop_calls_stop_when_active() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let backend = MockBackend::new(vec![Ok(())], vec![Ok(())], Rc::clone(&log));
        let mut inhibitor = SleepInhibitor::with_backend(Box::new(backend));
        inhibitor.activate();
        drop(inhibitor);
        assert_eq!(*log.borrow(), vec!["start", "stop"]);
    }

    #[test]
    fn drop_does_not_call_stop_when_inactive() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let backend = MockBackend::new(vec![], vec![], Rc::clone(&log));
        let inhibitor = SleepInhibitor::with_backend(Box::new(backend));
        drop(inhibitor);
        assert!(log.borrow().is_empty());
    }
}
