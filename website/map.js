// Draws the illustrative cemetery map in the hero. All names are invented.
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
  const MONTHS = ["január", "február", "március", "április", "május", "június", "július", "augusztus", "szeptember", "október", "november", "december"];

  function el(name, attrs, parent) {
    const n = document.createElementNS(NS, name);
    for (const k in attrs) n.setAttribute(k, attrs[k]);
    (parent || svg).appendChild(n);
    return n;
  }

  // Ground: avenue, cross path, cypress rows, chapel, gate.
  el("rect", { x: 0, y: 0, width: 640, height: 520, rx: 18, class: "m-ground" });
  el("rect", { x: 302, y: 0, width: 36, height: 520, class: "m-path" });
  el("rect", { x: 0, y: 250, width: 640, height: 26, class: "m-path" });
  for (let y = 22; y < 240; y += 34) {
    el("ellipse", { cx: 309, cy: y + 10, rx: 4.5, ry: 11, class: "m-tree" });
    el("ellipse", { cx: 331, cy: y + 10, rx: 4.5, ry: 11, class: "m-tree" });
  }
  const chapel = el("g", { class: "m-chapel", transform: "translate(470 438)" });
  el("path", { d: "M-34 46V8L0-14 34 8v38z" }, chapel);
  el("path", { d: "M0-34v18M-6-28h12", class: "m-cross" }, chapel);
  el("rect", { x: 300, y: 506, width: 40, height: 14, class: "m-gate" });

  const plots = [];
  function parcel(letter, x0, y0, cols, rows, w, h, gx, gy, vault) {
    el("text", { x: x0, y: y0 - 7, class: "m-label" }).textContent = `${letter} parcella`;
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const roll = rnd();
        const state = vault ? (roll < 0.2 ? "free" : "vault") : roll < 0.2 ? "free" : roll < 0.3 ? "due" : "taken";
        plots.push({ letter, row: r + 1, num: c + 1, state, x: x0 + c * (w + gx), y: y0 + r * (h + gy), w, h });
      }
    }
  }
  parcel("A", 26, 34, 10, 5, 20, 30, 6.4, 12, false);
  parcel("B", 356, 34, 10, 5, 20, 30, 6.4, 12, false);
  parcel("C", 26, 304, 10, 4, 20, 30, 6.4, 12, false);
  parcel("D", 356, 304, 5, 2, 38, 30, 14, 12, true);

  const featured = plots.find((p) => p.letter === "B" && p.row === 3 && p.num === 6);
  featured.state = "due";

  const g = el("g", { class: "m-plots" });
  plots.forEach((p, i) => {
    const node = el("rect", { x: p.x, y: p.y, width: p.w, height: p.h, rx: 3.5, class: `plot p-${p.state}` }, g);
    node.style.setProperty("--d", `${(p.x + p.y) * 1.4}ms`);
    p.node = node;
    p.i = i;
  });

  function details(p) {
    const s = seed;
    seed = 7919 * (p.i + 1); // stable text per plot
    const name = `${pick(FAMILY)} ${pick(GIVEN)}`;
    const born = 1900 + Math.floor(rnd() * 60);
    const died = Math.min(2025, born + 45 + Math.floor(rnd() * 45));
    const year = p.state === "due" ? 2027 : 2028 + Math.floor(rnd() * 20);
    const date = `${year}. ${MONTHS[Math.floor(rnd() * 12)]} ${1 + Math.floor(rnd() * 28)}.`;
    const places = 4 + 2 * Math.floor(rnd() * 3);
    const used = 1 + Math.floor(rnd() * (places - 1));
    seed = s;
    const where = p.letter === "D" ? `D parcella, ${p.row}. sor, ${p.num}. sírbolt` : `${p.letter} parcella, ${p.row}. sor, ${p.num}. sír`;
    switch (p.state) {
      case "free":
        return { where, name: "Szabad sírhely", years: "Egyes sírhely", state: "Megváltható 25 évre", dot: "free" };
      case "vault":
        return { where, name: `${name.split(" ")[0]} család sírboltja`, years: `${places} férőhely, ${used} foglalt`, state: `Megváltva: ${date}-ig`, dot: "vault" };
      case "due":
        return { where, name, years: `${born}–${died}`, state: `Lejár: ${date}`, dot: "due" };
      default:
        return { where, name, years: `${born}–${died}`, state: `Megváltva: ${date}-ig`, dot: "taken" };
    }
  }

  const card = $("plotcard");
  const figure = svg.parentElement;
  let current = null;

  function show(p) {
    if (current === p) return;
    if (current) current.node.classList.remove("on");
    current = p;
    p.node.classList.add("on");
    const d = p === featured ? { where: "B parcella, 3. sor, 6. sír", name: "Varga Gábor", years: "1938–2006", state: "Lejár: 2027. április 12.", dot: "due" } : details(p);
    $("pc-where").textContent = d.where;
    $("pc-name").textContent = d.name;
    $("pc-years").textContent = d.years;
    const st = $("pc-state");
    st.textContent = d.state;
    const dot = document.createElement("span");
    dot.className = `dot dot-${d.dot}`;
    st.prepend(dot);
    place(p);
  }

  function place(p) {
    if (!window.matchMedia("(min-width: 720px)").matches) {
      card.style.left = card.style.top = "";
      return;
    }
    const box = svg.getBoundingClientRect();
    const fig = figure.getBoundingClientRect();
    const k = box.width / 640;
    const right = p.x + p.w / 2 < 380;
    const x = box.left - fig.left + (right ? p.x + p.w + 10 : p.x - 10) * k;
    const y = box.top - fig.top + (p.y + p.h / 2) * k;
    card.style.left = `${x}px`;
    card.style.top = `${y}px`;
    card.dataset.side = right ? "right" : "left";
  }

  svg.addEventListener("pointerover", (e) => {
    const p = e.target.classList && e.target.classList.contains("plot") && plots[[...g.children].indexOf(e.target)];
    if (p) show(p);
  });
  svg.addEventListener("click", (e) => {
    const i = [...g.children].indexOf(e.target);
    if (i >= 0) show(plots[i]);
  });
  window.addEventListener("resize", () => current && place(current));

  show(featured);
  requestAnimationFrame(() => figure.classList.add("ready"));
})();
