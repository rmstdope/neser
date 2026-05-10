//! ROM browser application for the native frontend.
//!
//! Provides a graphical ROM library browser that displays cover art,
//! metadata, and allows the user to search, filter, and launch ROMs.
//! This module implements the `ApplicationHandler` for the browser state.

mod app;

pub use app::{BrowserResult, RomBrowserApp};
