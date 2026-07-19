import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed port and must not fall back to a random one.
const host = process.env.TAURI_DEV_HOST;

const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf-8"));

// `release.yml` tags a build `v0.0.1-rc.1`; GitHub Actions sets GITHUB_REF_NAME
// to that tag for every step automatically, no extra wiring needed. The
// manifest itself must stay numeric (Tauri's Windows bundler rejects a
// pre-release suffix — see check-version.mjs), so the RC identifier only ever
// exists on the git tag; this is how it reaches the running app so a tester
// can tell which RC they're on. Guarded to actual version tags so a plain CI
// run on a branch (GITHUB_REF_NAME = "main") doesn't show up as the version.
const ref = process.env.GITHUB_REF_NAME;
const appVersion = ref && /^v\d/.test(ref) ? ref.replace(/^v/, "") : pkg.version;

export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
