use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub const TOAST_LIFETIME_SECS: u64 = 4;
pub const MAX_VISIBLE_TOASTS: usize = 3;

#[derive(Debug, Clone)]
pub struct AppContext {
    toast_manager: Rc<RefCell<ToastManager>>,
}

impl Default for AppContext {
    fn default() -> Self {
        Self {
            toast_manager: Rc::new(RefCell::new(ToastManager::new())),
        }
    }
}

impl AppContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_toast(&self, text: impl Into<String>) {
        self.toast_manager
            .borrow_mut()
            .push(text.into(), Instant::now());
    }

    pub fn visible_toasts(&self, now: Instant) -> Vec<String> {
        self.toast_manager
            .borrow_mut()
            .visible_toasts(now)
            .into_iter()
            .map(|toast| toast.text.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Toast {
    text: String,
    created_at: Instant,
}

#[derive(Debug, Default)]
struct ToastManager {
    toasts: Vec<Toast>,
}

impl ToastManager {
    fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, text: String, now: Instant) {
        self.toasts.push(Toast {
            text,
            created_at: now,
        });
    }

    fn prune_expired(&mut self, now: Instant) {
        let lifetime = Duration::from_secs(TOAST_LIFETIME_SECS);
        self.toasts
            .retain(|toast| now.saturating_duration_since(toast.created_at) <= lifetime);
    }

    fn visible_toasts(&mut self, now: Instant) -> Vec<&Toast> {
        self.prune_expired(now);
        let start = self.toasts.len().saturating_sub(MAX_VISIBLE_TOASTS);
        self.toasts[start..].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_manager_expires_toast_after_lifetime() {
        let mut manager = ToastManager::new();
        let now = Instant::now();
        manager.push("Saved state".to_string(), now);

        let visible = manager.visible_toasts(now + Duration::from_secs(TOAST_LIFETIME_SECS));
        assert_eq!(visible.len(), 1);

        let visible = manager.visible_toasts(now + Duration::from_secs(TOAST_LIFETIME_SECS + 1));
        assert!(visible.is_empty());
    }

    #[test]
    fn test_toast_manager_expires_without_extra_truncated_second() {
        let mut manager = ToastManager::new();
        let now = Instant::now();
        manager.push("Saved state".to_string(), now);

        let visible = manager.visible_toasts(
            now + Duration::from_secs(TOAST_LIFETIME_SECS) + Duration::from_millis(999),
        );
        assert!(
            visible.is_empty(),
            "toast should expire once lifetime is exceeded, even within the next second"
        );
    }

    #[test]
    fn test_toast_manager_limits_visible_to_three() {
        let mut manager = ToastManager::new();
        let now = Instant::now();

        manager.push("One".to_string(), now);
        manager.push("Two".to_string(), now + Duration::from_millis(1));
        manager.push("Three".to_string(), now + Duration::from_millis(2));
        manager.push("Four".to_string(), now + Duration::from_millis(3));

        let visible = manager.visible_toasts(now + Duration::from_millis(3));
        assert_eq!(visible.len(), MAX_VISIBLE_TOASTS);
        assert_eq!(visible[0].text, "Two");
        assert_eq!(visible[1].text, "Three");
        assert_eq!(visible[2].text, "Four");
    }

    #[test]
    fn test_toast_manager_returns_oldest_to_newest_for_stacking() {
        let mut manager = ToastManager::new();
        let now = Instant::now();

        manager.push("Oldest".to_string(), now);
        manager.push("Middle".to_string(), now + Duration::from_millis(1));
        manager.push("Newest".to_string(), now + Duration::from_millis(2));

        let visible = manager.visible_toasts(now + Duration::from_millis(2));
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].text, "Oldest");
        assert_eq!(visible[1].text, "Middle");
        assert_eq!(visible[2].text, "Newest");
    }

    #[test]
    fn test_app_context_exposes_toast_visibility() {
        let context = AppContext::new();
        context.add_toast("Saved state");

        let visible = context.visible_toasts(Instant::now());
        assert_eq!(visible, vec!["Saved state".to_string()]);
    }
}
