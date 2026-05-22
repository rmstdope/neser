use imgui::Io;
use imgui::Key;

/// Keyboard keys relevant to the native UI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiKey {
    Tab,
    LeftArrow,
    RightArrow,
    UpArrow,
    DownArrow,
    PageUp,
    PageDown,
    Home,
    End,
    Insert,
    Delete,
    Backspace,
    Space,
    Enter,
    Escape,
    A,
    C,
    V,
    X,
    Y,
    Z,
    F1,
    F5,
    F10,
    F11,
}

pub(crate) fn imgui_key_for(key: UiKey) -> Key {
    match key {
        UiKey::Tab => Key::Tab,
        UiKey::LeftArrow => Key::LeftArrow,
        UiKey::RightArrow => Key::RightArrow,
        UiKey::UpArrow => Key::UpArrow,
        UiKey::DownArrow => Key::DownArrow,
        UiKey::PageUp => Key::PageUp,
        UiKey::PageDown => Key::PageDown,
        UiKey::Home => Key::Home,
        UiKey::End => Key::End,
        UiKey::Insert => Key::Insert,
        UiKey::Delete => Key::Delete,
        UiKey::Backspace => Key::Backspace,
        UiKey::Space => Key::Space,
        UiKey::Enter => Key::Enter,
        UiKey::Escape => Key::Escape,
        UiKey::A => Key::A,
        UiKey::C => Key::C,
        UiKey::V => Key::V,
        UiKey::X => Key::X,
        UiKey::Y => Key::Y,
        UiKey::Z => Key::Z,
        UiKey::F1 => Key::F1,
        UiKey::F5 => Key::F5,
        UiKey::F10 => Key::F10,
        UiKey::F11 => Key::F11,
    }
}

/// Mouse buttons relevant to the renderer input layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Input events forwarded to the renderer (backend-agnostic).
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// Mouse movement in window coordinates.
    MouseMotion { x: f32, y: f32 },
    /// Mouse button press/release.
    MouseButton { button: MouseButton, pressed: bool },
    /// Mouse wheel scroll delta.
    MouseWheel { x: f32, y: f32 },
    /// Text input for the UI layer.
    TextInput(String),
    /// Key press/release events routed to the UI layer.
    Key { key: UiKey, down: bool },
}

/// Applies a single input event to the current ImGui adapter state.
pub fn apply_imgui_input(io: &mut Io, event: &InputEvent) {
    match event {
        InputEvent::MouseMotion { x, y } => {
            io.mouse_pos = [*x, *y];
        }

        InputEvent::MouseButton { button, pressed } => {
            let index = match button {
                MouseButton::Left => 0,
                MouseButton::Right => 1,
                MouseButton::Middle => 2,
            };
            io.mouse_down[index] = *pressed;
        }
        InputEvent::MouseWheel { x, y } => {
            io.mouse_wheel_h += *x;
            io.mouse_wheel += *y;
        }
        InputEvent::TextInput(text) => {
            for ch in text.chars() {
                io.add_input_character(ch);
            }
        }

        InputEvent::Key { key, down } => {
            io.add_key_event(imgui_key_for(*key), *down);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn ui_key_space_maps_to_imgui_space() {
        // Given the UI-neutral Space key.
        let key = UiKey::Space;

        // When bridging to the current ImGui backend.
        let imgui_key = imgui_key_for(key);

        // Then it maps to the matching backend key.
        assert_eq!(imgui_key, Key::Space);
    }

    #[serial]
    #[test]
    fn apply_imgui_input_sets_mouse_position() {
        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);
        let io = imgui.io_mut();
        apply_imgui_input(io, &InputEvent::MouseMotion { x: 10.0, y: 20.0 });
        assert_eq!(io.mouse_pos, [10.0, 20.0]);
    }

    #[serial]
    #[test]
    fn apply_imgui_input_sets_mouse_button_down() {
        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);
        let io = imgui.io_mut();
        apply_imgui_input(
            io,
            &InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: true,
            },
        );
        assert!(io.mouse_down[0]);
    }

    #[serial]
    #[test]
    fn apply_imgui_input_updates_mouse_wheel() {
        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);
        let io = imgui.io_mut();
        apply_imgui_input(io, &InputEvent::MouseWheel { x: 1.0, y: -2.0 });
        assert_eq!(io.mouse_wheel_h, 1.0);
        assert_eq!(io.mouse_wheel, -2.0);
    }

    #[serial]
    #[test]
    fn apply_imgui_input_sets_key_state() {
        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);
        let io = imgui.io_mut();
        apply_imgui_input(
            io,
            &InputEvent::Key {
                key: UiKey::Space,
                down: true,
            },
        );
        let io = imgui.io_mut();
        io.display_size = [1.0, 1.0];
        io.delta_time = 1.0 / 60.0;
        let _ = imgui.fonts().build_rgba32_texture();
        imgui.frame();
        assert!(imgui.io().keys_down[Key::Space as usize]);
    }
}
