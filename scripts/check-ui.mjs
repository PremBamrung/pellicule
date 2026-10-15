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
  await sleep(1500);
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
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
  check("home photo", await ev(`!!document.querySelector(".hero img")`));
  const slides = await ev(`document.querySelectorAll(".hero-slide").length`);
  const hrefs = await ev(`[...document.querySelectorAll(".album-card")].map(a => a.getAttribute("href"))`);
  check("album cards", hrefs.length > 0, hrefs.join(", "));
  await shot("home");
  if (slides > 1) {
    check("home photo dots", await ev(`!document.querySelector(".hero-dots").hidden`), `${slides} photos`);
    await sleep(7000);
    check("home photos crossfade", await ev(`document.querySelectorAll(".hero-slide")[1].classList.contains("is-current")`));
    check(
      "caption follows the photo",
      await ev(`document.querySelector(".hero-caption a").textContent === document.querySelector(".hero-slide.is-current").dataset.album`),
    );
  }

  // A home photo lands on that photo in its album.
  const photoHref = await ev(`document.querySelector(".hero-slide.is-current")?.getAttribute("href")`);
  if (photoHref?.includes("#p=")) {
    await go(photoHref);
    const rect = await ev(`(() => { const r = document.querySelector(".tile.is-target")?.getBoundingClientRect(); return r && [r.top, r.bottom, innerHeight]; })()`);
    check("home photo opens its album", (await ev("location.pathname")) === photoHref.split("#")[0]);
    check("lands on the photo", !!rect && rect[0] >= 0 && rect[1] <= rect[2], JSON.stringify(rect));
    await shot("landing");
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
  const days = await ev(`document.querySelectorAll(".day").length`);
  const hasFilters = await ev(`!!document.querySelector(".filters")`);
  if (hasFilters) check("filters shown", await ev(`!document.querySelector(".filters").hidden`));
  else console.log("skip filters: this album has a single value for every filter");
  await shot("album");
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
  await shot("viewer");
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

  // Theme toggle (restored afterwards)
  await ev(`document.querySelector(".theme-toggle").click()`);
  await sleep(300);
  check("dark theme", (await ev(`document.documentElement.dataset.theme`)) === "dark");
  await shot("album-dark");
  await ev(`document.querySelector(".theme-toggle").click()`);

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
