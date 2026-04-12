function escapeHtml(text: string) {
    return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

export function formatDisasmBytes(bytes: number[] | null) {
    if (!bytes || bytes.length === 0) return "  ";
    return bytes.map((byte: number) => byte.toString(16).toUpperCase().padStart(2, "0")).join(" ");
}

export function buildDisasmLineHtml(line: { addr: number; bytes: number[] | null; text: string; is_current: boolean }) {
    const addr = line.addr.toString(16).toUpperCase().padStart(4, "0");
    const bytesStr = formatDisasmBytes(line.bytes).padEnd(8);
    const lineText = `${addr}: ${bytesStr}  ${escapeHtml(line.text)}`;
    if (line.is_current) {
        return `<span class="disasm-current">&gt; ${lineText}</span>`;
    }
    return `<span class="disasm-line">  ${lineText}</span>`;
}

export function renderDisasmLines(lines: { addr: number; bytes: number[] | null; text: string; is_current: boolean }[]) {
    if (!Array.isArray(lines)) return "";
    return lines.map(buildDisasmLineHtml).join("");
}