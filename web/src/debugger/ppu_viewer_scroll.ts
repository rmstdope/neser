export function sanitizeScrollTop(value: number) {
    if (!Number.isFinite(value) || value < 0) {
        return 0;
    }
    return value;
}

export function clampScrollTop(savedScrollTop: number, metrics: { scrollHeight?: number; clientHeight?: number } | null) {
    const safeSaved = sanitizeScrollTop(savedScrollTop);
    const scrollHeight = Number.isFinite(metrics?.scrollHeight) ? metrics!.scrollHeight! : 0;
    const clientHeight = Number.isFinite(metrics?.clientHeight) ? metrics!.clientHeight! : 0;
    const maxScrollTop = Math.max(0, scrollHeight - clientHeight);
    return Math.min(safeSaved, maxScrollTop);
}
