//! Launch the emulator as a child process for a selected ROM.
//!
//! The TUI suspends (leaves raw mode + alternate screen), waits for the child
//! emulator process to finish, then restores the terminal.

use std::path::Path;
use std::process::{Command, ExitStatus};

/// The action to perform on a selected ROM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchAction {
    Play,
    Record,
    Playback,
}

/// Result of a launch operation.
#[derive(Debug)]
pub struct LaunchResult {
    pub action: LaunchAction,
    pub rom_path: String,
    pub exit_status: Option<ExitStatus>,
    pub error: Option<String>,
}

impl LaunchResult {
    /// Returns a short human-readable summary for the status bar.
    pub fn summary(&self) -> String {
        let action = match self.action {
            LaunchAction::Play => "Played",
            LaunchAction::Record => "Recorded",
            LaunchAction::Playback => "Played back",
        };
        let stem = Path::new(&self.rom_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&self.rom_path);
        if let Some(err) = &self.error {
            format!("{action}: {stem} — error: {err}")
        } else if let Some(status) = self.exit_status {
            if status.success() {
                format!("{action}: {stem} — OK")
            } else {
                format!("{action}: {stem} — exited {status}")
            }
        } else {
            format!("{action}: {stem}")
        }
    }
}

/// Build the command arguments for launching the emulator.
///
/// Returns `(executable, args)` where executable is the current binary path.
pub fn build_launch_command(rom_path: &str, action: LaunchAction) -> (String, Vec<String>) {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| "neser".to_string());

    let mut args = vec![rom_path.to_string()];
    match action {
        LaunchAction::Play => {}
        LaunchAction::Record => args.push("--create-recording".to_string()),
        LaunchAction::Playback => args.push("--playback".to_string()),
    }

    (exe, args)
}

/// Suspend the TUI, spawn the emulator, wait for it to complete, return result.
///
/// The caller is responsible for restoring the terminal after this returns
/// (typically via the `TerminalHandle` Drop impl after re-entering the TUI).
pub fn launch_rom(rom_path: &str, action: LaunchAction) -> LaunchResult {
    let (exe, args) = build_launch_command(rom_path, action);

    // Restore terminal to normal state before handing control to SDL emulator.
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);

    let exit_status = Command::new(&exe)
        .args(&args)
        .status()
        .map_err(|e| e.to_string());

    match exit_status {
        Ok(status) => LaunchResult {
            action,
            rom_path: rom_path.to_string(),
            exit_status: Some(status),
            error: None,
        },
        Err(err) => LaunchResult {
            action,
            rom_path: rom_path.to_string(),
            exit_status: None,
            error: Some(err),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_launch_command_play_has_no_extra_flags() {
        let (_, args) = build_launch_command("/roms/game.nes", LaunchAction::Play);
        assert_eq!(args, vec!["/roms/game.nes"]);
        assert!(!args.contains(&"--create-recording".to_string()));
        assert!(!args.contains(&"--playback".to_string()));
    }

    #[test]
    fn test_build_launch_command_record_has_create_recording_flag() {
        let (_, args) = build_launch_command("/roms/game.nes", LaunchAction::Record);
        assert!(
            args.contains(&"--create-recording".to_string()),
            "Record should add --create-recording"
        );
    }

    #[test]
    fn test_build_launch_command_playback_has_playback_flag() {
        let (_, args) = build_launch_command("/roms/game.nes", LaunchAction::Playback);
        assert!(
            args.contains(&"--playback".to_string()),
            "Playback should add --playback"
        );
    }

    #[test]
    fn test_build_launch_command_rom_path_always_first() {
        for action in [
            LaunchAction::Play,
            LaunchAction::Record,
            LaunchAction::Playback,
        ] {
            let (_, args) = build_launch_command("/roms/game.nes", action);
            assert_eq!(args[0], "/roms/game.nes", "ROM path must be first argument");
        }
    }

    #[test]
    fn test_launch_result_summary_play_success() {
        let result = LaunchResult {
            action: LaunchAction::Play,
            rom_path: "/roms/game.nes".to_string(),
            exit_status: None,
            error: None,
        };
        assert!(result.summary().contains("Played"));
        assert!(result.summary().contains("game.nes"));
    }

    #[test]
    fn test_launch_result_summary_error() {
        let result = LaunchResult {
            action: LaunchAction::Record,
            rom_path: "/roms/game.nes".to_string(),
            exit_status: None,
            error: Some("not found".to_string()),
        };
        let summary = result.summary();
        assert!(summary.contains("error"));
        assert!(summary.contains("not found"));
    }
}
