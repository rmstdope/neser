function defaultCreateNode(message) {
    const node = document.createElement("div");
    node.className = "neser-toast";
    node.textContent = message;
    return node;
}

function removeNodeIfPresent(container, node) {
    if (typeof container.contains === "function") {
        if (container.contains(node)) {
            container.removeChild(node);
        }
        return;
    }
    container.removeChild(node);
}

function normalizeToastMessages(messages) {
    return messages
        .map((message) => String(message))
        .filter((message) => message.length > 0);
}

export function createToastContainer(host) {
    const container = document.createElement("div");
    container.className = "neser-toast-container";
    host.appendChild(container);
    return container;
}

export function createToastOverlay({
    container,
    createNode = defaultCreateNode,
    schedule = (callback, delayMs) => setTimeout(callback, delayMs),
    durationMs = 3000
}) {
    function show(message) {
        const node = createNode(String(message));
        container.appendChild(node);
        schedule(() => {
            removeNodeIfPresent(container, node);
        }, durationMs);
    }

    function showMany(messages) {
        for (const message of messages) {
            show(message);
        }
    }

    return {
        show,
        showMany
    };
}

export function drainNesToasts(nes, overlay) {
    if (!nes || !overlay || typeof nes.drain_toasts !== "function") {
        return;
    }
    const messages = normalizeToastMessages(nes.drain_toasts());
    if (messages.length > 0) {
        overlay.showMany(messages);
    }
}
