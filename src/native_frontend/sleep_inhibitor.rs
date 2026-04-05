//! Display sleep inhibitor for the native frontend.
//!
//! Prevents the OS from dimming or sleeping the display while the emulator is
//! actively running. Uses the `nosleep` crate for cross-platform support.

use nosleep::{NoSleep, NoSleepType};

/// Tracks whether display sleep is currently inhibited.
///
/// Wraps the platform-specific sleep prevention mechanism and provides
/// idempotent `activate()` / `deactivate()` transitions.
pub struct SleepInhibitor {
    inner: NoSleep,
    active: bool,
}

impl SleepInhibitor {
    /// Creates a new sleep inhibitor (initially inactive).
    pub fn new() -> Result<Self, String> {
        let inner =
            NoSleep::new().map_err(|e| format!("failed to create sleep inhibitor: {e:?}"))?;
        Ok(Self {
            inner,
            active: false,
        })
    }

    /// Inhibits display sleep if not already inhibited.
    pub fn activate(&mut self) {
        if self.active {
            return;
        }
        if let Err(e) = self.inner.start(NoSleepType::PreventUserIdleDisplaySleep) {
            crate::debugging::log_info(format!("Failed to inhibit display sleep: {e:?}"));
            return;
        }
        self.active = true;
    }

    /// Releases display sleep inhibition if currently active.
    pub fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        if let Err(e) = self.inner.stop() {
            crate::debugging::log_info(format!(
                "Failed to release display sleep inhibition: {e:?}"
            ));
        }
        self.active = false;
    }

    /// Returns whether display sleep is currently inhibited.
    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Drop for SleepInhibitor {
    fn drop(&mut self) {
        self.deactivate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_inhibitor_is_inactive() {
        let inhibitor = SleepInhibitor::new();
        // new() may fail in test environments without display services;
        // skip gracefully in that case.
        let Ok(inhibitor) = inhibitor else {
            return;
        };
        assert!(!inhibitor.is_active());
    }

    #[test]
    fn activate_sets_active_state() {
        let Ok(mut inhibitor) = SleepInhibitor::new() else {
            return;
        };
        inhibitor.activate();
        assert!(inhibitor.is_active());
    }

    #[test]
    fn deactivate_clears_active_state() {
        let Ok(mut inhibitor) = SleepInhibitor::new() else {
            return;
        };
        inhibitor.activate();
        inhibitor.deactivate();
        assert!(!inhibitor.is_active());
    }

    #[test]
    fn activate_is_idempotent() {
        let Ok(mut inhibitor) = SleepInhibitor::new() else {
            return;
        };
        inhibitor.activate();
        inhibitor.activate(); // should not panic or error
        assert!(inhibitor.is_active());
    }

    #[test]
    fn deactivate_when_inactive_is_noop() {
        let Ok(mut inhibitor) = SleepInhibitor::new() else {
            return;
        };
        inhibitor.deactivate(); // should not panic or error
        assert!(!inhibitor.is_active());
    }

    #[test]
    fn activate_deactivate_cycle() {
        let Ok(mut inhibitor) = SleepInhibitor::new() else {
            return;
        };
        inhibitor.activate();
        assert!(inhibitor.is_active());
        inhibitor.deactivate();
        assert!(!inhibitor.is_active());
        inhibitor.activate();
        assert!(inhibitor.is_active());
    }
}
