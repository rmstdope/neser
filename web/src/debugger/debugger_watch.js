const h2 = (n) => n.toString(16).toUpperCase().padStart(2, "0");
const h4 = (n) => n.toString(16).toUpperCase().padStart(4, "0");

export function formatWatchEntry(address, value) {
    const safeAddress = Number(address) & 0xFFFF;
    const safeValue = Number(value) & 0xFF;
    const bits = safeValue.toString(2).padStart(8, "0");
    return `$${h4(safeAddress)}: $${h2(safeValue)} / 0b${bits}`;
}

export function parseWatchAddressInput(value) {
    if (typeof value !== "string") return null;
    const trimmed = value.trim().replace(/^0x/i, "");
    if (!/^[0-9a-fA-F]{1,4}$/.test(trimmed)) {
        return null;
    }
    return Number.parseInt(trimmed, 16) & 0xFFFF;
}
