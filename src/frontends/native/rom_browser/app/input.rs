use super::*;

impl RomBrowserApp {
    /// Poll gamepad events and return browser actions.
    pub(super) fn poll_gamepad(&mut self) -> Vec<BrowserAction> {
        let gilrs = match self.gilrs.as_mut() {
            Some(g) => g,
            None => return Vec::new(),
        };

        let mut actions = Vec::new();
        let now = Instant::now();

        while let Some(event) = gilrs.next_event() {
            match event.event {
                EventType::ButtonPressed(button, _) => {
                    // Track held state for directional buttons.
                    if let Some(dir) = Self::button_to_direction(button) {
                        self.gamepad_repeat.held.insert(dir, now);
                        self.gamepad_repeat.last_repeat.remove(&dir);
                    }
                    if let Some(action) = Self::map_button(button) {
                        actions.push(action);
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if let Some(dir) = Self::button_to_direction(button) {
                        self.gamepad_repeat.held.remove(&dir);
                        self.gamepad_repeat.last_repeat.remove(&dir);
                    }
                }
                EventType::AxisChanged(axis, value, _) => {
                    let axis_actions = Self::update_axis(
                        &mut self.gamepad_axis,
                        axis,
                        value,
                        &mut self.gamepad_repeat,
                        now,
                    );
                    actions.extend(axis_actions);
                }
                _ => {}
            }
        }

        // Generate repeat actions for held directional inputs.
        let held_snapshot: Vec<(RepeatDirection, Instant)> = self
            .gamepad_repeat
            .held
            .iter()
            .map(|(&d, &t)| (d, t))
            .collect();
        for (dir, press_time) in held_snapshot {
            let elapsed = now.duration_since(press_time).as_millis();
            if elapsed < REPEAT_DELAY_MS {
                continue;
            }
            let should_fire = match self.gamepad_repeat.last_repeat.get(&dir) {
                Some(&last) => now.duration_since(last).as_millis() >= REPEAT_INTERVAL_MS,
                None => true,
            };
            if should_fire {
                actions.push(Self::direction_to_action(dir));
                self.gamepad_repeat.last_repeat.insert(dir, now);
            }
        }

        actions
    }

    /// Map a D-pad button to a logical repeat direction.
    fn button_to_direction(button: gilrs::Button) -> Option<RepeatDirection> {
        match button {
            gilrs::Button::DPadUp => Some(RepeatDirection::Up),
            gilrs::Button::DPadDown => Some(RepeatDirection::Down),
            gilrs::Button::DPadLeft => Some(RepeatDirection::Left),
            gilrs::Button::DPadRight => Some(RepeatDirection::Right),
            _ => None,
        }
    }

    /// Convert a repeat direction back to a browser action.
    fn direction_to_action(dir: RepeatDirection) -> BrowserAction {
        match dir {
            RepeatDirection::Up => BrowserAction::Up,
            RepeatDirection::Down => BrowserAction::Down,
            RepeatDirection::Left => BrowserAction::Left,
            RepeatDirection::Right => BrowserAction::Right,
        }
    }

    /// Map a gilrs button to a browser action.
    fn map_button(button: gilrs::Button) -> Option<BrowserAction> {
        match button {
            gilrs::Button::DPadUp => Some(BrowserAction::Up),
            gilrs::Button::DPadDown => Some(BrowserAction::Down),
            gilrs::Button::DPadLeft => Some(BrowserAction::Left),
            gilrs::Button::DPadRight => Some(BrowserAction::Right),
            gilrs::Button::East => Some(BrowserAction::Confirm), // Nintendo A button
            gilrs::Button::South => Some(BrowserAction::Back),   // Nintendo B button
            gilrs::Button::Start | gilrs::Button::RightTrigger2 => Some(BrowserAction::Search),
            gilrs::Button::North => Some(BrowserAction::Detail), // Nintendo X button
            gilrs::Button::West => Some(BrowserAction::GenreFilter), // Nintendo Y button
            gilrs::Button::Select | gilrs::Button::LeftTrigger2 => Some(BrowserAction::Favorite),
            _ => None,
        }
    }

    /// Update axis state, return actions for new presses, and track repeat state.
    fn update_axis(
        state: &mut GamepadAxisState,
        axis: Axis,
        value: f32,
        repeat: &mut GamepadRepeatState,
        now: Instant,
    ) -> Vec<BrowserAction> {
        let mut actions = Vec::new();
        match axis {
            Axis::LeftStickX | Axis::RightStickX => {
                let new_left = value < -AXIS_DEAD_ZONE;
                let new_right = value > AXIS_DEAD_ZONE;
                if new_left && !state.left {
                    actions.push(BrowserAction::Left);
                    repeat.held.insert(RepeatDirection::Left, now);
                    repeat.last_repeat.remove(&RepeatDirection::Left);
                }
                if !new_left && state.left {
                    repeat.held.remove(&RepeatDirection::Left);
                    repeat.last_repeat.remove(&RepeatDirection::Left);
                }
                if new_right && !state.right {
                    actions.push(BrowserAction::Right);
                    repeat.held.insert(RepeatDirection::Right, now);
                    repeat.last_repeat.remove(&RepeatDirection::Right);
                }
                if !new_right && state.right {
                    repeat.held.remove(&RepeatDirection::Right);
                    repeat.last_repeat.remove(&RepeatDirection::Right);
                }
                state.left = new_left;
                state.right = new_right;
            }
            Axis::LeftStickY | Axis::RightStickY => {
                // gilrs on macOS: positive = up (Cartesian convention)
                let new_up = value > AXIS_DEAD_ZONE;
                let new_down = value < -AXIS_DEAD_ZONE;
                if new_up && !state.up {
                    actions.push(BrowserAction::Up);
                    repeat.held.insert(RepeatDirection::Up, now);
                    repeat.last_repeat.remove(&RepeatDirection::Up);
                }
                if !new_up && state.up {
                    repeat.held.remove(&RepeatDirection::Up);
                    repeat.last_repeat.remove(&RepeatDirection::Up);
                }
                if new_down && !state.down {
                    actions.push(BrowserAction::Down);
                    repeat.held.insert(RepeatDirection::Down, now);
                    repeat.last_repeat.remove(&RepeatDirection::Down);
                }
                if !new_down && state.down {
                    repeat.held.remove(&RepeatDirection::Down);
                    repeat.last_repeat.remove(&RepeatDirection::Down);
                }
                state.up = new_up;
                state.down = new_down;
            }
            _ => {}
        }
        actions
    }
}
