// Draws the illustrative cemetery map in the hero window. All names are invented.
(function () {
  const svg = document.getElementById("plotmap");
  if (!svg) return;
  const NS = "http://www.w3.org/2000/svg";
  const $ = (id) => document.getElementById(id);

  let seed = 1999;
  const rnd = () => ((seed = (seed * 16807) % 2147483647) - 1) / 2147483646;
  const pick = (a) => a[Math.floor(rnd() * a.length)];

  const FAMILY = ["Nagy", "Kovács", "Tóth", "Szabó", "Horváth", "Varga", "Kiss", "Molnár", "Németh", "Farkas", "Balogh", "Papp", "Takács", "Juhász", "Lakatos", "Mészáros", "Oláh", "Simon", "Rácz", "Fekete"];
  const GIVEN = ["János", "István", "Ilona", "Erzsébet", "József", "Mária", "Ferenc", "Katalin", "Sándor", "Margit", "Imre", "Rozália", "Gábor", "Teréz", "Pál", "Julianna", "Lajos", "Anna"];
  const HOLDER = ["Péter", "Zsolt", "Andrea", "Krisztina", "Tamás", "Éva", "László", "Judit"];
  const MONTHS = ["január", "február", "március", "április", "május", "június", "július", "augusztus", "szeptember", "október", "november", "december"];

  function el(name, attrs, parent) {
    const n = document.createElementNS(NS, name);
    for (const k in attrs) n.setAttribute(k, attrs[k]);
    (parent || svg).appendChild(n);
    return n;
  }

  // Ground: avenue, cross path, cypress row, chapel, gate.
  el("rect", { x: 0, y: 0, width: 880, height: 430, class: "m-ground" });
  el("rect", { x: 425, y: 0, width: 30, height: 430, class: "m-path" });
  el("rect", { x: 0, y: 206, width: 880, height: 22, class: "m-path" });
  for (let y = 14; y < 200; y += 30) {
    // cypresses as tall triangles, the same form as in the emblem
    el("path", { d: `M427 ${y + 21}l4-22 4 22z`, class: "m-tree" });
    el("path", { d: `M445 ${y + 21}l4-22 4 22z`, class: "m-tree" });
  }
  const chapel = el("g", { class: "m-chapel", transform: "translate(668 378)" });
  el("path", { d: "M-30 40V8L0-12 30 8v32z" }, chapel);
  el("path", { d: "M0-30v15M-5-24.5h10", class: "m-cross" }, chapel);
  el("rect", { x: 425, y: 420, width: 30, height: 10, class: "m-gate" });

  const plots = [];
  function parcel(letter, x0, y0, cols, rows, w, h, gx, gy, vault) {
    el("text", { x: x0, y: y0 - 8, class: "m-label" }).textContent = `${letter} parcella`;
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const roll = rnd();
        const state = vault ? (roll < 0.25 ? "free" : "vault") : roll < 0.2 ? "free" : roll < 0.29 ? "due" : "taken";
        plots.push({ letter, row: r + 1, num: c + 1, state, x: x0 + c * (w + gx), y: y0 + r * (h + gy), w, h });
      }
    }
  }
  parcel("A", 22, 32, 14, 4, 20, 30, 8, 12, false);
  parcel("B", 478, 32, 14, 4, 20, 30, 8, 12, false);
  parcel("C", 22, 258, 14, 4, 20, 30, 8, 12, false);
  parcel("D", 478, 258, 7, 1, 40, 30, 14, 0, true);

  const featured = plots.find((p) => p.letter === "B" && p.row === 2 && p.num === 6);
  featured.state = "due";
  plots.filter((p) => p.letter === "D").forEach((p, i) => (p.state = i === 2 || i === 5 ? "free" : "vault"));

  const g = el("g", { class: "m-plots" });
  plots.forEach((p, i) => {
    const node = el("rect", { x: p.x, y: p.y, width: p.w, height: p.h, rx: 1, class: `plot p-${p.state}` }, g);
    node.style.setProperty("--d", `${(p.x * 0.6 + p.y * 1.4) | 0}ms`);
    p.node = node;
    p.i = i;
  });

  // Legend with live counts.
  const LABELS = { free: "Szabad", taken: "Megváltott", due: "Egy éven belül lejár", vault: "Sírbolt" };
  const counts = plots.reduce((m, p) => ((m[p.state] = (m[p.state] || 0) + 1), m), {});
  $("legend").innerHTML = Object.keys(LABELS)
    .map((k) => `<span><i class="dot dot-${k}"></i>${LABELS[k]} <b>${counts[k] || 0}</b></span>`)
    .join("");

  function details(p) {
    if (p === featured) {
      return { label: "Nyugvó", name: "Varga Gábor", years: "1938–2006", until: "2027. április 12-ig", holder: "Varga Péter" };
    }
    const s = seed;
    seed = 7919 * (p.i + 1); // stable text per plot
    const family = pick(FAMILY);
    const name = `${family} ${pick(GIVEN)}`;
    const born = 1900 + Math.floor(rnd() * 60);
    const died = Math.min(2025, born + 45 + Math.floor(rnd() * 45));
    const year = p.state === "due" ? 2027 : 2028 + Math.floor(rnd() * 20);
    const until = `${year}. ${MONTHS[Math.floor(rnd() * 12)]} ${1 + Math.floor(rnd() * 28)}-ig`;
    const holder = `${family} ${pick(HOLDER)}`;
    const places = 4 + 2 * Math.floor(rnd() * 3);
    const used = 1 + Math.floor(rnd() * (places - 1));
    seed = s;
    if (p.state === "free") return { label: "Nyugvó", name: "Nincs", years: "", until: "Megváltható 25 évre", holder: "Nincs" };
    if (p.state === "vault") return { label: "Férőhely", name: `${places} férőhely`, years: `${used} foglalt`, until, holder };
    return { label: "Nyugvó", name, years: `${born}–${died}`, until, holder };
  }

  let current = null;
  function show(p) {
    if (current === p) return;
    if (current) current.node.classList.remove("on");
    current = p;
    p.node.classList.add("on");
    const d = details(p);
    $("pc-where").textContent = p.letter === "D" ? `D parcella, ${p.num}. sírbolt` : `${p.letter} parcella, ${p.row}. sor, ${p.num}. sír`;
    $("pc-state").textContent = LABELS[p.state];
    $("pc-chip").className = `chip chip-${p.state}`;
    $("pc-chip").firstElementChild.className = `dot dot-${p.state}`;
    $("pc-label").textContent = d.label;
    $("pc-name").textContent = d.name;
    $("pc-years").textContent = d.years;
    $("pc-until").textContent = d.until;
    $("pc-holder").textContent = d.holder;
  }

  const nodes = [...g.children];
  svg.addEventListener("pointerover", (e) => {
    const i = nodes.indexOf(e.target);
    if (i >= 0) show(plots[i]);
  });
  svg.addEventListener("click", (e) => {
    const i = nodes.indexOf(e.target);
    if (i >= 0) show(plots[i]);
  });

  // Arrow keys move to the nearest plot in that direction.
  svg.addEventListener("keydown", (e) => {
    const dir = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key];
    if (!dir || !current) return;
    e.preventDefault();
    const cx = current.x + current.w / 2, cy = current.y + current.h / 2;
    let best = null, bestScore = Infinity;
    for (const p of plots) {
      const dx = p.x + p.w / 2 - cx, dy = p.y + p.h / 2 - cy;
      const along = dx * dir[0] + dy * dir[1];
      if (along <= 0) continue;
      const score = along + 3 * Math.abs(dx * dir[1] + dy * dir[0]);
      if (score < bestScore) (bestScore = score), (best = p);
    }
    if (best) show(best);
  });

  show(featured);
  requestAnimationFrame(() => svg.closest(".window").classList.add("ready"));
})();
