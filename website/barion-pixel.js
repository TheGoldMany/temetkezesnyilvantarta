// Barion Base Pixel: fraud prevention for card payments (required by Barion;
// see the privacy notice). Loaded as a file so the strict Content Security
// Policy needs no inline script. Set BARION_PIXEL_ID to the "BP-…" id shown
// at the Barion shop's details.
(function () {
  var BARION_PIXEL_ID = "BP-XXXXXXXXXX-XX";
  if (!/^BP-\d{10}-\d{2}$/.test(BARION_PIXEL_ID)) return;
  window.bp =
    window.bp ||
    function () {
      (window.bp.q = window.bp.q || []).push(arguments);
    };
  window.bp.l = 1 * new Date();
  var s = document.createElement("script");
  s.async = true;
  s.src = "https://pixel.barion.com/bp.js";
  document.head.appendChild(s);
  window.barion_pixel_id = BARION_PIXEL_ID;
  window.bp("init", "addBarionPixelId", BARION_PIXEL_ID);
})();
