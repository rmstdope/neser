export function buildFontString(fontSizePx: number, fontFamily: string) {
    return `bold ${fontSizePx}px ${fontFamily}`;
}

export function sampleWaveY({ x, timeSeconds, baseY, amplitude, frequency }: { x: number; timeSeconds: number; baseY: number; amplitude: number; frequency: number }) {
    const phase = (x * frequency) + timeSeconds;
    return baseY + Math.sin(phase) * amplitude;
}

export function createSineScroller({
    text,
    width,
    height,
    speed = 2,
    amplitude = 20,
    frequency = 0.01,
    fontSizePx = 32,
    fontFamily = "'Courier New', monospace"
}: {
    text: string;
    width: number;
    height: number;
    speed?: number;
    amplitude?: number;
    frequency?: number;
    fontSizePx?: number;
    fontFamily?: string;
}) {
    const buffer = new Uint8Array(width * height * 4);
    const canvas = (typeof OffscreenCanvas !== "undefined")
        ? new OffscreenCanvas(width, height)
        : (() => {
            if (typeof document === "undefined") {
                throw new Error("No canvas available for sine scroller");
            }
            const element = document.createElement("canvas");
            element.width = width;
            element.height = height;
            return element;
        })();
    const ctx = canvas.getContext("2d", { willReadFrequently: true }) as CanvasRenderingContext2D;
    if (!ctx) {
        throw new Error("2D canvas context unavailable for sine scroller");
    }
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    ctx.font = buildFontString(fontSizePx, fontFamily);

    const chars = Array.from(text);
    const charWidths = chars.map(char => ctx.measureText(char).width);
    const textWidth = charWidths.reduce((total, value) => total + value, 0);

    let time = 0;
    let scrollX = width;
    let lastTimestamp: number | null = null;

    function renderFrame(timestampMs: number) {
        if (lastTimestamp === null) {
            lastTimestamp = timestampMs;
        }
        const deltaSeconds = (timestampMs - lastTimestamp) / 1000;
        lastTimestamp = timestampMs;

        ctx.fillStyle = "#000000";
        ctx.fillRect(0, 0, width, height);

        scrollX -= speed * deltaSeconds * 60;
        if (scrollX < -textWidth) {
            scrollX = width;
        }

        let charX = scrollX;
        for (let i = 0; i < chars.length; i++) {
            const char = chars[i];
            const charWidth = charWidths[i];
            const sineOffset = sampleWaveY({
                x: charX,
                timeSeconds: time,
                baseY: 0,
                amplitude,
                frequency
            });
            const charY = (height / 2) + sineOffset;

            const gradient = ctx.createLinearGradient(charX, charY - 20, charX, charY + 20);
            gradient.addColorStop(0, "#ff00ff");
            gradient.addColorStop(0.5, "#00ffff");
            gradient.addColorStop(1, "#ffff00");

            ctx.fillStyle = gradient;
            ctx.fillText(char, charX, charY);

            ctx.shadowColor = "#00ffff";
            ctx.shadowBlur = 10;
            ctx.fillText(char, charX, charY);
            ctx.shadowBlur = 0;

            charX += charWidth;
        }

        time += 0.05;

        const imageData = ctx.getImageData(0, 0, width, height);
        buffer.set(imageData.data);
        return buffer;
    }

    return {
        renderFrame
    };
}
