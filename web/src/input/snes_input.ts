/**
 * SNES-specific input routing.
 *
 * Handles mouse, Super Scope, and multitap input for the WasmSnes bridge.
 * All port numbers here are 1-based (matching the WasmSnes public API).
 */

export interface SnesInputBridge {
    has_mouse(): boolean;
    has_mouse_on_port(port: number): boolean;
    has_superscope(): boolean;
    has_superscope_on_port(port: number): boolean;
    is_multitap_on_port(port: number): boolean;
    add_mouse_delta(port: number, dx: number, dy: number): void;
    set_mouse_left_button(port: number, pressed: boolean): void;
    set_mouse_right_button(port: number, pressed: boolean): void;
    set_superscope_position(port: number, x: number, y: number): void;
    set_superscope_trigger(port: number, pressed: boolean): void;
    set_superscope_cursor(port: number, pressed: boolean): void;
    set_superscope_turbo(port: number, pressed: boolean): void;
    set_superscope_pause(port: number, pressed: boolean): void;
}

/** Returns true if a SNES mouse peripheral is active on any port. */
export function isSnesMouseActive(snes: SnesInputBridge): boolean {
    return snes.has_mouse();
}

/** Returns true if a Super Scope peripheral is active on any port. */
export function isSnesSuperScopeActive(snes: SnesInputBridge): boolean {
    return snes.has_superscope();
}

/**
 * Forward relative mouse movement to the SNES mouse peripheral.
 *
 * If the mouse is not active on `port`, the call is a no-op.
 * Call with no `port` argument or `port = 0` to skip without error.
 */
export function applySnesMouseDelta(
    snes: SnesInputBridge,
    port = 0,
    dx = 0,
    dy = 0,
): void {
    if (port === 0 || !snes.has_mouse_on_port(port)) {
        return;
    }
    snes.add_mouse_delta(port, dx, dy);
}

/**
 * Forward a mouse button press/release to the SNES mouse peripheral.
 *
 * button 0 → left, button 2 → right. Other buttons are ignored.
 * No-op when the mouse is not active on `port`.
 */
export function applySnesMouseButton(
    snes: SnesInputBridge,
    port: number,
    button: number,
    pressed: boolean,
): void {
    if (!snes.has_mouse_on_port(port)) {
        return;
    }
    if (button === 0) {
        snes.set_mouse_left_button(port, pressed);
    } else if (button === 2) {
        snes.set_mouse_right_button(port, pressed);
    }
}

/**
 * Map a canvas x-coordinate to a SNES screen x-coordinate (0–255).
 */
export function mapSnesScreenX(x: number, canvasWidth: number): number {
    if (canvasWidth <= 1) {
        return 0;
    }
    const maxX = canvasWidth - 1;
    const clamped = Math.min(Math.max(x, 0), maxX);
    const normalized = clamped / maxX;
    return Math.min(255, Math.max(0, Math.round(normalized * 255)));
}

/**
 * Map a canvas y-coordinate to a SNES screen y-coordinate (0–223).
 */
export function mapSnesScreenY(y: number, canvasHeight: number): number {
    if (canvasHeight <= 1) {
        return 0;
    }
    const maxY = canvasHeight - 1;
    const clamped = Math.min(Math.max(y, 0), maxY);
    const normalized = clamped / maxY;
    return Math.min(223, Math.max(0, Math.round(normalized * 223)));
}

/**
 * Forward an absolute canvas position to the Super Scope peripheral.
 *
 * Coordinates are remapped from canvas space to SNES screen space (256×224).
 * No-op when the Super Scope is not active on `port`.
 */
export function applySnesSuperScopePosition(
    snes: SnesInputBridge,
    port: number,
    x: number,
    y: number,
    canvasWidth: number,
    canvasHeight: number,
): void {
    if (!snes.has_superscope_on_port(port)) {
        return;
    }
    const snesX = mapSnesScreenX(x, canvasWidth);
    const snesY = mapSnesScreenY(y, canvasHeight);
    snes.set_superscope_position(port, snesX, snesY);
}

/**
 * Forward a mouse button press/release to the Super Scope peripheral.
 *
 * button 0 → trigger, button 2 → cursor. Other buttons are ignored.
 * No-op when the Super Scope is not active on `port`.
 */
export function applySnesSuperScopeButton(
    snes: SnesInputBridge,
    port: number,
    button: number,
    pressed: boolean,
): void {
    if (!snes.has_superscope_on_port(port)) {
        return;
    }
    if (button === 0) {
        snes.set_superscope_trigger(port, pressed);
    } else if (button === 2) {
        snes.set_superscope_cursor(port, pressed);
    }
}

/**
 * Returns true if joypad input should be suppressed on the given 1-based port.
 *
 * Joypad input is suppressed on a port occupied by a mouse or Super Scope,
 * since those peripherals use the mouse for input rather than a gamepad.
 * Multitap ports are NOT suppressed — they accept normal joypad input.
 */
export function shouldSuppressSnesJoypadInput(
    snes: SnesInputBridge,
    port: number,
): boolean {
    // Only suppress ports 1 or 2 that physically host a non-joypad peripheral.
    if (port === 1) {
        return snes.has_mouse_on_port(1) || snes.has_superscope_on_port(1);
    }
    if (port === 2) {
        return snes.has_mouse_on_port(2) || snes.has_superscope_on_port(2);
    }
    // Ports 3-5 are multitap sub-controllers — never suppress.
    return false;
}
