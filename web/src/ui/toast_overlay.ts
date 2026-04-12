function defaultCreateNode(message: string) {
    const node = document.createElement("div");
    node.className = "neser-toast";
    node.textContent = message;
    return node;
}

function removeNodeIfPresent(container: HTMLElement, node: HTMLElement) {
    if (typeof container.contains === "function") {
        if (container.contains(node)) {
            container.removeChild(node);
        }
        return;
    }
    container.removeChild(node);
}

function normalizeToastMessages(messages: unknown[]) {
    return messages
        .map((message: unknown) => String(message))
        .filter((message: string) => message.length > 0);
}

export function createToastContainer(host: HTMLElement) {
    const container = document.createElement("div");
    container.className = "neser-toast-container";
    host.appendChild(container);
    return container;
}

export function createToastOverlay({
    container,
    createNode = defaultCreateNode,
    schedule = (callback: () => void, delayMs: number) => setTimeout(callback, delayMs),
    durationMs = 3000
}: {
    container: HTMLElement;
    createNode?: (message: string) => HTMLElement;
    schedule?: (callback: () => void, delayMs: number) => number | ReturnType<typeof setTimeout>;
    durationMs?: number;
}) {
    function show(message: string) {
        const node = createNode(String(message));
        container.appendChild(node);
        schedule(() => {
            removeNodeIfPresent(container, node);
        }, durationMs);
    }

    function showMany(messages: string[]) {
        for (const message of messages) {
            show(message);
        }
    }

    return {
        show,
        showMany
    };
}

export function drainNesToasts(nes: { drain_toasts?: () => unknown[] } | null, overlay: { showMany: (messages: string[]) => void } | null) {
    if (!nes || !overlay || typeof nes.drain_toasts !== "function") {
        return;
    }
    const messages = normalizeToastMessages(nes.drain_toasts());
    if (messages.length > 0) {
        overlay.showMany(messages);
    }
}
