// Finds the installer for the visitor's system in the latest GitHub release
// (NFR-PLT-05: download from the product website with platform detection).
(function () {
  const REPO = "thegoldmany/temetkezesnyilvantarta";
  const RELEASES = `https://github.com/${REPO}/releases`;

  const KINDS = [
    { id: "win-x64", label: "Letöltés Windowsra", sub: "64 bites telepítő (.exe)", match: (n) => /_x64-setup\.exe$/i.test(n) },
    { id: "win-arm64", label: "Letöltés Windows ARM-ra", sub: "ARM64 telepítő (.exe)", match: (n) => /_arm64-setup\.exe$/i.test(n) },
    { id: "win-msi", label: "Windows MSI-csomag", sub: "rendszergazdai telepítéshez", match: (n) => /_x64_.*\.msi$/i.test(n) },
    { id: "mac", label: "Letöltés Macre", sub: "Apple Silicon és Intel (.dmg)", match: (n) => /\.dmg$/i.test(n) },
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
    return null; // Linux, phone: show every option
  }

  function size(bytes) {
    return (bytes / 1024 / 1024).toLocaleString("hu-HU", { maximumFractionDigits: 0 }) + " MB";
  }

  async function main() {
    const $ = (id) => document.getElementById(id);
    let release;
    try {
      const r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } });
      if (!r.ok) throw new Error(String(r.status));
      release = await r.json();
    } catch (_) {
      $("no-release").hidden = false;
      $("primary").href = RELEASES;
      $("primary-sub").textContent = "A kiadások listája a GitHubon.";
      return;
    }
    const found = KINDS.map((k) => ({ ...k, asset: release.assets.find((a) => k.match(a.name)) })).filter((k) => k.asset);
    if (!found.length) {
      $("no-release").hidden = false;
      return;
    }
    const want = await detect();
    const primary = found.find((k) => k.id === want) || (want === "win-arm64" && found.find((k) => k.id === "win-x64")) || found[0];
    $("primary").href = primary.asset.browser_download_url;
    $("primary-label").textContent = want ? primary.label : "Letöltés";
    const date = new Date(release.published_at).toLocaleDateString("hu-HU", { year: "numeric", month: "long", day: "numeric" });
    $("primary-sub").textContent = `${primary.sub}, ${size(primary.asset.size)}. Verzió ${release.tag_name.replace(/^v/, "")}, ${date}`;
    const list = $("other-list");
    for (const k of found) {
      if (k === primary) continue;
      const li = document.createElement("li");
      li.innerHTML = `<a href="${k.asset.browser_download_url}"></a> <span></span>`;
      li.querySelector("a").textContent = k.label;
      li.querySelector("span").textContent = `${k.sub}, ${size(k.asset.size)}`;
      list.appendChild(li);
    }
    $("others").hidden = list.children.length === 0;
  }

  main();
})();
