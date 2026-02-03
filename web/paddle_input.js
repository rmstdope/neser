export function mapMouseXToPaddlePosition(x, windowWidth) {
    if (windowWidth <= 1) {
        return 0;
    }

    const maxX = windowWidth - 1;
    const clampedX = Math.min(Math.max(x, 0), maxX);
    const normalized = (clampedX / maxX) * 2 - 1;
    const curved = Math.sign(normalized) * Math.pow(Math.abs(normalized), 1.5);
    const scaled = (curved + 1) * 0.5 * 255;

    return Math.min(255, Math.max(0, Math.round(scaled)));
}

export function applyPaddleMouseMotion(nes, x, windowWidth) {
    if (!nes.paddle1_enabled()) {
        return;
    }

    const position = mapMouseXToPaddlePosition(x, windowWidth);
    nes.set_paddle1_position(position);
}

export function applyPaddleMouseButton(nes, button, pressed) {
    if (!nes.paddle1_enabled()) {
        return;
    }

    if (button === 0) {
        nes.set_paddle1_trigger(pressed);
    }
}

export function applyJoypadButtonIfAllowed(nes, controller, button, pressed) {
    if (controller === 1 && nes.paddle1_enabled()) {
        return;
    }

    nes.set_button(controller, button, pressed);
}
