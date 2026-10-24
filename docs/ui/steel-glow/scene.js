// A made-up 256x240 platformer frame, so each drawing has something on the screen.
function drawScene(canvas) {
  const c = canvas.getContext("2d");
  const r = (x, y, w, h, col) => { c.fillStyle = col; c.fillRect(x, y, w, h); };
  r(0, 0, 256, 240, "#5c94fc");
  // clouds
  [[30, 40], [150, 28], [200, 70]].forEach(([x, y]) => {
    r(x + 6, y, 28, 6, "#fcfcfc"); r(x, y + 6, 40, 8, "#fcfcfc"); r(x + 4, y + 14, 32, 4, "#a4e4fc");
  });
  // hills
  for (let i = 0; i < 60; i++) r(10 + i, 176 - Math.min(i, 59 - i) * 0.9, 1, 200, "#00a800");
  for (let i = 0; i < 90; i++) r(150 + i, 176 - Math.min(i, 89 - i) * 0.6, 1, 200, "#58d854");
  // ground bricks
  for (let x = 0; x < 256; x += 16) for (let y = 192; y < 240; y += 16) {
    r(x, y, 16, 16, "#c84c0c"); r(x, y, 16, 1, "#fcbcb0"); r(x, y + 8, 16, 1, "#000"); r(x + ((y / 16) % 2 ? 8 : 0), y, 1, 16, "#000");
  }
  // floating blocks
  [[88, 128], [104, 128], [120, 128]].forEach(([x, y], i) => {
    r(x, y, 16, 16, i === 1 ? "#fc9838" : "#c84c0c"); r(x, y, 16, 1, "#fcbcb0"); r(x, y + 15, 16, 1, "#000");
    if (i === 1) { r(x + 6, y + 4, 4, 2, "#000"); r(x + 8, y + 6, 2, 3, "#000"); r(x + 7, y + 11, 2, 2, "#000"); }
  });
  // coins
  [[140, 100], [156, 100], [172, 100]].forEach(([x, y]) => { r(x + 2, y, 4, 12, "#fcd800"); r(x, y + 2, 8, 8, "#fcd800"); r(x + 3, y + 2, 2, 8, "#fc9838"); });
  // hero
  const hx = 60, hy = 168;
  r(hx + 4, hy, 10, 4, "#d82800"); r(hx + 2, hy + 4, 12, 8, "#fca044"); r(hx + 8, hy + 6, 2, 2, "#000");
  r(hx + 2, hy + 12, 12, 8, "#2038ec"); r(hx, hy + 14, 2, 4, "#fca044"); r(hx + 14, hy + 14, 2, 4, "#fca044");
  r(hx + 2, hy + 20, 4, 4, "#6c3400"); r(hx + 10, hy + 20, 4, 4, "#6c3400");
  // enemy
  const ex = 190, ey = 176;
  r(ex + 2, ey, 12, 8, "#9c4a00"); r(ex, ey + 6, 16, 6, "#9c4a00"); r(ex + 4, ey + 4, 2, 3, "#fcfcfc"); r(ex + 10, ey + 4, 2, 3, "#fcfcfc");
  r(ex + 2, ey + 12, 4, 4, "#000"); r(ex + 10, ey + 12, 4, 4, "#000");
  // HUD
  c.fillStyle = "#fcfcfc"; c.font = "8px monospace";
  c.fillText("SCORE 004200", 16, 16); c.fillText("x03", 120, 16); c.fillText("WORLD 1-2", 176, 16);
}
document.querySelectorAll("canvas[data-scene]").forEach(drawScene);
