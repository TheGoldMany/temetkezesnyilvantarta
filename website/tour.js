// Feature tour: an ARIA tab list that switches the panes in the program window.
(function () {
  const tabs = [...document.querySelectorAll('.tour-list [role="tab"]')];
  if (!tabs.length) return;
  const title = document.getElementById("tour-title");

  function select(tab, focus) {
    for (const t of tabs) {
      const on = t === tab;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      const pane = document.getElementById(t.getAttribute("aria-controls"));
      pane.hidden = !on;
      if (on) title.textContent = pane.dataset.title;
    }
    if (focus) tab.focus();
  }

  tabs.forEach((t, i) => {
    t.addEventListener("click", () => select(t, false));
    t.addEventListener("keydown", (e) => {
      const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
      if (step) {
        e.preventDefault();
        select(tabs[(i + step + tabs.length) % tabs.length], true);
      } else if (e.key === "Home" || e.key === "End") {
        e.preventDefault();
        select(tabs[e.key === "Home" ? 0 : tabs.length - 1], true);
      }
    });
  });
})();
