//! Mouse input handling for the native winit frontend.
//!
//! Routes host mouse events to NES mouse-emulated controllers
//! (Zapper, Arkanoid paddle, SNES Mouse) and manages the cursor
//! grab/release state machine.

use crate::frontends::native::gl_backend::Crosshair;
use crate::nes::input::ControllerInput;
use crate::nes::input::mouse_mapping;
use crate::platform::emulator::Console;

/// Mouse button abstraction (frontend-independent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

// ── Device detection ─────────────────────────────────────────────────────────

/// Returns `true` when any controller port or expansion device uses mouse input.
///
/// Always returns `false` for non-NES consoles.
pub fn has_any_mouse_controller(console: &Console) -> bool {
    let Console::Nes(nes) = console else {
        return false;
    };
    (1..=2).any(|port| nes.controller_input_type(port) == Some(ControllerInput::Mouse))
        || nes.has_expansion_mouse_controller()
}

/// Returns `true` when a Zapper is connected on any port or expansion.
///
/// Always returns `false` for non-NES consoles.
pub fn has_zapper(console: &Console) -> bool {
    let Console::Nes(nes) = console else {
        return false;
    };
    (1..=2).any(|port| nes.is_zapper_active(port)) || nes.has_expansion_zapper()
}

pub fn has_snes_mouse(console: &Console) -> bool {
    match console {
        Console::Nes(nes) => nes.has_snes_mouse(),
        Console::Snes(snes) => snes.has_mouse(),
        _ => false,
    }
}

fn snes_mouse_ports(console: &Console) -> [bool; 2] {
    match console {
        Console::Snes(snes) => [snes.has_mouse_on_port(0), snes.has_mouse_on_port(1)],
        _ => [false, false],
    }
}

// ── Coordinate routing ───────────────────────────────────────────────────────

/// Routes absolute mouse coordinates to the appropriate NES controller.
///
/// - Zapper / expansion Zapper / SNES Mouse: linear mapping on both axes (0–255).
/// - Arkanoid paddle: non-linear curve on X axis only.
///
/// Returns `Some((x, y))` in NES coordinates when a Zapper-style device is
/// active (for crosshair rendering), or `None` for paddle-only input or
/// non-NES consoles.
pub fn update_mouse_motion(
    console: &mut Console,
    x: i32,
    y: i32,
    window_width: u32,
    window_height: u32,
) -> Option<(u8, u8)> {
    let Console::Nes(nes) = console else {
        return None;
    };
    if has_zapper_nes(nes) || nes.has_snes_mouse() {
        let x_pos = mouse_mapping::map_mouse_axis_to_zapper_position(x, window_width);
        let y_pos = mouse_mapping::map_mouse_axis_to_zapper_position(y, window_height);
        nes.set_mouse_x_position(x_pos);
        nes.set_mouse_y_position(y_pos);
        Some((x_pos, y_pos))
    } else {
        let position = mouse_mapping::map_mouse_x_to_paddle_position(x, window_width);
        nes.set_mouse_x_position(position);
        None
    }
}

/// Applies relative mouse deltas (from locked cursor mode) to the SNES Mouse.
///
/// No-op for consoles without an SNES mouse.
pub fn apply_snes_mouse_relative_motion(
    console: &mut Console,
    xrel: i32,
    yrel: i32,
    window_width: u32,
    window_height: u32,
) {
    let snes_ports = snes_mouse_ports(console);
    let dx = mouse_mapping::map_relative_mouse_delta_to_axis_delta(xrel, window_width);
    let dy = mouse_mapping::map_relative_mouse_delta_to_axis_delta(yrel, window_height);
    match console {
        Console::Nes(nes) => nes.add_mouse_delta(dx, dy),
        Console::Snes(snes) => {
            for (port, enabled) in snes_ports.into_iter().enumerate() {
                if enabled {
                    snes.add_mouse_delta(port as u8, dx as i16, dy as i16);
                }
            }
        }
        _ => {}
    }
}

/// Forwards a mouse button press/release to the appropriate NES controller.
///
/// No-op for consoles without a mouse-driven controller.
pub fn update_mouse_button(console: &mut Console, button: MouseButton, pressed: bool) {
    let snes_ports = snes_mouse_ports(console);
    match console {
        Console::Nes(nes) => {
            if has_any_mouse_controller_nes(nes) {
                match button {
                    MouseButton::Left => nes.set_mouse_left_button(pressed),
                    MouseButton::Right => nes.set_mouse_right_button(pressed),
                }
            }
        }
        Console::Snes(snes) => {
            for (port, enabled) in snes_ports.into_iter().enumerate() {
                if enabled {
                    match button {
                        MouseButton::Left => snes.set_mouse_left_button(port as u8, pressed),
                        MouseButton::Right => snes.set_mouse_right_button(port as u8, pressed),
                    }
                }
            }
        }
        _ => {}
    }
}

/// Returns a [`Crosshair`] for the Zapper if one is connected and a position
/// has been recorded.
///
/// Always returns `None` for non-NES consoles.
pub fn zapper_crosshair(console: &Console, last_position: Option<(u8, u8)>) -> Option<Crosshair> {
    let Console::Nes(nes) = console else {
        return None;
    };
    if !has_zapper_nes(nes) {
        None
    } else {
        last_position.map(|(x, y)| Crosshair {
            x: x as f32,
            y: y as f32,
        })
    }
}

// ── NES-specific internal helpers ─────────────────────────────────────────────

fn has_any_mouse_controller_nes(nes: &crate::nes::console::Nes) -> bool {
    (1..=2).any(|port| nes.controller_input_type(port) == Some(ControllerInput::Mouse))
        || nes.has_expansion_mouse_controller()
}

fn has_zapper_nes(nes: &crate::nes::console::Nes) -> bool {
    (1..=2).any(|port| nes.is_zapper_active(port)) || nes.has_expansion_zapper()
}

/// Scales factor applied to raw `DeviceEvent::MouseMotion` deltas when
/// building the virtual cursor position for Zapper / Arkanoid.
/// A value of 2.0 matches typical SDL2 grab sensitivity.
pub const VIRTUAL_CURSOR_SENSITIVITY: f32 = 2.0;

/// Applies a raw mouse delta to a virtual cursor position, clamped to the
/// window dimensions.
///
/// Used for Zapper and Arkanoid when the cursor is locked (`CursorGrabMode::Locked`):
/// SDL2 synthesised absolute x,y from accumulated deltas internally;
/// this function replicates that behaviour in the winit frontend.
pub fn accumulate_virtual_cursor(
    current: (f32, f32),
    dx: f32,
    dy: f32,
    window_width: u32,
    window_height: u32,
) -> (f32, f32) {
    if window_width == 0 || window_height == 0 {
        return (0.0, 0.0);
    }
    let new_x =
        (current.0 + dx * VIRTUAL_CURSOR_SENSITIVITY).clamp(0.0, (window_width as f32) - 1.0);
    let new_y =
        (current.1 + dy * VIRTUAL_CURSOR_SENSITIVITY).clamp(0.0, (window_height as f32) - 1.0);
    (new_x, new_y)
}

/// Returns `true` when a left-click that also triggers a mouse grab should be
/// forwarded to the NES controller as a button press.
///
/// When the mouse was explicitly released by Escape (`was_released_by_escape`
/// is `true`), the click serves only to re-grab the cursor and must be
/// silently discarded so Zapper shots / Arkanoid button presses are not
/// accidentally triggered.  In all other cases (initial grab) the click is
/// also forwarded.
pub fn should_forward_grab_click(was_released_by_escape: bool) -> bool {
    !was_released_by_escape
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::console::Config;
    use crate::platform::app_context::AppContext;
    use crate::platform::config::Config as PlatformConfig;
    use crate::platform::emulator::Console;
    use crate::snes::input::SnesControllerType;

    fn make_console() -> Console {
        Console::new_nes(AppContext::new_with_config(Config::default()))
    }

    fn make_console_with_controller(
        port: u8,
        controller_type: crate::nes::input::ControllerType,
    ) -> Console {
        let console = make_console();
        let Console::Nes(ref nes) = console else {
            panic!("expected NES console");
        };
        nes.bus()
            .borrow_mut()
            .set_controller_type(port, controller_type);
        console
    }

    fn valid_snes_lorom_nop_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        let header = 0x7FC0;
        rom[header..header + 21].copy_from_slice(b"SNES TEST ROM        ");
        rom[header + 0x3C] = 0x00;
        rom[header + 0x3D] = 0x80;
        rom[header + 0xD5] = 0x20;
        rom[header + 0xD6] = 0x00;
        rom[header + 0xD7] = 0x07;
        rom[header + 0xD8] = 0x00;
        rom[header + 0xD9] = 0x00;
        rom[header + 0xDC] = 0x34;
        rom[header + 0xDD] = 0x12;
        rom[header + 0xDE] = 0xCB;
        rom[header + 0xDF] = 0xED;
        rom[0x0000] = 0xEA;
        rom
    }

    fn make_snes_console_with_mouse() -> Console {
        let mut config = PlatformConfig::default();
        config.snes.controller_port1 = SnesControllerType::Mouse;
        let app_context = AppContext::new_with_config(config);
        let mut console = Console::new_snes(app_context);
        console
            .load_rom(&valid_snes_lorom_nop_rom(), "mouse.sfc")
            .expect("load snes rom");
        console
    }

    fn make_snes_console_with_mouse_on_port2() -> Console {
        let mut config = PlatformConfig::default();
        config.snes.controller_port2 = SnesControllerType::Mouse;
        let app_context = AppContext::new_with_config(config);
        let mut console = Console::new_snes(app_context);
        console
            .load_rom(&valid_snes_lorom_nop_rom(), "mouse-port2.sfc")
            .expect("load snes rom");
        console
    }

    // ── Device detection ─────────────────────────────────────────────────

    #[test]
    fn no_mouse_controller_by_default() {
        let console = make_console();
        assert!(!has_any_mouse_controller(&console));
    }

    #[test]
    fn detects_zapper_as_mouse_controller() {
        let console = make_console_with_controller(1, crate::nes::input::ControllerType::Zapper);
        assert!(has_any_mouse_controller(&console));
    }

    #[test]
    fn detects_arkanoid_as_mouse_controller() {
        let console = make_console_with_controller(1, crate::nes::input::ControllerType::Arkanoid);
        assert!(has_any_mouse_controller(&console));
    }

    #[test]
    fn detects_snes_mouse_as_mouse_controller() {
        let console = make_console_with_controller(1, crate::nes::input::ControllerType::SnesMouse);
        assert!(has_any_mouse_controller(&console));
    }

    #[test]
    fn detects_snes_mouse_on_snes_console() {
        let console = make_snes_console_with_mouse();
        assert!(has_snes_mouse(&console));
    }

    #[test]
    fn detects_snes_mouse_on_snes_console_port2() {
        let console = make_snes_console_with_mouse_on_port2();
        assert!(has_snes_mouse(&console));
        assert_eq!(snes_mouse_ports(&console), [false, true]);
    }

    #[test]
    fn joypad_is_not_a_mouse_controller() {
        let console = make_console_with_controller(1, crate::nes::input::ControllerType::Joypad);
        assert!(!has_any_mouse_controller(&console));
    }

    #[test]
    fn has_zapper_detects_port1() {
        let console = make_console_with_controller(1, crate::nes::input::ControllerType::Zapper);
        assert!(has_zapper(&console));
    }

    #[test]
    fn has_zapper_detects_port2() {
        let console = make_console_with_controller(2, crate::nes::input::ControllerType::Zapper);
        assert!(has_zapper(&console));
    }

    #[test]
    fn has_zapper_false_for_arkanoid() {
        let console = make_console_with_controller(1, crate::nes::input::ControllerType::Arkanoid);
        assert!(!has_zapper(&console));
    }

    // ── Coordinate routing ───────────────────────────────────────────────

    #[test]
    fn zapper_motion_returns_nes_coordinates() {
        let mut console =
            make_console_with_controller(2, crate::nes::input::ControllerType::Zapper);
        let result = update_mouse_motion(&mut console, 160, 120, 320, 240);
        assert!(result.is_some());
        let (x, y) = result.unwrap();
        assert_eq!(x, 128); // 160/319 * 255 ≈ 128
        assert_eq!(y, 128); // 120/239 * 255 ≈ 128
    }

    #[test]
    fn arkanoid_motion_returns_none() {
        let mut console =
            make_console_with_controller(1, crate::nes::input::ControllerType::Arkanoid);
        let result = update_mouse_motion(&mut console, 160, 120, 320, 240);
        assert!(result.is_none());
    }

    #[test]
    fn snes_mouse_motion_returns_nes_coordinates() {
        let mut console =
            make_console_with_controller(1, crate::nes::input::ControllerType::SnesMouse);
        let result = update_mouse_motion(&mut console, 160, 120, 320, 240);
        assert!(result.is_some());
    }

    // ── Relative motion ──────────────────────────────────────────────────

    #[test]
    fn snes_mouse_relative_motion_applies_delta() {
        let mut console =
            make_console_with_controller(1, crate::nes::input::ControllerType::SnesMouse);
        apply_snes_mouse_relative_motion(&mut console, 10, 5, 320, 240);
        let Console::Nes(ref nes) = console else {
            panic!("expected NES console");
        };
        let state = nes.bus().borrow().capture_state();
        if let crate::nes::bus::ControllerStateWrapper::SnesAdapter(snes) = state.port1_controller {
            // Accumulator starts at 0, so after a (+10,+5) delta the positions
            // must both be non-zero.
            assert!(
                snes.mouse_x_position > 0,
                "Expected non-zero x after positive delta"
            );
            assert!(
                snes.mouse_y_position > 0,
                "Expected non-zero y after positive delta"
            );
        } else {
            panic!("Expected SnesAdapter state on port 1");
        }
    }

    // ── Mouse button routing ─────────────────────────────────────────────

    #[test]
    fn button_ignored_when_no_mouse_controller() {
        let mut console = make_console();
        // Should not panic
        update_mouse_button(&mut console, MouseButton::Left, true);
        update_mouse_button(&mut console, MouseButton::Right, true);
    }

    #[test]
    fn button_routes_left_to_zapper_trigger() {
        let mut console =
            make_console_with_controller(1, crate::nes::input::ControllerType::Zapper);

        update_mouse_button(&mut console, MouseButton::Left, true);
        let Console::Nes(ref nes) = console else {
            panic!("expected NES console");
        };
        let state = nes.bus().borrow().capture_state();
        if let crate::nes::bus::ControllerStateWrapper::Zapper(z) = state.port1_controller {
            assert!(z.trigger, "Expected trigger set after left-button press");
        } else {
            panic!("Expected Zapper state on port 1");
        }

        update_mouse_button(&mut console, MouseButton::Left, false);
        let Console::Nes(ref nes) = console else {
            unreachable!()
        };
        let state = nes.bus().borrow().capture_state();
        if let crate::nes::bus::ControllerStateWrapper::Zapper(z) = state.port1_controller {
            assert!(
                !z.trigger,
                "Expected trigger cleared after left-button release"
            );
        } else {
            panic!("Expected Zapper state on port 1");
        }
    }

    // ── Crosshair ────────────────────────────────────────────────────────

    #[test]
    fn crosshair_returns_none_without_zapper() {
        let console = make_console();
        assert!(zapper_crosshair(&console, Some((128, 128))).is_none());
    }

    #[test]
    fn crosshair_returns_none_when_no_position() {
        let console = make_console_with_controller(2, crate::nes::input::ControllerType::Zapper);
        assert!(zapper_crosshair(&console, None).is_none());
    }

    #[test]
    fn crosshair_returns_position_with_zapper() {
        let console = make_console_with_controller(2, crate::nes::input::ControllerType::Zapper);
        let ch = zapper_crosshair(&console, Some((100, 200)));
        assert!(ch.is_some());
        let ch = ch.unwrap();
        assert_eq!(ch.x, 100.0);
        assert_eq!(ch.y, 200.0);
    }

    // ── Virtual cursor accumulation ───────────────────────────────────────

    #[test]
    fn virtual_cursor_accumulates_delta_from_centre() {
        let (nx, ny) = accumulate_virtual_cursor((160.0, 120.0), 10.0, -5.0, 320, 240);
        assert_eq!(nx, 160.0 + 10.0 * VIRTUAL_CURSOR_SENSITIVITY);
        assert_eq!(ny, 120.0 + (-5.0) * VIRTUAL_CURSOR_SENSITIVITY);
    }

    #[test]
    fn virtual_cursor_clamps_to_window_bounds() {
        let (nx, ny) = accumulate_virtual_cursor((0.0, 0.0), -100.0, -100.0, 320, 240);
        assert_eq!(nx, 0.0);
        assert_eq!(ny, 0.0);

        let (nx, ny) = accumulate_virtual_cursor((319.0, 239.0), 100.0, 100.0, 320, 240);
        assert_eq!(nx, 319.0);
        assert_eq!(ny, 239.0);
    }

    #[test]
    fn virtual_cursor_degenerate_window() {
        let (nx, ny) = accumulate_virtual_cursor((0.0, 0.0), 10.0, 10.0, 0, 0);
        assert_eq!(nx, 0.0);
        assert_eq!(ny, 0.0);
    }

    // ── Grab-click forwarding ─────────────────────────────────────────────

    #[test]
    fn grab_click_is_forwarded_on_initial_grab() {
        // Given: mouse was NOT released by Escape (initial grab)
        // Then: click is forwarded to the NES controller
        assert!(
            should_forward_grab_click(false),
            "Initial grab click should be forwarded to the NES"
        );
    }

    #[test]
    fn grab_click_is_discarded_after_escape_release() {
        // Given: mouse was released by pressing Escape
        // When: user clicks to re-grab
        // Then: the click is NOT forwarded (it is silently discarded)
        assert!(
            !should_forward_grab_click(true),
            "Re-grab click after Escape should be silently discarded"
        );
    }
}
