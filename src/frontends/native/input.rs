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

pub(crate) fn egui_key_for(key: UiKey) -> egui::Key {
    match key {
        UiKey::Tab => egui::Key::Tab,
        UiKey::LeftArrow => egui::Key::ArrowLeft,
        UiKey::RightArrow => egui::Key::ArrowRight,
        UiKey::UpArrow => egui::Key::ArrowUp,
        UiKey::DownArrow => egui::Key::ArrowDown,
        UiKey::PageUp => egui::Key::PageUp,
        UiKey::PageDown => egui::Key::PageDown,
        UiKey::Home => egui::Key::Home,
        UiKey::End => egui::Key::End,
        UiKey::Insert => egui::Key::Insert,
        UiKey::Delete => egui::Key::Delete,
        UiKey::Backspace => egui::Key::Backspace,
        UiKey::Space => egui::Key::Space,
        UiKey::Enter => egui::Key::Enter,
        UiKey::Escape => egui::Key::Escape,
        UiKey::A => egui::Key::A,
        UiKey::C => egui::Key::C,
        UiKey::V => egui::Key::V,
        UiKey::X => egui::Key::X,
        UiKey::Y => egui::Key::Y,
        UiKey::Z => egui::Key::Z,
        UiKey::F1 => egui::Key::F1,
        UiKey::F5 => egui::Key::F5,
        UiKey::F10 => egui::Key::F10,
        UiKey::F11 => egui::Key::F11,
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

#[derive(Debug, Default)]
pub(crate) struct EguiInputState {
    events: Vec<egui::Event>,
    pointer_pos: Option<egui::Pos2>,
}

impl EguiInputState {
    pub(crate) fn apply_input(&mut self, event: &InputEvent) {
        match event {
            InputEvent::MouseMotion { x, y } => {
                let pos = egui::pos2(*x, *y);
                self.pointer_pos = Some(pos);
                self.events.push(egui::Event::PointerMoved(pos));
            }
            InputEvent::MouseButton { button, pressed } => {
                if let Some(pos) = self.pointer_pos {
                    self.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui_pointer_button_for(*button),
                        pressed: *pressed,
                        modifiers: egui::Modifiers::default(),
                    });
                }
            }
            InputEvent::MouseWheel { x, y } => {
                self.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(*x, *y),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::default(),
                });
            }
            InputEvent::TextInput(text) => {
                self.events.push(egui::Event::Text(text.clone()));
            }
            InputEvent::Key { key, down } => {
                let key = egui_key_for(*key);
                self.events.push(egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: *down,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                });
            }
        }
    }

    pub(crate) fn take_events(&mut self) -> Vec<egui::Event> {
        std::mem::take(&mut self.events)
    }
}

fn egui_pointer_button_for(button: MouseButton) -> egui::PointerButton {
    match button {
        MouseButton::Left => egui::PointerButton::Primary,
        MouseButton::Right => egui::PointerButton::Secondary,
        MouseButton::Middle => egui::PointerButton::Middle,
    }
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

    #[test]
    fn ui_key_escape_maps_to_egui_escape() {
        // Given the UI-neutral Escape key.
        let key = UiKey::Escape;

        // When bridging to egui.
        let egui_key = egui_key_for(key);

        // Then it maps to the matching egui key.
        assert_eq!(egui_key, egui::Key::Escape);
    }

    #[test]
    fn egui_input_state_records_text_and_key_events() {
        // Given fresh egui input adapter state.
        let mut state = EguiInputState::default();

        // When applying text and key events.
        state.apply_input(&InputEvent::TextInput("A".to_string()));
        state.apply_input(&InputEvent::Key {
            key: UiKey::Enter,
            down: true,
        });

        // Then matching egui events are queued for the next egui frame.
        assert_eq!(
            state.take_events(),
            vec![
                egui::Event::Text("A".to_string()),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: Some(egui::Key::Enter),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ]
        );
    }

    #[test]
    fn egui_input_state_records_pointer_button_at_last_position() {
        // Given fresh egui input adapter state.
        let mut state = EguiInputState::default();

        // When the pointer moves and then a mouse button is pressed.
        state.apply_input(&InputEvent::MouseMotion { x: 12.0, y: 34.0 });
        state.apply_input(&InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        });

        // Then egui receives both the move and button event at the last pointer position.
        assert_eq!(
            state.take_events(),
            vec![
                egui::Event::PointerMoved(egui::pos2(12.0, 34.0)),
                egui::Event::PointerButton {
                    pos: egui::pos2(12.0, 34.0),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ]
        );
    }
}
