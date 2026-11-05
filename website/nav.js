// Main menu: the "Funkciók" panel (dropdown on desktop, accordion on phones)
// and the full-screen phone menu behind the "Menü" button.
(function () {
  const bar = document.getElementById("bar");
  const menuBtn = bar.querySelector(".menu-btn");
  const megaBtn = bar.querySelector(".has-menu > .nav-top");
  const mega = megaBtn.parentElement;
  const phone = window.matchMedia("(max-width: 899px)");

  function setMega(open) {
    mega.classList.toggle("open", open);
    megaBtn.setAttribute("aria-expanded", String(open));
  }

  function setMenu(open) {
    bar.classList.toggle("menu-open", open);
    menuBtn.setAttribute("aria-expanded", String(open));
    document.body.style.overflow = open ? "hidden" : "";
    if (!open) setMega(false);
  }

  megaBtn.addEventListener("click", () => setMega(!mega.classList.contains("open")));
  menuBtn.addEventListener("click", () => setMenu(!bar.classList.contains("menu-open")));

  // Following a link closes everything.
  bar.querySelector(".nav").addEventListener("click", (e) => {
    if (e.target.closest("a")) setMenu(false);
  });

  document.addEventListener("click", (e) => {
    if (!phone.matches && !mega.contains(e.target)) setMega(false);
  });

  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (mega.classList.contains("open") && !phone.matches) {
      setMega(false);
      megaBtn.focus();
    } else if (bar.classList.contains("menu-open")) {
      setMenu(false);
      menuBtn.focus();
    }
  });

  phone.addEventListener("change", () => setMenu(false));
})();
