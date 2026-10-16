// End-to-end UI check against a running Pellicule (no dependencies: drives
// headless Chrome over the DevTools protocol with Node's built-in WebSocket).
//
//   node scripts/check-ui.mjs [out-dir]
//
// Env: BASE (default http://localhost:8080), CHROME (path to Chrome).
// Saves screenshots to out-dir (default ./target/ui-check) and prints what it
// checked. It never starts a scan. Needs at least one album with photos taken
// on two days for every check to apply.

import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const out = resolve(process.argv[2] ?? "target/ui-check");
const base = process.env.BASE ?? "http://localhost:8080";
const chromePath =
  process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const port = 9400 + Math.floor(Math.random() * 500);
mkdirSync(out, { recursive: true });

const chrome = spawn(
  chromePath,
  [
    "--headless=new",
    "--disable-gpu",
    "--hide-scrollbars",
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${out}/profile`,
    "--window-size=1440,900",
    "about:blank",
  ],
  { stdio: "ignore" },
);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? `: ${detail}` : ""}`);
};

try {
  // Chrome can take a few seconds to start listening.
  let targets = null;
  for (let tries = 0; !targets; tries++) {
    await sleep(500);
    targets = await fetch(`http://127.0.0.1:${port}/json`).then((r) => r.json(), (error) => {
      if (tries > 40) throw error;
      return null;
    });
  }
  const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener("open", r));

  let id = 0;
  const pending = new Map();
  const errors = [];
  ws.addEventListener("message", ({ data }) => {
    const msg = JSON.parse(data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg.result ?? msg.error);
      pending.delete(msg.id);
    }
    if (msg.method === "Runtime.exceptionThrown") {
      errors.push(msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
    }
    if (msg.method === "Log.entryAdded" && msg.params.entry.level === "error") {
      errors.push(`${msg.params.entry.text} ${msg.params.entry.url ?? ""}`);
    }
  });
  const send = (method, params = {}) =>
    new Promise((r) => {
      const i = ++id;
      pending.set(i, r);
      ws.send(JSON.stringify({ id: i, method, params }));
    });
  const ev = async (expression) =>
    (await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true })).result?.value;
  const shot = async (name) =>
    writeFileSync(`${out}/${name}.png`, Buffer.from((await send("Page.captureScreenshot")).data, "base64"));
  const go = async (path) => {
    await send("Page.navigate", { url: base + path });
    await sleep(2500);
  };

  await send("Runtime.enable");
  await send("Page.enable");
  await send("Log.enable");

  // Home
  await go("/");
  check("home is white by default", (await ev(`getComputedStyle(document.documentElement).backgroundColor`)) === "rgb(255, 255, 255)");
  check("Geist loaded", await ev(`document.fonts.check("500 16px Geist")`));
  check("home photos", await ev(`document.querySelectorAll(".hero-row.is-current img").length > 0`));
  const cropped = await ev(`[...document.querySelectorAll(".hero-row.is-current img")]
    .filter((img) => Math.abs(img.clientWidth / img.clientHeight - img.naturalWidth / img.naturalHeight) > 0.02).length`);
  check("home photos are not cropped", cropped === 0, `${cropped} cropped`);
  const fill = await ev(`(() => {
    const frame = document.querySelector(".hero-frame").getBoundingClientRect();
    const row = [...document.querySelectorAll(".hero-row.is-current .hero-photo")].map((p) => p.getBoundingClientRect());
    return [Math.round(row.at(-1).right - frame.left), Math.round(frame.width), Math.round(row[0].height), Math.round(frame.height)];
  })()`);
  check(
    "the row fills the width or the height",
    Math.abs(fill[0] - fill[1]) <= 2 || Math.abs(fill[2] - fill[3]) <= 2,
    `row ${fill[0]}×${fill[2]}, frame ${fill[1]}×${fill[3]}`,
  );
  check(
    "pages are compressed",
    (await ev(`fetch("/").then((r) => r.headers.get("content-encoding"))`)) === "br",
  );
  const rows = await ev(`document.querySelectorAll(".hero-row").length`);
  const hrefs = await ev(`[...document.querySelectorAll(".album-card")].map(a => a.getAttribute("href"))`);
  check("album cards", hrefs.length > 0, hrefs.join(", "));
  check(
    "cards show only the cover",
    await ev(`[...document.querySelectorAll(".album-card")].every((card) =>
      [...card.querySelectorAll("img")].every((img) => img.closest(".album-cover")))`),
  );
  check(
    "cards are cover, title and summary",
    await ev(`!document.querySelector(".album-palette, .album-card-gear")
      && [...document.querySelectorAll(".album-card")].every((c) => c.querySelector(".album-card-title") && c.querySelector(".album-card-summary"))`),
  );
  await shot("home");
  // Resting the pointer on a cover plays a short preview; leaving brings
  // the cover back.
  const card = await ev(`(() => {
    const card = [...document.querySelectorAll(".album-card")].find((c) => c.querySelector(".album-flip"));
    if (!card) return null;
    card.scrollIntoView({ block: "center" });
    const box = card.querySelector(".album-cover").getBoundingClientRect();
    return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
  })()`);
  if (card) {
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: card.x, y: card.y });
    await sleep(1600);
    check(
      "cover preview plays while pointed at",
      await ev(`!!document.elementFromPoint(${card.x}, ${card.y}).closest(".album-card").querySelector(".album-preview.is-shown")`),
    );
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 1, y: 1 });
    await sleep(800);
    check("cover returns when the pointer leaves", await ev(`!document.querySelector(".album-preview.is-shown")`));
    await ev(`scrollTo(0, 0)`);
  } else console.log("skip cover preview: no album has enough photos");
  if (rows > 1) {
    check("home photo dots", await ev(`!document.querySelector(".hero-dots").hidden`), `${rows} rows`);
    await sleep(9000);
    check("home rows crossfade", await ev(`document.querySelectorAll(".hero-row")[1].classList.contains("is-current")`));
  }
  // One album per row, named with its dates under the row.
  check(
    "each home row is one album",
    await ev(`[...document.querySelectorAll(".hero-row")].every((row) =>
      new Set([...row.querySelectorAll(".hero-photo")].map((p) => p.dataset.album)).size === 1)`),
  );
  check(
    "caption names the row's album and dates",
    await ev(`(() => {
      const first = document.querySelector(".hero-row.is-current .hero-photo");
      const [link, date] = document.querySelector(".hero-label").children;
      return link.textContent === first.dataset.album && date.textContent === first.dataset.albumDates;
    })()`),
  );
  await ev(`document.querySelector(".hero-row.is-current .hero-photo").dispatchEvent(new PointerEvent("pointerover", { bubbles: true }))`);
  check(
    "caption names the photo pointed at",
    await ev(`document.querySelector(".hero-label a").textContent === document.querySelector(".hero-row.is-current .hero-photo").dataset.album`),
  );

  // A home photo opens the viewer on that photo, in its album.
  const photoHref = await ev(`document.querySelector(".hero-row.is-current .hero-photo")?.getAttribute("href")`);
  if (photoHref?.includes("#p=")) {
    await go(photoHref);
    check("home photo opens its album", (await ev("location.pathname")) === photoHref.split("#")[0]);
    check(
      "and the viewer on the photo",
      await ev(`!!document.querySelector(".pswp--open") && location.hash === ${JSON.stringify("#" + photoHref.split("#")[1])}`),
    );
    await shot("landing");
  }

  // Every album with photos has the same five menus; a menu's only value
  // is shown, not offered.
  let withoutRecipe = null;
  for (const href of hrefs) {
    await go(href);
    if (!(await ev(`document.querySelectorAll("a.tile").length`))) continue;
    if (await ev(`!document.querySelector("a.tile[data-recipe]")`)) withoutRecipe ??= href;
    const menus = await ev(`[...document.querySelectorAll(".filter")].map((f) => f.dataset.filter).join(",")`);
    check(`five menus on ${href}`, menus === "day,camera,lens,focal,film", menus);
    const single = await ev(`[...document.querySelectorAll(".filter.single input")].every((i) => i.disabled)`);
    check(`single values are not choices on ${href}`, single);
  }

  // Prefer an album with several days, so filters and day sections apply.
  let albumHref = hrefs[0];
  for (const href of hrefs) {
    await go(href);
    if ((await ev(`document.querySelectorAll(".day").length`)) > 1) {
      albumHref = href;
      break;
    }
  }

  // Album, filters, density
  await go(albumHref);
  const tiles = await ev(`document.querySelectorAll("a.tile").length`);
  check("album grid", tiles > 0, `${albumHref}, ${tiles} photos`);
  const edges = await ev(`[document.querySelector(".grid").getBoundingClientRect().left,
    document.querySelector("a.tile").getBoundingClientRect().left,
    document.querySelector(".album-head h1").getBoundingClientRect().left].map(Math.round)`);
  check("grid aligned with the title", Math.abs(edges[1] - edges[2]) <= 6, `tile ${edges[1]}px, title ${edges[2]}px`);
  const days = await ev(`document.querySelectorAll(".day").length`);
  const hasFilters = await ev(`!!document.querySelector(".filters")`);
  if (hasFilters) check("filters shown", await ev(`!document.querySelector(".filters").hidden`));
  else console.log("skip filters: this album has a single value for every filter");
  await shot("album");
  check("album bar not stuck at the top", await ev(`!document.querySelector(".album-bar").classList.contains("is-stuck")`));
  if (days > 1) {
    await ev(`scrollTo(0, document.querySelectorAll(".day")[1].getBoundingClientRect().top + scrollY + 400)`);
    await sleep(500);
    check("album bar sticks", await ev(`document.querySelector(".album-bar").classList.contains("is-stuck")`));
    check(
      "and names the current day",
      await ev(`document.querySelector(".bar-day").textContent === document.querySelectorAll(".day-title")[1].textContent`),
      await ev(`document.querySelector(".bar-day").textContent`),
    );
    await shot("album-bar");
    await ev("scrollTo(0, 0)");
  }
  if (days > 1 && hasFilters) {
    await ev(`document.querySelector('[data-filter=day] input').click()`);
    await sleep(300);
    check("day filter hides other days", await ev(`[...document.querySelectorAll(".day")].slice(1).every(d => d.hidden)`));
    check("filter in URL", (await ev("location.search")).includes("day="));
    await ev(`document.querySelector(".filter-clear").click()`);
  } else {
    console.log("skip day filter: no album spans several days");
  }
  await ev(`document.querySelector('[data-density=l]').click()`);
  await sleep(300);
  check("density L", (await ev(`getComputedStyle(document.documentElement).getPropertyValue("--row").trim()`)) === "460px");
  await ev(`document.querySelector('[data-density=m]').click()`);

  // Viewer + slideshow
  await ev(`document.querySelector("a.tile").click()`);
  await sleep(2000);
  check("viewer opens", await ev(`!!document.querySelector(".pswp--open")`));
  check("museum label", !!(await ev(`document.querySelector(".pswp__label")?.innerText`)));
  check("photo in the URL", (await ev("location.hash")).startsWith("#p="));
  await shot("viewer");
  // With a mouse, the controls fade after 2 s still and come back on a move.
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 700, y: 450 });
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 710, y: 460 });
  await sleep(2600);
  check("controls fade when the mouse is still", await ev(`document.querySelector(".pswp").classList.contains("pswp--idle")`));
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 720, y: 470 });
  await sleep(200);
  check("and come back when it moves", await ev(`!document.querySelector(".pswp").classList.contains("pswp--idle")`));
  await ev(`document.querySelector(".pswp__button--zoom").click()`);
  await sleep(800);
  check("label hides while zoomed", (await ev(`getComputedStyle(document.querySelector(".pswp__label")).opacity`)) === "0");
  await ev(`document.querySelector(".pswp__button--zoom").click()`);
  await sleep(600);
  const key = async (key, text) => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", key, ...(text ? { text } : {}) });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key });
    await sleep(300);
  };
  await key("?", "?");
  check("? lists the shortcuts", await ev(`!document.querySelector(".pswp__help").hidden`));
  await shot("viewer-help");
  await key("Escape");
  check(
    "Esc closes the list first",
    await ev(`document.querySelector(".pswp__help").hidden && !!document.querySelector(".pswp--open")`),
  );
  // Fujifilm recipe: film on the label, the panel on "r", Esc closes it first.
  const film = await ev(`document.querySelector("a.tile").dataset.fFilm`);
  if (film) {
    check("label names the film", (await ev(`document.querySelector(".pswp__label-gear").textContent`)).endsWith(` · ${film}`));
    await key("r", "r");
    check(
      "r opens the recipe",
      await ev(`!document.querySelector(".pswp__recipe").hidden
        && document.querySelector(".pswp__recipe-film").textContent === ${JSON.stringify(film)}
        && document.querySelectorAll(".pswp__recipe dt").length > 3`),
    );
    await shot("viewer-recipe");
    await key("Escape");
    check(
      "Esc closes the recipe first",
      await ev(`document.querySelector(".pswp__recipe").hidden && !!document.querySelector(".pswp--open")`),
    );
  } else console.log("skip recipe: the first photo has no film simulation");
  await ev(`document.querySelector(".pswp__button--play").click()`);
  await sleep(4500);
  check("slideshow advances", (await ev(`document.querySelector(".pswp__counter").textContent`)).startsWith("2 /"));
  await ev(`document.querySelector(".pswp__button--play").click()`);

  // Fullscreen needs a user gesture: send real key presses.
  const press = async (key) => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", key, text: key });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key });
    await sleep(700);
  };
  if (await ev(`!!document.querySelector(".pswp__button--fullscreen")`)) {
    await press("f");
    check("f enters fullscreen", await ev(`!!document.fullscreenElement`));
    check("button shows exit", (await ev(`document.querySelector(".pswp__button--fullscreen").title`)).startsWith("Exit"));
    await press("f");
    check("f leaves fullscreen", await ev(`!document.fullscreenElement`));
    await press("f");
    await ev(`document.querySelector(".pswp__button--close").click()`);
    await sleep(800);
    check("closing the viewer leaves fullscreen", await ev(`!document.fullscreenElement`));
  } else {
    console.log("skip fullscreen: not supported by this browser");
    await ev(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }))`);
    await sleep(800);
  }

  // Closing: the URL loses the photo, focus is back on the tile.
  check("closing clears the URL", (await ev("location.hash")) === "");
  check("focus returns to the photo", await ev(`document.activeElement.matches("a.tile")`));

  // Back closes the viewer and stays on the album.
  await ev(`document.querySelectorAll("a.tile")[2].click()`);
  await sleep(1500);
  await ev("history.back()");
  await sleep(1000);
  check(
    "Back closes the viewer",
    await ev(`!document.querySelector(".pswp--open") && location.pathname === ${JSON.stringify(albumHref)}`),
  );

  // A link to a photo (reload, shared link) opens the viewer on it.
  const third = await ev(`document.querySelectorAll("a.tile")[2].dataset.key`);
  await go("/");
  await go(`${albumHref}#p=${third}`);
  check("photo link opens the viewer", (await ev(`document.querySelector(".pswp__counter")?.textContent`))?.startsWith("3 /"));
  await ev(`document.querySelector(".pswp__button--close").click()`);
  await sleep(800);

  // Photos from other makers have no recipe button.
  if (withoutRecipe) {
    await go(withoutRecipe);
    await ev(`document.querySelector("a.tile").click()`);
    await sleep(1500);
    await key("r", "r");
    check(
      "no recipe without Fujifilm",
      await ev(`document.querySelector(".pswp__button--recipe").hidden && document.querySelector(".pswp__recipe").hidden`),
      withoutRecipe,
    );
    await ev(`document.querySelector(".pswp__button--close").click()`);
    await sleep(800);
    await go(albumHref);
  }

  // Theme toggle (restored afterwards)
  await ev(`document.querySelector(".theme-toggle").click()`);
  await sleep(300);
  check("dark theme", (await ev(`document.documentElement.dataset.theme`)) === "dark");
  await shot("album-dark");
  await ev(`document.querySelector(".theme-toggle").click()`);

  // Phone (iPhone-sized viewport): S / L only, a one-line strip follows
  // the scroll, and the recipe sheet repeats the label it covers.
  await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 3, mobile: true });
  await send("Emulation.setTouchEmulationEnabled", { enabled: true });
  await go(albumHref);
  check("phone: no M size", await ev(`getComputedStyle(document.querySelector("[data-density=m]")).display === "none"`));
  await ev(`document.querySelector("[data-density=l]").click()`);
  await sleep(300);
  const oneUp = await ev(`(() => {
    const [a, b] = [...document.querySelectorAll("a.tile")].map((t) => t.getBoundingClientRect());
    const grid = document.querySelector(".grid");
    const width = grid.clientWidth - parseFloat(getComputedStyle(grid).paddingLeft) * 2;
    return [Math.round(a.width), Math.round(width), b.top > a.bottom];
  })()`);
  check("phone: L is one photo per row", Math.abs(oneUp[0] - oneUp[1]) <= 1 && oneUp[2], `${oneUp[0]}px of ${oneUp[1]}px`);
  await shot("phone-album-l");
  await ev(`document.querySelector("[data-density=s]").click()`);
  await sleep(300);
  if (days > 1) {
    await ev(`scrollTo(0, document.querySelectorAll(".day")[1].getBoundingClientRect().top + scrollY + 400)`);
    await sleep(500);
    const strip = await ev(`(() => {
      const strip = document.querySelector(".bar-id");
      return [getComputedStyle(strip).position, Math.round(strip.getBoundingClientRect().height),
        document.querySelector(".bar-day").textContent === document.querySelectorAll(".day-title")[1].textContent];
    })()`);
    check("phone: one-line strip follows the scroll", strip[0] === "fixed" && strip[1] < 64 && strip[2], strip.join(", "));
    check("phone: the filters scroll away", await ev(`document.querySelector(".toolbar").getBoundingClientRect().bottom < 0`));
    await shot("phone-album-strip");
    await ev("scrollTo(0, 0)");
  }
  if (film) {
    await ev(`document.querySelector("a.tile").click()`);
    await sleep(1500);
    await key("r", "r");
    await sleep(300);
    check(
      "phone: the recipe sheet starts with the label",
      await ev(`(() => {
        const date = document.querySelector(".pswp__recipe-label .pswp__label-date");
        return !!date && date.textContent === document.querySelector("a.tile").dataset.date
          && getComputedStyle(document.querySelector(".pswp__label")).visibility === "hidden";
      })()`),
    );
    check(
      "phone: the photo fits above the sheet",
      await ev(`document.querySelector(".pswp__item:not([aria-hidden=true]) .pswp__img:not(.pswp__img--placeholder)").getBoundingClientRect().bottom
        <= document.querySelector(".pswp__recipe").getBoundingClientRect().top + 1`),
    );
    await shot("phone-viewer-recipe");
    await key("Escape");
    await key("Escape");
    await sleep(800);
  }
  // Back to the desktop default.
  await ev(`document.querySelector("[data-density=m]").click()`);
  await send("Emulation.setTouchEmulationEnabled", { enabled: false });
  await send("Emulation.clearDeviceMetricsOverride");

  // Library page (read-only: no scan is started)
  await go("/library");
  check("library page", !!(await ev(`document.querySelector(".page-lede")?.textContent`)));
  await shot("library");

  check("no browser errors", errors.length === 0, errors.join(" | "));
  ws.close();
} finally {
  chrome.kill();
}

console.log(`\nScreenshots in ${out}`);
process.exitCode = failures ? 1 : 0;
