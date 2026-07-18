import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte()],
  // Force the browser build of Svelte: the default server condition resolves
  // `mount()` to its SSR stub, which throws on render.
  resolve: { conditions: ["browser"] },
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
