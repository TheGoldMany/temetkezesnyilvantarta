// temisz.hu/bemutato: the table is sent as JSON (base64) to /api/public/demo-request.
(function () {
  var form = document.getElementById("demo-form");
  var status = document.getElementById("demo-status");
  var button = document.getElementById("demo-send");
  var MAX = 3 * 1024 * 1024;
  var messages = {
    required: "Töltse ki a település nevét és a saját nevét.",
    email_invalid: "Ellenőrizze az e-mail-címet.",
    consent_required: "Kérjük, fogadja el az adatkezelési feltételeket.",
    file_type: "Excel-, Word-, CSV- vagy PDF-fájlt töltsön fel.",
    file_too_large: "A fájl legfeljebb 3 MB lehet. Küldjön kisebbet, vagy hívjon minket.",
    file_missing: "Válassza ki a feltöltendő táblázatot.",
    rate_limited: "Túl sok kérés érkezett erről a gépről. Próbálja újra egy óra múlva, vagy hívjon minket.",
  };
  function show(text, ok) {
    status.textContent = text;
    status.className = "pay-status " + (ok ? "pay-ok" : "pay-warn");
  }
  form.addEventListener("submit", function (e) {
    e.preventDefault();
    var f = form.elements;
    var file = f.file.files[0];
    if (!f.settlement.value.trim() || !f.name.value.trim()) return show(messages.required);
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(f.email.value.trim())) return show(messages.email_invalid);
    if (!file) return show(messages.file_missing);
    if (file.size > MAX) return show(messages.file_too_large);
    if (!f.consent.checked) return show(messages.consent_required);
    button.disabled = true;
    show("Feltöltés…", true);
    var reader = new FileReader();
    reader.onload = function () {
      var b64 = String(reader.result).split(",")[1] || "";
      fetch("/api/public/demo-request", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          settlement: f.settlement.value,
          name: f.name.value,
          role: f.role.value,
          email: f.email.value,
          phone: f.phone.value,
          message: f.message.value,
          consent: f.consent.checked,
          fileName: file.name,
          content: b64,
        }),
      })
        .then(function (r) {
          return r.json().then(function (body) {
            return { ok: r.ok, body: body };
          });
        })
        .then(function (r) {
          if (r.ok) {
            form.reset();
            show("Köszönjük! Megkaptuk a táblázatot, és visszaigazolást küldtünk e-mailben. Egy munkanapon belül jelentkezünk a belépéssel.", true);
          } else {
            show(messages[r.body.error] || "A feltöltés nem sikerült. Próbálja újra, vagy hívjon minket.");
          }
        })
        .catch(function () {
          show("A feltöltés nem sikerült. Ellenőrizze az internetkapcsolatot, vagy hívjon minket.");
        })
        .then(function () {
          button.disabled = false;
        });
    };
    reader.readAsDataURL(file);
  });
})();
