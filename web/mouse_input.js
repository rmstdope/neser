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

export function mapMouseXToZapperPosition(x, windowWidth) {
    if (windowWidth <= 1) {
        return 0;
    }

    const maxX = windowWidth - 1;
    const clampedX = Math.min(Math.max(x, 0), maxX);
    const normalized = clampedX / maxX;
    const scaled = normalized * 255;

    return Math.min(255, Math.max(0, Math.round(scaled)));
}

export function mapMouseYToZapperPosition(y, windowHeight) {
    if (windowHeight <= 1) {
        return 0;
    }

    const maxY = windowHeight - 1;
    const clampedY = Math.min(Math.max(y, 0), maxY);
    const normalized = clampedY / maxY;
    const scaled = normalized * 239;

    return Math.min(239, Math.max(0, Math.round(scaled)));
}

export function applyMouseMotion(nes, x, y, windowWidth, windowHeight) {
    const isZapper = isZapperActive(nes);
    
    if (isZapper) {
        const positionX = mapMouseXToZapperPosition(x, windowWidth);
        const positionY = mapMouseYToZapperPosition(y, windowHeight);
        nes.set_mouse_x_position(positionX);
        nes.set_mouse_y_position(positionY);
    } else {
        // Arkanoid paddle only uses X position
        const positionX = mapMouseXToPaddlePosition(x, windowWidth);
        nes.set_mouse_x_position(positionX);
    }
}

export function applyMouseButton(nes, button, pressed) {
    if (button === 0) {
        nes.set_mouse_left_button(pressed);
    }
}

export function applyJoypadButtonIfAllowed(nes, controller, button, pressed) {
    // Check if there's a mouse-emulated controller on this port
    if (shouldSuppressJoypadInput(nes, controller)) {
        return; // Suppress joypad input on the port with mouse-emulated controller
    }

    nes.set_button(controller, button, pressed);
}

export function isZapperActive(nes) {
    // Check if Zapper is active on either port
    return nes.is_zapper_active(1) || nes.is_zapper_active(2);
}
