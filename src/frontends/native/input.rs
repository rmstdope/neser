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

/// Modifier keys relevant to UI input.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UiModifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub mac_cmd: bool,
    pub command: bool,
}

impl UiModifiers {
    fn egui(self) -> egui::Modifiers {
        egui::Modifiers {
            alt: self.alt,
            ctrl: self.ctrl,
            shift: self.shift,
            mac_cmd: self.mac_cmd,
            command: self.command,
        }
    }
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
    /// Current keyboard modifier state.
    ModifiersChanged(UiModifiers),
}

#[derive(Debug, Default)]
pub(crate) struct EguiInputState {
    events: Vec<egui::Event>,
    pointer_pos: Option<egui::Pos2>,
    modifiers: egui::Modifiers,
}

impl EguiInputState {
    pub(crate) fn apply_input(&mut self, event: &InputEvent) {
        match event {
            InputEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.egui();
            }
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
                        modifiers: self.modifiers,
                    });
                }
            }
            InputEvent::MouseWheel { x, y } => {
                self.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(*x, *y),
                    phase: egui::TouchPhase::Move,
                    modifiers: self.modifiers,
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
                    modifiers: self.modifiers,
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn egui_input_state_applies_current_modifiers_to_key_events() {
        // Given current Ctrl+Shift modifier state.
        let mut state = EguiInputState::default();
        let modifiers = UiModifiers {
            ctrl: true,
            shift: true,
            command: true,
            ..Default::default()
        };

        // When applying a key event after the modifier change.
        state.apply_input(&InputEvent::ModifiersChanged(modifiers));
        state.apply_input(&InputEvent::Key {
            key: UiKey::C,
            down: true,
        });

        // Then egui receives the key with the current modifiers.
        assert_eq!(
            state.take_events(),
            vec![egui::Event::Key {
                key: egui::Key::C,
                physical_key: Some(egui::Key::C),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    command: true,
                    ..Default::default()
                },
            }]
        );
    }

    #[test]
    fn egui_input_state_applies_current_modifiers_to_pointer_and_wheel_events() {
        // Given current Alt modifier state and a known pointer position.
        let mut state = EguiInputState::default();
        state.apply_input(&InputEvent::MouseMotion { x: 12.0, y: 34.0 });
        state.apply_input(&InputEvent::ModifiersChanged(UiModifiers {
            alt: true,
            ..Default::default()
        }));

        // When applying pointer and wheel events after the modifier change.
        state.apply_input(&InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        });
        state.apply_input(&InputEvent::MouseWheel { x: 1.0, y: -2.0 });

        // Then both egui events carry the current modifier state.
        assert_eq!(
            state.take_events(),
            vec![
                egui::Event::PointerMoved(egui::pos2(12.0, 34.0)),
                egui::Event::PointerButton {
                    pos: egui::pos2(12.0, 34.0),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                },
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(1.0, -2.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                },
            ]
        );
    }
}
