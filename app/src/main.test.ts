import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

describe("main", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("monta la aplicación en #app", async () => {
    document.body.innerHTML = '<div id="app"></div>';

    await import("./main");

    const app = document.getElementById("app")!;
    expect(app.querySelector("nav")).not.toBeNull();
    expect(app.textContent).toContain("Anonimizar");
  });

  it("falla con un mensaje claro si falta #app", async () => {
    await expect(import("./main")).rejects.toThrow("Falta el elemento #app en index.html");
  });
});
