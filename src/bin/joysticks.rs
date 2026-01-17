fn main() {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    // Create a hidden window to ensure keyboard events are captured
    let window = video_subsystem
        .window("Joystick Diagnostic - Press 'q' to quit", 800, 600)
        .position_centered()
        .build()
        .unwrap();
    println!("Focus the SDL window and press 'q' to quit.");

    // Initialize SDL2_ttf for text rendering
    let ttf_context = sdl2::ttf::init().unwrap();
    let font_path = "/System/Library/Fonts/SFNSRounded.ttf"; // Use a system font on macOS
    let font = ttf_context.load_font(font_path, 16).unwrap();
    let mut canvas = window.into_canvas().build().unwrap();
    let texture_creator = canvas.texture_creator();

    let joystick_subsystem = sdl_context.joystick().unwrap();
    let count = joystick_subsystem.num_joysticks().unwrap();
    println!("Found {} joystick(s)", count);
    for i in 0..count {
        let name = joystick_subsystem
            .name_for_index(i)
            .unwrap_or("Unknown".to_string());
        let guid = joystick_subsystem.device_guid(i);
        match guid {
            Ok(g) => println!("Joystick {}: {} (GUID: {})", i, name, g),
            Err(e) => println!("Joystick {}: {} (GUID error: {:?})", i, name, e),
        }
    }
    let mut joysticks = Vec::new();
    for i in 0..count {
        match joystick_subsystem.open(i) {
            Ok(joy) => joysticks.push(joy),
            Err(e) => println!("Failed to open joystick {}: {}", i, e),
        }
    }

    let mut sdl_event = sdl_context.event_pump().unwrap();

    loop {
        for event in sdl_event.poll_iter() {
            if let sdl2::event::Event::KeyDown {
                keycode: Some(sdl2::keyboard::Keycode::Q),
                ..
            } = event
            {
                println!("Quitting on 'q' press.");
                return;
            }
        }

        // Prepare text for all joysticks, split axes/buttons/hats onto separate lines
        let mut lines = Vec::new();
        for (i, joy) in joysticks.iter().enumerate() {
            lines.push(format!("Joystick {}:", i));
            let mut axis_line = String::from("  Axes:   ");
            for a in 0..joy.num_axes() {
                let val = joy.axis(a).unwrap_or_default();
                axis_line += &format!("A{}:{} ", a, val);
            }
            lines.push(axis_line);
            let mut button_line = String::from("  Buttons:");
            for b in 0..joy.num_buttons() {
                let pressed = joy.button(b).unwrap_or(false);
                button_line += &format!("B{}:{} ", b, pressed);
            }
            lines.push(button_line);
            let mut hat_line = String::from("  Hats:   ");
            for h in 0..joy.num_hats() {
                let hat = joy.hat(h).unwrap_or(sdl2::joystick::HatState::Centered);
                hat_line += &format!("H{}:{:?} ", h, hat);
            }
            if joy.num_hats() > 0 {
                lines.push(hat_line);
            }
            lines.push(String::new()); // Blank line between joysticks
        }

        // Clear window
        canvas.set_draw_color(sdl2::pixels::Color::RGB(30, 30, 30));
        canvas.clear();

        // Render each line of joystick info
        let mut y = 10;
        for line in lines.iter() {
            if line.is_empty() {
                y += 12; // Add a small vertical gap for blank lines
                continue;
            }
            let surface = font
                .render(line)
                .blended(sdl2::pixels::Color::RGB(220, 220, 220))
                .unwrap();
            let texture = texture_creator
                .create_texture_from_surface(&surface)
                .unwrap();
            let target = sdl2::rect::Rect::new(10, y, surface.width(), surface.height());
            canvas.copy(&texture, None, Some(target)).unwrap();
            y += surface.height() as i32 + 4;
        }

        canvas.present();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
