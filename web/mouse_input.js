import { shouldSuppressJoypadInput } from "./input_routing.js";

export function mapMouseXToScreenPosition(x, windowWidth) {
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

export function applyMouseMotion(nes, x, windowWidth) {
    const port = nes.mouse_controller_port ? nes.mouse_controller_port() : null;
    if (!port) {
        return;
    }

    const position = mapMouseXToScreenPosition(x, windowWidth);
    nes.set_mouse_x_position(port, position);
}

export function applyMouseButton(nes, button, pressed) {
    const port = nes.mouse_controller_port ? nes.mouse_controller_port() : null;
    if (!port) {
        return;
    }

    if (button === 0) {
        nes.set_mouse_left_button(port, pressed);
    }
}

export function applyJoypadButtonIfAllowed(nes, controller, button, pressed) {
    // Check if there's a mouse-emulated controller on this port
    if (shouldSuppressJoypadInput(nes, controller)) {
        return; // Suppress joypad input on the port with paddle
    }

    nes.set_button(controller, button, pressed);
}
