//! Gamepad diagnostic tool.
//!
//! Opens all connected gamepads using gilrs with the SDL-format
//! `gamecontrollerdb.txt` mapping database and prints detailed information
//! for every button press, release, and axis change.

use gilrs::{Axis, Button, EventType, GilrsBuilder, MappingSource};
use std::time::Instant;

fn format_uuid(bytes: [u8; 16]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
}

fn mapping_source_label(src: MappingSource) -> &'static str {
    match src {
        MappingSource::None => "None (unmapped)",
        MappingSource::SdlMappings => "SDL mappings",
        MappingSource::Driver => "Driver",
    }
}

fn button_label(button: Button) -> &'static str {
    match button {
        Button::South => "South (A / Cross)",
        Button::East => "East (B / Circle)",
        Button::North => "North (Y / Triangle)",
        Button::West => "West (X / Square)",
        Button::C => "C",
        Button::Z => "Z",
        Button::LeftTrigger => "Left Trigger (L1 / LB)",
        Button::LeftTrigger2 => "Left Trigger 2 (L2 / LT)",
        Button::RightTrigger => "Right Trigger (R1 / RB)",
        Button::RightTrigger2 => "Right Trigger 2 (R2 / RT)",
        Button::Select => "Select (Back / Share)",
        Button::Start => "Start (Forward / Options)",
        Button::Mode => "Mode (Guide / Home)",
        Button::LeftThumb => "Left Thumb (L3)",
        Button::RightThumb => "Right Thumb (R3)",
        Button::DPadUp => "D-Pad Up",
        Button::DPadDown => "D-Pad Down",
        Button::DPadLeft => "D-Pad Left",
        Button::DPadRight => "D-Pad Right",
        Button::Unknown => "Unknown",
    }
}

fn axis_label(axis: Axis) -> &'static str {
    match axis {
        Axis::LeftStickX => "Left Stick X",
        Axis::LeftStickY => "Left Stick Y",
        Axis::LeftZ => "Left Z (L2 analog)",
        Axis::RightStickX => "Right Stick X",
        Axis::RightStickY => "Right Stick Y",
        Axis::RightZ => "Right Z (R2 analog)",
        Axis::DPadX => "D-Pad X",
        Axis::DPadY => "D-Pad Y",
        Axis::Unknown => "Unknown Axis",
    }
}

fn print_gamepad_info(id: gilrs::GamepadId, gilrs: &gilrs::Gilrs) {
    let gamepad = gilrs.gamepad(id);
    let uuid = format_uuid(gamepad.uuid());
    println!("  ID:             {id:?}");
    println!("  Name:           {}", gamepad.name());
    if let Some(map_name) = gamepad.map_name() {
        println!("  Map name:       {map_name}");
    }
    println!("  OS name:        {}", gamepad.os_name());
    println!("  UUID:           {uuid}");
    if let Some(vid) = gamepad.vendor_id() {
        println!("  Vendor ID:      0x{vid:04x}");
    }
    if let Some(pid) = gamepad.product_id() {
        println!("  Product ID:     0x{pid:04x}");
    }
    println!(
        "  Mapping source: {}",
        mapping_source_label(gamepad.mapping_source())
    );
    println!("  Power info:     {:?}", gamepad.power_info());
    println!("  FF supported:   {}", gamepad.is_ff_supported());
    println!("  Connected:      {}", gamepad.is_connected());
}

fn format_event_message(
    timestamp: &str,
    event_type: &str,
    name: &str,
    uuid: &str,
    details: String,
) -> String {
    format!("[{timestamp}] {event_type:8} | {name:25} | UUID: {uuid} | {details}")
}

fn main() {
    println!("=== Gamepad Test Tool ===");
    println!();

    let mut builder = GilrsBuilder::new()
        .with_default_filters(true)
        .add_env_mappings(true);

    match std::fs::read_to_string("gamecontrollerdb.txt") {
        Ok(mappings) => {
            let line_count = mappings.lines().count();
            println!("Loaded gamecontrollerdb.txt ({line_count} lines)");
            builder = builder.add_mappings(&mappings);
        }
        Err(e) => {
            println!("Warning: could not load gamecontrollerdb.txt: {e}");
        }
    }

    let mut gilrs = builder.build().expect("failed to initialize gilrs");

    println!();
    println!("--- Connected gamepads ---");
    let mut count = 0;
    for (id, gamepad) in gilrs.gamepads() {
        if gamepad.is_connected() {
            count += 1;
            println!();
            println!("Gamepad #{count}:");
            print_gamepad_info(id, &gilrs);
        }
    }
    if count == 0 {
        println!("  (none — connect a gamepad and it will be detected)");
    }
    println!();
    println!("--- Listening for events (press Ctrl+C to quit) ---");
    println!();

    let start = Instant::now();

    loop {
        while let Some(event) = gilrs.next_event() {
            let elapsed = start.elapsed();
            let gamepad = gilrs.gamepad(event.id);
            let uuid = format_uuid(gamepad.uuid());
            let name = gamepad.name().to_string();
            let timestamp = format!("{:.3}s", elapsed.as_secs_f64());

            match event.event {
                EventType::ButtonPressed(button, code) => {
                    println!(
                        "{}",
                        format_event_message(
                            &timestamp,
                            "PRESSED",
                            &name,
                            &uuid,
                            format!("Button: {} | Code: {code:?}", button_label(button))
                        )
                    );
                }
                EventType::ButtonReleased(button, code) => {
                    println!(
                        "{}",
                        format_event_message(
                            &timestamp,
                            "RELEASED",
                            &name,
                            &uuid,
                            format!("Button: {} | Code: {code:?}", button_label(button))
                        )
                    );
                }
                EventType::ButtonRepeated(button, code) => {
                    println!(
                        "{}",
                        format_event_message(
                            &timestamp,
                            "REPEATED",
                            &name,
                            &uuid,
                            format!("Button: {} | Code: {code:?}", button_label(button))
                        )
                    );
                }
                EventType::ButtonChanged(button, value, code) => {
                    println!(
                        "{}",
                        format_event_message(
                            &timestamp,
                            "CHANGED",
                            &name,
                            &uuid,
                            format!(
                                "Button: {} | Value: {value:.4} | Code: {code:?}",
                                button_label(button)
                            )
                        )
                    );
                }
                EventType::AxisChanged(axis, value, code) => {
                    println!(
                        "{}",
                        format_event_message(
                            &timestamp,
                            "AXIS",
                            &name,
                            &uuid,
                            format!(
                                "Axis: {} | Value: {value:+.4} | Code: {code:?}",
                                axis_label(axis)
                            )
                        )
                    );
                }
                EventType::Connected => {
                    println!("[{timestamp}] CONNECTED:");
                    print_gamepad_info(event.id, &gilrs);
                    println!();
                }
                EventType::Disconnected => {
                    println!(
                        "{}",
                        format_event_message(
                            &timestamp,
                            "DISCONNECTED",
                            &name,
                            &uuid,
                            String::new()
                        )
                    );
                }
                _ => {
                    println!(
                        "{}",
                        format_event_message(
                            &timestamp,
                            "OTHER",
                            &name,
                            &uuid,
                            format!("{:?}", event.event)
                        )
                    );
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
