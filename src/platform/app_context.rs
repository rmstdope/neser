use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::platform::config::Config;

pub const TOAST_LIFETIME_SECS: u64 = 4;
pub const MAX_VISIBLE_TOASTS: usize = 3;

pub type SharedAppContext = Rc<RefCell<AppContext>>;

pub trait IntoSharedAppContext {
    fn into_shared(self) -> SharedAppContext;
}

#[derive(Debug, Clone)]
pub struct AppContext {
    toast_manager: ToastManager,
    config: Config,
}

impl Default for AppContext {
    fn default() -> Self {
        Self {
            toast_manager: ToastManager::new(),
            config: Config::default(),
        }
    }
}

impl IntoSharedAppContext for AppContext {
    fn into_shared(self) -> SharedAppContext {
        Rc::new(RefCell::new(self))
    }
}

impl IntoSharedAppContext for &AppContext {
    fn into_shared(self) -> SharedAppContext {
        Rc::new(RefCell::new(self.clone()))
    }
}

impl IntoSharedAppContext for SharedAppContext {
    fn into_shared(self) -> SharedAppContext {
        self
    }
}

impl AppContext {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_config(config: Config) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Queues a toast. It reads no clock: `Instant::now()` panics on wasm32-unknown-unknown,
    /// and this is reached from load paths the browser build takes (nr-6sm). The toast's
    /// lifetime starts at the first [`Self::visible_toasts`] call after it is added.
    pub fn add_toast(&mut self, text: impl Into<String>) {
        self.toast_manager.push(text.into(), None);
    }

    /// Removes every queued toast and returns its text, in the order raised. For a frontend
    /// that shows toasts itself (the web), so a toast raised in a core reaches it and is
    /// not kept after. Reads no clock, like [`Self::add_toast`].
    pub fn take_toasts(&mut self) -> Vec<String> {
        self.toast_manager.take()
    }

    pub fn visible_toasts(&mut self, now: Instant) -> Vec<String> {
        self.toast_manager
            .visible_toasts(now)
            .into_iter()
            .map(|toast| toast.text.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Toast {
    text: String,
    /// `None` until the first `visible_toasts` call after it is added; see [`AppContext::add_toast`].
    created_at: Option<Instant>,
}

#[derive(Debug, Clone, Default)]
struct ToastManager {
    toasts: Vec<Toast>,
}

impl ToastManager {
    fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, text: String, created_at: Option<Instant>) {
        self.toasts.push(Toast { text, created_at });
    }

    fn take(&mut self) -> Vec<String> {
        self.toasts.drain(..).map(|toast| toast.text).collect()
    }

    fn prune_expired(&mut self, now: Instant) {
        let lifetime = Duration::from_secs(TOAST_LIFETIME_SECS);
        self.toasts.retain(|toast| {
            toast
                .created_at
                .is_none_or(|created_at| now.saturating_duration_since(created_at) <= lifetime)
        });
    }

    fn stamp_new(&mut self, now: Instant) {
        for toast in &mut self.toasts {
            toast.created_at.get_or_insert(now);
        }
    }

    fn visible_toasts(&mut self, now: Instant) -> Vec<&Toast> {
        self.prune_expired(now);
        self.stamp_new(now);
        let start = self.toasts.len().saturating_sub(MAX_VISIBLE_TOASTS);
        self.toasts[start..].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_toasts_returns_queued_toasts_in_order_and_empties_the_queue() {
        let mut context = AppContext::new();
        context.add_toast("first");
        context.add_toast("second");

        assert_eq!(context.take_toasts(), ["first", "second"]);
        assert!(context.take_toasts().is_empty());
        assert!(context.visible_toasts(Instant::now()).is_empty());
    }

    #[test]
    fn test_toast_manager_expires_toast_after_lifetime() {
        let mut manager = ToastManager::new();
        let now = Instant::now();
        manager.push("Saved state".to_string(), Some(now));

        let visible = manager.visible_toasts(now + Duration::from_secs(TOAST_LIFETIME_SECS));
        assert_eq!(visible.len(), 1);

        let visible = manager.visible_toasts(now + Duration::from_secs(TOAST_LIFETIME_SECS + 1));
        assert!(visible.is_empty());
    }

    #[test]
    fn test_toast_manager_expires_without_extra_truncated_second() {
        let mut manager = ToastManager::new();
        let now = Instant::now();
        manager.push("Saved state".to_string(), Some(now));

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

        manager.push("One".to_string(), Some(now));
        manager.push("Two".to_string(), Some(now + Duration::from_millis(1)));
        manager.push("Three".to_string(), Some(now + Duration::from_millis(2)));
        manager.push("Four".to_string(), Some(now + Duration::from_millis(3)));

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

        manager.push("Oldest".to_string(), Some(now));
        manager.push("Middle".to_string(), Some(now + Duration::from_millis(1)));
        manager.push("Newest".to_string(), Some(now + Duration::from_millis(2)));

        let visible = manager.visible_toasts(now + Duration::from_millis(2));
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].text, "Oldest");
        assert_eq!(visible[1].text, "Middle");
        assert_eq!(visible[2].text, "Newest");
    }

    #[test]
    fn test_app_context_exposes_toast_visibility() {
        let mut context = AppContext::new();
        context.add_toast("Saved state");

        let visible = context.visible_toasts(Instant::now());
        assert_eq!(visible, vec!["Saved state".to_string()]);
    }

    #[test]
    fn test_added_toast_lives_from_when_it_is_first_shown() {
        let mut context = AppContext::new();
        context.add_toast("Saved state");
        let shown = Instant::now() + Duration::from_secs(60);

        assert_eq!(context.visible_toasts(shown).len(), 1);
        let lifetime = Duration::from_secs(TOAST_LIFETIME_SECS);
        assert_eq!(context.visible_toasts(shown + lifetime).len(), 1);
        assert!(
            context
                .visible_toasts(shown + lifetime + Duration::from_millis(1))
                .is_empty()
        );
    }
}
