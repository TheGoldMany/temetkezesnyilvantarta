// Payment page of an operator's invoice: /fizetes?t=<link>[&r=<payment request>]
(function () {
  var q = new URLSearchParams(location.search);
  var token = q.get("t") || "";
  var request = q.get("r") || "";
  var $ = function (id) { return document.getElementById(id); };
  var status = $("pay-status");
  var huf = function (n) { return n.toLocaleString("hu-HU") + " Ft"; };
  var date = function (d) { return d ? d.replace(/-/g, ". ") + "." : "–"; };
  var say = function (text, tone) { status.textContent = text; status.className = "pay-status" + (tone ? " pay-" + tone : ""); };
  var api = function (path, opts) {
    return fetch("/api/public/pay" + path + (path.indexOf("?") < 0 ? "?" : "&") + "t=" + encodeURIComponent(token), opts).then(function (r) {
      return r.json().then(function (j) { if (!r.ok) throw j; return j; });
    });
  };
  var errorText = function (e) {
    var c = e && e.error;
    if (c === "not_found") return "Ez a fizetési link nem érvényes. Kérjen újat a temető irodájától.";
    if (c === "rate_limited") return "Túl sok próbálkozás. Kérjük, várjon egy percet.";
    if (c === "invoice_not_open") return "Ez a számla már nem fizethető online (kifizették vagy érvénytelenítették).";
    if (c === "online_payment_off") return "A temető jelenleg nem fogad online fizetést. Kérjük, fizessen átutalással vagy a temető irodájában.";
    if (c === "terms_required") return "A fizetéshez fogadja el a fizetési feltételeket.";
    return "A fizetőoldalt most nem sikerült megnyitni. Kérjük, próbálja újra pár perc múlva.";
  };

  function show(info) {
    $("pay-card").hidden = false;
    $("pay-operator").textContent = info.operator || "–";
    $("pay-cemetery").textContent = info.cemetery || "–";
    $("pay-number").textContent = info.number || "–";
    $("pay-payer").textContent = info.payer || "–";
    $("pay-due").textContent = date(info.dueDate);
    $("pay-amount").textContent = huf(info.state === "paid" ? 0 : info.dueHuf);
    if (info.state === "paid") return say("Ez a számla ki van fizetve. Köszönjük!", "ok");
    if (info.state !== "open") return say("Ez a számla már nem fizethető online.", "warn");
    if (!info.enabled) return say(errorText({ error: "online_payment_off" }), "warn");
    say("");
    $("pay-terms-link").href = info.termsUrl;
    $("pay-form").hidden = false;
  }

  function load() {
    return api("").then(show, function (e) { say(errorText(e), "warn"); });
  }

  $("pay-terms").addEventListener("change", function (e) { $("pay-button").disabled = !e.target.checked; });
  $("pay-form").addEventListener("submit", function (e) {
    e.preventDefault();
    var button = $("pay-button");
    button.disabled = true;
    button.textContent = "Barion megnyitása…";
    api("/start", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ termsAccepted: $("pay-terms").checked }) }).then(
      function (r) { location.assign(r.gatewayUrl); },
      function (err) { say(errorText(err), "warn"); button.disabled = false; button.textContent = "Fizetés bankkártyával"; }
    );
  });

  if (!/^[0-9a-f]{32}$/.test(token)) return say(errorText({ error: "not_found" }), "warn");
  if (!request) return load();
  // back from Barion: ask the payment's state, then show the invoice
  say("A fizetés ellenőrzése…");
  var tries = 0;
  (function check() {
    api("/status?r=" + encodeURIComponent(request)).then(
      function (r) {
        if (r.status === "Succeeded") {
          history.replaceState(null, "", location.pathname + "?t=" + token);
          return load().then(function () { say("Sikeres fizetés. Köszönjük! A befizetés a számlán is megjelent.", "ok"); });
        }
        if (["Canceled", "Failed", "Expired", "Rejected"].indexOf(r.status) >= 0) {
          return load().then(function () { say("A fizetés nem sikerült vagy megszakadt. Újra megpróbálhatja.", "warn"); });
        }
        if (++tries < 6) return setTimeout(check, 2500);
        load().then(function () { say("A fizetés feldolgozása folyamatban van. Néhány perc múlva frissítse az oldalt.", "warn"); });
      },
      function (err) { say(errorText(err), "warn"); }
    );
  })();
})();
