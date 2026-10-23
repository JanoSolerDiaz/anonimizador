import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import App from "./App.svelte";

function tabs() {
  return {
    anonymize: screen.getByRole("button", { name: "Anonimizar" }),
    deanonymize: screen.getByRole("button", { name: "Desanonimizar" }),
  };
}

describe("App", () => {
  it("muestra las dos pestañas dentro de la navegación de operaciones", () => {
    render(App);

    const nav = screen.getByRole("navigation", { name: "Operación" });
    const buttons = Array.from(nav.querySelectorAll("button")).map((b) => b.textContent?.trim());
    expect(buttons).toEqual(["Anonimizar", "Desanonimizar"]);
  });

  it("empieza en la pestaña Anonimizar", () => {
    render(App);
    const { anonymize, deanonymize } = tabs();

    expect(anonymize.getAttribute("aria-pressed")).toBe("true");
    expect(deanonymize.getAttribute("aria-pressed")).toBe("false");
    expect(anonymize.classList.contains("activa")).toBe(true);
    expect(screen.getByText(/documentos que se quieren anonimizar/)).toBeTruthy();
  });

  it("cambia a Desanonimizar al pulsar su pestaña", async () => {
    const user = userEvent.setup();
    render(App);
    const { anonymize, deanonymize } = tabs();

    await user.click(deanonymize);

    expect(deanonymize.getAttribute("aria-pressed")).toBe("true");
    expect(anonymize.getAttribute("aria-pressed")).toBe("false");
    expect(deanonymize.classList.contains("activa")).toBe(true);
    expect(anonymize.classList.contains("activa")).toBe(false);
    expect(screen.getByText(/documentos o el texto que se quieren restaurar/)).toBeTruthy();
    expect(screen.queryByText(/documentos que se quieren anonimizar/)).toBeNull();
  });

  it("vuelve a Anonimizar al pulsar su pestaña", async () => {
    const user = userEvent.setup();
    render(App);
    const { anonymize, deanonymize } = tabs();

    await user.click(deanonymize);
    await user.click(anonymize);

    expect(anonymize.getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByText(/documentos que se quieren anonimizar/)).toBeTruthy();
  });

  it("pulsar la pestaña activa no la desactiva", async () => {
    const user = userEvent.setup();
    render(App);
    const { anonymize } = tabs();

    await user.click(anonymize);

    expect(anonymize.getAttribute("aria-pressed")).toBe("true");
  });
});
