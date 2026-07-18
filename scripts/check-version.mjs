#!/usr/bin/env node
/**
 * Assert the three version declarations agree.
 *
 * package.json, tauri.conf.json and Cargo.toml each carry the version
 * independently. tauri.conf.json is what ends up in the installer filenames and
 * the app metadata, so a drift here ships a build labelled with the wrong
 * version — and it is invisible until someone downloads it.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8");

const versions = {
  "package.json": JSON.parse(read("package.json")).version,
  "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
  // First `version = "..."` in [package]; dependency versions come later.
  "src-tauri/Cargo.toml": read("src-tauri/Cargo.toml").match(/^version = "(.+?)"/m)?.[1],
};

const semver = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
const unique = [...new Set(Object.values(versions))];
let failed = false;

for (const [file, version] of Object.entries(versions)) {
  if (!version) {
    console.error(`✗ ${file}: no version found`);
    failed = true;
  } else if (!semver.test(version)) {
    console.error(`✗ ${file}: "${version}" is not valid semver`);
    failed = true;
  }
}

// Tauri's Windows bundlers reject pre-release identifiers outright
// (tauri-apps/tauri#5286, #12470). Such a version builds fine on macOS and
// Linux and then fails only in the Windows release job, so catch it here.
// Pre-release status belongs on the git tag (v0.0.1-rc.1), not in the manifest.
const appVersion = versions["src-tauri/tauri.conf.json"];
if (appVersion && /[-+]/.test(appVersion)) {
  console.error(
    `✗ src-tauri/tauri.conf.json: "${appVersion}" has a pre-release or build ` +
      `identifier, which breaks the Windows MSI/NSIS bundler.\n` +
      `    Keep the manifest numeric (e.g. 0.0.1) and tag the release v0.0.1-rc.1.`
  );
  failed = true;
}

if (unique.length > 1) {
  console.error("✗ versions disagree:");
  for (const [file, version] of Object.entries(versions)) {
    console.error(`    ${version}\t${file}`);
  }
  failed = true;
}

if (failed) process.exit(1);
console.log(`✓ version ${unique[0]} consistent across all three manifests`);
