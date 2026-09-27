/**
 * Crosshair rendering for the light guns: the NES Zapper's plus and the Super Scope's ring
 * sight. Creates an overlay canvas for drawing the crosshair cursor.
 */

/** Picture width in console pixels, which the ring sight's sizes are measured in. */
const PICTURE_WIDTH = 256;

export type CrosshairStyle = "plus" | "ring";

export function createCrosshair(
    targetCanvas: HTMLCanvasElement,
    { style = "plus" }: { style?: CrosshairStyle } = {},
) {
    const parent = targetCanvas.parentElement;
    if (!parent) {
        throw new Error("Crosshair requires a parent element for the target canvas");
    }

    if (!parent.style.position || parent.style.position === "static") {
        parent.style.position = "relative";
    }

    // Create overlay canvas for crosshair
    const overlayCanvas = document.createElement("canvas");
    overlayCanvas.style.position = "absolute";
    overlayCanvas.style.top = "0";
    overlayCanvas.style.left = "0";
    overlayCanvas.style.pointerEvents = "none"; // Allow mouse events to pass through
    overlayCanvas.style.zIndex = "10";
    
    // Match the target canvas size
    overlayCanvas.width = targetCanvas.width;
    overlayCanvas.height = targetCanvas.height;
    overlayCanvas.style.width = targetCanvas.style.width;
    overlayCanvas.style.height = targetCanvas.style.height;
    
    // Insert overlay after target canvas
    parent.appendChild(overlayCanvas);
    
    const ctx = overlayCanvas.getContext("2d")!;
    let visible = false;
    let currentX = 0;
    let currentY = 0;

    function clampPosition(x: number, y: number) {
        const dpr = window.devicePixelRatio || 1;
        const maxX = Math.max(0, overlayCanvas.width / dpr - 1);
        const maxY = Math.max(0, overlayCanvas.height / dpr - 1);

        return {
            x: Math.min(Math.max(x, 0), maxX),
            y: Math.min(Math.max(y, 0), maxY),
        };
    }

    function syncOverlayPlacement() {
        overlayCanvas.style.left = `${targetCanvas.offsetLeft}px`;
        overlayCanvas.style.top = `${targetCanvas.offsetTop}px`;
    }
    
    function updateCanvasSize() {
        overlayCanvas.width = targetCanvas.width;
        overlayCanvas.height = targetCanvas.height;
        overlayCanvas.style.width = targetCanvas.style.width;
        overlayCanvas.style.height = targetCanvas.style.height;
        syncOverlayPlacement();
        const clamped = clampPosition(currentX, currentY);
        currentX = clamped.x;
        currentY = clamped.y;
        drawCrosshair(currentX, currentY);
    }
    
    /**
     * The Super Scope's sight, as agreed in docs/ui/nr-yvv-super-scope.html: in picture
     * pixels a ring of radius 9 and ticks from 4 to 14 off centre, white 1.5 wide over a
     * black 3.5 outline, scaled with the picture.
     */
    function drawRing(cx: number, cy: number) {
        const scale = overlayCanvas.width / PICTURE_WIDTH;
        const inner = 4 * scale;
        const outer = 14 * scale;
        for (const [color, width] of [["#000", 3.5], ["#fff", 1.5]] as const) {
            ctx.strokeStyle = color;
            ctx.lineWidth = width * scale;
            ctx.lineCap = "butt";
            ctx.beginPath();
            ctx.arc(cx, cy, 9 * scale, 0, Math.PI * 2);
            ctx.stroke();
            ctx.beginPath();
            ctx.moveTo(cx - outer, cy);
            ctx.lineTo(cx - inner, cy);
            ctx.moveTo(cx + inner, cy);
            ctx.lineTo(cx + outer, cy);
            ctx.moveTo(cx, cy - outer);
            ctx.lineTo(cx, cy - inner);
            ctx.moveTo(cx, cy + inner);
            ctx.lineTo(cx, cy + outer);
            ctx.stroke();
        }
    }

    function drawCrosshair(x: number, y: number) {
        ctx.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
        
        if (!visible) {
            return;
        }
        
        const dpr = window.devicePixelRatio || 1;
        const scaledX = x * dpr;
        const scaledY = y * dpr;

        if (style === "ring") {
            drawRing(scaledX, scaledY);
            return;
        }
        
        // Crosshair dimensions
        const lineLength = 20 * dpr;
        const gap = 8 * dpr;
        const lineWidth = 2 * dpr;
        
        ctx.strokeStyle = "rgba(255, 255, 255, 0.9)";
        ctx.lineWidth = lineWidth;
        ctx.lineCap = "round";
        
        // Draw outer white lines
        ctx.beginPath();
        // Top
        ctx.moveTo(scaledX, scaledY - gap);
        ctx.lineTo(scaledX, scaledY - gap - lineLength);
        // Bottom
        ctx.moveTo(scaledX, scaledY + gap);
        ctx.lineTo(scaledX, scaledY + gap + lineLength);
        // Left
        ctx.moveTo(scaledX - gap, scaledY);
        ctx.lineTo(scaledX - gap - lineLength, scaledY);
        // Right
        ctx.moveTo(scaledX + gap, scaledY);
        ctx.lineTo(scaledX + gap + lineLength, scaledY);
        ctx.stroke();
        
        // Draw red center dot
        ctx.fillStyle = "rgba(255, 0, 0, 0.8)";
        ctx.beginPath();
        ctx.arc(scaledX, scaledY, 3 * dpr, 0, Math.PI * 2);
        ctx.fill();
    }
    
    function show() {
        visible = true;
        syncOverlayPlacement();
        drawCrosshair(currentX, currentY);
    }
    
    function hide() {
        visible = false;
        ctx.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
    }
    
    function updatePosition(x: number, y: number) {
        syncOverlayPlacement();
        const clamped = clampPosition(x, y);
        currentX = clamped.x;
        currentY = clamped.y;
        drawCrosshair(currentX, currentY);
    }
    
    function destroy() {
        overlayCanvas.remove();
    }
    
    return {
        show,
        hide,
        updatePosition,
        updateCanvasSize,
        destroy,
        get visible() {
            return visible;
        }
    };
}
