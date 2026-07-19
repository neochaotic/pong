import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte()],
  // Force the browser build of Svelte: the default server condition resolves
  // `mount()` to its SSR stub, which throws on render.
  resolve: { conditions: ["browser"] },
  // Mirrors vite.config.ts's `__APP_VERSION__` — SettingsView.svelte reads it
  // unconditionally, so it must exist under test too. The exact value
  // (real version vs. tag) doesn't matter here; tests only check the string
  // renders, not what it says.
  define: { __APP_VERSION__: JSON.stringify("test") },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.{test,spec}.{js,ts}"],
    coverage: {
      provider: "v8",
      // Report every source file, not just the ones a test happened to import.
      // Without an explicit include, untested modules are simply absent and the
      // summary looks far better than it actually is.
      include: ["src/**/*.{ts,svelte}"],
      exclude: ["src/**/*.{test,spec}.ts", "src/test/**", "src/main.ts"],
      // Fail the run if coverage regresses below the agreed floor.
      thresholds: {
        statements: 70,
        branches: 70,
        functions: 70,
        lines: 70,
      },
    },
  },
});
