/**
 * Crosshair rendering for Zapper light gun controller.
 * Creates an overlay canvas for drawing the crosshair cursor.
 */

export function createCrosshair(targetCanvas) {
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
    targetCanvas.parentElement.appendChild(overlayCanvas);
    
    const ctx = overlayCanvas.getContext("2d");
    let visible = false;
    let currentX = 0;
    let currentY = 0;
    
    function updateCanvasSize() {
        overlayCanvas.width = targetCanvas.width;
        overlayCanvas.height = targetCanvas.height;
        overlayCanvas.style.width = targetCanvas.style.width;
        overlayCanvas.style.height = targetCanvas.style.height;
        drawCrosshair(currentX, currentY);
    }
    
    function drawCrosshair(x, y) {
        ctx.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
        
        if (!visible) {
            return;
        }
        
        const dpr = window.devicePixelRatio || 1;
        const scaledX = x * dpr;
        const scaledY = y * dpr;
        
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
        drawCrosshair(currentX, currentY);
    }
    
    function hide() {
        visible = false;
        ctx.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
    }
    
    function updatePosition(x, y) {
        currentX = x;
        currentY = y;
        drawCrosshair(x, y);
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
