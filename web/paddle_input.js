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
