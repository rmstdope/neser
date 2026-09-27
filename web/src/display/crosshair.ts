/**
 * The light guns' sight: the Super Scope's white ring, drawn identically for the NES Zapper.
 * Creates an overlay canvas for drawing it over the picture.
 *
 * `pictureWidth` is the width, in console pixels, of the picture the canvas shows, which the
 * ring's sizes are measured in: 256 for the Super Scope, the cropped width for the NES.
 */
export function createCrosshair(
    targetCanvas: HTMLCanvasElement,
    { pictureWidth = 256 }: { pictureWidth?: number } = {},
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
     * The sight, as agreed in docs/ui/nr-yvv-super-scope.html and docs/ui/nr-9qg-zapper-sight.html: in picture
     * pixels a ring of radius 9 and ticks from 4 to 14 off centre, white 1.5 wide over a
     * black 3.5 outline, scaled with the picture.
     */
    function drawRing(cx: number, cy: number) {
        const scale = overlayCanvas.width / pictureWidth;
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

        drawRing(scaledX, scaledY);
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
