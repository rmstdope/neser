export function planFrame({ shouldRender }) {
    const render = Boolean(shouldRender);
    return {
        shouldStep: render,
        shouldRender: render
    };
}
