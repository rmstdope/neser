const h2 = (n) => n.toString(16).toUpperCase().padStart(2, "0");

/**
 * Formats a single OAM sprite entry as a string.
 * Format: "NN: Y=YY tile=TT attr=AA X=XX" (all values in hex).
 *
 * @param {number} index - Sprite index (0–63).
 * @param {number} y     - Y position.
 * @param {number} tile  - Tile index.
 * @param {number} attrs - Attribute byte.
 * @param {number} x     - X position.
 * @returns {string}
 */
export function formatOamEntry(index, y, tile, attrs, x) {
    return `${index.toString().padStart(2, "0")}: Y=${h2(y)} tile=${h2(tile)} attr=${h2(attrs)} X=${h2(x)}`;
}

/**
 * Builds the HTML for the OAM debugger panel.
 *
 * @param {number[]|null} oam - 256-byte OAM array from the snapshot.
 * @returns {string} HTML string for the OAM panel.
 */
export function buildOamHtml(oam) {
    if (!Array.isArray(oam) || oam.length < 256) return "";

    const rows = [];
    for (let i = 0; i < 64; i++) {
        const base = i * 4;
        const entry = formatOamEntry(i, oam[base], oam[base + 1], oam[base + 2], oam[base + 3]);
        const esc = entry.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
        rows.push(`<span class="debugger-oam-row">${esc}</span>`);
    }

    return (
        `<span class="debugger-oam-title">OAM</span>` +
        `<span class="debugger-oam-header"># : Y    tile attr X</span>` +
        `<span class="debugger-oam-block">${rows.join("")}</span>`
    );
}
