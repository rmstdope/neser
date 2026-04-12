export function createGamepadInitToastNotifier({ buildMessage, showToast }: { buildMessage: (gamepadsEnabled: boolean, detectedControllers: number) => string; showToast: (message: string) => void }) {
    let hasShownToast = false;

    function showOnce(gamepadsEnabled: boolean, detectedControllers: number) {
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
