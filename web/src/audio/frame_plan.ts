export function planFrame({ shouldRender }: { shouldRender: boolean }) {
    const render = Boolean(shouldRender);
    return {
        shouldStep: render,
        shouldRender: render
    };
}
