use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::cartridge::{RomDb, RomDbEntry};
use crate::console::Config;

pub const TOAST_LIFETIME_SECS: u64 = 4;
pub const MAX_VISIBLE_TOASTS: usize = 3;

pub type SharedAppContext = Rc<RefCell<AppContext>>;

pub trait IntoSharedAppContext {
    fn into_shared(self) -> SharedAppContext;
}

#[derive(Debug, Clone)]
pub struct AppContext {
    toast_manager: ToastManager,
    rom_db: RomDb,
    config: Config,
}

impl Default for AppContext {
    fn default() -> Self {
        Self {
            toast_manager: ToastManager::new(),
            rom_db: load_rom_db(),
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

// TODO ???
#[cfg(not(target_arch = "wasm32"))]
fn load_rom_db() -> RomDb {
    RomDb::new().unwrap()
}

#[cfg(target_arch = "wasm32")]
fn load_rom_db() -> RomDb {
    RomDb::from_csv_content(include_str!("cartridge/rom_db.csv"))
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

    pub fn rom_db(&self) -> &RomDb {
        &self.rom_db
    }

    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn add_toast(&mut self, text: impl Into<String>) {
        self.toast_manager.push(text.into(), Instant::now());
    }

    pub fn visible_toasts(&mut self, now: Instant) -> Vec<String> {
        self.toast_manager
            .visible_toasts(now)
            .into_iter()
            .map(|toast| toast.text.clone())
            .collect()
    }

    #[allow(dead_code)]
    pub fn get_db_entry_by_crc(&self, crc: u32) -> Option<RomDbEntry> {
        self.rom_db.get_by_crc(crc).cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Toast {
    text: String,
    created_at: Instant,
}

#[derive(Debug, Clone, Default)]
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
        let mut context = AppContext::new();
        context.add_toast("Saved state");

        let visible = context.visible_toasts(Instant::now());
        assert_eq!(visible, vec!["Saved state".to_string()]);
    }

    #[test]
    fn test_app_context_get_db_entry_by_crc() {
        let context = AppContext::new();
        let entry = context
            .get_db_entry_by_crc(0x836C4FA7)
            .expect("known CRC should exist in rom_db.csv");

        assert_eq!(entry.crc, Some(0x836C4FA7));
    }
}
