//! Mouse input handling for the native winit frontend.
//!
//! Routes host mouse events to NES mouse-emulated controllers
//! (Zapper, Arkanoid paddle, SNES Mouse) and manages the cursor
//! grab/release state machine.

use crate::frontends::native::gl_backend::{Crosshair, CrosshairStyle};
use crate::nes::input::mouse_mapping;
use crate::platform::emulator::{Console, MouseInputButton};

/// Mouse button abstraction (frontend-independent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

// ── Device detection ─────────────────────────────────────────────────────────

/// Returns `true` when the current console exposes any mouse-driven controller.
///
/// This includes NES Zapper / paddle input and SNES mouse input, and returns
/// `false` for consoles without mouse-capable controllers.
pub fn has_any_mouse_controller(console: &Console) -> bool {
    console
        .as_mouse_input()
        .is_some_and(|mouse| mouse.has_any_mouse_controller())
}

/// Returns `true` when a Zapper is connected on any port or expansion.
pub fn has_zapper(console: &Console) -> bool {
    console
        .as_mouse_input()
        .is_some_and(|mouse| mouse.has_zapper())
}

pub fn has_snes_mouse(console: &Console) -> bool {
    console
        .as_mouse_input()
        .is_some_and(|mouse| mouse.has_snes_mouse())
}

/// Returns `true` when a Super Scope is plugged into either SNES port.
pub fn has_super_scope(console: &Console) -> bool {
    console
        .as_mouse_input()
        .is_some_and(|mouse| mouse.has_super_scope())
}

fn snes_mouse_ports(console: &Console) -> [bool; 2] {
    let Some(mouse) = console.as_mouse_input() else {
        return [false, false];
    };
    [
        mouse.has_snes_mouse_on_port(0),
        mouse.has_snes_mouse_on_port(1),
    ]
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
    let picture_height = console.screen_height();
    let mouse = console.as_mouse_input_mut()?;
    if mouse.has_super_scope() {
        // The scope aims at a picture pixel: the whole window spans the 256-pixel-wide
        // picture and its visible lines, like the NES light gun's mapping.
        let x_pos = mouse_mapping::map_mouse_axis_to_range(x, window_width, 256);
        let y_pos = mouse_mapping::map_mouse_axis_to_range(y, window_height, picture_height);
        mouse.set_mouse_position(x_pos, y_pos);
        Some((x_pos, y_pos))
    } else if mouse.has_zapper() || mouse.has_snes_mouse() {
        let x_pos = mouse_mapping::map_mouse_axis_to_zapper_position(x, window_width);
        let y_pos = mouse_mapping::map_mouse_axis_to_zapper_position(y, window_height);
        mouse.set_mouse_position(x_pos, y_pos);
        Some((x_pos, y_pos))
    } else {
        let position = mouse_mapping::map_mouse_x_to_paddle_position(x, window_width);
        mouse.set_paddle_position(position);
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
    let Some(mouse) = console.as_mouse_input_mut() else {
        return;
    };
    let dx = mouse_mapping::map_relative_mouse_delta_to_axis_delta(xrel, window_width);
    let dy = mouse_mapping::map_relative_mouse_delta_to_axis_delta(yrel, window_height);
    if snes_ports.into_iter().any(|enabled| enabled) || mouse.has_snes_mouse() {
        mouse.add_mouse_delta(dx, dy);
    }
}

/// Forwards a mouse button press/release to the appropriate NES controller.
///
/// No-op for consoles without a mouse-driven controller.
pub fn update_mouse_button(console: &mut Console, button: MouseButton, pressed: bool) {
    let Some(mouse) = console.as_mouse_input_mut() else {
        return;
    };
    if !mouse.has_any_mouse_controller() && !mouse.has_snes_mouse() && !mouse.has_super_scope() {
        return;
    }
    let capability_button = match button {
        MouseButton::Left => MouseInputButton::Left,
        MouseButton::Right => MouseInputButton::Right,
    };
    mouse.set_mouse_button(capability_button, pressed);
}

/// Returns a [`Crosshair`] for the Zapper if one is connected and a position
/// has been recorded.
///
/// Always returns `None` for non-NES consoles.
pub fn zapper_crosshair(console: &Console, last_position: Option<(u8, u8)>) -> Option<Crosshair> {
    let mouse = console.as_mouse_input()?;
    if !mouse.has_zapper() {
        None
    } else {
        last_position.map(|(x, y)| Crosshair {
            x: x as f32,
            y: y as f32,
            style: CrosshairStyle::Plus,
        })
    }
}

/// The Super Scope's ring sight at the last aimed picture pixel, drawn only while the
/// mouse is captured (`grabbed`). `None` without a scope or before the first aim.
pub fn super_scope_sight(
    console: &Console,
    grabbed: bool,
    last_position: Option<(u8, u8)>,
) -> Option<Crosshair> {
    if !grabbed || !has_super_scope(console) {
        return None;
    }
    last_position.map(|(x, y)| Crosshair {
        x: f32::from(x),
        y: f32::from(y),
        style: CrosshairStyle::Ring,
    })
}

// ── Capture policy ───────────────────────────────────────────────────────────

/// Whether the mouse should be captured this frame.
///
/// NES-style devices (`has_mouse_device`: Zapper, Arkanoid, SNES mouse) are captured
/// automatically while the window is focused, unless Escape released them. A Super Scope
/// is captured only by a click (see [`click_captures`]), so it merely keeps a capture it
/// already has while the window stays focused.
pub fn desired_mouse_grab(
    has_mouse_device: bool,
    has_super_scope: bool,
    grabbed: bool,
    window_focused: bool,
    released_by_escape: bool,
) -> bool {
    if has_mouse_device {
        mouse_mapping::should_grab_mouse_input(true, window_focused, released_by_escape)
    } else if has_super_scope {
        grabbed && window_focused
    } else {
        false
    }
}

/// Whether a press of `button` on the game captures the mouse: either button for the
/// Super Scope (fire or cursor), the left one for the NES-style devices.
pub fn click_captures(has_mouse_device: bool, has_super_scope: bool, button: MouseButton) -> bool {
    has_super_scope || (has_mouse_device && button == MouseButton::Left)
}

/// The message shown when the mouse stops aiming a Super Scope: only when a scope is
/// connected and the mouse had actually been captured.
pub fn super_scope_release_toast(has_super_scope: bool, was_grabbed: bool) -> Option<&'static str> {
    (has_super_scope && was_grabbed)
        .then_some(crate::snes::frontend_toasts::SUPER_SCOPE_MOUSE_RELEASED)
}

// ── NES-specific internal helpers ─────────────────────────────────────────────

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
    use crate::platform::emulator::Console;
    use crate::platform::save_state::Stateful;
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
        rom[header + 0x15] = 0x20;
        rom[header + 0x16] = 0x00;
        rom[header + 0x17] = 0x07;
        rom[header + 0x18] = 0x00;
        rom[header + 0x19] = 0x00;
        rom[header + 0x1C] = 0x34;
        rom[header + 0x1D] = 0x12;
        rom[header + 0x1E] = 0xCB;
        rom[header + 0x1F] = 0xED;
        rom[0x0000] = 0xEA;
        rom
    }

    fn make_snes_console_with_mouse() -> Console {
        let mut config = crate::snes::test_support::snes_test_config();
        config.snes.controller_port1 = SnesControllerType::Mouse;
        let app_context = AppContext::new_with_config(config);
        let mut console = Console::new_snes(app_context);
        console
            .load_rom(&valid_snes_lorom_nop_rom(), "mouse.sfc")
            .expect("load snes rom");
        console
    }

    fn make_snes_console_with_mouse_on_port2() -> Console {
        let mut config = crate::snes::test_support::snes_test_config();
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

    // ── Super Scope ──────────────────────────────────────────────────────

    fn make_snes_console_with_scope() -> Console {
        let mut console = Console::new_snes(crate::snes::test_support::snes_test_app_context());
        console
            .load_rom(
                &crate::snes::test_support::minimal_lorom(b"METAL COMBAT"),
                "metal-combat.sfc",
            )
            .expect("load snes rom");
        console
    }

    fn scope_state(console: &Console) -> crate::snes::input::SnesControllerState {
        console
            .as_snes()
            .and_then(|snes| snes.superscope_state(1))
            .expect("scope on port 2")
    }

    #[test]
    fn detects_super_scope_without_calling_it_a_mouse() {
        let console = make_snes_console_with_scope();
        assert!(has_super_scope(&console));
        assert!(
            !has_any_mouse_controller(&console),
            "the scope must not trigger the NES-style automatic grab"
        );
        assert!(!has_super_scope(&make_console()));
    }

    #[test]
    fn motion_aims_the_super_scope_across_the_picture() {
        let mut console = make_snes_console_with_scope();
        let aim = update_mouse_motion(&mut console, 160, 120, 320, 240);
        // x: 160/319 * 255 = 128; y: 120/239 * 223 = 112 (the picture is 224 lines).
        assert_eq!(aim, Some((128, 112)));
        let state = scope_state(&console);
        assert_eq!((state.superscope_x, state.superscope_y), (128, 112));

        assert_eq!(
            update_mouse_motion(&mut console, 319, 239, 320, 240),
            Some((255, 223)),
            "the bottom-right corner is the last picture pixel"
        );
    }

    #[test]
    fn left_fires_and_right_is_the_cursor_button() {
        let mut console = make_snes_console_with_scope();
        update_mouse_button(&mut console, MouseButton::Left, true);
        assert!(scope_state(&console).superscope_trigger);
        update_mouse_button(&mut console, MouseButton::Left, false);
        assert!(!scope_state(&console).superscope_trigger);

        update_mouse_button(&mut console, MouseButton::Right, true);
        assert!(scope_state(&console).superscope_cursor);
        update_mouse_button(&mut console, MouseButton::Right, false);
        assert!(!scope_state(&console).superscope_cursor);
    }

    #[test]
    fn the_sight_is_a_ring_shown_only_while_the_mouse_is_captured() {
        let console = make_snes_console_with_scope();
        assert_eq!(super_scope_sight(&console, false, Some((10, 20))), None);
        assert_eq!(super_scope_sight(&console, true, None), None);
        assert_eq!(
            super_scope_sight(&console, true, Some((10, 20))),
            Some(Crosshair {
                x: 10.0,
                y: 20.0,
                style: CrosshairStyle::Ring,
            })
        );
        assert_eq!(super_scope_sight(&make_console(), true, Some((1, 2))), None);
    }

    #[test]
    fn a_scope_is_captured_only_by_a_click_and_kept_until_released() {
        // Never grabbed automatically, even when focused and never released.
        assert!(!desired_mouse_grab(false, true, false, true, false));
        // Kept while grabbed and focused.
        assert!(desired_mouse_grab(false, true, true, true, false));
        // Dropped when focus goes.
        assert!(!desired_mouse_grab(false, true, true, false, false));
        // NES-style devices keep their automatic grab.
        assert!(desired_mouse_grab(true, false, false, true, false));
        assert!(!desired_mouse_grab(true, false, true, true, true));
        // Nothing to grab for.
        assert!(!desired_mouse_grab(false, false, true, true, false));
    }

    #[test]
    fn either_button_captures_for_the_scope_but_only_left_for_other_devices() {
        assert!(click_captures(false, true, MouseButton::Left));
        assert!(click_captures(false, true, MouseButton::Right));
        assert!(click_captures(true, false, MouseButton::Left));
        assert!(!click_captures(true, false, MouseButton::Right));
        assert!(!click_captures(false, false, MouseButton::Left));
    }

    #[test]
    fn the_release_message_is_for_a_captured_scope_only() {
        assert_eq!(
            super_scope_release_toast(true, true),
            Some("Mouse released — click the game to aim again")
        );
        assert_eq!(super_scope_release_toast(true, false), None);
        assert_eq!(super_scope_release_toast(false, true), None);
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
