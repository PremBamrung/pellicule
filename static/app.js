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
    // Filtered-out photos are skipped by the viewer too.
    children: "a.tile:not([hidden])",
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

// ---- theme: white by default, dark on request, remembered per browser --

const toggle = document.querySelector(".theme-toggle");
if (toggle) {
  const root = document.documentElement;
  const label = () => (toggle.textContent = root.dataset.theme === "dark" ? "Light" : "Dark");
  label();
  toggle.hidden = false;
  toggle.addEventListener("click", () => {
    root.dataset.theme = root.dataset.theme === "dark" ? "light" : "dark";
    try {
      localStorage.setItem("theme", root.dataset.theme);
    } catch {}
    label();
  });
}

// ---- filters: day, camera, lens, focal; state lives in the URL ----------

const filters = document.querySelector(".filters");
if (filters && grid) {
  const tiles = [...grid.querySelectorAll("a.tile")];
  const groups = [...filters.querySelectorAll(".filter")];
  const status = filters.querySelector(".filter-status");
  const count = filters.querySelector(".filter-count");
  const noun = (n) => (n === 1 ? "photograph" : "photographs");
  const empty = document.createElement("p");
  empty.className = "filter-empty";
  empty.textContent = "No photographs match these filters.";
  empty.hidden = true;
  grid.before(empty);

  // Selected values per group, e.g. { camera: Set(["Fujifilm X-T3"]) }.
  const selection = () =>
    Object.fromEntries(
      groups.map((group) => [
        group.dataset.filter,
        new Set([...group.querySelectorAll("input:checked")].map((input) => input.value)),
      ]),
    );

  const valueOf = (tile, key) => tile.dataset[`f${key[0].toUpperCase()}${key.slice(1)}`];
  const matches = (tile, selected, except) =>
    Object.entries(selected).every(
      ([key, values]) => key === except || values.size === 0 || values.has(valueOf(tile, key)),
    );

  const apply = () => {
    const selected = selection();
    let shown = 0;
    for (const tile of tiles) {
      const match = matches(tile, selected);
      tile.hidden = !match;
      if (match) shown++;
    }

    // Counts reflect the other active filters, so a choice never leads to an
    // empty grid; options that would are greyed out.
    for (const group of groups) {
      const key = group.dataset.filter;
      const counts = new Map();
      for (const tile of tiles) {
        if (matches(tile, selected, key)) {
          const value = valueOf(tile, key);
          counts.set(value, (counts.get(value) ?? 0) + 1);
        }
      }
      for (const input of group.querySelectorAll("input")) {
        const n = counts.get(input.value) ?? 0;
        input.closest("label").querySelector(".count").textContent = n;
        input.disabled = n === 0 && !input.checked;
      }
    }
    empty.hidden = shown > 0;

    const params = new URLSearchParams();
    for (const group of groups) {
      const values = selected[group.dataset.filter];
      const summary = group.querySelector("summary");
      group.classList.toggle("active", values.size > 0);
      summary.textContent =
        values.size === 0
          ? summary.dataset.label
          : values.size === 1
            ? group.querySelector("input:checked").nextElementSibling.textContent
            : `${summary.dataset.label} (${values.size})`;
      for (const value of values) params.append(group.dataset.filter, value);
    }

    const active = params.size > 0;
    status.hidden = !active;
    count.textContent = `${shown} of ${tiles.length} ${noun(tiles.length)}`;
    const query = params.toString();
    history.replaceState(null, "", query ? `?${query}` : location.pathname);
  };

  // Restore from the URL (bookmarks, shared links, back navigation).
  const initial = new URLSearchParams(location.search);
  for (const input of filters.querySelectorAll("input")) {
    input.checked = initial.getAll(input.name).includes(input.value);
  }

  filters.addEventListener("change", apply);
  filters.querySelector(".filter-clear").addEventListener("click", () => {
    for (const input of filters.querySelectorAll("input")) input.checked = false;
    apply();
  });

  // One menu open at a time; close on outside click or Escape.
  for (const group of groups) {
    group.addEventListener("toggle", () => {
      if (group.open) for (const other of groups) if (other !== group) other.open = false;
    });
  }
  document.addEventListener("click", (event) => {
    if (!event.target.closest(".filter")) for (const group of groups) group.open = false;
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") for (const group of groups) group.open = false;
  });

  filters.hidden = false;
  apply();
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
