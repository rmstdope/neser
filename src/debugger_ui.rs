use crate::debugger::DebuggerSnapshot;

pub fn layout_models(display_size: [f32; 2]) -> [(&'static str, [f32; 2], [f32; 2]); 3] {
    let [display_w, display_h] = display_size;

    let margin = 10.0;
    let gap = 10.0;

    let available_w = (display_w - 2.0 * margin - gap).max(0.0);
    let column_w = available_w / 2.0;

    let cpu_h = display_h * 0.60;
    let half_h = display_h * 0.30;

    let left_x = margin;
    let right_x = margin + column_w + gap;
    let top_y = margin;

    [
        ("CPU", [left_x, top_y], [column_w, cpu_h]),
        ("PPU", [right_x, top_y], [column_w, half_h]),
        ("APU", [right_x, top_y + half_h], [column_w, half_h]),
    ]
}

pub fn window_models(snapshot: &DebuggerSnapshot) -> [(&'static str, &str); 3] {
    [
        ("CPU", snapshot.cpu.as_str()),
        ("PPU", snapshot.ppu.as_str()),
        ("APU", snapshot.apu.as_str()),
    ]
}

pub fn render(ui: &imgui::Ui, snapshot: &DebuggerSnapshot) {
    let models = window_models(snapshot);
    let layouts = layout_models(ui.io().display_size);

    for (title, text) in models {
        let (_, pos, size) = layouts
            .iter()
            .copied()
            .find(|(t, _, _)| *t == title)
            .expect("layout entry must exist for each window");

        ui.window(title)
            .position(pos, imgui::Condition::Always)
            .size(size, imgui::Condition::Always)
            .build(|| {
                if title == "CPU" {
                    render_cpu_window(ui, snapshot);
                } else {
                    for line in text.lines() {
                        ui.text(line);
                    }
                }
            });
    }
}

fn render_cpu_window(ui: &imgui::Ui, _snapshot: &DebuggerSnapshot) {
    // Layout: left code view, right column split into registers (top) + PRG hexdump (bottom)
    let avail = ui.content_region_avail();
    let gap = 10.0;
    let left_w = (avail[0] * 0.50).max(0.0);
    let right_w = (avail[0] - left_w - gap).max(0.0);

    let cursor = ui.cursor_pos();
    let left_pos = cursor;
    let right_pos = [cursor[0] + left_w + gap, cursor[1]];

    ui.set_cursor_pos(left_pos);
    ui.child_window("cpu_code")
        .size([left_w, avail[1]])
        .border(true)
        .build(|| {
            ui.text("Code view (TODO)");
            ui.separator();
            ui.text("Show n instructions before/after PC");
        });

    ui.set_cursor_pos(right_pos);
    ui.child_window("cpu_right")
        .size([right_w, avail[1]])
        .border(false)
        .build(|| {
            let right_avail = ui.content_region_avail();
            let regs_h = right_avail[1] * 0.35;
            let hex_h = (right_avail[1] - regs_h - gap).max(0.0);

            ui.child_window("cpu_regs")
                .size([right_avail[0], regs_h])
                .border(true)
                .build(|| {
                    ui.text("Registers (TODO)");
                    ui.separator();
                    ui.text("Show A/X/Y/SP/PC/P and flags");
                });

            ui.dummy([0.0, gap]);

            ui.child_window("cpu_prg_hex")
                .size([right_avail[0], hex_h])
                .border(true)
                .build(|| {
                    ui.text("PRG-ROM hexdump (TODO)");
                    ui.separator();
                    ui.text("Show hexdump of PRG-ROM");
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debugger;
    use crate::nes::{Nes, TvSystem};

    fn assert_close(actual: f32, expected: f32) {
        let eps = 0.0001;
        assert!(
            (actual - expected).abs() <= eps,
            "expected {expected}, got {actual}",
            expected = expected,
            actual = actual
        );
    }

    #[test]
    fn test_window_models_have_three_debug_windows_with_text() {
        let nes = Nes::new(TvSystem::Ntsc);
        let snapshot = debugger::snapshot(&nes);

        let windows = window_models(&snapshot);
        assert_eq!(windows.len(), 3);

        assert_eq!(windows[0].0, "CPU");
        assert!(windows[0].1.contains("PC"));

        assert_eq!(windows[1].0, "PPU");
        assert!(windows[1].1.contains("scanline"));

        assert_eq!(windows[2].0, "APU");
        assert!(windows[2].1.contains("apu_cycle"));
    }

    #[test]
    fn test_layout_puts_cpu_left_and_stacks_ppu_apu_right_with_expected_heights() {
        let display_size = [800.0, 600.0];
        let layouts = layout_models(display_size);

        let cpu = layouts.iter().find(|l| l.0 == "CPU").unwrap();
        let ppu = layouts.iter().find(|l| l.0 == "PPU").unwrap();
        let apu = layouts.iter().find(|l| l.0 == "APU").unwrap();

        // CPU is left of the right column.
        assert!(cpu.1[0] < ppu.1[0]);
        assert_close(ppu.1[0], apu.1[0]);

        // Heights: CPU 60%, PPU 30%, APU 30%.
        assert_close(cpu.2[1], display_size[1] * 0.60);
        assert_close(ppu.2[1], display_size[1] * 0.30);
        assert_close(apu.2[1], display_size[1] * 0.30);

        // Right column stacks PPU above APU.
        assert_close(apu.1[1], ppu.1[1] + ppu.2[1]);
    }
}
