export function planFrame({ shouldRender }) {
    return {
        shouldStep: true,
        shouldRender: Boolean(shouldRender)
    };
}
