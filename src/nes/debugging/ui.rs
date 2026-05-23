#[cfg(feature = "native")]
use super::DebuggerSnapshot;
use crate::platform::debugging::breakpoints::{Breakpoint, BreakpointKind, BreakpointList};

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
    egui::FontId::monospace(13.0 * debugger_ui_font_scale())
}

#[cfg(feature = "native")]
fn debugger_window_frame(ui: &egui::Ui, alpha: f32) -> egui::Frame {
    let mut fill = ui.visuals().window_fill();
    fill = egui::Color32::from_rgba_unmultiplied(
        fill.r(),
        fill.g(),
        fill.b(),
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    );
    egui::Frame::window(ui.style()).fill(fill)
}

#[cfg(feature = "native")]
fn error_text(message: &str) -> egui::RichText {
    egui::RichText::new(message).color(egui::Color32::from_rgb(255, 102, 102))
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
    pub address_input: String,
    pub error: Option<String>,
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
    ("CPU/PPU Data", [margin, margin], [available_w, available_h])
}

#[cfg(feature = "native")]
pub fn render(
    ui: &mut egui::Ui,
    snapshot: &DebuggerSnapshot,
    alpha: f32,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    hexdump_state: &mut HexdumpUiState,
    watch_state: &mut WatchlistUiState,
) -> DebuggerUiAction {
    let mut action = DebuggerUiAction::default();
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
    // Layout: left code view, right column split into registers (top) + PRG hexdump (bottom)
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

#[cfg(feature = "native")]
fn render_cpu_window(
    ui: &mut egui::Ui,
    snapshot: &DebuggerSnapshot,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    hexdump_state: &mut HexdumpUiState,
    watch_state: &mut WatchlistUiState,
    action: &mut DebuggerUiAction,
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
fn render_breakpoint_panel(
    ui: &mut egui::Ui,
    breakpoints: &BreakpointList,
    add_state: &mut BreakpointAddUiState,
    action: &mut DebuggerUiAction,
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
    action: &mut DebuggerUiAction,
) {
    for (i, bp) in breakpoints.iter().enumerate() {
        ui.horizontal(|ui| {
            let mut enabled = bp.enabled;
            if ui.checkbox(&mut enabled, "").changed() {
                if enabled {
                    action.enable_breakpoint = Some(i);
                } else {
                    action.disable_breakpoint = Some(i);
                }
            }
            ui.label(format_breakpoint_label(bp));
            if ui.small_button("X").clicked() {
                action.remove_breakpoint = Some(i);
            }
        });
    }
}

#[cfg(feature = "native")]
fn render_add_breakpoint_row(
    ui: &mut egui::Ui,
    add_state: &mut BreakpointAddUiState,
    action: &mut DebuggerUiAction,
) {
    let kinds = ["PC", "Cycle", "Write", "Frame"];
    let mut enter_pressed = false;
    let mut add_clicked = false;
    ui.horizontal(|ui| {
        let selected_kind = kinds.get(add_state.kind_idx).copied().unwrap_or(kinds[0]);
        egui::ComboBox::from_id_salt("bp_kind")
            .selected_text(selected_kind)
            .width(60.0)
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
        add_clicked = ui.button("Add").clicked();
    });
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
        ui.label(error_text(message));
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

fn render_cpu_controls(ui: &mut egui::Ui, action: &mut DebuggerUiAction) {
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
        if ui.button("Run to next scanline").clicked() {
            action.run_to_next_scanline = true;
        }
        if ui.button("Run to NMI").clicked() {
            action.run_to_nmi = true;
        }
        if ui.button("Run to IRQ").clicked() {
            action.run_to_irq = true;
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

fn render_cpu_code_panel(ui: &mut egui::Ui, snapshot: &DebuggerSnapshot, size: [f32; 2]) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(egui::vec2(size[0], size[1]));
        apply_debugger_ui_font_scale(ui);
        ui.label("Code");
        ui.separator();

        let line_height =
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
        let disasm_height = code_panel_disasm_height_for_line_height(line_height)
            .min(ui.available_height().max(0.0));

        egui::ScrollArea::vertical()
            .id_salt("cpu_code_disasm")
            .max_height(disasm_height)
            .show(ui, |ui| {
                apply_debugger_ui_font_scale(ui);
                for line in &snapshot.cpu_disasm {
                    render_disasm_line(ui, line.addr, &line.bytes, &line.text, line.is_current);
                }
            });

        ui.separator();
        ui.label("Trace (recent 32)");
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("cpu_trace")
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
    let bytes = format_disasm_bytes(bytes);
    let text = format!("{addr:04X}: {bytes:<8} {text}");
    if is_current {
        let height = ui.text_style_height(&egui::TextStyle::Body);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 0.0, egui::Color32::WHITE);
        ui.painter().text(
            rect.left_top(),
            egui::Align2::LEFT_TOP,
            text,
            debugger_font_id(),
            egui::Color32::BLACK,
        );
    } else {
        ui.label(egui::RichText::new(text).monospace());
    }
}

fn render_cpu_right_panel(
    ui: &mut egui::Ui,
    snapshot: &DebuggerSnapshot,
    size: [f32; 2],
    gap: f32,
    hexdump_state: &mut HexdumpUiState,
    watch_state: &mut WatchlistUiState,
    action: &mut DebuggerUiAction,
) {
    ui.set_min_size(egui::vec2(size[0], size[1]));
    apply_debugger_ui_font_scale(ui);
    let right_avail = [ui.available_width(), ui.available_height()];
    let (regs_h, hex_h, oam_h, watch_h) = cpu_right_panel_split(right_avail, gap);

    render_panel(ui, [right_avail[0], regs_h], |ui| {
        render_cpu_registers(ui, snapshot);
    });

    ui.add_space(gap);

    render_panel(ui, [right_avail[0], hex_h], |ui| {
        render_hexdump_controls(ui, snapshot.prg_hexdump_base, hexdump_state, action);
        ui.label(format!(
            "PRG-ROM hexdump @ {:04X}",
            snapshot.prg_hexdump_base
        ));
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("cpu_prg_hex")
            .show(ui, |ui| {
                for line in
                    format_hexdump_lines(snapshot.prg_hexdump_base, &snapshot.prg_hexdump_bytes)
                {
                    ui.label(egui::RichText::new(line).monospace());
                }
            });
    });

    ui.add_space(gap);

    render_panel(ui, [right_avail[0], oam_h], |ui| {
        render_oam_panel(ui, snapshot);
    });

    ui.add_space(gap);

    render_panel(ui, [right_avail[0], watch_h], |ui| {
        render_memory_watch_panel(ui, snapshot, watch_state, action);
    });
}

#[cfg(feature = "native")]
fn render_panel(ui: &mut egui::Ui, size: [f32; 2], contents: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(size[0], size[1]),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_size(egui::vec2(size[0], size[1]));
                apply_debugger_ui_font_scale(ui);
                contents(ui);
            });
        },
    );
}

fn render_oam_panel(ui: &mut egui::Ui, snapshot: &DebuggerSnapshot) {
    ui.label("OAM (#  Y  tile attr  X)");
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("oam_panel")
        .show(ui, |ui| {
            for entry in format_oam_entries(&snapshot.oam) {
                ui.label(egui::RichText::new(entry).monospace());
            }
        });
}

fn render_hexdump_controls(
    ui: &mut egui::Ui,
    current_base: u16,
    hexdump_state: &mut HexdumpUiState,
    action: &mut DebuggerUiAction,
) {
    if hexdump_state.address_input.is_empty() {
        hexdump_state.address_input = format!("{:04X}", current_base);
    }

    let mut enter_pressed = false;
    let mut go_clicked = false;
    ui.horizontal(|ui| {
        if ui.small_button("-").clicked() {
            action.nudge_prg_hexdump_base_by_bytes = Some(-16);
            let next = normalize_hexdump_base_for_ui(current_base.saturating_sub(16));
            hexdump_state.address_input = format!("{next:04X}");
        }
        if ui.small_button("+").clicked() {
            action.nudge_prg_hexdump_base_by_bytes = Some(16);
            let next = normalize_hexdump_base_for_ui(current_base.saturating_add(16));
            hexdump_state.address_input = format!("{next:04X}");
        }

        let response = ui.add_sized(
            [90.0, 0.0],
            egui::TextEdit::singleline(&mut hexdump_state.address_input).desired_width(90.0),
        );
        enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        go_clicked = ui.small_button("Go").clicked();
    });
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
        ui.label(error_text(message));
    }
}

fn cpu_right_panel_split(avail: [f32; 2], gap: f32) -> (f32, f32, f32, f32) {
    let regs_h = avail[1] * 0.20;
    let oam_h = avail[1] * 0.30;
    let watch_h = avail[1] * 0.20;
    let hex_h = (avail[1] - regs_h - oam_h - watch_h - 3.0 * gap).max(0.0);
    (regs_h, hex_h, oam_h, watch_h)
}

fn render_memory_watch_panel(
    ui: &mut egui::Ui,
    snapshot: &DebuggerSnapshot,
    watch_state: &mut WatchlistUiState,
    action: &mut DebuggerUiAction,
) {
    egui::CollapsingHeader::new("Memory Watch")
        .id_salt("watch_header")
        .show(ui, |ui| {
            let mut add_enter_pressed = false;
            let mut add_clicked = false;
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [100.0, 0.0],
                    egui::TextEdit::singleline(&mut watch_state.add_input).desired_width(100.0),
                );
                add_enter_pressed =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                add_clicked = ui.small_button("Add").clicked();
            });
            if add_clicked || add_enter_pressed {
                match validate_watch_address_input(&watch_state.add_input) {
                    Ok(address) => {
                        action.add_watch_address = Some(address);
                        watch_state.add_input.clear();
                        watch_state.add_error = None;
                    }
                    Err(message) => {
                        watch_state.add_error = Some(message.to_string());
                    }
                }
            }

            if let Some(message) = watch_state.add_error.as_deref() {
                ui.label(error_text(message));
            }

            ensure_watch_row_state_capacity(watch_state, snapshot.watch_values.len());
            egui::ScrollArea::vertical()
                .id_salt("watch_rows")
                .show(ui, |ui| {
                    for (index, entry) in snapshot.watch_values.iter().enumerate() {
                        if watch_state.row_inputs[index].is_empty() {
                            watch_state.row_inputs[index] = format!("{:04X}", entry.address);
                        }

                        let mut edit_enter_pressed = false;
                        ui.horizontal(|ui| {
                            let response = ui.add_sized(
                                [70.0, 0.0],
                                egui::TextEdit::singleline(&mut watch_state.row_inputs[index])
                                    .desired_width(70.0),
                            );
                            edit_enter_pressed = response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter));

                            ui.label(format_watch_entry(entry.address, entry.value));
                            if ui.small_button("X").clicked() {
                                action.remove_watch_address = Some(index);
                            }
                        });
                        if edit_enter_pressed {
                            match validate_watch_address_input(&watch_state.row_inputs[index]) {
                                Ok(address) => {
                                    action.update_watch_address =
                                        Some(WatchAddressUpdate { index, address });
                                    watch_state.row_errors[index] = None;
                                }
                                Err(message) => {
                                    watch_state.row_errors[index] = Some(message.to_string());
                                }
                            }
                        }

                        if let Some(message) = watch_state.row_errors[index].as_deref() {
                            ui.label(error_text(message));
                        }
                    }
                });
        });
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

fn format_watch_entry(address: u16, value: u8) -> String {
    format!("${:04X}: ${:02X} ({})", address, value, value)
}

fn format_trace_entry(address: u16, bytes: &[u8], text: &str) -> String {
    let bytes_str = format_disasm_bytes(bytes);
    format!("{:04X}: {:<8} {}", address, bytes_str, text)
}

fn validate_watch_address_input(value: &str) -> Result<u16, &'static str> {
    parse_hex_u16(value).ok_or("Invalid watch address")
}

fn format_disasm_bytes(bytes: &[u8]) -> String {
    match bytes.len() {
        0 => String::new(),
        1 => format!("{:02X}", bytes[0]),
        2 => format!("{:02X} {:02X}", bytes[0], bytes[1]),
        _ => format!("{:02X} {:02X} {:02X}", bytes[0], bytes[1], bytes[2]),
    }
}

fn render_cpu_registers(ui: &mut egui::Ui, snapshot: &DebuggerSnapshot) {
    for line in cpu_register_lines(snapshot) {
        ui.label(egui::RichText::new(line).monospace());
    }
}

fn cpu_register_lines(snapshot: &DebuggerSnapshot) -> Vec<String> {
    let r = snapshot.cpu_regs;
    let interrupt = match r.interrupt {
        None => "-",
        Some(crate::nes::cpu::InterruptKind::Nmi) => "NMI",
        Some(crate::nes::cpu::InterruptKind::Irq) => "IRQ",
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
        BreakpointKind::GbInterrupt(kind) => format!("GB  {:?}", kind),
    }
}

/// Format a single OAM sprite entry as a human-readable string.
///
/// Format: `"NN: Y=YY tile=TT attr=AA X=XX"` where all values are hex and `NN` is the
/// zero-padded two-digit sprite index.
pub(crate) fn format_oam_entry(index: u8, y: u8, tile: u8, attrs: u8, x: u8) -> String {
    format!(
        "{:02}: Y={:02X} tile={:02X} attr={:02X} X={:02X}",
        index, y, tile, attrs, x
    )
}

/// Format all 64 OAM entries from raw 256-byte OAM data into strings.
pub(crate) fn format_oam_entries(oam: &[u8]) -> Vec<String> {
    (0..64)
        .map(|i| {
            let base = i * 4;
            let (y, tile, attrs, x) = (oam[base], oam[base + 1], oam[base + 2], oam[base + 3]);
            format_oam_entry(i as u8, y, tile, attrs, x)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::console::{Config, Nes};
    use crate::nes::debugging::snapshot;

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
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.cpu_mut().set_pc(0xC000);
        nes.cpu_mut().set_a_register(0x12);
        nes.cpu_mut().set_x(0x34);
        nes.cpu_mut().set_y(0x56);
        nes.cpu_mut().set_sp(0xFD);
        nes.cpu_mut().set_p(0b1010_0101);

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
        let avail = [100.0, 50.0];
        let layout = cpu_window_layout(avail);

        // With constrained width, keep space for right panel and collapse left panel.
        assert_close(layout.left_w, 0.0);
        assert_close(layout.gap, 8.0);
        assert_close(layout.right_w, 92.0);
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
        use crate::platform::debugging::breakpoints::{Breakpoint, BreakpointKind};
        let bp = Breakpoint::new(BreakpointKind::Pc(0xC000));
        assert_eq!(format_breakpoint_label(&bp), "PC  $C000");
    }

    #[test]
    fn test_format_breakpoint_label_cycle() {
        use crate::platform::debugging::breakpoints::{Breakpoint, BreakpointKind};
        let bp = Breakpoint::new(BreakpointKind::Cycle(12345));
        assert_eq!(format_breakpoint_label(&bp), "CYC 12345");
    }

    #[test]
    fn test_format_breakpoint_label_frame() {
        use crate::platform::debugging::breakpoints::{Breakpoint, BreakpointKind};
        let bp = Breakpoint::new(BreakpointKind::Frame(42));
        assert_eq!(format_breakpoint_label(&bp), "FRM 42");
    }

    #[test]
    fn test_format_breakpoint_label_write_address() {
        use crate::platform::debugging::breakpoints::{Breakpoint, BreakpointKind};
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

    #[test]
    fn test_format_oam_entry_contains_index_and_all_fields_in_hex() {
        let entry = format_oam_entry(5, 0x20, 0xAB, 0x03, 0x40);
        assert!(entry.contains("05"), "should contain sprite index 05");
        assert!(entry.contains("20"), "should contain Y=20");
        assert!(entry.contains("AB"), "should contain tile=AB");
        assert!(entry.contains("03"), "should contain attr=03");
        assert!(entry.contains("40"), "should contain X=40");
    }

    #[test]
    fn test_format_oam_entries_returns_64_lines_for_full_oam() {
        let oam = [0u8; 256];
        let entries = format_oam_entries(&oam);
        assert_eq!(entries.len(), 64);
    }

    #[test]
    fn test_format_oam_entries_reflects_sprite_data() {
        let mut oam = [0u8; 256];
        // Sprite 3: offset 12..16
        oam[12] = 0xBB; // Y
        oam[13] = 0xCC; // tile
        oam[14] = 0x01; // attrs
        oam[15] = 0xDD; // X
        let entries = format_oam_entries(&oam);
        let entry = &entries[3];
        assert!(entry.contains("BB"), "Y=BB");
        assert!(entry.contains("CC"), "tile=CC");
        assert!(entry.contains("01"), "attr=01");
        assert!(entry.contains("DD"), "X=DD");
    }

    #[test]
    fn test_format_watch_entry_renders_hex_and_decimal_value() {
        let row = format_watch_entry(0x0010, 0x7F);
        assert!(row.contains("$0010"), "address should be hex");
        assert!(row.contains("$7F"), "value should be hex");
        assert!(row.contains("127"), "value should include decimal");
    }

    #[test]
    fn test_validate_watch_address_input_accepts_hex_and_rejects_invalid() {
        assert_eq!(validate_watch_address_input("0010"), Ok(0x0010));
        assert_eq!(validate_watch_address_input("0x00FF"), Ok(0x00FF));
        assert_eq!(
            validate_watch_address_input("GG"),
            Err("Invalid watch address")
        );
    }

    #[test]
    fn test_format_trace_entry_renders_compact_trace_row() {
        let row = format_trace_entry(0xC000, &[0xA9, 0x01], "LDA #$01");
        assert!(row.contains("C000"));
        assert!(row.contains("A9 01"));
        assert!(row.contains("LDA #$01"));
    }

    #[test]
    fn test_code_panel_visible_lines_is_fixed_to_twenty() {
        assert_eq!(CODE_PANEL_VISIBLE_LINES, 20);
    }

    #[test]
    fn test_code_panel_disasm_height_matches_line_count() {
        assert_close(code_panel_disasm_height_for_line_height(2.5), 50.0);
    }
}
