import { afterEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import App from "./App.svelte";
import { aplicarTema, CLAVE_TEMA, leerTemaGuardado } from "./tema";
import { inspectByName, respond, status } from "./test/backend";

async function start() {
  respond("status", status());
  inspectByName();
  const user = userEvent.setup();
  render(App);
  await screen.findByRole("region", { name: "Zona para soltar archivos", hidden: false });
  return user;
}

function logo() {
  return screen.getByRole("img", { name: "Cuatroochenta" });
}

afterEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.tema;
});

describe("cabecera", () => {
  it("muestra el logo de Cuatroochenta y el título dentro de la cabecera", async () => {
    await start();

    const header = screen.getByRole("banner");
    expect(header.contains(logo())).toBe(true);
    expect(screen.getByRole("heading", { level: 1, name: "Anonimizador" })).toBeTruthy();
  });
});

describe("modo claro y oscuro", () => {
  it("sin elección guardada sigue al sistema y no marca nada en <html>", async () => {
    await start();

    expect(leerTemaGuardado()).toBeNull();
    expect(document.documentElement.dataset.tema).toBeUndefined();
    // jsdom no tiene modo oscuro: el sistema cuenta como claro.
    expect(screen.getByRole("button", { name: "Cambiar a modo oscuro" })).toBeTruthy();
    expect(logo().getAttribute("src")).toBe("/marca/480-color-fondo-claro.png");
  });

  it("el botón pasa a oscuro, cambia el logo y guarda la elección", async () => {
    const user = await start();

    await user.click(screen.getByRole("button", { name: "Cambiar a modo oscuro" }));

    expect(document.documentElement.dataset.tema).toBe("oscuro");
    expect(localStorage.getItem(CLAVE_TEMA)).toBe("oscuro");
    expect(logo().getAttribute("src")).toBe("/marca/480-color-fondo-oscuro.png");
    expect(screen.getByRole("button", { name: "Cambiar a modo claro" })).toBeTruthy();
  });

  it("pulsarlo otra vez vuelve a claro y lo guarda", async () => {
    const user = await start();

    await user.click(screen.getByRole("button", { name: "Cambiar a modo oscuro" }));
    await user.click(screen.getByRole("button", { name: "Cambiar a modo claro" }));

    expect(document.documentElement.dataset.tema).toBe("claro");
    expect(localStorage.getItem(CLAVE_TEMA)).toBe("claro");
  });

  it("al abrir la app recupera la última elección", async () => {
    localStorage.setItem(CLAVE_TEMA, "oscuro");
    aplicarTema(leerTemaGuardado()); // lo que hace main.ts al arrancar

    await start();

    expect(document.documentElement.dataset.tema).toBe("oscuro");
    expect(screen.getByRole("button", { name: "Cambiar a modo claro" })).toBeTruthy();
    expect(logo().getAttribute("src")).toBe("/marca/480-color-fondo-oscuro.png");
  });

  it("ignora un valor guardado que no es un tema", () => {
    localStorage.setItem(CLAVE_TEMA, "morado");
    expect(leerTemaGuardado()).toBeNull();
  });
});
