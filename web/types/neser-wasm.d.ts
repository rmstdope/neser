/* eslint-disable @typescript-eslint/no-empty-object-type */
/**
 * Type declarations for the neser WASM module.
 *
 * This file is maintained manually to provide TypeScript types for the
 * wasm-bindgen generated API. The actual JavaScript module is built by
 * wasm-pack and lives in web/pkg/ (gitignored).
 *
 * When new methods are added to WasmNes in Rust, update this file to match.
 */

// Ambient module declaration for the wasm-bindgen output.
// The specifier must match the import paths used in source files
// (e.g. `import init, { WasmNes } from "../pkg/neser"`).
// With moduleResolution:"bundler" TypeScript resolves relative imports
// against the importing file, so a non-relative ambient name is needed.
// We use a path-mapping-style name and rely on web/pkg/neser.d.ts at
// runtime; but when that file is absent (fresh clone), this fallback kicks in.

declare module "*/pkg/neser" {
    /**
     * Provides a minimal WASM bridge for running the emulator in the browser.
     */
    export class WasmNes {
        free(): void;
        [Symbol.dispose](): void;
        drain_toasts(): unknown[];
        cycle_palette(): string;
        frame_rate_hz(): number;
        get_audio_samples(): Float32Array;
        is_audio_muted(): boolean;
        is_mouse_emulated_controller(port: number): boolean;
        is_zapper_active(port: number): boolean;
        load_rom(rom: Uint8Array, rom_name: string): void;
        load_state_bytes(bytes: Uint8Array): void;
        constructor();
        render_frame(): Uint8Array;
        render_frame_rgba(): Uint8Array;
        reset(soft_reset: boolean): void;
        save_state_bytes(): Uint8Array;
        screen_height(): number;
        screen_width(): number;
        set_audio_muted(muted: boolean): void;
        set_audio_sample_rate(sample_rate: number): void;
        set_button(controller: number, button: number, pressed: boolean): void;
        set_controller_type(port: number, controller_type: string): void;
        set_mouse_left_button(pressed: boolean): void;
        set_mouse_x_position(position: number): void;
        set_mouse_y_position(position: number): void;
        // Autorun methods
        start_autorun_recording(): void;
        clear_autorun(): void;
        load_autorun_playback(bytes: Uint8Array, checkpoint_idx: number, extend: boolean): Uint8Array | null;
        stop_autorun(): Uint8Array;
        autorun_is_recording(): boolean;
        autorun_playback_finished(): boolean;
        autorun_recording_frame_count(): number;
        // Debugger methods
        debugger_open(): void;
        debugger_continue(): void;
        is_debugger_open(): boolean;
        debugger_disasm_json(): string;
        debugger_snapshot_json(): string;
        debugger_step_over(): void;
        debugger_step_into(): void;
        debugger_run_to_next_frame(): void;
        debugger_run_to_next_scanline(): void;
        debugger_run_to_nmi(): void;
        debugger_run_to_irq(): void;
        debugger_toggle_ppu_viewer(): void;
        debugger_is_ppu_viewer_open(): boolean;
        debugger_ppu_pattern_tables_rgba(): Uint8Array;
        debugger_ppu_nametables_rgba(): Uint8Array;
        debugger_ppu_scroll_json(): string;
        debugger_hexdump_prev_16(): void;
        debugger_hexdump_next_16(): void;
        debugger_hexdump_set_base(base: number): void;
        debugger_watch_add(address: number): void;
        debugger_watch_remove(index: number): void;
        debugger_watch_update(index: number, address: number): void;
        // Controller methods
        is_four_score_enabled(): boolean;
        has_expansion_mouse_controller(): boolean;
        set_snes_button(controller: number, button: number, pressed: boolean): boolean;
        set_mouse_right_button(pressed: boolean): void;
        is_snes_mouse_active(port: number): boolean;
    }

    export function gamepad_init_toast_message(gamepads_enabled: boolean, detected_controllers: number): string;

    /**
     * Provides a minimal WASM bridge for running the Game Boy emulator in the browser.
     */
    export class WasmGb {
        free(): void;
        [Symbol.dispose](): void;
        drain_toasts(): unknown[];
        frame_rate_hz(): number;
        get_audio_samples(): Float32Array;
        is_audio_muted(): boolean;
        load_rom(rom: Uint8Array, rom_name: string): void;
        constructor();
        render_frame_rgba(): Uint8Array;
        reset(soft_reset: boolean): void;
        screen_height(): number;
        screen_width(): number;
        set_audio_muted(muted: boolean): void;
        set_audio_sample_rate(sample_rate: number): void;
        set_button(controller: number, button: number, pressed: boolean): void;
    }

    /**
     * Provides a minimal WASM bridge for running the Game Boy Advance emulator in the browser.
     */
    export class WasmGba {
        free(): void;
        [Symbol.dispose](): void;
        drain_toasts(): unknown[];
        frame_rate_hz(): number;
        get_audio_samples(): Float32Array;
        get_audio_samples_stereo(): Float32Array;
        is_audio_muted(): boolean;
        load_rom(rom: Uint8Array, rom_name: string): void;
        constructor();
        render_frame_rgb(): Uint8Array;
        render_frame_rgba(): Uint8Array;
        reset(soft_reset: boolean): void;
        screen_height(): number;
        screen_width(): number;
        set_audio_muted(muted: boolean): void;
        set_audio_sample_rate(sample_rate: number): void;
        set_button(controller: number, button: number, pressed: boolean): void;
    }

    /**
     * Provides a minimal WASM bridge for running the Super Nintendo emulator in the browser.
     */
    export class WasmSnes {
        free(): void;
        [Symbol.dispose](): void;
        drain_toasts(): unknown[];
        frame_rate_hz(): number;
        get_audio_samples(): Float32Array;
        get_audio_samples_stereo(): Float32Array;
        is_audio_muted(): boolean;
        load_rom(rom: Uint8Array, rom_name: string): void;
        constructor();
        render_frame_rgba(): Uint8Array;
        reset(soft_reset: boolean): void;
        screen_height(): number;
        screen_width(): number;
        set_audio_muted(muted: boolean): void;
        set_audio_sample_rate(sample_rate: number): void;
        set_button(controller: number, button: number, pressed: boolean): void;
        save_state_bytes(): Uint8Array;
        load_state_bytes(bytes: Uint8Array): void;
        // Mouse peripheral methods
        has_mouse(): boolean;
        has_mouse_on_port(port: number): boolean;
        add_mouse_delta(port: number, dx: number, dy: number): void;
        set_mouse_left_button(port: number, pressed: boolean): void;
        set_mouse_right_button(port: number, pressed: boolean): void;
        // Super Scope peripheral methods
        has_superscope(): boolean;
        has_superscope_on_port(port: number): boolean;
        set_superscope_position(port: number, x: number, y: number): void;
        set_superscope_trigger(port: number, pressed: boolean): void;
        set_superscope_cursor(port: number, pressed: boolean): void;
        set_superscope_turbo(port: number, pressed: boolean): void;
        set_superscope_pause(port: number, pressed: boolean): void;
        // Multitap
        is_multitap_on_port(port: number): boolean;
    }

    export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

    export interface InitOutput {
        readonly memory: WebAssembly.Memory;
    }

    export type SyncInitInput = BufferSource | WebAssembly.Module;
    export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;
    export default function __wbg_init(module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
}
