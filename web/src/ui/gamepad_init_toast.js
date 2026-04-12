export function createGamepadInitToastNotifier({ buildMessage, showToast }) {
    let hasShownToast = false;

    function showOnce(gamepadsEnabled, detectedControllers) {
        if (hasShownToast) {
            return false;
        }

        const message = buildMessage(gamepadsEnabled, detectedControllers);
        showToast(message);
        hasShownToast = true;
        return true;
    }

    return {
        showOnce
    };
}
