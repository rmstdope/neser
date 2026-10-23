/**
 * A frame as the emulator hands it to the renderer (`channels` bytes per pixel: 4 for RGBA,
 * 3 for the GBA's RGB) turned into opaque RGBA pixels for a 2D canvas.
 */
export function frameToRgba(frame: Uint8Array, channels: 3 | 4, width: number, height: number): Uint8ClampedArray<ArrayBuffer> {
    const pixels = width * height;
    const out = new Uint8ClampedArray(new ArrayBuffer(pixels * 4));
    for (let i = 0; i < pixels; i++) {
        const from = i * channels;
        const to = i * 4;
        if (from + 2 < frame.length) {
            out[to] = frame[from];
            out[to + 1] = frame[from + 1];
            out[to + 2] = frame[from + 2];
        }
        out[to + 3] = 255;
    }
    return out;
}

/** A PNG data URL of the frame, scaled to `thumbWidth` wide; null where no 2D canvas is available. */
export function frameThumbnail(frame: Uint8Array, channels: 3 | 4, width: number, height: number, thumbWidth = 192): string | null {
    const source = document.createElement("canvas");
    source.width = width;
    source.height = height;
    const sourceContext = source.getContext("2d");
    if (!sourceContext) return null;
    sourceContext.putImageData(new ImageData(frameToRgba(frame, channels, width, height), width, height), 0, 0);
    const thumb = document.createElement("canvas");
    thumb.width = thumbWidth;
    thumb.height = Math.round((thumbWidth * height) / width);
    const thumbContext = thumb.getContext("2d");
    if (!thumbContext) return null;
    thumbContext.imageSmoothingEnabled = false;
    thumbContext.drawImage(source, 0, 0, thumb.width, thumb.height);
    return thumb.toDataURL("image/png");
}
