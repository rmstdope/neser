#[cfg(feature = "sdl")]
use super::DebuggerSnapshot;
use crate::debugging::breakpoints::{Breakpoint, BreakpointKind, BreakpointList};

const DEBUGGER_OUTER_MARGIN: f32 = 10.0;
pub(crate) const DEBUGGER_UI_FONT_SCALE: f32 = 0.85;
const CODE_VIEW_LONGEST_LINE_CHARS: f32 = 28.0;
const CODE_VIEW_PREFIX_CHARS: f32 = 15.0;
const CODE_VIEW_CHAR_WIDTH_AT_SCALE_ONE: f32 = 8.0;
const CODE_VIEW_CHILD_WINDOW_HORIZONTAL_PADDING: f32 = 16.0;
const CODE_VIEW_SCROLLBAR_GUTTER: f32 = 16.0;
const CODE_VIEW_HIGHLIGHT_OVERDRAW_MARGIN: f32 = 4.0;
const CPU_RIGHT_PANEL_MIN_WIDTH: f32 = 320.0;

fn debugger_ui_font_scale() -> f32 {
    DEBUGGER_UI_FONT_SCALE
}

fn apply_debugger_ui_font_scale(ui: &imgui::Ui) {
    ui.set_window_font_scale(debugger_ui_font_scale());
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DebuggerUiAction {
    pub step_over: bool,
    pub step_into: bool,
    pub continue_run: bool,
    pub run_to_next_frame: bool,
    pub run_to_next_scanline: bool,
    pub run_to_nmi: bool,
    pub run_to_irq: bool,
    pub toggle_ppu_viewer: bool,
    pub increase_opacity: bool,
    pub decrease_opacity: bool,
    /// Add a new breakpoint of this kind.
    pub add_breakpoint: Option<BreakpointKind>,
    /// Remove the breakpoint at this list index.
    pub remove_breakpoint: Option<usize>,
    /// Enable the breakpoint at this list index.
    pub enable_breakpoint: Option<usize>,
    /// Disable the breakpoint at this list index.
    pub disable_breakpoint: Option<usize>,
    /// Set PRG hexdump base address (normalized in snapshot/view state layer).
    pub set_prg_hexdump_base: Option<u16>,
    /// Move PRG hexdump base by byte delta (e.g. -16 or +16).
    pub nudge_prg_hexdump_base_by_bytes: Option<i16>,
}

/// Persistent state for the "add breakpoint" row in the breakpoint panel.
#[derive(Debug, Default)]
pub struct BreakpointAddUiState {
    /// Index into the kind combo: 0=PC, 1=Cycle, 2=Write, 3=Frame
    pub kind_idx: usize,
    /// Text input buffer for the breakpoint value.
    pub value: String,
    /// Validation error shown inline when Add/Enter input is invalid.
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct HexdumpUiState {
    pub address_input: String,
    pub error: Option<String>,
}
pub fn layout_model(display_size: [f32; 2]) -> (&'static str, [f32; 2], [f32; 2]) {
    let [display_w, display_h] = display_size;
    let margin = DEBUGGER_OUTER_MARGIN;
    let available_w = (display_w - 2.0 * margin).max(0.0);
    let available_h = (display_h - 2.0 * margin).max(0.0);
    ("CPU/PPU Data", [margin, margin], [available_w, available_h])
}

#[cfg(feature = "sdl")]
pub fn render(
    ui: &imgui::Ui,
    snapshot: &DebuggerSnapshot,
    alpha: f32,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    hexdump_state: &mut HexdumpUiState,
) -> DebuggerUiAction {
    let mut action = DebuggerUiAction::default();
    let (title, pos, size) = layout_model(ui.io().display_size);

    ui.window(title)
        .position(pos, imgui::Condition::Always)
        .size(size, imgui::Condition::Always)
        .bring_to_front_on_focus(false)
        .bg_alpha(alpha)
        .build(|| {
            apply_debugger_ui_font_scale(ui);
            render_cpu_window(
                ui,
                snapshot,
                breakpoints,
                add_state,
                hexdump_state,
                &mut action,
            );
        });

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

#[cfg(feature = "sdl")]
fn cpu_window_layout(avail: [f32; 2], cursor: [f32; 2]) -> CpuWindowLayout {
    // Layout: left code view, right column split into registers (top) + PRG hexdump (bottom)
    let gap = 8.0;
    let target_left_w = code_view_target_width_for_font_scale(debugger_ui_font_scale());
    let max_left_w = (avail[0] - gap - CPU_RIGHT_PANEL_MIN_WIDTH).max(0.0);
    let left_w = target_left_w.min(max_left_w);
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

fn code_view_target_width_for_font_scale(font_scale: f32) -> f32 {
    let total_line_chars = CODE_VIEW_PREFIX_CHARS + CODE_VIEW_LONGEST_LINE_CHARS;
    (total_line_chars * CODE_VIEW_CHAR_WIDTH_AT_SCALE_ONE * font_scale)
        + CODE_VIEW_CHILD_WINDOW_HORIZONTAL_PADDING
        + CODE_VIEW_SCROLLBAR_GUTTER
        + CODE_VIEW_HIGHLIGHT_OVERDRAW_MARGIN
}

#[cfg(feature = "sdl")]
fn render_cpu_window(
    ui: &imgui::Ui,
    snapshot: &DebuggerSnapshot,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    hexdump_state: &mut HexdumpUiState,
    action: &mut DebuggerUiAction,
) {
    render_cpu_controls(ui, action);
    render_breakpoint_panel(ui, breakpoints, add_state, action);
    ui.separator();

    let avail = ui.content_region_avail();
    let layout = cpu_window_layout(avail, ui.cursor_pos());

    ui.set_cursor_pos(layout.left_pos);
    render_cpu_code_panel(ui, snapshot, [layout.left_w, avail[1]]);

    ui.set_cursor_pos(layout.right_pos);
    render_cpu_right_panel(
        ui,
        snapshot,
        [layout.right_w, avail[1]],
        layout.gap,
        hexdump_state,
        action,
    );
}

#[cfg(feature = "sdl")]
fn render_breakpoint_panel(
    ui: &imgui::Ui,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    action: &mut DebuggerUiAction,
) {
    if !ui.collapsing_header("Breakpoints##bp_header", imgui::TreeNodeFlags::empty()) {
        return;
    }

    render_existing_breakpoints(ui, breakpoints, action);
    ui.separator();
    render_add_breakpoint_row(ui, add_state, action);
}

#[cfg(feature = "sdl")]
fn render_existing_breakpoints(
    ui: &imgui::Ui,
    breakpoints: &BreakpointList,
    action: &mut DebuggerUiAction,
) {
    for (i, bp) in breakpoints.iter().enumerate() {
        let mut enabled = bp.enabled;
        if ui.checkbox(format!("##bp_en_{}", i), &mut enabled) {
            if enabled {
                action.enable_breakpoint = Some(i);
            } else {
                action.disable_breakpoint = Some(i);
            }
        }
        ui.same_line();
        ui.text(format_breakpoint_label(bp));
        ui.same_line();
        if ui.small_button(format!("X##bp_rm_{}", i)) {
            action.remove_breakpoint = Some(i);
        }
    }
}

#[cfg(feature = "sdl")]
fn render_add_breakpoint_row(
    ui: &imgui::Ui,
    add_state: &mut BreakpointAddUiState,
    action: &mut DebuggerUiAction,
) {
    let kinds = ["PC", "Cycle", "Write", "Frame"];
    let _width = ui.push_item_width(60.0);
    ui.combo_simple_string("##bp_kind", &mut add_state.kind_idx, &kinds);
    drop(_width);
    ui.same_line();
    let _width = ui.push_item_width(120.0);
    let enter_pressed = ui
        .input_text("##bp_val", &mut add_state.value)
        .flags(imgui::InputTextFlags::ENTER_RETURNS_TRUE)
        .build();
    drop(_width);
    ui.same_line();
    let add_clicked = ui.button("Add##bp_add");
    if breakpoint_add_submit_requested(add_clicked, enter_pressed) {
        match validate_breakpoint_input(add_state.kind_idx, &add_state.value) {
            Ok(kind) => {
                action.add_breakpoint = Some(kind);
                add_state.value.clear();
                add_state.error = None;
            }
            Err(message) => {
                add_state.error = Some(message.to_string());
            }
        }
    }

    if let Some(message) = add_state.error.as_deref() {
        ui.text_colored([1.0, 0.4, 0.4, 1.0], message);
    }
}

fn breakpoint_add_submit_requested(add_clicked: bool, enter_pressed: bool) -> bool {
    add_clicked || enter_pressed
}

fn parse_breakpoint_kind_from_input(kind_idx: usize, value: &str) -> Option<BreakpointKind> {
    match kind_idx {
        0 => parse_hex_u16(value).map(BreakpointKind::Pc),
        1 => value.trim().parse::<u64>().ok().map(BreakpointKind::Cycle),
        2 => parse_hex_u16(value).map(BreakpointKind::WriteAddress),
        3 => value.trim().parse::<u64>().ok().map(BreakpointKind::Frame),
        _ => None,
    }
}

fn validate_breakpoint_input(kind_idx: usize, value: &str) -> Result<BreakpointKind, &'static str> {
    parse_breakpoint_kind_from_input(kind_idx, value).ok_or("Invalid breakpoint value")
}

fn parse_hex_u16(s: &str) -> Option<u16> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).ok()
}

fn parse_hexdump_base_input(value: &str) -> Option<u16> {
    parse_hex_u16(value).filter(|addr| *addr >= 0x8000)
}

fn validate_hexdump_input(value: &str) -> Result<u16, &'static str> {
    parse_hexdump_base_input(value).ok_or("Invalid hexdump address")
}

fn normalize_hexdump_base_for_ui(addr: u16) -> u16 {
    (addr & 0xFFF0).clamp(0x8000, 0xFF00)
}

fn hexdump_go_submit_requested(go_clicked: bool, enter_pressed: bool) -> bool {
    go_clicked || enter_pressed
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
    if ui.button("Run to next scanline") {
        action.run_to_next_scanline = true;
    }
    ui.same_line();
    if ui.button("Run to NMI") {
        action.run_to_nmi = true;
    }
    ui.same_line();
    if ui.button("Run to IRQ") {
        action.run_to_irq = true;
    }
    ui.same_line();
    if ui.button("PPU Viewer") {
        action.toggle_ppu_viewer = true;
    }
    ui.same_line();
    if ui.button("α-") {
        action.decrease_opacity = true;
    }
    ui.same_line();
    if ui.button("α+") {
        action.increase_opacity = true;
    }
}

fn render_cpu_code_panel(ui: &imgui::Ui, snapshot: &DebuggerSnapshot, size: [f32; 2]) {
    ui.child_window("cpu_code")
        .size(size)
        .border(true)
        .build(|| {
            apply_debugger_ui_font_scale(ui);
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
                    ui.set_scroll_here_y_with_ratio(0.5);
                } else {
                    ui.text(text);
                }
            }
        });
}

fn render_cpu_right_panel(
    ui: &imgui::Ui,
    snapshot: &DebuggerSnapshot,
    size: [f32; 2],
    gap: f32,
    hexdump_state: &mut HexdumpUiState,
    action: &mut DebuggerUiAction,
) {
    ui.child_window("cpu_right")
        .size(size)
        .border(false)
        .build(|| {
            apply_debugger_ui_font_scale(ui);
            let right_avail = ui.content_region_avail();
            let (regs_h, hex_h) = cpu_right_panel_split(right_avail, gap);

            ui.child_window("cpu_regs")
                .size([right_avail[0], regs_h])
                .border(true)
                .build(|| {
                    apply_debugger_ui_font_scale(ui);
                    render_cpu_registers(ui, snapshot);
                });

            ui.dummy([0.0, gap]);

            ui.child_window("cpu_prg_hex")
                .size([right_avail[0], hex_h])
                .border(true)
                .build(|| {
                    apply_debugger_ui_font_scale(ui);
                    render_hexdump_controls(ui, snapshot.prg_hexdump_base, hexdump_state, action);
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

fn render_hexdump_controls(
    ui: &imgui::Ui,
    current_base: u16,
    hexdump_state: &mut HexdumpUiState,
    action: &mut DebuggerUiAction,
) {
    if hexdump_state.address_input.is_empty() {
        hexdump_state.address_input = format!("{:04X}", current_base);
    }

    if ui.small_button("-##hex_step") {
        action.nudge_prg_hexdump_base_by_bytes = Some(-16);
        let next = normalize_hexdump_base_for_ui(current_base.saturating_sub(16));
        hexdump_state.address_input = format!("{:04X}", next);
    }
    ui.same_line();
    if ui.small_button("+##hex_step") {
        action.nudge_prg_hexdump_base_by_bytes = Some(16);
        let next = normalize_hexdump_base_for_ui(current_base.saturating_add(16));
        hexdump_state.address_input = format!("{:04X}", next);
    }
    ui.same_line();

    let _width = ui.push_item_width(90.0);
    let enter_pressed = ui
        .input_text("##hex_base", &mut hexdump_state.address_input)
        .flags(imgui::InputTextFlags::ENTER_RETURNS_TRUE)
        .build();
    drop(_width);
    ui.same_line();

    let go_clicked = ui.small_button("Go##hex_base");
    if hexdump_go_submit_requested(go_clicked, enter_pressed) {
        match validate_hexdump_input(&hexdump_state.address_input) {
            Ok(addr) => {
                action.set_prg_hexdump_base = Some(addr);
                hexdump_state.address_input =
                    format!("{:04X}", normalize_hexdump_base_for_ui(addr));
                hexdump_state.error = None;
            }
            Err(message) => {
                hexdump_state.error = Some(message.to_string());
            }
        }
    }

    if let Some(message) = hexdump_state.error.as_deref() {
        ui.text_colored([1.0, 0.4, 0.4, 1.0], message);
    }
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

    let lines = vec![
        format!("PC: {:04X}  SP: {:02X}", r.pc, r.sp),
        format!("A:  {:02X}  X:  {:02X}  Y:  {:02X}", r.a, r.x, r.y),
        format!("P:  {:02X}  {}", r.p, format_status_flags(r.p)),
        format!("INT: {interrupt}"),
        format!(
            "VEC: NMI {:04X}  RST {:04X}  IRQ {:04X}",
            r.nmi_vector, r.reset_vector, r.irq_vector
        ),
        format!("CYC: {}", r.cycles),
        format!(
            "Frame: {} Scanline: {} Pixel: {}",
            r.frame_count, r.scanline, r.pixel
        ),
        "---".to_string(),
    ];

    lines
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

/// Format a breakpoint as a short human-readable label for display in the UI.
/// Returns a fixed-width string with type tag and value, e.g. `"PC  $C000"`.
pub(crate) fn format_breakpoint_label(bp: &Breakpoint) -> String {
    match bp.kind {
        BreakpointKind::Pc(addr) => format!("PC  ${:04X}", addr),
        BreakpointKind::Cycle(n) => format!("CYC {}", n),
        BreakpointKind::Frame(n) => format!("FRM {}", n),
        BreakpointKind::WriteAddress(addr) => format!("WR  ${:04X}", addr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::{Config, Nes};
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
    fn test_layout_model_spans_full_available_area() {
        let display_size = [800.0, 600.0];
        let (title, pos, size) = layout_model(display_size);

        let margin = 10.0;
        assert_eq!(title, "CPU/PPU Data");
        assert_close(pos[0], margin);
        assert_close(pos[1], margin);
        assert_close(size[0], display_size[0] - 2.0 * margin);
        assert_close(size[1], display_size[1] - 2.0 * margin);
    }

    #[test]
    fn test_debugger_ui_font_scale_uses_single_shared_constant() {
        assert_close(debugger_ui_font_scale(), DEBUGGER_UI_FONT_SCALE);
        assert!(debugger_ui_font_scale() > 0.0);
    }

    #[test]
    fn test_code_view_target_width_scales_with_font_size() {
        let width_small = code_view_target_width_for_font_scale(0.8);
        let width_large = code_view_target_width_for_font_scale(1.2);

        assert!(width_large > width_small);
        assert_close(width_small, 311.2);
        assert_close(width_large, 448.8);
    }

    #[test]
    fn test_cpu_register_lines_render_expected_values() {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
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
        assert!(lines.iter().any(|l| l.contains("VEC:")
            && l.contains("NMI")
            && l.contains("RST")
            && l.contains("IRQ")));
        // PPU info integrated after separator
        assert!(lines.iter().any(|l| l == "---"));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Frame:") && l.contains("Scanline:") && l.contains("Pixel:"))
        );
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

        // With constrained width, keep space for right panel and collapse left panel.
        assert_close(layout.left_w, 0.0);
        assert_close(layout.gap, 8.0);
        assert_close(layout.right_w, 92.0);

        assert_close(layout.left_pos[0], 5.0);
        assert_close(layout.left_pos[1], 7.0);
        assert_close(layout.right_pos[0], 5.0 + 0.0 + 8.0);
        assert_close(layout.right_pos[1], 7.0);
    }

    #[test]
    fn test_debugger_ui_action_has_toggle_ppu_viewer_field() {
        let action = DebuggerUiAction::default();
        assert!(
            !action.toggle_ppu_viewer,
            "toggle_ppu_viewer should default to false"
        );
    }

    #[test]
    fn test_debugger_ui_action_has_run_to_next_scanline_field() {
        let action = DebuggerUiAction::default();
        assert!(
            !action.run_to_next_scanline,
            "run_to_next_scanline should default to false"
        );
    }

    // --- Breakpoint panel ---

    #[test]
    fn test_debugger_ui_action_has_add_breakpoint_field() {
        let action = DebuggerUiAction::default();
        assert!(
            action.add_breakpoint.is_none(),
            "add_breakpoint should default to None"
        );
    }

    #[test]
    fn test_debugger_ui_action_has_remove_breakpoint_field() {
        let action = DebuggerUiAction::default();
        assert!(
            action.remove_breakpoint.is_none(),
            "remove_breakpoint should default to None"
        );
    }

    #[test]
    fn test_debugger_ui_action_has_enable_breakpoint_field() {
        let action = DebuggerUiAction::default();
        assert!(
            action.enable_breakpoint.is_none(),
            "enable_breakpoint should default to None"
        );
    }

    #[test]
    fn test_debugger_ui_action_has_disable_breakpoint_field() {
        let action = DebuggerUiAction::default();
        assert!(
            action.disable_breakpoint.is_none(),
            "disable_breakpoint should default to None"
        );
    }

    #[test]
    fn test_format_breakpoint_label_pc() {
        use crate::debugging::breakpoints::{Breakpoint, BreakpointKind};
        let bp = Breakpoint::new(BreakpointKind::Pc(0xC000));
        assert_eq!(format_breakpoint_label(&bp), "PC  $C000");
    }

    #[test]
    fn test_format_breakpoint_label_cycle() {
        use crate::debugging::breakpoints::{Breakpoint, BreakpointKind};
        let bp = Breakpoint::new(BreakpointKind::Cycle(12345));
        assert_eq!(format_breakpoint_label(&bp), "CYC 12345");
    }

    #[test]
    fn test_format_breakpoint_label_frame() {
        use crate::debugging::breakpoints::{Breakpoint, BreakpointKind};
        let bp = Breakpoint::new(BreakpointKind::Frame(42));
        assert_eq!(format_breakpoint_label(&bp), "FRM 42");
    }

    #[test]
    fn test_format_breakpoint_label_write_address() {
        use crate::debugging::breakpoints::{Breakpoint, BreakpointKind};
        let bp = Breakpoint::new(BreakpointKind::WriteAddress(0x2006));
        assert_eq!(format_breakpoint_label(&bp), "WR  $2006");
    }

    #[test]
    fn test_parse_hexdump_base_input_hex_in_prg_range() {
        assert_eq!(parse_hexdump_base_input("8000"), Some(0x8000));
        assert_eq!(parse_hexdump_base_input("0xC123"), Some(0xC123));
        assert_eq!(parse_hexdump_base_input("7FFF"), None);
    }

    #[test]
    fn test_hexdump_go_submit_accepts_button_or_enter() {
        assert!(hexdump_go_submit_requested(true, false));
        assert!(hexdump_go_submit_requested(false, true));
        assert!(hexdump_go_submit_requested(true, true));
        assert!(!hexdump_go_submit_requested(false, false));
    }

    #[test]
    fn test_breakpoint_add_submit_accepts_button_or_enter() {
        assert!(breakpoint_add_submit_requested(true, false));
        assert!(breakpoint_add_submit_requested(false, true));
        assert!(breakpoint_add_submit_requested(true, true));
        assert!(!breakpoint_add_submit_requested(false, false));
    }

    #[test]
    fn test_validate_breakpoint_input_returns_error_for_invalid_value() {
        assert_eq!(
            validate_breakpoint_input(0, "XYZ"),
            Err("Invalid breakpoint value")
        );
    }

    #[test]
    fn test_validate_breakpoint_input_parses_frame_kind() {
        assert_eq!(
            validate_breakpoint_input(3, "42"),
            Ok(BreakpointKind::Frame(42))
        );
    }

    #[test]
    fn test_validate_hexdump_input_returns_error_for_invalid_address() {
        assert_eq!(
            validate_hexdump_input("7FFF"),
            Err("Invalid hexdump address")
        );
    }
}
