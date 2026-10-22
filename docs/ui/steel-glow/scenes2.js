// Placeholder pictures for the other systems and the debugger's tile viewers.
(function () {
  const ctx = (c) => c.getContext("2d");
  document.querySelectorAll("canvas[data-gb]").forEach((cv) => {
    const c = ctx(cv), g = ["#e0f8d0", "#88c070", "#346856", "#081820"];
    c.fillStyle = g[0]; c.fillRect(0, 0, 160, 144);
    c.fillStyle = g[2]; c.fillRect(40, 0, 4, 144); c.fillRect(116, 0, 4, 144);
    const shapes = [[48, 120, 3], [56, 120, 3], [64, 120, 1], [72, 120, 1], [80, 112, 2], [88, 112, 2], [96, 120, 3], [104, 120, 2], [48, 112, 1], [56, 112, 3], [72, 40, 3], [80, 40, 3], [88, 40, 3], [80, 32, 3]];
    shapes.forEach(([x, y, s]) => { c.fillStyle = g[s]; c.fillRect(x, y, 8, 8); c.fillStyle = g[0]; c.fillRect(x + 1, y + 1, 2, 2); });
    c.fillStyle = g[3]; c.font = "8px monospace"; c.fillText("SCORE", 124, 20); c.fillText("1240", 126, 32); c.fillText("LINES", 124, 56); c.fillText("12", 130, 68);
  });
  document.querySelectorAll("canvas[data-test]").forEach((cv) => {
    const c = ctx(cv); c.fillStyle = "#000"; c.fillRect(0, 0, 256, 240);
    c.fillStyle = "#fff"; c.font = "8px monospace";
    ["cpu_timing_test", "", "Official instructions", "Unofficial instr.", "", "Passed"].forEach((t, i) => c.fillText(t, 16, 30 + i * 12));
  });
  document.querySelectorAll("canvas[data-snes]").forEach((cv) => {
    const c = ctx(cv), sky = c.createLinearGradient(0, 0, 0, 100);
    sky.addColorStop(0, "#2a1b5e"); sky.addColorStop(1, "#f08a5d"); c.fillStyle = sky; c.fillRect(0, 0, 256, 100);
    for (let y = 100; y < 224; y++) { const k = (y - 100) / 124, stripe = Math.floor((1 / (k + .05)) * 6) % 2;
      c.fillStyle = stripe ? "#3d7a3a" : "#4f9a48"; c.fillRect(0, y, 256, 1);
      c.fillStyle = "#777"; c.fillRect(128 - 10 - k * 90, y, 20 + k * 180, 1); c.fillStyle = "#fff"; c.fillRect(127, y, 2 + k * 3, 1); }
    c.fillStyle = "#e33"; c.fillRect(116, 180, 24, 10); c.fillStyle = "#ccc"; c.fillRect(120, 174, 16, 6);
  });
  document.querySelectorAll("canvas[data-tiles]").forEach((cv) => {
    const c = ctx(cv), pal = cv.dataset.tiles === "2" ? ["#000", "#c84c0c", "#fcbcb0", "#fc9838"] : ["#000", "#5c94fc", "#fcfcfc", "#00a800"];
    let s = cv.dataset.tiles === "2" ? 7 : 3; const rnd = () => (s = (s * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
    for (let ty = 0; ty < 16; ty++) for (let tx = 0; tx < 16; tx++) { if (rnd() < .25) continue;
      for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) { const v = Math.floor(rnd() * 4 * (Math.abs(x - 3.5) + Math.abs(y - 3.5) < 5 ? 1 : .3)); c.fillStyle = pal[v]; c.fillRect(tx * 8 + x, ty * 8 + y, 1, 1); } }
  });
})();
