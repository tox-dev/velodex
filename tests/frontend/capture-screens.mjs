// Capture the documentation screenshots: each page in both themes, against a live peryx.
// Usage: node capture-screens.mjs [base-url]  (default is the port serve.mjs listens on)
import { chromium } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const base = process.argv[2] ?? "http://127.0.0.1:4455";
const outDir = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "site", "static", "screens");

// Each height frames that page's content at the 1360px capture width; the viewport is taller than
// any of them so a clip never runs past the rendered area.
const pages = [
  { name: "dashboard", path: "/", height: 1240 },
  { name: "stats-index", path: "/stats?index=root%2Fpypi", height: 455 },
  { name: "stats-project", path: "/stats?index=root%2Fpypi&project=veloxdemo", height: 495 },
  { name: "project", path: "/browse?index=root%2Fpypi&project=veloxdemo", height: 900 },
  { name: "oci-manifest", path: "/browse?index=images&project=app&ref=1.0", height: 550 },
];

// serve.mjs uploads the fixtures but never reads them back, so every usage counter is zero and the
// stats pages would screenshot empty. Serve each artifact a few times first.
const wheel = "veloxdemo-1.0.0-py3-none-any.whl";
const digest = "ab46ad722f3d0f9a9b655760ef0aa83233554531c5c02b722e84c658e0e462ec";
for (let round = 0; round < 3; round += 1) {
  await fetch(`${base}/root/pypi/simple/veloxdemo/`);
  await fetch(`${base}/root/pypi/files/${digest}/${wheel}`);
  await fetch(`${base}/root/pypi/files/${digest}/${wheel}.metadata`);
}
await fetch(`${base}/root/pypi/simple/`);

const browser = await chromium.launch();
for (const theme of ["light", "dark"]) {
  const context = await browser.newContext({ viewport: { width: 1360, height: 1280 }, colorScheme: theme });
  await context.addInitScript((value) => localStorage.setItem("theme", value), theme);
  const page = await context.newPage();
  for (const shot of pages) {
    await page.goto(base + shot.path, { waitUntil: "networkidle" });
    await page.waitForTimeout(400);
    await page.screenshot({
      path: join(outDir, `${shot.name}-${theme}.png`),
      clip: { x: 0, y: 0, width: 1360, height: shot.height },
    });
    console.log(`${shot.name}-${theme}.png`);
  }
  await context.close();
}
await browser.close();
