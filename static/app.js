import PhotoSwipeLightbox from "./vendor/photoswipe/photoswipe-lightbox.esm.min.js";
import { thumbHashToDataURL } from "./vendor/thumbhash/thumbhash.js";

const root = document.documentElement;
const remember = (key, value) => {
  try {
    localStorage.setItem(key, value);
  } catch {}
};

// `#p=<key>` names a photo to open. Read before the filters rewrite the URL.
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

// ---- home: rows of whole photos, taking turns ------------------------------

const hero = document.querySelector(".hero");
if (hero) {
  const frame = hero.querySelector(".hero-frame");
  const pool = [...frame.querySelectorAll(".hero-photo")].map((element) => ({
    element,
    ar: parseFloat(element.style.getPropertyValue("--ar")) || 1.5,
  }));
  const label = hero.querySelector(".hero-label");
  const dotsBox = hero.querySelector(".hero-dots");
  const still = matchMedia("(prefers-reduced-motion: reduce)");
  let rows = [];
  let current = 0;
  let timer = null;
  let paused = false;

  // The next photos only start loading when their row is about to show.
  const load = (row) =>
    Promise.all(
      [...row.element.querySelectorAll("img")].map((img) => {
        if (img.dataset.src) {
          img.srcset = img.dataset.srcset;
          img.src = img.dataset.src;
          delete img.dataset.src;
          delete img.dataset.srcset;
        }
        return img.decode().catch(() => {});
      }),
    );

  // Rows of whole photos at one height, close to the target: the order is
  // random anyway, so each row is the combination of the remaining photos
  // (up to five) that fills the width at the height nearest the target. A
  // row that would be too tall is held at the target and left narrower.
  const split = () => {
    const width = frame.clientWidth;
    const target = Math.min(innerHeight * 0.55, width * 0.75);
    const gap = parseFloat(getComputedStyle(frame).columnGap) || 0;
    const fit = (count, ratio) => (width - gap * (count - 1)) / ratio;
    // Share of the frame left empty: above a short row, or beside a narrow one.
    const waste = (count, ratio) => {
      const height = fit(count, ratio);
      return height < target ? 1 - height / target : 1 - (target * ratio + gap * (count - 1)) / width;
    };

    const groups = [];
    let left = [...pool];
    while (left.length && groups.length < 6) {
      let best = null;
      const search = (from, items, ratio) => {
        if (items.length) {
          const cost = waste(items.length, ratio);
          if (!best || cost < best.cost) best = { items: [...items], cost };
          // More photos only make the row lower.
          if (items.length === 5 || fit(items.length, ratio) < target) return;
        }
        for (let i = from; i < left.length; i++) {
          items.push(left[i]);
          search(i + 1, items, ratio + left[i].ar);
          items.pop();
        }
      };
      search(0, [], 0);
      // Keep going while rows fit well; the first one is always shown.
      if (groups.length && best.cost > 0.12) break;
      // In their random order, not the search's.
      const items = left.filter((photo) => best.items.includes(photo));
      groups.push({ items, height: Math.min(target, fit(items.length, best.items.reduce((sum, p) => sum + p.ar, 0))) });
      left = left.filter((photo) => !best.items.includes(photo));
    }
    return groups;
  };

  const layout = () => {
    for (const row of rows) row.element.remove();
    const groups = split();
    for (const photo of pool) photo.element.hidden = !groups.some((g) => g.items.includes(photo));
    rows = groups.map(({ items, height }) => {
      const element = document.createElement("div");
      element.className = "hero-row";
      for (const { element: photo, ar } of items) {
        const width = Math.floor(ar * height);
        photo.style.width = `${width}px`;
        photo.style.height = `${Math.round(height)}px`;
        photo.querySelector("img").sizes = `${width}px`;
        element.append(photo);
      }
      frame.append(element);
      return { element };
    });
    frame.style.height = `${Math.round(Math.max(...groups.map((g) => g.height)))}px`;

    dotsBox.replaceChildren(
      ...rows.map((_, i) => {
        const dot = document.createElement("button");
        dot.type = "button";
        dot.setAttribute("aria-label", `Photos ${i + 1} of ${rows.length}`);
        dot.addEventListener("click", async () => {
          await load(rows[i]);
          show(i);
        });
        return dot;
      }),
    );
    dotsBox.hidden = rows.length < 2;
    show(Math.min(current, rows.length - 1));
  };

  const show = (index) => {
    rows.forEach((row, i) => {
      const on = i === index;
      row.element.classList.toggle("is-current", on);
      row.element.toggleAttribute("aria-hidden", !on);
      for (const link of row.element.querySelectorAll("a")) {
        if (on) link.removeAttribute("tabindex");
        else link.tabIndex = -1;
      }
    });
    [...dotsBox.children].forEach((dot, i) => dot.setAttribute("aria-current", String(i === index)));
    caption(null);
    current = index;
  };

  // Under the row: the album and date of the photo pointed at or focused.
  const caption = (photo) => {
    const [link, date] = label.children;
    link.textContent = photo?.dataset.album ?? "";
    link.href = photo?.dataset.albumHref ?? "";
    date.textContent = photo?.dataset.date ?? "";
  };
  frame.addEventListener("pointerover", (event) => caption(event.target.closest(".hero-photo")));
  frame.addEventListener("focusin", (event) => caption(event.target.closest(".hero-photo")));
  frame.addEventListener("pointerleave", () => caption(null));

  // Every 8 s, once the next row is decoded; never while hovered, focused,
  // in a background tab, or when reduced motion is asked for.
  const schedule = () => {
    clearTimeout(timer);
    if (paused || document.hidden || still.matches || rows.length < 2) return;
    timer = setTimeout(async () => {
      const next = (current + 1) % rows.length;
      await load(rows[next]);
      if (!paused && !document.hidden) show(next);
      schedule();
    }, 8000);
  };
  hero.addEventListener("mouseenter", () => {
    paused = true;
    clearTimeout(timer);
  });
  hero.addEventListener("mouseleave", () => {
    paused = false;
    schedule();
  });
  hero.addEventListener("focusin", () => {
    paused = true;
    clearTimeout(timer);
  });
  hero.addEventListener("focusout", (event) => {
    if (hero.contains(event.relatedTarget)) return;
    paused = false;
    schedule();
  });
  document.addEventListener("visibilitychange", schedule);

  // Rows depend on the width: split again when it changes.
  let width = frame.clientWidth;
  let resizing;
  addEventListener("resize", () => {
    clearTimeout(resizing);
    resizing = setTimeout(() => {
      if (frame.clientWidth === width) return;
      width = frame.clientWidth;
      layout();
      load(rows[current]);
    }, 200);
  });

  layout();
  // Start counting once the first row is in.
  if (rows.length) load(rows[0]).then(schedule);
}

// ---- album cards: the pointer flips the cover through the strip -----------

// Across the cover, left to right: the cover, then each strip photo; over
// the strip, the photo pointed at. Larger versions load when the pointer
// arrives, and the cover only switches to one that is decoded.
if (matchMedia("(hover: hover) and (pointer: fine)").matches) {
  for (const card of document.querySelectorAll(".album-card")) {
    const cover = card.querySelector(".album-cover img");
    const strip = [...card.querySelectorAll(".album-strip-photo")];
    if (!cover || !strip.length) continue;
    const frames = [
      { src: cover.getAttribute("src"), srcset: cover.getAttribute("srcset"), ready: true },
      ...strip.map((photo) => ({ src: photo.dataset.coverSrc, srcset: photo.dataset.coverSrcset })),
    ];
    let shown = 0;
    let wanted = 0;
    let loading = null;
    const preload = () => {
      loading ??= frames.slice(1).map((frame, i) => {
        const img = new Image();
        img.sizes = cover.sizes;
        img.srcset = frame.srcset;
        img.src = frame.src;
        img.decode().then(() => {
          frame.ready = true;
          if (wanted === i + 1) show(i + 1);
        }, () => {});
        return img;
      });
    };
    const show = (index) => {
      wanted = index;
      if (index === shown || !frames[index].ready) return;
      shown = index;
      cover.srcset = frames[index].srcset;
      cover.src = frames[index].src;
      card.classList.toggle("is-flipping", index > 0);
      strip.forEach((photo, i) => photo.classList.toggle("is-shown", i === index - 1));
    };

    const area = cover.parentElement;
    area.addEventListener("pointerenter", preload);
    area.addEventListener("pointermove", (event) => {
      const box = area.getBoundingClientRect();
      const at = Math.floor(((event.clientX - box.left) / box.width) * frames.length);
      show(Math.max(0, Math.min(frames.length - 1, at)));
    });
    strip.forEach((photo, i) =>
      photo.addEventListener("pointerenter", () => {
        preload();
        show(i + 1);
      }),
    );
    card.addEventListener("pointerleave", () => show(0));
  }
}

// ---- viewer ---------------------------------------------------------------

const photos = document.querySelector(".photos");
// Opens the viewer on a tile; set up below when the page has photos.
let openPhoto = null;
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
    // Focus goes back to the photo last seen, not the one opened (below).
    returnFocus: false,
  });

  // The open photo is in the URL (`#p=<key>`): Back closes the viewer, and
  // a reload or a shared link opens it again on the same photo.
  const keyOf = (pswp) => pswp.currSlide?.data.element?.dataset.key;
  const urlFor = (key) => `${location.pathname}${location.search}${key ? `#p=${key}` : ""}`;
  // How this opening got its history entry: "pushed" by a click on a tile,
  // or the "link" it was opened from (Back then leaves for where it came from).
  let entry = null;
  let openedFromLink = false;
  let closing = false;
  let lastKey = null;

  lightbox.on("change", () => {
    lastKey = keyOf(lightbox.pswp);
    if (entry) {
      history.replaceState(history.state, "", urlFor(lastKey));
    } else if (openedFromLink) {
      entry = "link";
      history.replaceState(null, "", urlFor(lastKey));
    } else {
      entry = "pushed";
      history.pushState({ viewer: true }, "", urlFor(lastKey));
    }
  });
  addEventListener("popstate", () => {
    if (!lightbox.pswp || closing) return;
    entry = null; // the browser already left the viewer's entry
    lightbox.pswp.close();
  });
  lightbox.on("close", () => {
    closing = true;
    // Bring the photo into view so the viewer shrinks back onto it.
    lightbox.pswp.currSlide?.data.element?.scrollIntoView({ block: "center" });
    if (entry === "pushed") history.back();
    else if (entry === "link") history.replaceState(null, "", urlFor(null));
  });
  lightbox.on("destroy", () => {
    entry = null;
    openedFromLink = closing = false;
    document.querySelector(`a.tile[data-key="${CSS.escape(lastKey ?? "")}"]`)?.focus({ preventScroll: true });
  });

  openPhoto = (tile) => {
    const index = [...photos.querySelectorAll("a.tile:not([hidden])")].indexOf(tile);
    if (index < 0) return;
    openedFromLink = true;
    lightbox.loadAndOpen(index);
  };

  const SHORTCUTS = [
    ["← →", "Previous, next"],
    ["Space", "Slideshow"],
    ["F", "Fullscreen"],
    ["I", "Show or hide the label"],
    ["?", "This list"],
    ["Esc", "Close"],
  ];
  let labelHidden = false;
  const PLAY = '<svg aria-hidden="true" viewBox="0 0 32 32" width="32" height="32"><path d="M11 9l12 7-12 7z"/></svg>';
  const PAUSE = '<svg aria-hidden="true" viewBox="0 0 32 32" width="32" height="32"><path d="M10 9h4v14h-4zM18 9h4v14h-4z"/></svg>';
  const ENTER_FULL = '<svg aria-hidden="true" viewBox="0 0 32 32" width="32" height="32"><path d="M9 14V9h5M18 9h5v5M23 18v5h-5M14 23H9v-5"/></svg>';
  const EXIT_FULL = '<svg aria-hidden="true" viewBox="0 0 32 32" width="32" height="32"><path d="M14 9v5H9M23 14h-5V9M18 23v-5h5M9 18h5v5"/></svg>';

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

    // Fullscreen: the button or "f". Leaving the viewer leaves fullscreen.
    // Not offered where pages can't go fullscreen (iPhone).
    if (document.fullscreenEnabled) {
      pswp.ui.registerElement({
        name: "fullscreen",
        order: 9,
        isButton: true,
        title: "Fullscreen (f)",
        html: ENTER_FULL,
        onInit: (button) => {
          const sync = () => {
            const full = !!document.fullscreenElement;
            button.innerHTML = full ? EXIT_FULL : ENTER_FULL;
            button.title = full ? "Exit fullscreen (f)" : "Fullscreen (f)";
          };
          const toggle = () =>
            (document.fullscreenElement
              ? document.exitFullscreen()
              : root.requestFullscreen()
            ).catch(() => {});
          button.addEventListener("click", toggle);
          pswp.on("keydown", ({ originalEvent: event }) => {
            if (event.key === "f" && !event.metaKey && !event.ctrlKey && !event.altKey) toggle();
          });
          // Also when the browser leaves fullscreen itself (Esc).
          document.addEventListener("fullscreenchange", sync);
          pswp.on("close", () => {
            document.removeEventListener("fullscreenchange", sync);
            if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
          });
        },
      });
    }

    // Keyboard shortcuts: "?" or its button (only shown once a mouse is
    // used, like the arrows) lists them.
    const panel = document.createElement("div");
    panel.className = "pswp__help";
    panel.id = "pswp-help";
    panel.hidden = true;
    panel.innerHTML = `<p class="pswp__help-title">Keyboard</p><dl>${SHORTCUTS.map(
      ([keys, what]) => `<dt>${keys}</dt><dd>${what}</dd>`,
    ).join("")}</dl>`;
    const showHelp = (show) => {
      panel.hidden = !show;
      helpButton?.setAttribute("aria-expanded", String(show));
    };
    let helpButton = null;
    pswp.ui.registerElement({
      name: "help",
      order: 7,
      isButton: true,
      title: "Keyboard shortcuts (?)",
      html: '<svg aria-hidden="true" viewBox="0 0 32 32" width="32" height="32"><text x="16" y="22" text-anchor="middle">?</text></svg>',
      onInit: (button, pswp) => {
        helpButton = button;
        button.setAttribute("aria-controls", panel.id);
        button.setAttribute("aria-expanded", "false");
        button.addEventListener("click", () => showHelp(panel.hidden));
        pswp.element.append(panel);
        pswp.on("keydown", (event) => {
          const key = event.originalEvent.key;
          if (key === "?") showHelp(panel.hidden);
          else if (key === "Escape" && !panel.hidden) {
            // Esc first closes the list, then the viewer.
            event.preventDefault();
            showHelp(false);
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
        // A single value is information, not a choice.
        input.disabled = group.classList.contains("single") || (n === 0 && !input.checked);
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

// ---- album bar: frosted once the title has gone, naming the current day --

const bar = document.querySelector(".album-bar");
if (bar) {
  const head = document.querySelector(".album-head");
  const day = bar.querySelector(".bar-day");
  const sections = [...document.querySelectorAll(".day")].filter((s) => s.querySelector(".day-title"));
  new IntersectionObserver(([entry]) => {
    bar.classList.toggle("is-stuck", !entry.isIntersecting && entry.boundingClientRect.top < 0);
  }).observe(head);

  // The current day is the last shown one whose heading is under the bar.
  if (sections.length > 1) {
    let queued = false;
    const update = () => {
      queued = false;
      const line = bar.getBoundingClientRect().bottom + 1;
      let current = null;
      for (const section of sections) {
        if (section.hidden) continue;
        if (section.getBoundingClientRect().top > line) break;
        current = section;
      }
      day.textContent = current?.querySelector(".day-title").textContent ?? "";
    };
    addEventListener(
      "scroll",
      () => {
        if (!queued) requestAnimationFrame(update);
        queued = true;
      },
      { passive: true },
    );
    update();
  }
}

// ---- a link to a photo (`#p=<key>`) opens the viewer on it ---------------

if (landing && openPhoto) {
  const tile = photos.querySelector(`a.tile[data-key="${CSS.escape(landing)}"]`);
  if (tile) {
    // A filter from the URL could hide it.
    if (tile.hidden) document.querySelector(".filter-clear")?.click();
    // In view first, so the viewer grows out of it and shrinks back onto it.
    tile.scrollIntoView({ block: "center" });
    openPhoto(tile);
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
