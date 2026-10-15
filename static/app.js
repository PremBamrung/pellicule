import PhotoSwipeLightbox from "./vendor/photoswipe/photoswipe-lightbox.esm.min.js";

// Load the PhotoSwipe stylesheet only on pages that have a grid.
const grid = document.querySelector(".grid");
if (grid) {
  const css = document.createElement("link");
  css.rel = "stylesheet";
  css.href = "/static/vendor/photoswipe/photoswipe.css";
  document.head.append(css);

  const narrow = matchMedia("(max-width: 720px)").matches;
  const lightbox = new PhotoSwipeLightbox({
    gallery: ".grid",
    children: "a.tile",
    pswpModule: () => import("./vendor/photoswipe/photoswipe.esm.min.js"),
    bgOpacity: 1,
    showHideAnimationType: "zoom",
    // The slide size is the largest variant (4096 px): "fit" shows the
    // 2048 file, zooming in makes PhotoSwipe request the 4096 one.
    initialZoomLevel: "fit",
    secondaryZoomLevel: 1,
    maxZoomLevel: 1,
    padding: narrow
      ? { top: 44, bottom: 84, left: 0, right: 0 }
      : { top: 32, bottom: 104, left: 32, right: 32 },
  });

  let labelHidden = false;

  lightbox.on("uiRegister", () => {
    lightbox.pswp.ui.registerElement({
      name: "label",
      appendTo: "root",
      onInit: (element, pswp) => {
        const date = line("pswp__label-date");
        const gear = line("pswp__label-gear");
        const exposure = line("pswp__label-exposure");
        element.append(date, gear, exposure);
        element.hidden = labelHidden;

        pswp.on("change", () => {
          const data = pswp.currSlide.data.element?.dataset ?? {};
          date.textContent = data.date ?? "";
          gear.textContent = data.gear ?? "";
          exposure.textContent = data.exposure ?? "";
        });
        // "i" shows or hides the label.
        pswp.on("keydown", ({ originalEvent: event }) => {
          if (event.key === "i") {
            labelHidden = !labelHidden;
            element.hidden = labelHidden;
          }
        });
      },
    });
  });

  lightbox.init();
}

function line(className) {
  const div = document.createElement("div");
  div.className = className;
  return div;
}

// Album covers morph into the album title on the next page.
document.addEventListener("click", (event) => {
  const card = event.target.closest?.(".album-card");
  if (!card) return;
  const title = card.querySelector(".album-card-title");
  if (title) title.style.viewTransitionName = "album-title";
});
// Coming back (bfcache), clear the name so it stays unique.
addEventListener("pageshow", () => {
  for (const title of document.querySelectorAll(".album-card-title")) {
    title.style.viewTransitionName = "";
  }
});
