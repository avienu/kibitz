#!/usr/bin/env node
/**
 * Generate the Tauri updater static feed (latest.json) from a directory of
 * built updater artifacts + their .sig files.
 *
 * Usage:
 *   node scripts/release/generate_latest_json.mjs \
 *     --version 0.1.0 \
 *     --dir path/to/collected-artifacts \
 *     [--notes "release notes line"] \
 *     [--out latest.json]
 *
 * Artifact → platform-key mapping (only updater-capable artifacts):
 *   *.app.tar.gz  + .sig  → darwin-<arch>   (aarch64|arm64 → aarch64, x86_64|x64 → x86_64,
 *                                            universal → darwin-universal)
 *   *.AppImage    + .sig  → linux-<arch>    (amd64|x86_64 → x86_64, aarch64|arm64 → aarch64)
 *
 * URLs point at the GitHub release download path for tag v<version> on
 * avienu/kibitz. Exits 1 if no platform entry could be built (a feed with an
 * empty platforms map would brick the updater check).
 *
 * The feed shape is contract-tested in app/src/lib/updates.test.ts against
 * app/src/lib/__fixtures__/latest.json — keep the shapes in sync.
 */
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const REPO = "avienu/kibitz";

function arg(name, fallback = undefined) {
  const i = process.argv.indexOf(`--${name}`);
  if (i === -1 || i === process.argv.length - 1) return fallback;
  return process.argv[i + 1];
}

const version = arg("version");
const dir = arg("dir");
const notes = arg("notes", `Kibitz ${version}`);
const out = arg("out", "latest.json");

if (!version || !dir) {
  console.error("usage: generate_latest_json.mjs --version X.Y.Z --dir DIR [--notes N] [--out F]");
  process.exit(1);
}

function detectArch(name) {
  const n = name.toLowerCase();
  if (n.includes("universal")) return "universal";
  if (n.includes("aarch64") || n.includes("arm64")) return "aarch64";
  if (n.includes("x86_64") || n.includes("amd64") || n.includes("x64")) return "x86_64";
  return null;
}

/** Recursively list files below dir (artifacts may arrive in subdirs). */
function walk(d) {
  const entries = [];
  for (const e of readdirSync(d, { withFileTypes: true })) {
    const p = join(d, e.name);
    if (e.isDirectory()) entries.push(...walk(p));
    else entries.push(p);
  }
  return entries;
}

const files = walk(dir);
const platforms = {};

for (const path of files) {
  const name = path.split("/").pop();
  let os = null;
  if (name.endsWith(".app.tar.gz")) os = "darwin";
  else if (name.endsWith(".AppImage")) os = "linux";
  else continue;

  const sigPath = files.find((f) => f === `${path}.sig`);
  if (!sigPath) {
    console.error(`generate_latest_json: WARNING — no .sig next to ${name}; skipped (unsigned builds cannot feed the updater).`);
    continue;
  }
  const arch = detectArch(name);
  if (!arch) {
    console.error(`generate_latest_json: WARNING — cannot detect arch in ${name}; skipped.`);
    continue;
  }
  const key = os === "darwin" && arch === "universal" ? "darwin-universal" : `${os}-${arch}`;
  platforms[key] = {
    signature: readFileSync(sigPath, "utf8").trim(),
    url: `https://github.com/${REPO}/releases/download/v${version}/${encodeURIComponent(name)}`,
  };
}

if (Object.keys(platforms).length === 0) {
  console.error("generate_latest_json: ERROR — no updater artifacts with signatures found; refusing to write an empty feed.");
  process.exit(1);
}

const feed = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms,
};

writeFileSync(out, `${JSON.stringify(feed, null, 2)}\n`);
console.log(`generate_latest_json: wrote ${out} with platforms: ${Object.keys(platforms).join(", ")}`);
