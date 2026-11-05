import { defineConfig, mergeConfig } from "vitest/config";
import { svelteTesting } from "@testing-library/svelte/vite";
import viteConfig from "./vite.config.ts";

export default mergeConfig(
  viteConfig,
  defineConfig({
    // Usa el código de navegador de Svelte y limpia el DOM tras cada prueba.
    plugins: [svelteTesting()],
    test: {
      environment: "jsdom",
      include: ["src/**/*.test.ts"],
      setupFiles: ["src/test/setup.ts"],
      coverage: {
        provider: "v8",
        include: ["src/**/*.{ts,svelte}"],
        exclude: ["src/**/*.test.ts", "src/test/**"],
        reporter: ["text", "html"],
        thresholds: { lines: 80, functions: 80, branches: 80, statements: 80 },
      },
    },
  }),
);
