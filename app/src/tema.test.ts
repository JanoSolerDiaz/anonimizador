import { afterEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import App from "./App.svelte";
import { aplicarTema, CLAVE_TEMA, leerTemaGuardado } from "./tema";

afterEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.tema;
});

describe("Modo claro y oscuro", () => {
  it("sin elección guardada sigue al sistema y no marca nada en <html>", () => {
    render(App);

    expect(leerTemaGuardado()).toBeNull();
    expect(document.documentElement.dataset.tema).toBeUndefined();
    // jsdom no tiene modo oscuro: el sistema cuenta como claro.
    expect(screen.getByRole("button", { name: "Cambiar a modo oscuro" })).toBeTruthy();
    expect(screen.getByRole("img", { name: "Cuatroochenta" }).getAttribute("src")).toBe(
      "/marca/480-color-fondo-claro.png",
    );
  });

  it("el botón pasa a oscuro, cambia el logo y guarda la elección", async () => {
    const user = userEvent.setup();
    render(App);

    await user.click(screen.getByRole("button", { name: "Cambiar a modo oscuro" }));

    expect(document.documentElement.dataset.tema).toBe("oscuro");
    expect(localStorage.getItem(CLAVE_TEMA)).toBe("oscuro");
    expect(screen.getByRole("img", { name: "Cuatroochenta" }).getAttribute("src")).toBe(
      "/marca/480-color-fondo-oscuro.png",
    );
    expect(screen.getByRole("button", { name: "Cambiar a modo claro" })).toBeTruthy();
  });

  it("pulsarlo otra vez vuelve a claro y lo guarda", async () => {
    const user = userEvent.setup();
    render(App);

    await user.click(screen.getByRole("button", { name: "Cambiar a modo oscuro" }));
    await user.click(screen.getByRole("button", { name: "Cambiar a modo claro" }));

    expect(document.documentElement.dataset.tema).toBe("claro");
    expect(localStorage.getItem(CLAVE_TEMA)).toBe("claro");
  });

  it("al abrir la app recupera la última elección", () => {
    localStorage.setItem(CLAVE_TEMA, "oscuro");
    aplicarTema(leerTemaGuardado()); // lo que hace main.ts al arrancar

    render(App);

    expect(document.documentElement.dataset.tema).toBe("oscuro");
    expect(screen.getByRole("button", { name: "Cambiar a modo claro" })).toBeTruthy();
    expect(screen.getByRole("img", { name: "Cuatroochenta" }).getAttribute("src")).toBe(
      "/marca/480-color-fondo-oscuro.png",
    );
  });

  it("ignora un valor guardado que no es un tema", () => {
    localStorage.setItem(CLAVE_TEMA, "morado");
    expect(leerTemaGuardado()).toBeNull();
  });
});
