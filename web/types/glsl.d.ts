/** Allow importing GLSL files as raw strings via Vite's ?raw suffix. */
declare module "*.glsl?raw" {
    const source: string;
    export default source;
}
