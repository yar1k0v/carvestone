// Fails the build when the JavaScript that index.html loads up front exceeds the budget.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const BUDGET_BYTES = 400 * 1024;
const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
const html = readFileSync(join(dist, "index.html"), "utf8");

// Scripts and modulepreload hints are what the browser fetches before the first paint.
const refs = new Set();
for (const tag of html.match(/<(?:script|link)\b[^>]*>/g) ?? []) {
  const isScript = tag.startsWith("<script");
  const isPreload = /rel=["']modulepreload["']/.test(tag);
  if (!isScript && !isPreload) continue;
  const url = tag.match(/(?:src|href)=["']([^"']+)["']/)?.[1];
  if (url?.endsWith(".js")) refs.add(url);
}

let total = 0;
for (const url of refs) {
  const bytes = gzipSync(readFileSync(join(dist, url.replace(/^\//, "")))).length;
  total += bytes;
  console.log(`  ${url}  ${(bytes / 1024).toFixed(1)} KB gzipped`);
}
const kb = (total / 1024).toFixed(1);
if (total > BUDGET_BYTES) {
  console.error(`initial JS is ${kb} KB gzipped; the budget is ${BUDGET_BYTES / 1024} KB. Lazy-load the heavy view.`);
  process.exit(1);
}
console.log(`initial JS: ${kb} KB gzipped (budget ${BUDGET_BYTES / 1024} KB)`);
