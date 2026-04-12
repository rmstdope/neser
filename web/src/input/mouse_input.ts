import { shouldSuppressJoypadInput } from "./input_routing";

interface MouseNes {
    is_zapper_active(port: number): boolean;
    is_snes_mouse_active?(port: number): boolean;
    is_mouse_emulated_controller(port: number): boolean;
    set_mouse_x_position(position: number): void;
    set_mouse_y_position(position: number): void;
    set_mouse_left_button(pressed: boolean): void;
    set_mouse_right_button(pressed: boolean): void;
    set_button(controller: number, button: number, pressed: boolean): void;
}

export function mapMouseXToPaddlePosition(x: number, windowWidth: number) {
    const minPosition = 0x62;
    const maxPosition = 0xF2;
    const range = maxPosition - minPosition;

    if (windowWidth <= 1) {
        return minPosition;
    }

    const maxX = windowWidth - 1;
    const clampedX = Math.min(Math.max(x, 0), maxX);
    const normalized = (clampedX / maxX) * 2 - 1;
    const curved = Math.sign(normalized) * Math.pow(Math.abs(normalized), 1.5);
    const scaled = (curved + 1) * 0.5 * range + minPosition;

    return Math.min(maxPosition, Math.max(minPosition, Math.round(scaled)));
}

export function mapMouseXToZapperPosition(x: number, windowWidth: number) {
    if (windowWidth <= 1) {
        return 0;
    }

    const maxX = windowWidth - 1;
    const clampedX = Math.min(Math.max(x, 0), maxX);
    const normalized = clampedX / maxX;
    const scaled = normalized * 255;

    return Math.min(255, Math.max(0, Math.round(scaled)));
}

export function mapMouseYToZapperPosition(y: number, windowHeight: number) {
    if (windowHeight <= 1) {
        return 0;
    }

    const maxY = windowHeight - 1;
    const clampedY = Math.min(Math.max(y, 0), maxY);
    const normalized = clampedY / maxY;
    const scaled = normalized * 239;

    return Math.min(239, Math.max(0, Math.round(scaled)));
}

export function mapMouseAxisToSnesMousePosition(axis: number, windowExtent: number) {
    if (windowExtent <= 1) {
        return 0;
    }

    const maxAxis = windowExtent - 1;
    const clamped = Math.min(Math.max(axis, 0), maxAxis);
    const normalized = clamped / maxAxis;
    const scaled = normalized * 255;

    return Math.min(255, Math.max(0, Math.round(scaled)));
}

export function applyMouseMotion(nes: MouseNes, x: number, y: number, windowWidth: number, windowHeight: number) {
    const isZapper = isZapperActive(nes);
    const isSnesMouse =
        nes.is_snes_mouse_active?.(1) ||
        nes.is_snes_mouse_active?.(2);
    
    if (isZapper) {
        const positionX = mapMouseXToZapperPosition(x, windowWidth);
        const positionY = mapMouseYToZapperPosition(y, windowHeight);
        nes.set_mouse_x_position(positionX);
        nes.set_mouse_y_position(positionY);
    } else if (isSnesMouse) {
        const positionX = mapMouseAxisToSnesMousePosition(x, windowWidth);
        const positionY = mapMouseAxisToSnesMousePosition(y, windowHeight);
        nes.set_mouse_x_position(positionX);
        nes.set_mouse_y_position(positionY);
    } else {
        // Arkanoid paddle only uses X position
        const positionX = mapMouseXToPaddlePosition(x, windowWidth);
        nes.set_mouse_x_position(positionX);
    }
}

export function applyMouseButton(nes: MouseNes, button: number, pressed: boolean) {
    if (button === 0) {
        nes.set_mouse_left_button(pressed);
    } else if (button === 2) {
        nes.set_mouse_right_button(pressed);
    }
}

export function applyJoypadButtonIfAllowed(nes: MouseNes, controller: number, button: number, pressed: boolean) {
    // Check if there's a mouse-emulated controller on this port
    if (shouldSuppressJoypadInput(nes, controller)) {
        return; // Suppress joypad input on the port with mouse-emulated controller
    }

    nes.set_button(controller, button, pressed);
}

export function isZapperActive(nes: MouseNes) {
    // Check if Zapper is active on either port
    return nes.is_zapper_active(1) || nes.is_zapper_active(2);
}
