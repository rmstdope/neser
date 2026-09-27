// Vite asset imports used by the web frontend.
declare module "*.txt?url" {
    const url: string;
    export default url;
}

/** Vite's ?raw suffix on an HTML file: its text, as the markup tests read index.html. */
declare module "*.html?raw" {
    const source: string;
    export default source;
}
