// Lists the installers of the latest GitHub release and recommends the one for the
// visitor's system (NFR-PLT-05: download from the product website with platform detection).
(function () {
  const REPO = "thegoldmany/temetkezesnyilvantarta";

  const KINDS = [
    { id: "win-x64", os: "Windows 10 és 11", detail: "64 bites telepítő", cta: "Letöltés Windowsra", match: (n) => /_x64-setup\.exe$/i.test(n) },
    { id: "win-arm64", os: "Windows 11 ARM", detail: "ARM64 telepítő", cta: "Letöltés Windows ARM-ra", match: (n) => /_arm64-setup\.exe$/i.test(n) },
    { id: "mac", os: "macOS 12 vagy újabb", detail: "Apple Silicon és Intel", cta: "Letöltés Macre", match: (n) => /\.dmg$/i.test(n) },
    { id: "win-msi", os: "Windows, rendszergazdai", detail: "MSI-csomag csoportházirendhez", cta: "Letöltés", match: (n) => /_x64_.*\.msi$/i.test(n) },
  ];

  async function detect() {
    const ua = navigator.userAgent;
    const uad = navigator.userAgentData;
    let os = /Mac/i.test(ua) ? "mac" : /Win/i.test(ua) ? "win" : "other";
    let arch = "x64";
    if (uad && uad.getHighEntropyValues) {
      try {
        const v = await uad.getHighEntropyValues(["platform", "architecture"]);
        if (v.platform === "macOS") os = "mac";
        if (v.platform === "Windows") os = "win";
        if (v.architecture === "arm") arch = "arm64";
      } catch (_) {}
    }
    if (os === "mac") return "mac";
    if (os === "win") return arch === "arm64" ? "win-arm64" : "win-x64";
    return null; // Linux, phone: no recommendation
  }

  const size = (bytes) => (bytes / 1024 / 1024).toLocaleString("hu-HU", { maximumFractionDigits: 0 }) + " MB";

  async function main() {
    const $ = (id) => document.getElementById(id);
    let release;
    try {
      const r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } });
      if (!r.ok) throw new Error(String(r.status));
      release = await r.json();
    } catch (_) {
      $("no-release").hidden = false;
      return;
    }
    const found = KINDS.map((k) => ({ ...k, asset: release.assets.find((a) => k.match(a.name)) })).filter((k) => k.asset);
    if (!found.length) {
      $("no-release").hidden = false;
      return;
    }
    const want = await detect();
    const best = found.find((k) => k.id === want) || (want === "win-arm64" && found.find((k) => k.id === "win-x64")) || null;

    const version = release.tag_name.replace(/^v/, "");
    const date = new Date(release.published_at).toLocaleDateString("hu-HU", { year: "numeric", month: "long", day: "numeric" });
    $("release-line").textContent = `Verzió: ${version}, kiadva ${date} Első indításkor egy kitalált mintatemető tölthető be, így adatbevitel nélkül is kipróbálhatja.`;

    const body = $("dl-body");
    body.textContent = "";
    for (const k of found) {
      const tr = document.createElement("tr");
      if (k === best) tr.className = "best";
      tr.innerHTML = "<td><b></b><span></span></td><td class='fn'></td><td class='r'></td><td class='r'><a class='btn btn-sm'></a></td>";
      tr.querySelector("b").textContent = k.os;
      tr.querySelector("td span").textContent = k === best ? `${k.detail}. Ajánlott ehhez a géphez.` : k.detail;
      tr.querySelector(".fn").textContent = k.asset.name;
      tr.querySelector("td.r").textContent = size(k.asset.size);
      const a = tr.querySelector("a");
      a.href = k.asset.browser_download_url;
      a.textContent = "Letöltés";
      a.setAttribute("aria-label", `${k.os}: ${k.asset.name} letöltése`);
      if (k === best) a.classList.add("btn-solid");
      body.appendChild(tr);
    }

    if (best) {
      $("hero-dl").href = best.asset.browser_download_url;
      $("hero-dl").textContent = best.cta;
      $("hero-note").textContent = `${best.detail}, ${size(best.asset.size)}. Más rendszerre a Letöltés részben talál telepítőt.`;
    }
  }

  main();
})();
