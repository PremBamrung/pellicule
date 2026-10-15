import PhotoSwipeLightbox from "./vendor/photoswipe/photoswipe-lightbox.esm.min.js";
import { thumbHashToDataURL } from "./vendor/thumbhash/thumbhash.js";

const root = document.documentElement;
const remember = (key, value) => {
  try {
    localStorage.setItem(key, value);
  } catch {}
};

// `#p=<key>` names a photo to land on. Read before the filters rewrite the URL.
const landing = new URLSearchParams(location.hash.slice(1)).get("p");

// ---- placeholders: blurred ThumbHash behind each photo, then a fade-in ---

const decode = (element) => {
  const hash = element.dataset.thumbhash;
  if (!hash) return;
  try {
    const bytes = Uint8Array.from(atob(hash), (c) => c.charCodeAt(0));
    element.style.backgroundImage = `url(${thumbHashToDataURL(bytes)})`;
  } catch {}
};

// Decode only what is about to scroll into view: albums can be large.
const nearby = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      decode(entry.target);
      nearby.unobserve(entry.target);
    }
  },
  { rootMargin: "800px 0px" },
);

for (const element of document.querySelectorAll(".ph")) {
  const img = element.querySelector("img");
  if (img && !(img.complete && img.naturalWidth)) nearby.observe(element);
  if (!img) continue;
  if (img.complete && img.naturalWidth) img.classList.add("is-loaded");
  else {
    img.addEventListener("load", () => img.classList.add("is-loaded"), { once: true });
    img.addEventListener("error", () => img.classList.add("is-loaded"), { once: true });
  }
}

// ---- home: a few random photos, crossfading -----------------------------

const hero = document.querySelector(".hero");
if (hero) {
  const slides = [...hero.querySelectorAll(".hero-slide")];
  const albumLink = hero.querySelector(".hero-caption a");
  const date = hero.querySelector(".hero-caption > span:not(.hero-dots)");
  const dots = [...hero.querySelectorAll(".hero-dots button")];
  const still = matchMedia("(prefers-reduced-motion: reduce)");
  let current = 0;
  let timer = null;
  let paused = false;

  // The next photos only start loading when they are about to show.
  const load = (slide) => {
    const img = slide.querySelector("img");
    if (img.dataset.src) {
      img.srcset = img.dataset.srcset;
      img.src = img.dataset.src;
      delete img.dataset.src;
      delete img.dataset.srcset;
    }
    return img.decode().catch(() => {});
  };

  const show = (index) => {
    slides.forEach((slide, i) => {
      const on = i === index;
      slide.classList.toggle("is-current", on);
      slide.toggleAttribute("aria-hidden", !on);
      if (on) slide.removeAttribute("tabindex");
      else slide.tabIndex = -1;
    });
    dots.forEach((dot, i) => dot.setAttribute("aria-current", String(i === index)));
    const slide = slides[index];
    albumLink.textContent = slide.dataset.album;
    albumLink.href = slide.dataset.albumHref;
    date.textContent = slide.dataset.date;
    current = index;
  };

  // Every 6 s, once the next photo is decoded; never while hovered,
  // focused, in a background tab, or when reduced motion is asked for.
  const schedule = () => {
    clearTimeout(timer);
    if (paused || document.hidden || still.matches) return;
    timer = setTimeout(async () => {
      const next = (current + 1) % slides.length;
      await load(slides[next]);
      if (!paused && !document.hidden) show(next);
      schedule();
    }, 6000);
  };
  const pause = () => {
    paused = true;
    clearTimeout(timer);
  };
  const resume = () => {
    paused = false;
    schedule();
  };

  if (slides.length > 1) {
    hero.addEventListener("mouseenter", pause);
    hero.addEventListener("mouseleave", resume);
    hero.addEventListener("focusin", pause);
    hero.addEventListener("focusout", (event) => {
      if (!hero.contains(event.relatedTarget)) resume();
    });
    document.addEventListener("visibilitychange", schedule);
    dots.forEach((dot, i) =>
      dot.addEventListener("click", async () => {
        await load(slides[i]);
        show(i);
      }),
    );
    hero.querySelector(".hero-dots").hidden = false;
    // Start counting once the first photo is in.
    load(slides[0]).then(schedule);
  }
}

// ---- viewer ---------------------------------------------------------------

const photos = document.querySelector(".photos");
if (photos) {
  const css = document.createElement("link");
  css.rel = "stylesheet";
  css.href = "/static/vendor/photoswipe/photoswipe.css";
  document.head.append(css);

  const narrow = matchMedia("(max-width: 720px)").matches;
  const lightbox = new PhotoSwipeLightbox({
    gallery: ".photos",
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
  const PLAY = '<svg aria-hidden="true" viewBox="0 0 32 32" width="32" height="32"><path d="M11 9l12 7-12 7z"/></svg>';
  const PAUSE = '<svg aria-hidden="true" viewBox="0 0 32 32" width="32" height="32"><path d="M10 9h4v14h-4zM18 9h4v14h-4z"/></svg>';

  lightbox.on("uiRegister", () => {
    const pswp = lightbox.pswp;

    pswp.ui.registerElement({
      name: "label",
      appendTo: "root",
      onInit: (element) => {
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

    // Slideshow: the play button or the space bar; 4 s per photo.
    pswp.ui.registerElement({
      name: "play",
      order: 8,
      isButton: true,
      title: "Slideshow (space)",
      html: PLAY,
      onInit: (button) => {
        let timer = null;
        const stop = () => {
          clearInterval(timer);
          timer = null;
          button.innerHTML = PLAY;
          button.title = "Slideshow (space)";
        };
        const toggle = () => {
          if (timer) return stop();
          timer = setInterval(() => pswp.next(), 4000);
          button.innerHTML = PAUSE;
          button.title = "Pause (space)";
        };
        button.addEventListener("click", toggle);
        pswp.on("keydown", (event) => {
          if (event.originalEvent.key === " ") {
            event.preventDefault();
            toggle();
          }
        });
        pswp.on("close", stop);
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

// ---- theme: white by default, dark on request, remembered per browser --

const toggle = document.querySelector(".theme-toggle");
if (toggle) {
  const label = () => (toggle.textContent = root.dataset.theme === "dark" ? "Light" : "Dark");
  label();
  toggle.hidden = false;
  toggle.addEventListener("click", () => {
    root.dataset.theme = root.dataset.theme === "dark" ? "light" : "dark";
    remember("theme", root.dataset.theme);
    label();
  });
}

// ---- grid density: S / M / L, remembered per browser ---------------------

const density = document.querySelector(".density");
if (density) {
  const ROW = { s: 200, m: 300, l: 460 };
  const tiles = document.querySelectorAll("a.tile img");
  const apply = (size) => {
    if (size === "m") delete root.dataset.density;
    else root.dataset.density = size;
    for (const button of density.querySelectorAll("button")) {
      button.setAttribute("aria-pressed", String(button.dataset.density === size));
    }
    // Keep `sizes` in step with the row height so the right file loads.
    for (const img of tiles) {
      const ar = parseFloat(img.parentElement.style.getPropertyValue("--ar")) || 1.5;
      img.sizes = `${Math.round(ar * ROW[size])}px`;
    }
  };
  apply(root.dataset.density ?? "m");
  density.addEventListener("click", (event) => {
    const size = event.target.closest("button")?.dataset.density;
    if (!size) return;
    apply(size);
    remember("density", size);
  });
  density.hidden = false;
}

// ---- filters: day, camera, lens, focal; state lives in the URL ----------

const filters = document.querySelector(".filters");
if (filters && photos) {
  const tiles = [...photos.querySelectorAll("a.tile")];
  const days = [...photos.querySelectorAll(".day")];
  const groups = [...filters.querySelectorAll(".filter")];
  const status = filters.querySelector(".filter-status");
  const count = filters.querySelector(".filter-count");
  const noun = (n) => (n === 1 ? "photograph" : "photographs");
  const empty = document.createElement("p");
  empty.className = "filter-empty";
  empty.textContent = "No photographs match these filters.";
  empty.hidden = true;
  photos.before(empty);

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
    for (const day of days) day.hidden = !day.querySelector("a.tile:not([hidden])");

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

    status.hidden = params.size === 0;
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

// ---- landing on a photo: scroll to it and ring it briefly -----------------

if (landing && photos) {
  const tile = photos.querySelector(`a.tile[data-key="${CSS.escape(landing)}"]`);
  if (tile) {
    // A filter from the URL could hide it.
    if (tile.hidden) document.querySelector(".filter-clear")?.click();
    tile.scrollIntoView({ block: "center" });
    tile.classList.add("is-target");
    tile.addEventListener("animationend", () => tile.classList.remove("is-target"), { once: true });
  }
}

// ---- library page: start, cancel and follow scans -----------------------

const scanPanel = document.querySelector(".scan-panel");
if (scanPanel) {
  const buttons = Object.fromEntries(
    [...scanPanel.querySelectorAll("[data-scan]")].map((b) => [b.dataset.scan, b]),
  );
  const progress = scanPanel.querySelector(".scan-progress");
  const bar = progress.querySelector(".bar");
  const fill = bar.querySelector("span");
  const lineText = progress.querySelector(".scan-line");
  const current = progress.querySelector(".scan-current");
  const message = scanPanel.querySelector(".scan-message");
  let wasRunning = false;
  let timer;

  const minutes = (s) =>
    s < 60 ? `${Math.max(1, Math.round(s))}s` : s < 3600 ? `${Math.round(s / 60)} min` : `${(s / 3600).toFixed(1)} h`;

  const render = (p) => {
    buttons.rescan.disabled = buttons.rebuild.disabled = p.running;
    buttons.cancel.hidden = !p.running;
    progress.hidden = !p.running;
    if (p.running) {
      const finished = p.done + p.failed;
      const known = p.phase === "processing" && p.total > 0;
      bar.classList.toggle("indeterminate", !known);
      fill.style.width = known ? `${(100 * finished) / p.total}%` : "";
      const what = p.rebuild ? "Rebuilding" : "Scanning";
      lineText.textContent =
        p.phase === "walking"
          ? `${what}: looking for new and changed photos…`
          : p.phase === "finishing"
            ? `${what}: finishing…`
            : p.total === 0
              ? `${what}: nothing to process.`
              : [
                  `${what}: ${finished} of ${p.total} photos`,
                  p.failed ? `${p.failed} failed` : null,
                  p.rate ? `${p.rate.toFixed(1)} per second` : null,
                  p.eta_s ? `about ${minutes(p.eta_s)} left` : null,
                ]
                  .filter(Boolean)
                  .join(", ");
      current.textContent = p.current;
    }
    // When a scan ends, reload so the summary and history are up to date.
    if (wasRunning && !p.running) location.reload();
    wasRunning = p.running;
  };

  const poll = async () => {
    clearTimeout(timer);
    try {
      const response = await fetch("/library/status", { cache: "no-store" });
      const p = await response.json();
      render(p);
      timer = setTimeout(poll, p.running ? 1000 : 5000);
    } catch {
      timer = setTimeout(poll, 5000);
    }
  };

  const post = async (url, body) => {
    message.textContent = "";
    const response = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body ?? {}),
    });
    if (!response.ok) message.textContent = await response.text();
    poll();
  };

  buttons.rescan.addEventListener("click", () => post("/library/scan", { rebuild: false }));
  buttons.rebuild.addEventListener("click", () => {
    if (confirm("Reprocess every photo? This can take a long time on a large library.")) {
      post("/library/scan", { rebuild: true });
    }
  });
  buttons.cancel.addEventListener("click", () => post("/library/cancel"));
  poll();
}

// ---- page transitions: an album card's title grows into the album title ---

document.addEventListener("click", (event) => {
  const title = event.target.closest?.(".album-card")?.querySelector(".album-card-title");
  if (title) title.style.viewTransitionName = "album-title";
});
// Coming back (bfcache), clear the name so it stays unique.
addEventListener("pageshow", () => {
  for (const title of document.querySelectorAll(".album-card-title")) {
    title.style.viewTransitionName = "";
  }
});
