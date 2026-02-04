import { shouldSuppressJoypadInput } from "./input_routing.js";

export function mapMouseXToPaddlePosition(x, windowWidth) {
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

export function applyPaddleMouseMotion(nes, x, windowWidth) {
    const paddlePort = nes.paddle_port ? nes.paddle_port() : null;
    if (!paddlePort) {
        return;
    }

    const position = mapMouseXToPaddlePosition(x, windowWidth);
    nes.set_mouse_x_position(paddlePort, position);
}

export function applyPaddleMouseButton(nes, button, pressed) {
    const paddlePort = nes.paddle_port ? nes.paddle_port() : null;
    if (!paddlePort) {
        return;
    }

    if (button === 0) {
        nes.set_mouse_left_button(paddlePort, pressed);
    }
}

export function applyJoypadButtonIfAllowed(nes, controller, button, pressed) {
    // Check if there's a paddle on this controller's port
    if (shouldSuppressJoypadInput(nes, controller)) {
        return; // Suppress joypad input on the port with paddle
    }

    nes.set_button(controller, button, pressed);
}
