//! Gamepad input handling for the native frontend via gilrs.
//!
//! [`GamepadManager`] wraps the gilrs event loop, maps physical gamepad
//! buttons and analog axes to NES/SNES controller inputs, and manages
//! hot-plug player assignment.

use crate::console::Nes;
use crate::input::{Button, ControllerInput, SnesButton};

use gilrs::{Axis, EventType, GamepadId, Gilrs, GilrsBuilder};

use std::collections::HashMap;

/// Threshold for converting analog stick axes to digital D-pad presses.
/// Values beyond ±AXIS_DEAD_ZONE trigger the corresponding direction.
const AXIS_DEAD_ZONE: f32 = 0.5;

/// Threshold for converting analog button values to digital presses.
/// Some gamepads (especially generic USB ones) report face buttons as
/// analog values via `ButtonChanged` rather than `ButtonPressed`/`Released`.
const BUTTON_PRESS_THRESHOLD: f32 = 0.5;

/// Maximum number of controllers supported in normal mode.
const MAX_CONTROLLERS: usize = 2;

/// Maximum number of controllers supported in Four Score mode.
const MAX_CONTROLLERS_FOUR_SCORE: usize = 4;

/// Converts an analog button value to a digital pressed/released state.
///
/// Returns `true` (pressed) when `value > BUTTON_PRESS_THRESHOLD`.
pub fn is_button_pressed(value: f32) -> bool {
    value > BUTTON_PRESS_THRESHOLD
}

/// Maps a gilrs button to a standard NES joypad button.
///
/// Returns `None` for buttons that have no NES equivalent.
pub fn map_button_to_nes(button: gilrs::Button) -> Option<Button> {
    match button {
        gilrs::Button::South => Some(Button::A),
        gilrs::Button::East => Some(Button::B),
        gilrs::Button::West => Some(Button::A),
        gilrs::Button::North => Some(Button::B),
        gilrs::Button::Start => Some(Button::Start),
        gilrs::Button::Select => Some(Button::Select),
        gilrs::Button::DPadUp => Some(Button::Up),
        gilrs::Button::DPadDown => Some(Button::Down),
        gilrs::Button::DPadLeft => Some(Button::Left),
        gilrs::Button::DPadRight => Some(Button::Right),
        _ => None,
    }
}

/// Maps a gilrs button to an SNES adapter button.
///
/// The mapping follows the physical position convention:
/// - South (bottom face) → SNES B
/// - East (right face) → SNES A
/// - West (left face) → SNES Y
/// - North (top face) → SNES X
/// - Shoulder buttons → L / R
///
/// Returns `None` for buttons that have no SNES equivalent.
pub fn map_button_to_snes(button: gilrs::Button) -> Option<SnesButton> {
    match button {
        gilrs::Button::South => Some(SnesButton::B),
        gilrs::Button::East => Some(SnesButton::A),
        gilrs::Button::West => Some(SnesButton::Y),
        gilrs::Button::North => Some(SnesButton::X),
        gilrs::Button::Start => Some(SnesButton::Start),
        gilrs::Button::Select => Some(SnesButton::Select),
        gilrs::Button::LeftTrigger => Some(SnesButton::L),
        gilrs::Button::RightTrigger => Some(SnesButton::R),
        gilrs::Button::LeftTrigger2 => Some(SnesButton::L),
        gilrs::Button::RightTrigger2 => Some(SnesButton::R),
        gilrs::Button::DPadUp => Some(SnesButton::Up),
        gilrs::Button::DPadDown => Some(SnesButton::Down),
        gilrs::Button::DPadLeft => Some(SnesButton::Left),
        gilrs::Button::DPadRight => Some(SnesButton::Right),
        _ => None,
    }
}

/// Tracks the digital state derived from a single analog axis pair.
///
/// Converts continuous axis values into discrete D-pad presses,
/// emitting state transitions (press/release) when the value crosses
/// the dead zone threshold.
#[derive(Debug, Default)]
pub struct AxisState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl AxisState {
    /// Updates the horizontal axis (left stick X) and returns which
    /// buttons changed state as `(button, pressed)` pairs.
    pub fn update_x(&mut self, value: f32) -> Vec<(Button, bool)> {
        let mut changes = Vec::new();
        let new_left = value < -AXIS_DEAD_ZONE;
        let new_right = value > AXIS_DEAD_ZONE;

        if new_left != self.left {
            changes.push((Button::Left, new_left));
            self.left = new_left;
        }
        if new_right != self.right {
            changes.push((Button::Right, new_right));
            self.right = new_right;
        }
        changes
    }

    /// Updates the vertical axis (left stick Y) and returns which
    /// buttons changed state as `(button, pressed)` pairs.
    pub fn update_y(&mut self, value: f32) -> Vec<(Button, bool)> {
        let mut changes = Vec::new();
        let new_up = value < -AXIS_DEAD_ZONE;
        let new_down = value > AXIS_DEAD_ZONE;

        if new_up != self.up {
            changes.push((Button::Up, new_up));
            self.up = new_up;
        }
        if new_down != self.down {
            changes.push((Button::Down, new_down));
            self.down = new_down;
        }
        changes
    }
}

/// Per-gamepad tracking state.
#[derive(Default)]
struct GamepadState {
    axis: AxisState,
}

/// Manages gamepad input via gilrs for the native frontend.
///
/// Handles initialization, event polling, button/axis mapping,
/// and hot-plug player assignment.
pub struct GamepadManager {
    gilrs: Gilrs,
    /// Maps gamepad ID to player number (1-based).
    player_map: HashMap<GamepadId, u8>,
    /// Per-gamepad axis tracking state.
    gamepad_states: HashMap<GamepadId, GamepadState>,
    /// Maximum number of controllers to support.
    max_controllers: usize,
}

impl GamepadManager {
    /// Creates a new `GamepadManager`, initializing gilrs and enumerating
    /// any already-connected gamepads.
    ///
    /// `four_score` — when true, supports up to 4 gamepads instead of 2.
    pub fn new(four_score: bool) -> Result<Self, String> {
        let mut builder = GilrsBuilder::new()
            .with_default_filters(true)
            .add_env_mappings(true);

        // Load additional controller mappings from gamecontrollerdb.txt.
        // This is the same SDL-format mapping database the SDL frontend uses,
        // providing coverage for generic "USB gamepad" controllers and others
        // that aren't in gilrs's built-in database.
        if let Ok(mappings) = std::fs::read_to_string("gamecontrollerdb.txt") {
            builder = builder.add_mappings(&mappings);
        }

        let gilrs = builder
            .build()
            .map_err(|e| format!("failed to initialize gilrs: {e}"))?;

        let max_controllers = if four_score {
            MAX_CONTROLLERS_FOUR_SCORE
        } else {
            MAX_CONTROLLERS
        };

        let mut manager = Self {
            gilrs,
            player_map: HashMap::new(),
            gamepad_states: HashMap::new(),
            max_controllers,
        };

        // Two-phase detection: first scan the gamepads iterator (synchronous
        // on Linux, may return empty on macOS where IOKit enumerates async),
        // then drain any pending Connected events from the event queue.
        manager.enumerate_connected();
        manager.drain_pending_connections();
        Ok(manager)
    }

    /// Returns the number of currently connected gamepads.
    pub fn connected_count(&self) -> usize {
        self.player_map.len()
    }

    /// Polls all pending gilrs events and applies them to `nes`.
    pub fn process_events(&mut self, nes: &mut Nes) {
        while let Some(event) = self.gilrs.next_event() {
            match event.event {
                EventType::ButtonPressed(button, code) => {
                    crate::debugging::log_info(format!(
                        "Gamepad btn press: {:?} (code {:?}) from {:?}",
                        button, code, event.id
                    ));
                    self.handle_button(nes, event.id, button, true);
                }
                EventType::ButtonReleased(button, code) => {
                    self.handle_button(nes, event.id, button, false);
                }
                EventType::ButtonChanged(button, value, code) => {
                    crate::debugging::log_info(format!(
                        "Gamepad btn changed: {:?} val={value} (code {:?}) from {:?}",
                        button, code, event.id
                    ));
                    self.handle_button(nes, event.id, button, is_button_pressed(value));
                }
                EventType::AxisChanged(axis, value, code) => {
                    if value.abs() > AXIS_DEAD_ZONE {
                        crate::debugging::log_info(format!(
                            "Gamepad axis: {:?} val={value} (code {:?}) from {:?}",
                            axis, code, event.id
                        ));
                    }
                    self.handle_axis(nes, event.id, axis, value);
                }
                EventType::Connected => {
                    self.handle_connected(event.id);
                }
                EventType::Disconnected => {
                    self.handle_disconnected(event.id);
                }
                _ => {}
            }
        }
    }

    /// Enumerates already-connected gamepads at startup.
    fn enumerate_connected(&mut self) {
        let connected_ids: Vec<GamepadId> = self
            .gilrs
            .gamepads()
            .filter(|(_, gp)| gp.is_connected())
            .map(|(id, _)| id)
            .collect();

        for id in connected_ids {
            self.handle_connected(id);
        }
    }

    /// Drains any pending `Connected` events from the gilrs event queue.
    ///
    /// On macOS, IOKit enumerates gamepads asynchronously so `gamepads()` may
    /// return empty right after `build()`. The `Connected` events arrive in
    /// the event queue shortly after. This method consumes them so that
    /// `connected_count()` reflects reality before the toast is shown.
    fn drain_pending_connections(&mut self) {
        while let Some(event) = self.gilrs.next_event() {
            if let EventType::Connected = event.event {
                self.handle_connected(event.id);
            }
        }
    }

    /// Handles a gamepad connection event.
    fn handle_connected(&mut self, id: GamepadId) {
        if self.player_map.len() >= self.max_controllers {
            return;
        }
        if self.player_map.contains_key(&id) {
            return;
        }

        let player_num = self.next_available_player();
        self.player_map.insert(id, player_num);
        self.gamepad_states.insert(id, GamepadState::default());

        if let Some(gamepad) = self.gilrs.connected_gamepad(id) {
            crate::debugging::log_info(format!(
                "Gamepad connected for player {player_num}: {}",
                gamepad.name()
            ));
        }
    }

    /// Handles a gamepad disconnection event.
    fn handle_disconnected(&mut self, id: GamepadId) {
        if let Some(player_num) = self.player_map.remove(&id) {
            self.gamepad_states.remove(&id);
            crate::debugging::log_info(format!("Gamepad disconnected (was player {player_num})"));
            self.reassign_players();
        }
    }

    /// Finds the next available player number (1-based).
    fn next_available_player(&self) -> u8 {
        let used: Vec<u8> = self.player_map.values().copied().collect();
        for n in 1..=(self.max_controllers as u8) {
            if !used.contains(&n) {
                return n;
            }
        }
        (self.player_map.len() + 1) as u8
    }

    /// Reassigns player numbers consecutively after a disconnect.
    fn reassign_players(&mut self) {
        let mut ids: Vec<GamepadId> = self.player_map.keys().copied().collect();
        // Sort by current player number to maintain relative order.
        ids.sort_by_key(|id| self.player_map[id]);

        self.player_map.clear();
        for (idx, id) in ids.into_iter().enumerate() {
            self.player_map.insert(id, (idx + 1) as u8);
        }
    }

    /// Resolves a player number to the NES port it should control.
    ///
    /// Mirrors the SDL frontend's `assigned_gamepad_port` logic:
    /// only assigns to ports whose controller type accepts gamepad input.
    fn assigned_port(nes: &Nes, player_map: &HashMap<GamepadId, u8>, player_num: u8) -> Option<u8> {
        let gamepad_ports: Vec<u8> = (1..=4)
            .filter(|&port| nes.controller_input_type(port) == Some(ControllerInput::Gamepad))
            .collect();

        if gamepad_ports.is_empty() {
            return None;
        }

        let index = (player_num as usize).saturating_sub(1);
        gamepad_ports.get(index).copied().and_then(|port| {
            let assigned_count = player_map.len().min(gamepad_ports.len());
            if index < assigned_count {
                Some(port)
            } else {
                None
            }
        })
    }

    /// Handles a button press or release, dispatching to the correct NES port.
    fn handle_button(&self, nes: &mut Nes, id: GamepadId, button: gilrs::Button, pressed: bool) {
        let Some(&player_num) = self.player_map.get(&id) else {
            crate::debugging::log_info(format!("Gamepad dispatch: id {:?} not in player_map", id));
            return;
        };
        let Some(port) = Self::assigned_port(nes, &self.player_map, player_num) else {
            crate::debugging::log_info(format!(
                "Gamepad dispatch: player {player_num} has no assigned port"
            ));
            return;
        };

        // Port-aware mapping: try SNES first, fall back to NES.
        if let Some(snes_btn) = map_button_to_snes(button)
            && nes.set_snes_button(port, snes_btn, pressed)
        {
            return;
        }
        if let Some(nes_btn) = map_button_to_nes(button) {
            nes.set_button(port, nes_btn, pressed);
        } else {
            crate::debugging::log_info(format!(
                "Gamepad dispatch: button {:?} has no NES mapping",
                button
            ));
        }
    }

    /// Handles an axis change event, converting to digital D-pad presses.
    fn handle_axis(&mut self, nes: &mut Nes, id: GamepadId, axis: Axis, value: f32) {
        let Some(&player_num) = self.player_map.get(&id) else {
            return;
        };
        let Some(port) = Self::assigned_port(nes, &self.player_map, player_num) else {
            return;
        };
        let Some(state) = self.gamepad_states.get_mut(&id) else {
            return;
        };

        let changes = match axis {
            Axis::LeftStickX => state.axis.update_x(value),
            Axis::LeftStickY => state.axis.update_y(value),
            _ => return,
        };

        for (button, pressed) in changes {
            nes.set_button(port, button, pressed);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── map_button_to_nes ─────────────────────────────────────────────────

    #[test]
    fn nes_mapping_south_is_a() {
        assert_eq!(map_button_to_nes(gilrs::Button::South), Some(Button::A));
    }

    #[test]
    fn nes_mapping_east_is_b() {
        assert_eq!(map_button_to_nes(gilrs::Button::East), Some(Button::B));
    }

    #[test]
    fn nes_mapping_west_is_a_alternate() {
        assert_eq!(map_button_to_nes(gilrs::Button::West), Some(Button::A));
    }

    #[test]
    fn nes_mapping_north_is_b_alternate() {
        assert_eq!(map_button_to_nes(gilrs::Button::North), Some(Button::B));
    }

    #[test]
    fn nes_mapping_start() {
        assert_eq!(map_button_to_nes(gilrs::Button::Start), Some(Button::Start));
    }

    #[test]
    fn nes_mapping_select() {
        assert_eq!(
            map_button_to_nes(gilrs::Button::Select),
            Some(Button::Select)
        );
    }

    #[test]
    fn nes_mapping_dpad_up() {
        assert_eq!(map_button_to_nes(gilrs::Button::DPadUp), Some(Button::Up));
    }

    #[test]
    fn nes_mapping_dpad_down() {
        assert_eq!(
            map_button_to_nes(gilrs::Button::DPadDown),
            Some(Button::Down)
        );
    }

    #[test]
    fn nes_mapping_dpad_left() {
        assert_eq!(
            map_button_to_nes(gilrs::Button::DPadLeft),
            Some(Button::Left)
        );
    }

    #[test]
    fn nes_mapping_dpad_right() {
        assert_eq!(
            map_button_to_nes(gilrs::Button::DPadRight),
            Some(Button::Right)
        );
    }

    #[test]
    fn nes_mapping_shoulder_returns_none() {
        assert_eq!(map_button_to_nes(gilrs::Button::LeftTrigger), None);
        assert_eq!(map_button_to_nes(gilrs::Button::RightTrigger), None);
    }

    #[test]
    fn nes_mapping_unknown_returns_none() {
        assert_eq!(map_button_to_nes(gilrs::Button::Unknown), None);
        assert_eq!(map_button_to_nes(gilrs::Button::Mode), None);
    }

    // ── map_button_to_snes ────────────────────────────────────────────────

    #[test]
    fn snes_mapping_south_is_b() {
        assert_eq!(
            map_button_to_snes(gilrs::Button::South),
            Some(SnesButton::B)
        );
    }

    #[test]
    fn snes_mapping_east_is_a() {
        assert_eq!(map_button_to_snes(gilrs::Button::East), Some(SnesButton::A));
    }

    #[test]
    fn snes_mapping_west_is_y() {
        assert_eq!(map_button_to_snes(gilrs::Button::West), Some(SnesButton::Y));
    }

    #[test]
    fn snes_mapping_north_is_x() {
        assert_eq!(
            map_button_to_snes(gilrs::Button::North),
            Some(SnesButton::X)
        );
    }

    #[test]
    fn snes_mapping_start() {
        assert_eq!(
            map_button_to_snes(gilrs::Button::Start),
            Some(SnesButton::Start)
        );
    }

    #[test]
    fn snes_mapping_select() {
        assert_eq!(
            map_button_to_snes(gilrs::Button::Select),
            Some(SnesButton::Select)
        );
    }

    #[test]
    fn snes_mapping_left_trigger_is_l() {
        assert_eq!(
            map_button_to_snes(gilrs::Button::LeftTrigger),
            Some(SnesButton::L)
        );
    }

    #[test]
    fn snes_mapping_right_trigger_is_r() {
        assert_eq!(
            map_button_to_snes(gilrs::Button::RightTrigger),
            Some(SnesButton::R)
        );
    }

    #[test]
    fn snes_mapping_left_trigger2_is_l() {
        assert_eq!(
            map_button_to_snes(gilrs::Button::LeftTrigger2),
            Some(SnesButton::L)
        );
    }

    #[test]
    fn snes_mapping_right_trigger2_is_r() {
        assert_eq!(
            map_button_to_snes(gilrs::Button::RightTrigger2),
            Some(SnesButton::R)
        );
    }

    #[test]
    fn snes_mapping_dpad() {
        assert_eq!(
            map_button_to_snes(gilrs::Button::DPadUp),
            Some(SnesButton::Up)
        );
        assert_eq!(
            map_button_to_snes(gilrs::Button::DPadDown),
            Some(SnesButton::Down)
        );
        assert_eq!(
            map_button_to_snes(gilrs::Button::DPadLeft),
            Some(SnesButton::Left)
        );
        assert_eq!(
            map_button_to_snes(gilrs::Button::DPadRight),
            Some(SnesButton::Right)
        );
    }

    #[test]
    fn snes_mapping_unknown_returns_none() {
        assert_eq!(map_button_to_snes(gilrs::Button::Unknown), None);
        assert_eq!(map_button_to_snes(gilrs::Button::Mode), None);
    }

    // ── is_button_pressed (analog → digital threshold) ────────────────────

    #[test]
    fn analog_button_zero_is_released() {
        assert!(!is_button_pressed(0.0));
    }

    #[test]
    fn analog_button_at_threshold_is_released() {
        assert!(
            !is_button_pressed(BUTTON_PRESS_THRESHOLD),
            "exactly at threshold should be released (> not >=)"
        );
    }

    #[test]
    fn analog_button_above_threshold_is_pressed() {
        assert!(is_button_pressed(BUTTON_PRESS_THRESHOLD + 0.01));
    }

    #[test]
    fn analog_button_fully_pressed_is_pressed() {
        assert!(is_button_pressed(1.0));
    }

    #[test]
    fn analog_button_below_zero_is_released() {
        assert!(!is_button_pressed(-0.1));
    }

    // ── AxisState ─────────────────────────────────────────────────────────

    #[test]
    fn axis_x_neutral_produces_no_changes() {
        let mut state = AxisState::default();
        let changes = state.update_x(0.0);
        assert!(changes.is_empty());
    }

    #[test]
    fn axis_x_left_press() {
        let mut state = AxisState::default();
        let changes = state.update_x(-0.8);
        assert_eq!(changes, vec![(Button::Left, true)]);
        assert!(state.left);
        assert!(!state.right);
    }

    #[test]
    fn axis_x_right_press() {
        let mut state = AxisState::default();
        let changes = state.update_x(0.8);
        assert_eq!(changes, vec![(Button::Right, true)]);
        assert!(!state.left);
        assert!(state.right);
    }

    #[test]
    fn axis_x_left_release_on_return_to_neutral() {
        let mut state = AxisState::default();
        state.update_x(-0.8);
        let changes = state.update_x(0.0);
        assert_eq!(changes, vec![(Button::Left, false)]);
        assert!(!state.left);
    }

    #[test]
    fn axis_x_at_dead_zone_boundary_does_not_trigger() {
        let mut state = AxisState::default();
        let changes = state.update_x(-AXIS_DEAD_ZONE);
        assert!(
            changes.is_empty(),
            "exactly at dead zone should not trigger"
        );
    }

    #[test]
    fn axis_x_just_beyond_dead_zone_triggers() {
        let mut state = AxisState::default();
        let changes = state.update_x(-AXIS_DEAD_ZONE - 0.01);
        assert_eq!(changes, vec![(Button::Left, true)]);
    }

    #[test]
    fn axis_y_neutral_produces_no_changes() {
        let mut state = AxisState::default();
        let changes = state.update_y(0.0);
        assert!(changes.is_empty());
    }

    #[test]
    fn axis_y_up_press() {
        let mut state = AxisState::default();
        let changes = state.update_y(-0.8);
        assert_eq!(changes, vec![(Button::Up, true)]);
        assert!(state.up);
    }

    #[test]
    fn axis_y_down_press() {
        let mut state = AxisState::default();
        let changes = state.update_y(0.8);
        assert_eq!(changes, vec![(Button::Down, true)]);
        assert!(state.down);
    }

    #[test]
    fn axis_y_release_on_return_to_neutral() {
        let mut state = AxisState::default();
        state.update_y(0.8);
        let changes = state.update_y(0.0);
        assert_eq!(changes, vec![(Button::Down, false)]);
        assert!(!state.down);
    }

    #[test]
    fn axis_repeated_same_value_produces_no_changes() {
        let mut state = AxisState::default();
        state.update_x(-0.8);
        let changes = state.update_x(-0.9);
        assert!(
            changes.is_empty(),
            "same direction twice should not re-trigger"
        );
    }

    #[test]
    fn axis_snap_from_left_to_right() {
        let mut state = AxisState::default();
        state.update_x(-0.8);
        let changes = state.update_x(0.8);
        assert_eq!(changes, vec![(Button::Left, false), (Button::Right, true)]);
    }

    // ── PlayerAssignment (unit-testable helpers) ──────────────────────────

    /// A minimal test harness for player assignment logic.
    ///
    /// We test the pure `next_available_player` and `reassign_players` logic
    /// via a lightweight wrapper that doesn't require actual gilrs hardware.
    struct TestPlayerMap {
        player_map: HashMap<GamepadId, u8>,
        max_controllers: usize,
    }

    impl TestPlayerMap {
        fn new(max: usize) -> Self {
            Self {
                player_map: HashMap::new(),
                max_controllers: max,
            }
        }

        fn next_available_player(&self) -> u8 {
            let used: Vec<u8> = self.player_map.values().copied().collect();
            for n in 1..=(self.max_controllers as u8) {
                if !used.contains(&n) {
                    return n;
                }
            }
            (self.player_map.len() + 1) as u8
        }

        fn add(&mut self, id: GamepadId) -> Option<u8> {
            if self.player_map.len() >= self.max_controllers {
                return None;
            }
            if self.player_map.contains_key(&id) {
                return self.player_map.get(&id).copied();
            }
            let num = self.next_available_player();
            self.player_map.insert(id, num);
            Some(num)
        }

        fn remove(&mut self, id: GamepadId) -> Option<u8> {
            let removed = self.player_map.remove(&id);
            if removed.is_some() {
                self.reassign_players();
            }
            removed
        }

        fn reassign_players(&mut self) {
            let mut ids: Vec<GamepadId> = self.player_map.keys().copied().collect();
            ids.sort_by_key(|id| self.player_map[id]);
            self.player_map.clear();
            for (idx, id) in ids.into_iter().enumerate() {
                self.player_map.insert(id, (idx + 1) as u8);
            }
        }
    }

    /// Creates a fake GamepadId for testing. gilrs::GamepadId is opaque but
    /// can be created from usize via internal representation.
    fn fake_id(n: usize) -> GamepadId {
        // gilrs::GamepadId wraps a usize internally. We use transmute because
        // there's no public constructor. This is only safe in tests.
        unsafe { std::mem::transmute::<usize, GamepadId>(n) }
    }

    #[test]
    fn player_assignment_first_connected_is_p1() {
        let mut map = TestPlayerMap::new(2);
        let num = map.add(fake_id(0));
        assert_eq!(num, Some(1));
    }

    #[test]
    fn player_assignment_second_connected_is_p2() {
        let mut map = TestPlayerMap::new(2);
        map.add(fake_id(0));
        let num = map.add(fake_id(1));
        assert_eq!(num, Some(2));
    }

    #[test]
    fn player_assignment_rejects_third_in_normal_mode() {
        let mut map = TestPlayerMap::new(2);
        map.add(fake_id(0));
        map.add(fake_id(1));
        let num = map.add(fake_id(2));
        assert_eq!(num, None);
    }

    #[test]
    fn player_assignment_allows_four_in_four_score() {
        let mut map = TestPlayerMap::new(4);
        assert_eq!(map.add(fake_id(0)), Some(1));
        assert_eq!(map.add(fake_id(1)), Some(2));
        assert_eq!(map.add(fake_id(2)), Some(3));
        assert_eq!(map.add(fake_id(3)), Some(4));
    }

    #[test]
    fn player_assignment_rejects_fifth_in_four_score() {
        let mut map = TestPlayerMap::new(4);
        for i in 0..4 {
            map.add(fake_id(i));
        }
        assert_eq!(map.add(fake_id(4)), None);
    }

    #[test]
    fn player_assignment_duplicate_returns_existing() {
        let mut map = TestPlayerMap::new(2);
        map.add(fake_id(0));
        let second = map.add(fake_id(0));
        assert_eq!(second, Some(1));
        assert_eq!(map.player_map.len(), 1);
    }

    #[test]
    fn player_assignment_disconnect_and_reassign() {
        let mut map = TestPlayerMap::new(2);
        map.add(fake_id(0));
        map.add(fake_id(1));
        // Disconnect P1
        map.remove(fake_id(0));
        // Remaining gamepad (was P2) should be reassigned to P1
        assert_eq!(map.player_map.get(&fake_id(1)), Some(&1));
    }

    #[test]
    fn player_assignment_reconnect_after_disconnect() {
        let mut map = TestPlayerMap::new(2);
        map.add(fake_id(0));
        map.add(fake_id(1));
        // Disconnect P1
        map.remove(fake_id(0));
        // New gamepad connects
        let num = map.add(fake_id(2));
        assert_eq!(num, Some(2), "new gamepad should fill the P2 slot");
    }

    #[test]
    fn player_assignment_disconnect_middle_reassigns_consecutively() {
        let mut map = TestPlayerMap::new(4);
        map.add(fake_id(0));
        map.add(fake_id(1));
        map.add(fake_id(2));

        // Disconnect P2 (middle)
        map.remove(fake_id(1));

        // P1 stays, P3 becomes P2
        assert_eq!(map.player_map.get(&fake_id(0)), Some(&1));
        assert_eq!(map.player_map.get(&fake_id(2)), Some(&2));
    }
}
