use super::DebuggerSnapshot;

const DEBUGGER_OUTER_MARGIN: f32 = 10.0;
const DEBUGGER_OUTER_GAP: f32 = 10.0;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DebuggerUiAction {
    pub step_over: bool,
    pub step_into: bool,
    pub continue_run: bool,
    pub run_to_next_frame: bool,
    pub run_to_nmi: bool,
    pub run_to_irq: bool,
}

pub fn layout_models(display_size: [f32; 2]) -> [(&'static str, [f32; 2], [f32; 2]); 3] {
    let [display_w, display_h] = display_size;

    let margin = DEBUGGER_OUTER_MARGIN;
    let gap = DEBUGGER_OUTER_GAP;

    let available_h = (display_h - 2.0 * margin - gap).max(0.0);
    let bottom_h = available_h * 0.20;
    let cpu_h = (available_h - bottom_h).max(0.0);

    let cpu_w = (display_w - 2.0 * margin).max(0.0);
    let bottom_w = (display_w - 2.0 * margin - gap).max(0.0);
    let column_w = bottom_w / 2.0;

    let left_x = margin;
    let right_x = margin + column_w + gap;
    let top_y = margin;
    let bottom_y = top_y + cpu_h + gap;

    [
        ("CPU", [left_x, top_y], [cpu_w, cpu_h]),
        ("PPU", [left_x, bottom_y], [column_w, bottom_h]),
        ("APU", [right_x, bottom_y], [column_w, bottom_h]),
    ]
}

pub fn window_models(snapshot: &DebuggerSnapshot) -> [(&'static str, &str); 3] {
    [
        ("CPU", snapshot.cpu.as_str()),
        ("PPU", snapshot.ppu.as_str()),
        ("APU", snapshot.apu.as_str()),
    ]
}

pub fn render(ui: &imgui::Ui, snapshot: &DebuggerSnapshot) -> DebuggerUiAction {
    let mut action = DebuggerUiAction::default();
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
                    render_cpu_window(ui, snapshot, &mut action);
                } else {
                    for line in text.lines() {
                        ui.text(line);
                    }
                }
            });
    }

    action
}

#[derive(Debug, Clone, Copy)]
struct CpuWindowLayout {
    left_w: f32,
    right_w: f32,
    gap: f32,
    left_pos: [f32; 2],
    right_pos: [f32; 2],
}

fn cpu_window_layout(avail: [f32; 2], cursor: [f32; 2]) -> CpuWindowLayout {
    // Layout: left code view, right column split into registers (top) + PRG hexdump (bottom)
    let gap = 8.0;
    // Prefer more space for the right column so the hexdump can fit.
    let left_w = (avail[0] * 0.40).max(0.0);
    let right_w = (avail[0] - left_w - gap).max(0.0);

    let left_pos = cursor;
    let right_pos = [cursor[0] + left_w + gap, cursor[1]];

    CpuWindowLayout {
        left_w,
        right_w,
        gap,
        left_pos,
        right_pos,
    }
}

fn render_cpu_window(ui: &imgui::Ui, snapshot: &DebuggerSnapshot, action: &mut DebuggerUiAction) {
    render_cpu_controls(ui, action);
    ui.separator();

    let avail = ui.content_region_avail();
    let layout = cpu_window_layout(avail, ui.cursor_pos());

    ui.set_cursor_pos(layout.left_pos);
    render_cpu_code_panel(ui, snapshot, [layout.left_w, avail[1]]);

    ui.set_cursor_pos(layout.right_pos);
    render_cpu_right_panel(ui, snapshot, [layout.right_w, avail[1]], layout.gap);
}

fn render_cpu_controls(ui: &imgui::Ui, action: &mut DebuggerUiAction) {
    if ui.button("Step over") {
        action.step_over = true;
    }
    ui.same_line();
    if ui.button("Step into") {
        action.step_into = true;
    }
    ui.same_line();
    if ui.button("Continue") {
        action.continue_run = true;
    }

    if ui.button("Run to next frame") {
        action.run_to_next_frame = true;
    }
    ui.same_line();
    if ui.button("Run to NMI") {
        action.run_to_nmi = true;
    }
    ui.same_line();
    if ui.button("Run to IRQ") {
        action.run_to_irq = true;
    }
}

fn render_cpu_code_panel(ui: &imgui::Ui, snapshot: &DebuggerSnapshot, size: [f32; 2]) {
    ui.child_window("cpu_code")
        .size(size)
        .border(true)
        .build(|| {
            ui.text("Code");
            ui.separator();

            for line in &snapshot.cpu_disasm {
                let bytes = format_disasm_bytes(&line.bytes);
                let text = format!("{:04X}: {:<8} {}", line.addr, bytes, line.text);
                if line.is_current {
                    let cursor = ui.cursor_screen_pos();
                    let draw_w = ui.content_region_avail()[0];
                    let draw_h = ui.text_line_height();

                    ui.get_window_draw_list()
                        .add_rect(
                            cursor,
                            [cursor[0] + draw_w, cursor[1] + draw_h],
                            [1.0, 1.0, 1.0, 1.0],
                        )
                        .filled(true)
                        .build();

                    let _text = ui.push_style_color(imgui::StyleColor::Text, [0.0, 0.0, 0.0, 1.0]);
                    ui.text(text);
                } else {
                    ui.text(text);
                }
            }
        });
}

fn render_cpu_right_panel(ui: &imgui::Ui, snapshot: &DebuggerSnapshot, size: [f32; 2], gap: f32) {
    ui.child_window("cpu_right")
        .size(size)
        .border(false)
        .build(|| {
            let right_avail = ui.content_region_avail();
            let (regs_h, hex_h) = cpu_right_panel_split(right_avail, gap);

            ui.child_window("cpu_regs")
                .size([right_avail[0], regs_h])
                .border(true)
                .build(|| {
                    render_cpu_registers(ui, snapshot);
                });

            ui.dummy([0.0, gap]);

            ui.child_window("cpu_prg_hex")
                .size([right_avail[0], hex_h])
                .border(true)
                .build(|| {
                    ui.text(format!(
                        "PRG-ROM hexdump @ {:04X}",
                        snapshot.prg_hexdump_base
                    ));
                    ui.separator();

                    for line in
                        format_hexdump_lines(snapshot.prg_hexdump_base, &snapshot.prg_hexdump_bytes)
                    {
                        ui.text(line);
                    }
                });
        });
}

fn cpu_right_panel_split(avail: [f32; 2], gap: f32) -> (f32, f32) {
    let regs_h = avail[1] * 0.35;
    let hex_h = (avail[1] - regs_h - gap).max(0.0);
    (regs_h, hex_h)
}

fn format_disasm_bytes(bytes: &[u8]) -> String {
    match bytes.len() {
        0 => String::new(),
        1 => format!("{:02X}", bytes[0]),
        2 => format!("{:02X} {:02X}", bytes[0], bytes[1]),
        _ => format!("{:02X} {:02X} {:02X}", bytes[0], bytes[1], bytes[2]),
    }
}

fn render_cpu_registers(ui: &imgui::Ui, snapshot: &DebuggerSnapshot) {
    for line in cpu_register_lines(snapshot) {
        ui.text(line);
    }
}

fn cpu_register_lines(snapshot: &DebuggerSnapshot) -> Vec<String> {
    let r = snapshot.cpu_regs;
    let interrupt = match r.interrupt {
        None => "-",
        Some(crate::cpu::InterruptKind::Nmi) => "NMI",
        Some(crate::cpu::InterruptKind::Irq) => "IRQ",
    };

    vec![
        format!("PC: {:04X}  SP: {:02X}", r.pc, r.sp),
        format!("A:  {:02X}  X:  {:02X}  Y:  {:02X}", r.a, r.x, r.y),
        format!("P:  {:02X}  {}", r.p, format_status_flags(r.p)),
        format!("INT: {interrupt}"),
        format!("VEC: NMI {:04X}  IRQ {:04X}", r.nmi_vector, r.irq_vector),
        format!("CYC: {}", r.cycles),
    ]
}

fn format_status_flags(p: u8) -> String {
    // 6502 status register bits:
    // 7 N, 6 V, 5 U (unused), 4 B, 3 D, 2 I, 1 Z, 0 C
    let flag = |bit: u8, ch: char| if (p & (1 << bit)) != 0 { ch } else { '-' };

    let n = flag(7, 'N');
    let v = flag(6, 'V');
    let u = flag(5, 'U');
    let b = flag(4, 'B');
    let d = flag(3, 'D');
    let i = flag(2, 'I');
    let z = flag(1, 'Z');
    let c = flag(0, 'C');

    format!("{n}{v}{u}{b}{d}{i}{z}{c}")
}

fn format_hexdump_lines(base_addr: u16, bytes: &[u8]) -> Vec<String> {
    let mut lines = Vec::new();

    for (row, chunk) in bytes.chunks(16).enumerate() {
        let addr = base_addr.wrapping_add((row * 16) as u16);

        let mut hex = String::new();
        for i in 0..16 {
            if i > 0 {
                hex.push(' ');
            }

            if let Some(b) = chunk.get(i) {
                hex.push_str(&format!("{b:02X}"));
            } else {
                hex.push_str("  ");
            }
        }

        let ascii: String = chunk
            .iter()
            .map(|b| {
                if (0x20..=0x7E).contains(b) {
                    *b as char
                } else {
                    '.'
                }
            })
            .collect();

        lines.push(format!("{addr:04X}: {hex} |{ascii}|"));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::{Nes, TvSystem};
    use crate::debugging::snapshot;

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
        let snapshot = snapshot(&nes);

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
    fn test_layout_puts_cpu_full_width_and_places_ppu_apu_below_side_by_side() {
        let display_size = [800.0, 600.0];
        let layouts = layout_models(display_size);

        let cpu = layouts.iter().find(|l| l.0 == "CPU").unwrap();
        let ppu = layouts.iter().find(|l| l.0 == "PPU").unwrap();
        let apu = layouts.iter().find(|l| l.0 == "APU").unwrap();

        let margin = 10.0;
        let gap = 10.0;
        let available_h = (display_size[1] - 2.0 * margin - gap).max(0.0);
        let expected_bottom_h = available_h * 0.20;
        let expected_cpu_h = (available_h - expected_bottom_h).max(0.0);
        let expected_cpu_w = (display_size[0] - 2.0 * margin).max(0.0);
        let expected_bottom_w = (display_size[0] - 2.0 * margin - gap).max(0.0);
        let expected_col_w = expected_bottom_w / 2.0;

        // CPU spans full width (minus margins).
        assert_close(cpu.1[0], margin);
        assert_close(cpu.2[0], expected_cpu_w);

        // Heights: bottom row is 20% of available height.
        assert_close(ppu.2[1], expected_bottom_h);
        assert_close(apu.2[1], expected_bottom_h);
        assert_close(cpu.2[1], expected_cpu_h);

        // PPU/APU are below CPU.
        assert!(ppu.1[1] > cpu.1[1]);
        assert!(apu.1[1] > cpu.1[1]);
        assert_close(ppu.1[1], apu.1[1]);

        // PPU left, APU right, side-by-side with 50% width each (minus gap).
        assert_close(ppu.2[0], expected_col_w);
        assert_close(apu.2[0], expected_col_w);
        assert!(ppu.1[0] < apu.1[0]);
    }

    #[test]
    fn test_cpu_register_lines_render_expected_values() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.cpu.set_pc(0xC000);
        nes.cpu.set_a_register(0x12);
        nes.cpu.set_x(0x34);
        nes.cpu.set_y(0x56);
        nes.cpu.set_sp(0xFD);
        nes.cpu.set_p(0b1010_0101);

        let snapshot = snapshot(&nes);
        let lines = cpu_register_lines(&snapshot);

        assert!(lines.iter().any(|l| l.contains("PC: C000")));
        assert!(lines.iter().any(|l| l.contains("SP: FD")));
        assert!(lines.iter().any(|l| l.contains("A:  12")));
        assert!(lines.iter().any(|l| l.contains("X:  34")));
        assert!(lines.iter().any(|l| l.contains("Y:  56")));
        assert!(lines.iter().any(|l| l.contains("P:  A5")));
        // N(7)=1, V(6)=0, U(5)=1, B(4)=0, D(3)=0, I(2)=1, Z(1)=0, C(0)=1
        assert!(lines.iter().any(|l| l.contains("N-U--I-C")));
    }

    #[test]
    fn test_format_hexdump_lines_produces_expected_addresses_and_bytes() {
        let bytes: Vec<u8> = (0u8..=31u8).collect();
        let lines = format_hexdump_lines(0x8000, &bytes);

        assert!(lines.iter().any(|l| l.contains("8000:")));
        assert!(lines.iter().any(|l| l.contains("00 01 02 03")));
        assert!(lines.iter().any(|l| l.contains("8010:")));
        assert!(lines.iter().any(|l| l.contains("10 11 12 13")));
    }

    #[test]
    fn test_cpu_window_layout_splits_left_and_right_columns() {
        let cursor = [5.0, 7.0];
        let avail = [100.0, 50.0];
        let layout = cpu_window_layout(avail, cursor);

        // 40% left column width and a fixed gap of 8.0.
        assert_close(layout.left_w, 40.0);
        assert_close(layout.gap, 8.0);
        assert_close(layout.right_w, 52.0);

        assert_close(layout.left_pos[0], 5.0);
        assert_close(layout.left_pos[1], 7.0);
        assert_close(layout.right_pos[0], 5.0 + 40.0 + 8.0);
        assert_close(layout.right_pos[1], 7.0);
    }
}
