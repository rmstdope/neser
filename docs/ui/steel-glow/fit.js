// Scales each 1280x800 drawing to the width of the page, and paints icons and game frames.
(function () {
  const icons = {
    play: '<path d="M7 5l12 7-12 7z"/>', pause: '<path d="M8 5v14M16 5v14"/>', reset: '<path d="M4 12a8 8 0 1 0 3-6.2"/><path d="M4 4v5h5"/>',
    save: '<path d="M5 4h11l3 3v13H5z"/><path d="M8 4v5h7V4M8 20v-6h8v6"/>', load: '<path d="M12 3v12M7 10l5 5 5-5"/><path d="M4 20h16"/>',
    eject: '<path d="M12 5l7 8H5z"/><path d="M5 18h14"/>', zoomin: '<circle cx="11" cy="11" r="6"/><path d="M20 20l-4.5-4.5M11 8v6M8 11h6"/>',
    zoomout: '<circle cx="11" cy="11" r="6"/><path d="M20 20l-4.5-4.5M8 11h6"/>', filter: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 9h18M3 13h18M3 17h18" opacity=".5"/>',
    palette: '<circle cx="12" cy="12" r="9"/><circle cx="8" cy="10" r="1.2"/><circle cx="12" cy="7.5" r="1.2"/><circle cx="16" cy="10" r="1.2"/><path d="M12 21a3 3 0 0 1 0-6h2"/>',
    vol: '<path d="M4 9h4l5-4v14l-5-4H4z"/><path d="M16 9a4 4 0 0 1 0 6M18.5 6.5a8 8 0 0 1 0 11"/>', mute: '<path d="M4 9h4l5-4v14l-5-4H4z"/><path d="M17 9l5 6M22 9l-5 6"/>',
    full: '<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/>', gear: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1"/>',
    help: '<circle cx="12" cy="12" r="9"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6V14M12 17h.01"/>', lib: '<rect x="4" y="4" width="6" height="16" rx="1"/><rect x="12" y="4" width="4" height="16" rx="1"/><path d="M18 5l3 14"/>',
    pad: '<path d="M6 8h12a4 4 0 0 1 4 4v1a4 4 0 0 1-7 2.6L14 14h-4l-1 1.6A4 4 0 0 1 2 13v-1a4 4 0 0 1 4-4z"/><path d="M7 11v3M5.5 12.5h3"/><circle cx="16.5" cy="11.5" r=".8"/><circle cx="18" cy="13.5" r=".8"/>',
    bug: '<rect x="7" y="7" width="10" height="13" rx="5"/><path d="M9 7a3 3 0 0 1 6 0M3 12h4M17 12h4M4 7l3 2M20 7l-3 2M4 18l3-2M20 18l-3-2"/>',
    rec: '<circle cx="12" cy="12" r="7"/>', x: '<path d="M6 6l12 12M18 6L6 18"/>', chev: '<path d="M6 9l6 6 6-6"/>', check: '<path d="M5 12l5 5 9-10"/>',
    up: '<path d="M12 19V5M5 12l7-7 7 7"/>', search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/>', alert: '<circle cx="12" cy="12" r="9"/><path d="M12 7v6M12 16.5h.01"/>',
    chip: '<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4"/>', menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
    step: '<path d="M5 12h10M11 6l6 6-6 6M20 5v14"/>', stepin: '<path d="M12 4v11M7 10l5 5 5-5M5 20h14"/>', more: '<circle cx="5" cy="12" r="1.3"/><circle cx="12" cy="12" r="1.3"/><circle cx="19" cy="12" r="1.3"/>',
    file: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/>', trash: '<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/>', clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
    star: '<path d="M12 3l2.8 5.8 6.2.9-4.5 4.4 1 6.2L12 17.3 6.5 20.3l1-6.2L3 9.7l6.2-.9z"/>', info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5h.01"/>'
  };
  const sprite = '<svg width="0" height="0" style="position:absolute">' +
    Object.entries(icons).map(([k, v]) => `<symbol id="i-${k}" viewBox="0 0 24 24">${v}</symbol>`).join("") + "</svg>";
  document.body.insertAdjacentHTML("afterbegin", sprite);
  document.querySelectorAll("i[data-i]").forEach((el) => {
    el.outerHTML = `<svg class="ico ${el.className}" style="${el.getAttribute("style") || ""}"><use href="#i-${el.dataset.i}"/></svg>`;
  });
  function fit() {
    document.querySelectorAll(".fit").forEach((f) => {
      const s = f.firstElementChild, w = s.offsetWidth, h = s.offsetHeight, k = f.clientWidth / w;
      s.style.transform = `scale(${k})`; f.style.height = h * k + "px";
    });
  }
  addEventListener("resize", fit); fit();
})();
// Drawing tool: #only=N shows a single numbered screen, for screenshots.
(function () {
  const m = location.hash.match(/only=(\d+)/); if (!m) return;
  document.querySelectorAll(".shot").forEach((s, i) => { if (i + 1 !== +m[1]) s.style.display = "none"; });
  document.querySelector(".sheet > header").style.display = "none";
  dispatchEvent(new Event("resize"));
})();
