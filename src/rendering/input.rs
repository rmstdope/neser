use imgui::Io;
use imgui::Key;

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
    /// Text input for ImGui.
    TextInput(String),
    /// Key press/release events routed to ImGui.
    Key { key: Key, down: bool },
}

/// Applies a single input event to the ImGui IO state.
pub fn apply_input(io: &mut Io, event: &InputEvent) {
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
            io.add_key_event(*key, *down);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_input_sets_mouse_position() {
        let mut imgui = imgui::Context::create();
        let io = imgui.io_mut();
        apply_input(io, &InputEvent::MouseMotion { x: 10.0, y: 20.0 });
        assert_eq!(io.mouse_pos, [10.0, 20.0]);
    }

    #[test]
    fn apply_input_sets_mouse_button_down() {
        let mut imgui = imgui::Context::create();
        let io = imgui.io_mut();
        apply_input(
            io,
            &InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: true,
            },
        );
        assert!(io.mouse_down[0]);
    }

    #[test]
    fn apply_input_updates_mouse_wheel() {
        let mut imgui = imgui::Context::create();
        let io = imgui.io_mut();
        apply_input(io, &InputEvent::MouseWheel { x: 1.0, y: -2.0 });
        assert_eq!(io.mouse_wheel_h, 1.0);
        assert_eq!(io.mouse_wheel, -2.0);
    }

    #[test]
    fn apply_input_sets_key_state() {
        let mut imgui = imgui::Context::create();
        let io = imgui.io_mut();
        apply_input(
            io,
            &InputEvent::Key {
                key: Key::Space,
                down: true,
            },
        );
        imgui.frame();
        assert!(imgui.io().keys_down[Key::Space as usize]);
    }
}
