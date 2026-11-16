// Main menu: the dropdown panels ("Funkciók", "Információk"; accordions on
// phones) and the full-screen phone menu behind the "Menü" button.
(function () {
  const bar = document.getElementById("bar");
  if (!bar) return;
  const menuBtn = bar.querySelector(".menu-btn");
  const menus = Array.from(bar.querySelectorAll(".has-menu"));
  const phone = window.matchMedia("(max-width: 899px)");

  function setMega(menu, open) {
    menu.classList.toggle("open", open);
    menu.querySelector(".nav-top").setAttribute("aria-expanded", String(open));
  }
  const closeAll = (except) => menus.forEach((m) => m !== except && setMega(m, false));

  function setMenu(open) {
    bar.classList.toggle("menu-open", open);
    menuBtn.setAttribute("aria-expanded", String(open));
    document.body.style.overflow = open ? "hidden" : "";
    if (!open) closeAll();
  }

  menus.forEach((m) => {
    m.querySelector(".nav-top").addEventListener("click", () => {
      const open = !m.classList.contains("open");
      // one panel at a time
      closeAll(m);
      setMega(m, open);
    });
  });
  menuBtn.addEventListener("click", () => setMenu(!bar.classList.contains("menu-open")));

  // Following a link closes everything.
  bar.querySelector(".nav").addEventListener("click", (e) => {
    if (e.target.closest("a")) setMenu(false);
  });

  document.addEventListener("click", (e) => {
    if (!phone.matches && !menus.some((m) => m.contains(e.target))) closeAll();
  });

  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    const open = menus.find((m) => m.classList.contains("open"));
    if (open && !phone.matches) {
      setMega(open, false);
      open.querySelector(".nav-top").focus();
    } else if (bar.classList.contains("menu-open")) {
      setMenu(false);
      menuBtn.focus();
    }
  });

  phone.addEventListener("change", () => setMenu(false));
})();
