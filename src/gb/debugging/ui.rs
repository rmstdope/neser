//! Game Boy debugger UI using egui.
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

#[cfg(feature = "native")]
fn apply_debugger_ui_font_scale(ui: &mut egui::Ui) {
    ui.style_mut().override_font_id = Some(debugger_font_id());
}

#[cfg(feature = "native")]
fn debugger_font_id() -> egui::FontId {
    egui::FontId::monospace(debugger_font_size())
}

#[cfg(feature = "native")]
fn debugger_window_frame(ui: &egui::Ui, alpha: f32) -> egui::Frame {
    let fill = ui.visuals().window_fill();
    egui::Frame::window(ui.style()).fill(egui::Color32::from_rgba_unmultiplied(
        fill.r(),
        fill.g(),
        fill.b(),
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    ))
}

#[cfg(feature = "native")]
fn error_text(message: &str) -> egui::RichText {
    egui::RichText::new(message).color(egui::Color32::from_rgb(255, 102, 102))
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
    ui: &mut egui::Ui,
    snapshot: &GbDebuggerSnapshot,
    alpha: f32,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    hexdump_state: &mut HexdumpUiState,
    watch_state: &mut WatchlistUiState,
) -> GbDebuggerUiAction {
    let mut action = GbDebuggerUiAction::default();
    let content_size = ui.ctx().content_rect().size();
    let (title, pos, size) = layout_model([content_size.x, content_size.y]);

    egui::Window::new(title)
        .fixed_pos(egui::pos2(pos[0], pos[1]))
        .fixed_size(egui::vec2(size[0], size[1]))
        .frame(debugger_window_frame(ui, alpha))
        .show(ui.ctx(), |ui| {
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
}

#[cfg(feature = "native")]
fn cpu_window_layout(avail: [f32; 2]) -> CpuWindowLayout {
    let gap = 8.0;
    let target_left_w = code_view_target_width_for_font_scale(debugger_ui_font_scale());
    let max_left_w = (avail[0] - gap - CPU_RIGHT_PANEL_MIN_WIDTH).max(0.0);
    let left_w = target_left_w.min(max_left_w);
    let right_w = (avail[0] - left_w - gap).max(0.0);

    CpuWindowLayout {
        left_w,
        right_w,
        gap,
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

fn debugger_font_size() -> f32 {
    13.0 * debugger_ui_font_scale()
}

fn submit_requested(clicked: bool, enter_pressed: bool) -> bool {
    clicked || enter_pressed
}

fn ensure_watch_row_state_capacity(watch_state: &mut WatchlistUiState, len: usize) {
    if watch_state.row_inputs.len() < len {
        watch_state.row_inputs.resize_with(len, String::new);
    } else if watch_state.row_inputs.len() > len {
        watch_state.row_inputs.truncate(len);
    }

    if watch_state.row_errors.len() < len {
        watch_state.row_errors.resize(len, None);
    } else if watch_state.row_errors.len() > len {
        watch_state.row_errors.truncate(len);
    }
}

fn format_trace_entry(address: u16, bytes: &[u8], text: &str) -> String {
    let bytes = format_disasm_bytes(bytes);
    format!("{address:04X}: {bytes:<8} {text}")
}

#[cfg(feature = "native")]
fn render_cpu_window(
    ui: &mut egui::Ui,
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

    let avail = [ui.available_width(), ui.available_height()];
    let layout = cpu_window_layout(avail);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(layout.left_w, avail[1]),
            egui::Layout::top_down(egui::Align::Min),
            |ui| render_cpu_code_panel(ui, snapshot, [layout.left_w, avail[1]]),
        );
        ui.add_space(layout.gap);
        ui.allocate_ui_with_layout(
            egui::vec2(layout.right_w, avail[1]),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                render_cpu_right_panel(
                    ui,
                    snapshot,
                    [layout.right_w, avail[1]],
                    layout.gap,
                    hexdump_state,
                    watch_state,
                    action,
                );
            },
        );
    });
}

#[cfg(feature = "native")]
fn render_cpu_controls(ui: &mut egui::Ui, action: &mut GbDebuggerUiAction) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Step over").clicked() {
            action.step_over = true;
        }
        if ui.button("Step into").clicked() {
            action.step_into = true;
        }
        if ui.button("Continue").clicked() {
            action.continue_run = true;
        }
        if ui.button("Run to next frame").clicked() {
            action.run_to_next_frame = true;
        }
        ui.add_enabled(false, egui::Button::new("Run to next scanline"));
        if ui.button("Run to VBlank").clicked() {
            action.run_to_vblank = true;
        }
        if ui.button("Run to STAT").clicked() {
            action.run_to_stat = true;
        }
        if ui.button("Run to Timer").clicked() {
            action.run_to_timer = true;
        }
        if ui.button("PPU Viewer").clicked() {
            action.toggle_ppu_viewer = true;
        }
        if ui.button("alpha-").clicked() {
            action.decrease_opacity = true;
        }
        if ui.button("alpha+").clicked() {
            action.increase_opacity = true;
        }
    });
}

#[cfg(feature = "native")]
fn render_breakpoint_panel(
    ui: &mut egui::Ui,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    action: &mut GbDebuggerUiAction,
) {
    egui::CollapsingHeader::new("Breakpoints")
        .id_salt("bp_header")
        .show(ui, |ui| {
            render_existing_breakpoints(ui, breakpoints, action);
            ui.separator();
            render_add_breakpoint_row(ui, add_state, action);
        });
}

#[cfg(feature = "native")]
fn render_existing_breakpoints(
    ui: &mut egui::Ui,
    breakpoints: &BreakpointList,
    action: &mut GbDebuggerUiAction,
) {
    for (index, bp) in breakpoints.iter().enumerate() {
        ui.horizontal(|ui| {
            let mut enabled = bp.enabled;
            if ui.checkbox(&mut enabled, "").changed() {
                if enabled {
                    action.enable_breakpoint = Some(index);
                } else {
                    action.disable_breakpoint = Some(index);
                }
            }
            ui.label(format!("{}", bp.kind));
            if ui.small_button("X").clicked() {
                action.remove_breakpoint = Some(index);
            }
        });
    }
}

#[cfg(feature = "native")]
fn render_add_breakpoint_row(
    ui: &mut egui::Ui,
    add_state: &mut BreakpointAddUiState,
    action: &mut GbDebuggerUiAction,
) {
    let kinds = ["PC", "Cycle", "Write", "Frame"];
    let mut enter_pressed = false;
    let mut add_clicked = false;
    ui.horizontal(|ui| {
        let selected_kind = kinds.get(add_state.kind_idx).copied().unwrap_or(kinds[0]);
        egui::ComboBox::from_id_salt("gb_bp_kind")
            .selected_text(selected_kind)
            .width(80.0)
            .show_ui(ui, |ui| {
                for (index, kind) in kinds.iter().enumerate() {
                    ui.selectable_value(&mut add_state.kind_idx, index, *kind);
                }
            });

        let response = ui.add_sized(
            [120.0, 0.0],
            egui::TextEdit::singleline(&mut add_state.value).desired_width(120.0),
        );
        enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        add_clicked = ui.small_button("Add").clicked();
    });

    if submit_requested(add_clicked, enter_pressed) {
        if let Some(kind) = parse_breakpoint_kind(add_state.kind_idx, &add_state.value) {
            action.add_breakpoint = Some(kind);
            add_state.value.clear();
            add_state.error = None;
        } else {
            add_state.error = Some("Invalid value".to_string());
        }
    }

    if let Some(ref err) = add_state.error {
        ui.label(error_text(err));
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
fn render_cpu_code_panel(ui: &mut egui::Ui, snapshot: &GbDebuggerSnapshot, size: [f32; 2]) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(egui::vec2(size[0], size[1]));
        apply_debugger_ui_font_scale(ui);
        ui.label("Disassembly");
        ui.separator();

        let line_height =
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
        let desired_height = code_panel_disasm_height_for_line_height(line_height);
        let disasm_height = desired_height.min(ui.available_height().max(0.0));

        egui::ScrollArea::vertical()
            .id_salt("gb_disasm_window")
            .max_height(disasm_height)
            .show(ui, |ui| {
                apply_debugger_ui_font_scale(ui);
                for line in &snapshot.cpu_disasm {
                    render_disasm_line(ui, line.addr, &line.bytes, &line.text, line.is_current);
                }
            });

        ui.separator();
        ui.label("CPU Trace");
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("gb_trace_window")
            .max_height(ui.available_height().max(0.0))
            .show(ui, |ui| {
                apply_debugger_ui_font_scale(ui);
                for line in &snapshot.recent_trace {
                    ui.label(
                        egui::RichText::new(format_trace_entry(line.addr, &line.bytes, &line.text))
                            .monospace(),
                    );
                }
            });
    });
}

#[cfg(feature = "native")]
fn render_disasm_line(ui: &mut egui::Ui, addr: u16, bytes: &[u8], text: &str, is_current: bool) {
    let text = format_trace_entry(addr, bytes, text);
    if is_current {
        let height = ui.text_style_height(&egui::TextStyle::Body);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(0, 255, 0, 77),
        );
        ui.painter().text(
            rect.left_top(),
            egui::Align2::LEFT_TOP,
            text,
            debugger_font_id(),
            ui.visuals().text_color(),
        );
    } else {
        ui.label(egui::RichText::new(text).monospace());
    }
}

fn format_disasm_bytes(bytes: &[u8]) -> String {
    match bytes.len() {
        0 => String::new(),
        1 => format!("{:02X}", bytes[0]),
        2 => format!("{:02X} {:02X}", bytes[0], bytes[1]),
        _ => format!("{:02X} {:02X} {:02X}", bytes[0], bytes[1], bytes[2]),
    }
}

#[cfg(feature = "native")]
fn render_cpu_right_panel(
    ui: &mut egui::Ui,
    snapshot: &GbDebuggerSnapshot,
    size: [f32; 2],
    gap: f32,
    hexdump_state: &mut HexdumpUiState,
    watch_state: &mut WatchlistUiState,
    action: &mut GbDebuggerUiAction,
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(egui::vec2(size[0], size[1]));
        apply_debugger_ui_font_scale(ui);
        render_cpu_registers(ui, snapshot);
        ui.add_space(gap);
        render_memory_watch(ui, snapshot, watch_state, action);
        ui.add_space(gap);
        render_hexdump_panel(ui, snapshot, hexdump_state, action, size);
    });
}

#[cfg(feature = "native")]
fn render_cpu_registers(ui: &mut egui::Ui, snapshot: &GbDebuggerSnapshot) {
    ui.label("Registers");
    ui.separator();

    let regs = &snapshot.cpu_regs;

    ui.label(egui::RichText::new(format!("AF: {:04X}  BC: {:04X}", regs.af, regs.bc)).monospace());
    ui.label(egui::RichText::new(format!("DE: {:04X}  HL: {:04X}", regs.de, regs.hl)).monospace());
    ui.label(egui::RichText::new(format!("SP: {:04X}  PC: {:04X}", regs.sp, regs.pc)).monospace());

    ui.add_space(4.0);

    let z = if regs.z_flag { 'Z' } else { '-' };
    let n = if regs.n_flag { 'N' } else { '-' };
    let h = if regs.h_flag { 'H' } else { '-' };
    let c = if regs.c_flag { 'C' } else { '-' };
    ui.label(egui::RichText::new(format!("Flags: {}{}{}{}", z, n, h, c)).monospace());

    ui.add_space(4.0);

    ui.label(egui::RichText::new(format!("Cycles: {}", regs.cycles)).monospace());
    ui.label(egui::RichText::new(format!("Frame: {}", regs.frame_count)).monospace());
    ui.label(
        egui::RichText::new(format!("Scanline: {}  Dot: {}", regs.scanline, regs.dot)).monospace(),
    );
    ui.label(egui::RichText::new(format!("PPU Mode: {}", regs.ppu_mode)).monospace());

    ui.add_space(4.0);

    let ime = if regs.ime { "ON" } else { "OFF" };
    let halted = if regs.halted { "HALT" } else { "RUN" };
    ui.label(egui::RichText::new(format!("IME: {}  State: {}", ime, halted)).monospace());
    ui.label(
        egui::RichText::new(format!("IE: {:02X}  IF: {:02X}", regs.ie, regs.if_reg)).monospace(),
    );
}

#[cfg(feature = "native")]
fn render_memory_watch(
    ui: &mut egui::Ui,
    snapshot: &GbDebuggerSnapshot,
    watch_state: &mut WatchlistUiState,
    action: &mut GbDebuggerUiAction,
) {
    ui.label("Memory Watch");
    ui.separator();

    ensure_watch_row_state_capacity(watch_state, snapshot.watch_values.len());

    egui::ScrollArea::vertical()
        .id_salt("gb_watch_rows")
        .max_height(120.0)
        .show(ui, |ui| {
            for (index, entry) in snapshot.watch_values.iter().enumerate() {
                if watch_state.row_inputs[index].is_empty() {
                    watch_state.row_inputs[index] = format!("{:04X}", entry.address);
                }

                let mut edit_enter_pressed = false;
                ui.horizontal(|ui| {
                    let response = ui.add_sized(
                        [80.0, 0.0],
                        egui::TextEdit::singleline(&mut watch_state.row_inputs[index])
                            .desired_width(80.0),
                    );
                    edit_enter_pressed =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    ui.label(egui::RichText::new(format!("= {:02X}", entry.value)).monospace());
                    if ui.small_button("X").clicked() {
                        action.remove_watch_address = Some(index);
                    }
                });

                if edit_enter_pressed {
                    if let Some(addr) = parse_address(&watch_state.row_inputs[index]) {
                        action.update_watch_address = Some(WatchAddressUpdate {
                            index,
                            address: addr,
                        });
                        watch_state.row_inputs[index] = format!("{addr:04X}");
                        watch_state.row_errors[index] = None;
                    } else {
                        watch_state.row_errors[index] = Some("Invalid".to_string());
                    }
                }

                if let Some(err) = watch_state.row_errors[index].as_deref() {
                    ui.label(error_text(err));
                }
            }
        });

    let mut add_enter_pressed = false;
    let mut add_clicked = false;
    ui.horizontal(|ui| {
        let response = ui.add_sized(
            [80.0, 0.0],
            egui::TextEdit::singleline(&mut watch_state.add_input).desired_width(80.0),
        );
        add_enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        add_clicked = ui.small_button("Add").clicked();
    });

    if submit_requested(add_clicked, add_enter_pressed) {
        if let Some(addr) = parse_address(&watch_state.add_input) {
            action.add_watch_address = Some(addr);
            watch_state.add_input.clear();
            watch_state.add_error = None;
        } else {
            watch_state.add_error = Some("Invalid".to_string());
        }
    }

    if let Some(ref err) = watch_state.add_error {
        ui.label(error_text(err));
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
    ui: &mut egui::Ui,
    snapshot: &GbDebuggerSnapshot,
    hexdump_state: &mut HexdumpUiState,
    action: &mut GbDebuggerUiAction,
    _size: [f32; 2],
) {
    egui::CollapsingHeader::new("WRAM Hexdump")
        .id_salt("wram_header")
        .show(ui, |ui| {
            render_hexdump_controls(
                ui,
                "WRAM",
                snapshot.wram_hexdump_base,
                &mut hexdump_state.wram_address_input,
                &mut hexdump_state.wram_error,
                |addr| action.set_wram_hexdump_base = Some(addr),
                |delta| action.nudge_wram_hexdump_base_by_bytes = Some(delta),
            );
            render_hexdump_bytes(ui, snapshot.wram_hexdump_base, &snapshot.wram_hexdump_bytes);
        });

    ui.add_space(8.0);

    egui::CollapsingHeader::new("VRAM Hexdump")
        .id_salt("vram_header")
        .show(ui, |ui| {
            render_hexdump_controls(
                ui,
                "VRAM",
                snapshot.vram_hexdump_base,
                &mut hexdump_state.vram_address_input,
                &mut hexdump_state.vram_error,
                |addr| action.set_vram_hexdump_base = Some(addr),
                |delta| action.nudge_vram_hexdump_base_by_bytes = Some(delta),
            );
            render_hexdump_bytes(ui, snapshot.vram_hexdump_base, &snapshot.vram_hexdump_bytes);
        });
}

#[cfg(feature = "native")]
fn render_hexdump_controls(
    ui: &mut egui::Ui,
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

    let mut enter_pressed = false;
    let mut minus_clicked = false;
    let mut plus_clicked = false;
    ui.horizontal(|ui| {
        ui.label(format!("{label} Base:"));
        let response = ui.add_sized(
            [80.0, 0.0],
            egui::TextEdit::singleline(input).desired_width(80.0),
        );
        enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        minus_clicked = ui.small_button("-16").clicked();
        plus_clicked = ui.small_button("+16").clicked();
    });

    if enter_pressed {
        if let Some(addr) = parse_address(input) {
            set_action(addr);
            *input = format!("{addr:04X}");
            *error = None;
        } else {
            *error = Some("Invalid".to_string());
        }
    }
    if minus_clicked {
        nudge_action(-16);
    }
    if plus_clicked {
        nudge_action(16);
    }

    if let Some(err) = error {
        ui.label(error_text(err));
    }
}

#[cfg(feature = "native")]
fn render_hexdump_bytes(ui: &mut egui::Ui, base: u16, bytes: &[u8]) {
    const BYTES_PER_ROW: usize = 16;
    egui::ScrollArea::vertical()
        .id_salt((base, bytes.len()))
        .max_height(140.0)
        .show(ui, |ui| {
            for (row_idx, chunk) in bytes.chunks(BYTES_PER_ROW).enumerate() {
                let addr = base.wrapping_add((row_idx * BYTES_PER_ROW) as u16);
                let hex_str = chunk
                    .iter()
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                ui.label(egui::RichText::new(format!("{addr:04X}: {hex_str}")).monospace());
            }
        });
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

    #[test]
    fn debugger_font_size_uses_shared_scale() {
        assert_eq!(debugger_font_size(), 13.0 * DEBUGGER_UI_FONT_SCALE);
    }

    #[test]
    fn submit_requested_accepts_button_or_enter() {
        assert!(submit_requested(true, false));
        assert!(submit_requested(false, true));
        assert!(!submit_requested(false, false));
    }

    #[test]
    fn ensure_watch_row_state_capacity_grows_and_truncates_inputs_and_errors() {
        let mut state = WatchlistUiState::default();

        ensure_watch_row_state_capacity(&mut state, 2);
        assert_eq!(state.row_inputs, vec![String::new(), String::new()]);
        assert_eq!(state.row_errors, vec![None, None]);

        state.row_inputs[0] = "C000".to_string();
        state.row_errors[1] = Some("Invalid".to_string());

        ensure_watch_row_state_capacity(&mut state, 1);
        assert_eq!(state.row_inputs, vec!["C000".to_string()]);
        assert_eq!(state.row_errors, vec![None]);
    }

    #[test]
    fn format_trace_entry_renders_address_bytes_and_text() {
        assert_eq!(
            format_trace_entry(0x0150, &[0x3E, 0x42], "LD A,$42"),
            "0150: 3E 42    LD A,$42"
        );
    }
}
