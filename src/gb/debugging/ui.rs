//! Game Boy debugger UI using ImGui.
//!
//! Renders CPU registers, disassembly, breakpoints, memory hexdumps, and watch list.
//! Follows the same pattern as the NES debugger UI.

#[cfg(feature = "native")]
use super::GbDebuggerSnapshot;
use crate::platform::debugging::breakpoints::{BreakpointKind, BreakpointList};

const DEBUGGER_OUTER_MARGIN: f32 = 10.0;
pub(crate) const DEBUGGER_UI_FONT_SCALE: f32 = 0.85;
const CODE_VIEW_LONGEST_LINE_CHARS: f32 = 28.0;
const CODE_VIEW_PREFIX_CHARS: f32 = 15.0;
const CODE_VIEW_CHAR_WIDTH_AT_SCALE_ONE: f32 = 8.0;
const CODE_VIEW_CHILD_WINDOW_HORIZONTAL_PADDING: f32 = 16.0;
const CODE_VIEW_SCROLLBAR_GUTTER: f32 = 16.0;
const CODE_VIEW_HIGHLIGHT_OVERDRAW_MARGIN: f32 = 4.0;
const CPU_RIGHT_PANEL_MIN_WIDTH: f32 = 320.0;
const CODE_PANEL_VISIBLE_LINES: usize = 20;

fn debugger_ui_font_scale() -> f32 {
    DEBUGGER_UI_FONT_SCALE
}

fn apply_debugger_ui_font_scale(ui: &imgui::Ui) {
    ui.set_window_font_scale(debugger_ui_font_scale());
}

/// Actions that can be triggered from the debugger UI.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GbDebuggerUiAction {
    pub step_over: bool,
    pub step_into: bool,
    pub continue_run: bool,
    pub run_to_next_frame: bool,
    pub run_to_next_scanline: bool,
    pub run_to_vblank: bool,
    pub run_to_stat: bool,
    pub run_to_timer: bool,
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
    /// Set WRAM hexdump base address.
    pub set_wram_hexdump_base: Option<u16>,
    /// Move WRAM hexdump base by byte delta.
    pub nudge_wram_hexdump_base_by_bytes: Option<i16>,
    /// Set VRAM hexdump base address.
    pub set_vram_hexdump_base: Option<u16>,
    /// Move VRAM hexdump base by byte delta.
    pub nudge_vram_hexdump_base_by_bytes: Option<i16>,
    /// Add a memory watch address.
    pub add_watch_address: Option<u16>,
    /// Remove memory watch entry by index.
    pub remove_watch_address: Option<usize>,
    /// Update memory watch entry address by index.
    pub update_watch_address: Option<WatchAddressUpdate>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WatchAddressUpdate {
    pub index: usize,
    pub address: u16,
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
    pub wram_address_input: String,
    pub wram_error: Option<String>,
    pub vram_address_input: String,
    pub vram_error: Option<String>,
}

#[derive(Debug, Default)]
pub struct WatchlistUiState {
    pub add_input: String,
    pub add_error: Option<String>,
    pub row_inputs: Vec<String>,
    pub row_errors: Vec<Option<String>>,
}

pub fn layout_model(display_size: [f32; 2]) -> (&'static str, [f32; 2], [f32; 2]) {
    let [display_w, display_h] = display_size;
    let margin = DEBUGGER_OUTER_MARGIN;
    let available_w = (display_w - 2.0 * margin).max(0.0);
    let available_h = (display_h - 2.0 * margin).max(0.0);
    (
        "GB CPU/PPU Data",
        [margin, margin],
        [available_w, available_h],
    )
}

#[cfg(feature = "native")]
pub fn render(
    ui: &imgui::Ui,
    snapshot: &GbDebuggerSnapshot,
    alpha: f32,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    hexdump_state: &mut HexdumpUiState,
    watch_state: &mut WatchlistUiState,
) -> GbDebuggerUiAction {
    let mut action = GbDebuggerUiAction::default();
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
                watch_state,
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

#[cfg(feature = "native")]
fn cpu_window_layout(avail: [f32; 2], cursor: [f32; 2]) -> CpuWindowLayout {
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

fn code_panel_disasm_height_for_line_height(line_height_with_spacing: f32) -> f32 {
    line_height_with_spacing * CODE_PANEL_VISIBLE_LINES as f32
}

#[cfg(feature = "native")]
fn render_cpu_window(
    ui: &imgui::Ui,
    snapshot: &GbDebuggerSnapshot,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    hexdump_state: &mut HexdumpUiState,
    watch_state: &mut WatchlistUiState,
    action: &mut GbDebuggerUiAction,
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
        watch_state,
        action,
    );
}

#[cfg(feature = "native")]
fn render_cpu_controls(ui: &imgui::Ui, action: &mut GbDebuggerUiAction) {
    if ui.small_button("Step") {
        action.step_into = true;
    }
    ui.same_line();
    if ui.small_button("Next") {
        action.step_over = true;
    }
    ui.same_line();
    if ui.small_button("Continue") {
        action.continue_run = true;
    }
    ui.same_line();
    ui.text("│");
    ui.same_line();
    if ui.small_button("Frame") {
        action.run_to_next_frame = true;
    }
    ui.same_line();
    ui.text_disabled("Scanline (unimpl)");
    ui.same_line();
    ui.text("│");
    ui.same_line();
    if ui.small_button("VBlank") {
        action.run_to_vblank = true;
    }
    ui.same_line();
    if ui.small_button("STAT") {
        action.run_to_stat = true;
    }
    ui.same_line();
    if ui.small_button("Timer") {
        action.run_to_timer = true;
    }
    ui.same_line();
    ui.text("│");
    ui.same_line();
    if ui.small_button("PPU") {
        action.toggle_ppu_viewer = true;
    }
    ui.same_line();
    if ui.small_button("+α") {
        action.increase_opacity = true;
    }
    ui.same_line();
    if ui.small_button("−α") {
        action.decrease_opacity = true;
    }
}

#[cfg(feature = "native")]
fn render_breakpoint_panel(
    ui: &imgui::Ui,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    action: &mut GbDebuggerUiAction,
) {
    if !ui.collapsing_header("Breakpoints##bp_header", imgui::TreeNodeFlags::empty()) {
        return;
    }

    render_existing_breakpoints(ui, breakpoints, action);
    ui.separator();
    render_add_breakpoint_row(ui, add_state, action);
}

#[cfg(feature = "native")]
fn render_existing_breakpoints(
    ui: &imgui::Ui,
    breakpoints: &BreakpointList,
    action: &mut GbDebuggerUiAction,
) {
    for (index, bp) in breakpoints.iter().enumerate() {
        let mut enabled = bp.enabled;
        if ui.checkbox(format!("##bp_enable_{}", index), &mut enabled) {
            if enabled {
                action.enable_breakpoint = Some(index);
            } else {
                action.disable_breakpoint = Some(index);
            }
        }
        ui.same_line();
        ui.text(format!("{}", bp.kind));
        ui.same_line();
        if ui.small_button(format!("X##bp_remove_{}", index)) {
            action.remove_breakpoint = Some(index);
        }
    }
}

#[cfg(feature = "native")]
fn render_add_breakpoint_row(
    ui: &imgui::Ui,
    add_state: &mut BreakpointAddUiState,
    action: &mut GbDebuggerUiAction,
) {
    let kinds = ["PC", "Cycle", "Write", "Frame"];
    ui.set_next_item_width(80.0);
    ui.combo_simple_string("##bp_kind", &mut add_state.kind_idx, &kinds);

    ui.same_line();
    ui.set_next_item_width(120.0);
    if ui
        .input_text("##bp_value", &mut add_state.value)
        .flags(imgui::InputTextFlags::ENTER_RETURNS_TRUE)
        .build()
    {
        if let Some(kind) = parse_breakpoint_kind(add_state.kind_idx, &add_state.value) {
            action.add_breakpoint = Some(kind);
            add_state.value.clear();
            add_state.error = None;
        } else {
            add_state.error = Some("Invalid value".to_string());
        }
    }

    ui.same_line();
    if ui.small_button("Add##bp_add") {
        if let Some(kind) = parse_breakpoint_kind(add_state.kind_idx, &add_state.value) {
            action.add_breakpoint = Some(kind);
            add_state.value.clear();
            add_state.error = None;
        } else {
            add_state.error = Some("Invalid value".to_string());
        }
    }

    if let Some(ref err) = add_state.error {
        ui.same_line();
        ui.text_colored([1.0, 0.0, 0.0, 1.0], err);
    }
}

fn parse_breakpoint_kind(kind_idx: usize, value_str: &str) -> Option<BreakpointKind> {
    let value_str = value_str.trim();
    match kind_idx {
        0 => {
            // PC - parse as hex by default (addresses are displayed in hex)
            let value = if value_str.starts_with("0x") || value_str.starts_with("0X") {
                u16::from_str_radix(&value_str[2..], 16).ok()?
            } else {
                // Accept plain hex like "C000" or decimal like "49152"
                u16::from_str_radix(value_str, 16)
                    .or_else(|_| value_str.parse::<u16>())
                    .ok()?
            };
            Some(BreakpointKind::Pc(value))
        }
        1 => {
            // Cycle
            Some(BreakpointKind::Cycle(value_str.parse().ok()?))
        }
        2 => {
            // Write - parse as hex by default (addresses are displayed in hex)
            let value = if value_str.starts_with("0x") || value_str.starts_with("0X") {
                u16::from_str_radix(&value_str[2..], 16).ok()?
            } else {
                // Accept plain hex like "C000" or decimal like "60000"
                u16::from_str_radix(value_str, 16)
                    .or_else(|_| value_str.parse::<u16>())
                    .ok()?
            };
            Some(BreakpointKind::WriteAddress(value))
        }
        3 => {
            // Frame
            Some(BreakpointKind::Frame(value_str.parse().ok()?))
        }
        _ => None,
    }
}

#[cfg(feature = "native")]
fn render_cpu_code_panel(ui: &imgui::Ui, snapshot: &GbDebuggerSnapshot, size: [f32; 2]) {
    ui.group(|| {
        ui.text("Disassembly");
        ui.separator();

        let line_height = ui.text_line_height_with_spacing();
        let desired_height = code_panel_disasm_height_for_line_height(line_height);
        let disasm_height = desired_height
            .min(size[1] - ui.cursor_pos()[1] - 100.0)
            .max(100.0);

        ui.child_window("disasm_window")
            .size([size[0], disasm_height])
            .build(|| {
                for line in &snapshot.cpu_disasm {
                    let is_current = line.is_current;
                    if is_current {
                        let draw_list = ui.get_window_draw_list();
                        let pos = ui.cursor_screen_pos();
                        let size = [size[0], line_height];
                        draw_list
                            .add_rect(
                                pos,
                                [pos[0] + size[0], pos[1] + size[1]],
                                [0.0, 1.0, 0.0, 0.3],
                            )
                            .filled(true)
                            .build();
                    }
                    ui.text(&line.text);
                }
            });

        ui.dummy([0.0, 8.0]);
        ui.text("CPU Trace");
        ui.separator();

        let trace_height = (size[1] - ui.cursor_pos()[1]).max(50.0);
        ui.child_window("trace_window")
            .size([size[0], trace_height])
            .build(|| {
                for line in &snapshot.recent_trace {
                    ui.text(&line.text);
                }
            });
    });
}

#[cfg(feature = "native")]
fn render_cpu_right_panel(
    ui: &imgui::Ui,
    snapshot: &GbDebuggerSnapshot,
    size: [f32; 2],
    _gap: f32,
    hexdump_state: &mut HexdumpUiState,
    watch_state: &mut WatchlistUiState,
    action: &mut GbDebuggerUiAction,
) {
    ui.group(|| {
        render_cpu_registers(ui, snapshot);
        ui.dummy([0.0, 8.0]);
        render_memory_watch(ui, snapshot, watch_state, action);
        ui.dummy([0.0, 8.0]);
        render_hexdump_panel(ui, snapshot, hexdump_state, action, size);
    });
}

#[cfg(feature = "native")]
fn render_cpu_registers(ui: &imgui::Ui, snapshot: &GbDebuggerSnapshot) {
    ui.text("Registers");
    ui.separator();

    let regs = &snapshot.cpu_regs;

    // AF, BC, DE, HL
    ui.text(format!("AF: {:04X}  BC: {:04X}", regs.af, regs.bc));
    ui.text(format!("DE: {:04X}  HL: {:04X}", regs.de, regs.hl));
    ui.text(format!("SP: {:04X}  PC: {:04X}", regs.sp, regs.pc));

    ui.dummy([0.0, 4.0]);

    // Flags
    let z = if regs.z_flag { 'Z' } else { '-' };
    let n = if regs.n_flag { 'N' } else { '-' };
    let h = if regs.h_flag { 'H' } else { '-' };
    let c = if regs.c_flag { 'C' } else { '-' };
    ui.text(format!("Flags: {}{}{}{}", z, n, h, c));

    ui.dummy([0.0, 4.0]);

    // Timing
    ui.text(format!("Cycles: {}", regs.cycles));
    ui.text(format!("Frame: {}", regs.frame_count));
    ui.text(format!("Scanline: {}  Dot: {}", regs.scanline, regs.dot));
    ui.text(format!("PPU Mode: {}", regs.ppu_mode));

    ui.dummy([0.0, 4.0]);

    // CPU state
    let ime = if regs.ime { "ON" } else { "OFF" };
    let halted = if regs.halted { "HALT" } else { "RUN" };
    ui.text(format!("IME: {}  State: {}", ime, halted));
    ui.text(format!("IE: {:02X}  IF: {:02X}", regs.ie, regs.if_reg));
}

#[cfg(feature = "native")]
fn render_memory_watch(
    ui: &imgui::Ui,
    snapshot: &GbDebuggerSnapshot,
    watch_state: &mut WatchlistUiState,
    action: &mut GbDebuggerUiAction,
) {
    ui.text("Memory Watch");
    ui.separator();

    // Ensure row_inputs and row_errors match the watch list size
    while watch_state.row_inputs.len() < snapshot.watch_values.len() {
        watch_state.row_inputs.push(String::new());
        watch_state.row_errors.push(None);
    }
    while watch_state.row_inputs.len() > snapshot.watch_values.len() {
        watch_state.row_inputs.pop();
        watch_state.row_errors.pop();
    }

    // Display existing watch entries
    for (index, entry) in snapshot.watch_values.iter().enumerate() {
        if watch_state.row_inputs[index].is_empty() {
            watch_state.row_inputs[index] = format!("{:04X}", entry.address);
        }

        ui.set_next_item_width(80.0);
        if ui
            .input_text(
                &format!("##watch_{}", index),
                &mut watch_state.row_inputs[index],
            )
            .flags(imgui::InputTextFlags::ENTER_RETURNS_TRUE)
            .build()
        {
            if let Some(addr) = parse_address(&watch_state.row_inputs[index]) {
                action.update_watch_address = Some(WatchAddressUpdate {
                    index,
                    address: addr,
                });
                watch_state.row_inputs[index] = format!("{:04X}", addr);
                watch_state.row_errors[index] = None;
            } else {
                watch_state.row_errors[index] = Some("Invalid".to_string());
            }
        }

        ui.same_line();
        ui.text(format!("= {:02X}", entry.value));
        ui.same_line();
        if ui.small_button(format!("X##watch_rm_{}", index)) {
            action.remove_watch_address = Some(index);
        }

        if let Some(ref err) = watch_state.row_errors[index] {
            ui.same_line();
            ui.text_colored([1.0, 0.0, 0.0, 1.0], err);
        }
    }

    // Add new watch entry
    ui.set_next_item_width(80.0);
    if ui
        .input_text("##watch_add", &mut watch_state.add_input)
        .flags(imgui::InputTextFlags::ENTER_RETURNS_TRUE)
        .build()
    {
        if let Some(addr) = parse_address(&watch_state.add_input) {
            action.add_watch_address = Some(addr);
            watch_state.add_input.clear();
            watch_state.add_error = None;
        } else {
            watch_state.add_error = Some("Invalid".to_string());
        }
    }

    ui.same_line();
    if ui.small_button("Add##watch_add_btn") {
        if let Some(addr) = parse_address(&watch_state.add_input) {
            action.add_watch_address = Some(addr);
            watch_state.add_input.clear();
            watch_state.add_error = None;
        } else {
            watch_state.add_error = Some("Invalid".to_string());
        }
    }

    if let Some(ref err) = watch_state.add_error {
        ui.same_line();
        ui.text_colored([1.0, 0.0, 0.0, 1.0], err);
    }
}

fn parse_address(s: &str) -> Option<u16> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        u16::from_str_radix(&s[2..], 16).ok()
    } else {
        s.parse::<u16>().ok()
    }
}

#[cfg(feature = "native")]
fn render_hexdump_panel(
    ui: &imgui::Ui,
    snapshot: &GbDebuggerSnapshot,
    hexdump_state: &mut HexdumpUiState,
    action: &mut GbDebuggerUiAction,
    size: [f32; 2],
) {
    if ui.collapsing_header("WRAM Hexdump##wram_header", imgui::TreeNodeFlags::empty()) {
        render_hexdump_controls(
            ui,
            "WRAM",
            snapshot.wram_hexdump_base,
            &mut hexdump_state.wram_address_input,
            &mut hexdump_state.wram_error,
            |addr| action.set_wram_hexdump_base = Some(addr),
            |delta| action.nudge_wram_hexdump_base_by_bytes = Some(delta),
        );

        render_hexdump_bytes(
            ui,
            snapshot.wram_hexdump_base,
            &snapshot.wram_hexdump_bytes,
            size[0],
        );

        ui.dummy([0.0, 8.0]);
    }

    if ui.collapsing_header("VRAM Hexdump##vram_header", imgui::TreeNodeFlags::empty()) {
        render_hexdump_controls(
            ui,
            "VRAM",
            snapshot.vram_hexdump_base,
            &mut hexdump_state.vram_address_input,
            &mut hexdump_state.vram_error,
            |addr| action.set_vram_hexdump_base = Some(addr),
            |delta| action.nudge_vram_hexdump_base_by_bytes = Some(delta),
        );

        render_hexdump_bytes(
            ui,
            snapshot.vram_hexdump_base,
            &snapshot.vram_hexdump_bytes,
            size[0],
        );
    }
}

#[cfg(feature = "native")]
fn render_hexdump_controls(
    ui: &imgui::Ui,
    label: &str,
    current_base: u16,
    input: &mut String,
    error: &mut Option<String>,
    set_action: impl FnOnce(u16),
    mut nudge_action: impl FnMut(i16),
) {
    if input.is_empty() {
        *input = format!("{:04X}", current_base);
    }

    ui.text(format!("{} Base:", label));
    ui.same_line();
    ui.set_next_item_width(80.0);
    if ui
        .input_text(&format!("##{}_base", label.to_lowercase()), input)
        .flags(imgui::InputTextFlags::ENTER_RETURNS_TRUE)
        .build()
    {
        if let Some(addr) = parse_address(input) {
            set_action(addr);
            *input = format!("{:04X}", addr);
            *error = None;
        } else {
            *error = Some("Invalid".to_string());
        }
    }

    ui.same_line();
    if ui.small_button(format!("−16##{}_minus", label.to_lowercase())) {
        nudge_action(-16);
    }
    ui.same_line();
    if ui.small_button(format!("+16##{}_plus", label.to_lowercase())) {
        nudge_action(16);
    }

    if let Some(err) = error {
        ui.same_line();
        ui.text_colored([1.0, 0.0, 0.0, 1.0], err);
    }
}

#[cfg(feature = "native")]
fn render_hexdump_bytes(ui: &imgui::Ui, base: u16, bytes: &[u8], _width: f32) {
    const BYTES_PER_ROW: usize = 16;
    for (row_idx, chunk) in bytes.chunks(BYTES_PER_ROW).enumerate() {
        let addr = base.wrapping_add((row_idx * BYTES_PER_ROW) as u16);
        let hex_str = chunk
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        ui.text(format!("{:04X}: {}", addr, hex_str));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_model_applies_margins() {
        let (title, pos, size) = layout_model([800.0, 600.0]);
        assert_eq!(title, "GB CPU/PPU Data");
        assert_eq!(pos, [10.0, 10.0]); // DEBUGGER_OUTER_MARGIN
        assert_eq!(size, [780.0, 580.0]); // display - 2*margin
    }

    #[test]
    fn layout_model_handles_small_display() {
        let (title, pos, size) = layout_model([10.0, 10.0]);
        assert_eq!(title, "GB CPU/PPU Data");
        assert_eq!(pos, [10.0, 10.0]);
        assert_eq!(size, [0.0, 0.0]); // clamped to 0
    }

    #[test]
    fn parse_address_accepts_decimal_and_hex() {
        assert_eq!(parse_address("4660"), Some(4660));
        assert_eq!(parse_address("0x1234"), Some(0x1234));
        assert_eq!(parse_address("0X00ff"), Some(0x00ff));
        assert_eq!(parse_address("  0x00a0  "), Some(0x00a0));
    }

    #[test]
    fn parse_address_rejects_invalid_input() {
        assert_eq!(parse_address(""), None);
        assert_eq!(parse_address("xyz"), None);
        assert_eq!(parse_address("0x"), None);
        assert_eq!(parse_address("70000"), None);
        assert_eq!(parse_address("0x10000"), None);
    }

    #[test]
    fn parse_breakpoint_kind_pc_accepts_hex_and_decimal() {
        assert_eq!(
            parse_breakpoint_kind(0, "0x1234"),
            Some(BreakpointKind::Pc(0x1234))
        );
        // Without 0x prefix, parse as hex by default (per review comment)
        assert_eq!(
            parse_breakpoint_kind(0, "1234"),
            Some(BreakpointKind::Pc(0x1234))
        );
        assert_eq!(
            parse_breakpoint_kind(0, "  0x00FF  "),
            Some(BreakpointKind::Pc(0x00FF))
        );
        // Pure hex like "C000" works without 0x prefix
        assert_eq!(
            parse_breakpoint_kind(0, "C000"),
            Some(BreakpointKind::Pc(0xC000))
        );
        // Decimal values that don't look like hex still work as fallback
        assert_eq!(
            parse_breakpoint_kind(0, "60000"),
            Some(BreakpointKind::Pc(60000))
        );
    }

    #[test]
    fn parse_breakpoint_kind_cycle() {
        assert_eq!(
            parse_breakpoint_kind(1, "12345"),
            Some(BreakpointKind::Cycle(12345))
        );
        assert_eq!(
            parse_breakpoint_kind(1, "  999  "),
            Some(BreakpointKind::Cycle(999))
        );
    }

    #[test]
    fn parse_breakpoint_kind_write() {
        assert_eq!(
            parse_breakpoint_kind(2, "0xC000"),
            Some(BreakpointKind::WriteAddress(0xC000))
        );
        // Without 0x prefix, parse as hex by default (per review comment)
        assert_eq!(
            parse_breakpoint_kind(2, "C000"),
            Some(BreakpointKind::WriteAddress(0xC000))
        );
        // Decimal values that don't look like hex still work as fallback
        assert_eq!(
            parse_breakpoint_kind(2, "60000"),
            Some(BreakpointKind::WriteAddress(60000))
        );
    }

    #[test]
    fn parse_breakpoint_kind_frame() {
        assert_eq!(
            parse_breakpoint_kind(3, "100"),
            Some(BreakpointKind::Frame(100))
        );
    }

    #[test]
    fn parse_breakpoint_kind_rejects_invalid_input() {
        assert_eq!(parse_breakpoint_kind(0, ""), None);
        assert_eq!(parse_breakpoint_kind(0, "invalid"), None);
        assert_eq!(parse_breakpoint_kind(1, "not_a_number"), None);
        assert_eq!(parse_breakpoint_kind(4, "100"), None); // Invalid kind index
    }

    #[test]
    fn debugger_ui_action_default_has_no_pending_actions() {
        let action = GbDebuggerUiAction::default();

        assert!(!action.step_over);
        assert!(!action.step_into);
        assert!(!action.continue_run);
        assert_eq!(action.add_watch_address, None);
        assert_eq!(action.set_wram_hexdump_base, None);
        assert_eq!(action.nudge_wram_hexdump_base_by_bytes, None);
        assert_eq!(action.set_vram_hexdump_base, None);
        assert_eq!(action.nudge_vram_hexdump_base_by_bytes, None);
        assert_eq!(action.add_breakpoint, None);
        assert_eq!(action.remove_breakpoint, None);
    }
}
