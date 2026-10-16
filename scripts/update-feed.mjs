#!/usr/bin/env node
// Builds the updater feed (latest.json) for a published release from the
// `.sig` files package-release.sh collects, and prints it.
//
// Usage: node scripts/update-feed.mjs <tag> <folder with .sig files>
//
// Each platform gets an entry per installer the updater can replace
// (`windows-x86_64-nsis`, `darwin-aarch64-app`, …) plus a plain
// `<os>-<arch>` entry, so copies that only notify (portable, .deb/.rpm)
// still learn about the release. Signatures cover the files themselves.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const REPO = "dumitreskw/markdraft";
const OS = { windows: "windows", macos: "darwin", linux: "linux" };
const ARCH = { x64: "x86_64", x86: "i686", arm64: "aarch64" };
/** Package suffix → installer the updater uses for it; the first one per platform is the fallback. */
const KINDS = [
  ["-setup.exe", "nsis"],
  [".msi", "msi"],
  [".app.tar.gz", "app"],
  [".AppImage", "appimage"],
];

export function buildFeed(tag, sigFiles, readSig, now = new Date()) {
  const version = tag.replace(/^v/, "");
  const platforms = {};
  const fallbacks = {};
  for (const sig of [...sigFiles].sort()) {
    const file = sig.replace(/\.sig$/, "");
    const m = file.match(/^MarkDraft-.+?-(windows|macos|linux)-(x64|x86|arm64)(.*)$/);
    if (!m) continue;
    const [, os, arch, rest] = m;
    const kind = KINDS.findIndex(([suffix]) => rest === suffix);
    if (kind < 0) continue;
    const key = `${OS[os]}-${ARCH[arch]}`;
    const entry = {
      signature: readSig(sig).trim(),
      url: `https://github.com/${REPO}/releases/download/${tag}/${file}`,
    };
    platforms[`${key}-${KINDS[kind][1]}`] = entry;
    if (!fallbacks[key] || kind < fallbacks[key].kind) fallbacks[key] = { kind, entry };
  }
  for (const [key, { entry }] of Object.entries(fallbacks)) platforms[key] = entry;
  if (!Object.keys(platforms).length) throw new Error("no updater signatures found");
  return {
    version,
    notes: `MarkDraft ${version}`,
    pub_date: now.toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms: Object.fromEntries(Object.entries(platforms).sort(([a], [b]) => a.localeCompare(b))),
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [tag, dir] = process.argv.slice(2);
  if (!tag || !dir) {
    console.error("usage: update-feed.mjs <tag> <folder with .sig files>");
    process.exit(2);
  }
  const sigs = readdirSync(dir).filter((f) => f.endsWith(".sig"));
  const feed = buildFeed(tag, sigs, (f) => readFileSync(join(dir, f), "utf8"));
  console.log(JSON.stringify(feed, null, 2));
}
