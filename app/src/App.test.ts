import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import App from "./App.svelte";
import type { AnonymizeJob, DeanonymizeJob } from "./lib/api";
import {
  backend,
  dropFiles,
  emit,
  file,
  hoverFiles,
  inspectByName,
  respond,
  status,
} from "./test/backend";

async function start(overrides = {}) {
  respond("status", status(overrides));
  inspectByName();
  const user = userEvent.setup();
  render(App);
  await screen.findByRole("region", { name: "Zona para soltar archivos", hidden: false });
  return user;
}

function tab(name: string) {
  const nav = screen.getByRole("navigation", { name: "Operación" });
  return within(nav).getByRole("button", { name });
}

/** La pestaña a la vista. */
function visible() {
  const panes = document.querySelectorAll(".contenido > div");
  const shown = [...panes].find((pane) => !(pane as HTMLElement).hidden) as HTMLElement;
  return within(shown);
}

describe("pestañas", () => {
  it("muestra las dos pestañas y empieza en Anonimizar", async () => {
    await start();
    const nav = screen.getByRole("navigation", { name: "Operación" });
    const buttons = [...nav.querySelectorAll("button")].map((b) => b.textContent?.trim());
    expect(buttons).toEqual(["Anonimizar", "Desanonimizar"]);
    expect(tab("Anonimizar").getAttribute("aria-pressed")).toBe("true");
    expect(visible().getByText(/Suelte aquí documentos PDF/)).toBeTruthy();
  });

  it("cambia de pestaña sin perder la lista de la otra", async () => {
    const user = await start();
    dropFiles(["C:\\docs\\contrato.docx"]);
    await visible().findByText("contrato.docx");

    await user.click(tab("Desanonimizar"));
    expect(tab("Desanonimizar").getAttribute("aria-pressed")).toBe("true");
    expect(visible().getByText(/documentos anonimizados/)).toBeTruthy();
    expect(visible().queryByText("contrato.docx")).toBeNull();

    await user.click(tab("Anonimizar"));
    expect(visible().getByText("contrato.docx")).toBeTruthy();
  });

  it("solo la pestaña a la vista recoge los archivos soltados", async () => {
    const user = await start();
    await user.click(tab("Desanonimizar"));
    dropFiles(["C:\\docs\\respuesta.md"]);
    await visible().findByText("respuesta.md");
    await user.click(tab("Anonimizar"));
    expect(visible().queryByText("respuesta.md")).toBeNull();
  });
});

describe("avisos al arrancar", () => {
  it("dice que el modelo se está cargando y no deja anonimizar hasta que está", async () => {
    await start({ model: { state: "loading" } });
    expect(screen.getByText(/Preparando el modelo/)).toBeTruthy();
    dropFiles(["C:\\docs\\nota.txt"]);
    await visible().findByText("nota.txt");
    const button = visible().getByRole("button", { name: "Anonimizar" });
    expect((button as HTMLButtonElement).disabled).toBe(true);

    emit({ type: "model", status: { state: "ready" } });
    await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
    expect(screen.queryByText(/Preparando el modelo/)).toBeNull();
  });

  it("explica por qué falla el modelo", async () => {
    await start({ model: { state: "failed", message: "Falta un archivo." } });
    expect(screen.getByRole("alert").textContent).toContain("Falta un archivo.");
  });

  it("ofrece empezar un almacén nuevo si el actual no se abre", async () => {
    const user = await start({
      store: { state: "failed", message: "El almacén no se puede descifrar." },
    });
    expect(screen.getByText("El almacén no se puede descifrar.")).toBeTruthy();
    respond("store_start_over", "C:\\datos\\almacen.anon.no-se-pudo-abrir-2026-11-05");
    respond("status", status());
    await user.click(screen.getByRole("button", { name: "Empezar un almacén nuevo" }));
    expect(await screen.findByText(/El anterior está en C:\\datos/)).toBeTruthy();
    expect(screen.queryByText("El almacén no se puede descifrar.")).toBeNull();
  });

  it("dice si no puede hablar con la aplicación", async () => {
    render(App);
    expect((await screen.findByRole("alert")).textContent).toContain("Sin respuesta para status");
  });
});

describe("anonimizar", () => {
  it("lista lo soltado con su tipo y avisa de lo que no se admite", async () => {
    await start();
    hoverFiles(true);
    await waitFor(() =>
      expect(document.querySelector(".zona.encima")).not.toBeNull(),
    );
    dropFiles(["C:\\docs\\informe.pdf", "C:\\docs\\carta.rtf"]);
    await visible().findByText("informe.pdf");
    expect(visible().getByText("PDF digital")).toBeTruthy();
    expect(visible().getByText("El formato «rtf» no se admite.")).toBeTruthy();
    expect(document.querySelector(".zona.encima")).toBeNull();
  });

  it("elige el formato para todos o archivo a archivo", async () => {
    const user = await start();
    dropFiles(["C:\\docs\\informe.pdf", "C:\\docs\\nota.txt"]);
    await visible().findByText("nota.txt");
    const pdf = visible().getByLabelText("Formato de salida de informe.pdf") as HTMLSelectElement;
    const txt = visible().getByLabelText("Formato de salida de nota.txt") as HTMLSelectElement;
    expect([pdf.value, txt.value]).toEqual(["pdf", "txt"]);

    await user.selectOptions(visible().getByLabelText("Formato de entrega"), "md");
    expect([pdf.value, txt.value]).toEqual(["md", "md"]);
    await user.selectOptions(visible().getByLabelText("Formato de entrega"), "docx");
    expect([pdf.value, txt.value]).toEqual(["docx", "txt"]);
    await user.selectOptions(txt, "md");
    expect(txt.value).toBe("md");
  });

  it("envía el lote, muestra el progreso y el resumen, y abre la carpeta", async () => {
    const user = await start();
    let finish: () => void = () => {};
    respond("anonymize", () => new Promise<void>((resolve) => (finish = resolve)));
    respond("open_folder", null);
    dropFiles(["C:\\docs\\contrato.docx", "C:\\docs\\carta.rtf"]);
    await visible().findByText("contrato.docx");

    await user.click(visible().getByRole("button", { name: "Anonimizar" }));
    const [{ job }] = backend.callsTo("anonymize") as { job: AnonymizeJob }[];
    expect(job.dir).toBe("C:\\salida");
    expect(job.files).toEqual([
      { id: expect.any(String), path: "C:\\docs\\contrato.docx", format: "docx" },
    ]);
    expect(job.skipped).toEqual([]);
    expect(job.resolution).toBe(200);
    const id = job.files[0].id;

    emit({ type: "progress", file: id, phase: { state: "analyzing", fraction: 0.5 } });
    expect(await visible().findByText("Analizando 50 %")).toBeTruthy();
    expect(visible().getByRole("button", { name: "Anonimizando…" })).toBeTruthy();

    emit({
      type: "progress",
      file: id,
      phase: {
        state: "anonymized",
        output: "C:\\salida\\contrato_anonimizado.docx",
        entities: { NOMBRE_PERSONA: 2, EMAIL: 1 },
        warnings: { images: 1, objects: 0, attachments: 0, links: 0 },
      },
    });
    emit({ type: "batchDone", done: 1, failed: 0, storeError: null });
    finish();
    expect(await visible().findByText("Nombres de personas: 2")).toBeTruthy();
    expect(visible().getByText("Correos electrónicos: 1")).toBeTruthy();
    expect(visible().getByText(/tiene 1 imagen\. Su contenido no se ha revisado/)).toBeTruthy();

    await user.click(await visible().findByRole("button", { name: "Abrir carpeta" }));
    expect(backend.callsTo("open_folder")).toEqual([{ path: "C:\\salida" }]);
    // Lo ya hecho no se vuelve a enviar.
    expect(
      (visible().getByRole("button", { name: "Anonimizar" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("manda los tipos que no se sustituyen, los importes de Excel y la resolución", async () => {
    const user = await start({ settings: { lastDir: "C:\\salida", skipped: ["CARGO"] } });
    respond("anonymize", null);
    expect(visible().queryByText("Importes en Excel")).toBeNull();
    dropFiles(["C:\\docs\\nominas.xlsx", "C:\\docs\\informe.pdf"]);
    await visible().findByText("nominas.xlsx");

    const cargo = visible().getByLabelText("Cargos") as HTMLInputElement;
    expect(cargo.checked).toBe(false);
    await user.click(visible().getByLabelText("Lugares"));
    await user.click(visible().getByLabelText("Cargos"));
    await user.click(visible().getByLabelText("Importes en Excel"));
    await user.selectOptions(
      visible().getByLabelText("Resolución de las páginas de PDF con datos"),
      "300",
    );
    await user.click(visible().getByRole("button", { name: "Anonimizar" }));

    const [{ job }] = backend.callsTo("anonymize") as { job: AnonymizeJob }[];
    expect(job.skipped).toEqual(["LUGAR"]);
    expect(job.spreadsheetAmounts).toBe(true);
    expect(job.resolution).toBe(300);
  });

  it("pide carpeta si no hay ninguna y explica los errores", async () => {
    const user = await start({ settings: { lastDir: null, skipped: [] } });
    respond("pick_files", ["C:\\docs\\nota.txt"]);
    await user.click(visible().getByRole("button", { name: "Elegir archivos…" }));
    await visible().findByText("nota.txt");
    const run = visible().getByRole("button", { name: "Anonimizar" }) as HTMLButtonElement;
    expect(run.disabled).toBe(true);

    respond("choose_folder", "C:\\otra");
    await user.click(visible().getByRole("button", { name: "Cambiar…" }));
    expect(await visible().findByText("C:\\otra")).toBeTruthy();
    expect(run.disabled).toBe(false);

    respond("anonymize", () => Promise.reject("Ya hay un lote en marcha."));
    await user.click(run);
    expect((await visible().findByRole("alert")).textContent).toBe("Ya hay un lote en marcha.");
  });

  it("muestra el fallo de un archivo y lo vuelve a intentar", async () => {
    const user = await start();
    respond("anonymize", null);
    dropFiles(["C:\\docs\\nota.txt"]);
    await visible().findByText("nota.txt");
    await user.click(visible().getByRole("button", { name: "Anonimizar" }));
    const id = (backend.callsTo("anonymize")[0] as { job: AnonymizeJob }).job.files[0].id;
    emit({ type: "progress", file: id, phase: { state: "failed", message: "Está protegido." } });
    expect(await visible().findByText("Está protegido.")).toBeTruthy();

    await user.click(visible().getByRole("button", { name: "Anonimizar" }));
    expect(backend.callsTo("anonymize")).toHaveLength(2);
  });

  it("quita archivos y vacía la lista", async () => {
    const user = await start();
    dropFiles(["C:\\docs\\nota.txt", "C:\\docs\\acta.md"]);
    await visible().findByText("acta.md");
    await user.click(visible().getByRole("button", { name: "Quitar nota.txt" }));
    expect(visible().queryByText("nota.txt")).toBeNull();
    await user.click(visible().getByRole("button", { name: "Vaciar lista" }));
    expect(visible().queryByText("acta.md")).toBeNull();
  });

  it("no repite un archivo que ya está en la lista", async () => {
    await start();
    dropFiles(["C:\\docs\\nota.txt"]);
    await visible().findByText("nota.txt");
    dropFiles(["C:\\docs\\nota.txt"]);
    await waitFor(() => expect(backend.callsTo("inspect")).toHaveLength(2));
    expect(visible().getAllByText("nota.txt")).toHaveLength(1);
  });

  it("avisa si no puede mirar los archivos", async () => {
    await start();
    respond("inspect", () => Promise.reject("No se puede leer."));
    dropFiles(["C:\\docs\\nota.txt"]);
    expect((await visible().findByRole("alert")).textContent).toBe("No se puede leer.");
  });

  it("avisa si no se pudo guardar el almacén", async () => {
    await start();
    emit({ type: "batchDone", done: 1, failed: 0, storeError: "Disco lleno." });
    expect(await screen.findByText(/No se ha podido guardar el almacén: Disco lleno\./)).toBeTruthy();
  });
});

describe("desanonimizar", () => {
  it("no deja desanonimizar un PDF y procesa el resto", async () => {
    const user = await start();
    respond("deanonymize", null);
    await user.click(tab("Desanonimizar"));
    dropFiles(["C:\\docs\\informe.pdf", "C:\\docs\\respuesta.md"]);
    await visible().findByText("respuesta.md");
    expect(visible().getByText(/Un PDF no se puede desanonimizar/)).toBeTruthy();

    await user.click(visible().getByRole("button", { name: "Desanonimizar" }));
    const [{ job }] = backend.callsTo("deanonymize") as { job: DeanonymizeJob }[];
    expect(job.files).toEqual([
      { id: expect.any(String), path: "C:\\docs\\respuesta.md", source: null },
    ]);
    const id = job.files[0].id;
    emit({
      type: "progress",
      file: id,
      phase: {
        state: "deanonymized",
        output: "C:\\salida\\respuesta_desanonimizado.md",
        restored: ["[EMAIL_0001]", "[NOMBRE_PERSONA_0001]"],
        missing: ["[TELEFONO_0003]"],
      },
    });
    expect(await visible().findByText("2 marcadores restaurados.")).toBeTruthy();
    expect(visible().getByText(/No están en el almacén.*\[TELEFONO_0003\]/)).toBeTruthy();
  });

  it("pide la fuente de un conflicto y repite solo ese archivo", async () => {
    const user = await start();
    respond("deanonymize", null);
    await user.click(tab("Desanonimizar"));
    dropFiles(["C:\\docs\\respuesta.md", "C:\\docs\\otra.txt"]);
    await visible().findByText("otra.txt");
    await user.click(visible().getByRole("button", { name: "Desanonimizar" }));
    const [{ job }] = backend.callsTo("deanonymize") as { job: DeanonymizeJob }[];
    const [first, second] = job.files.map((f) => f.id);
    emit({
      type: "progress",
      file: first,
      phase: {
        state: "conflict",
        conflicts: [
          {
            marker: "[NOMBRE_PERSONA_0001]",
            sources: [{ kind: "store" }, { kind: "dictionary", name: "de-ana.json" }],
          },
        ],
      },
    });
    emit({
      type: "progress",
      file: second,
      phase: { state: "deanonymized", output: "x", restored: [], missing: [] },
    });
    await user.click(await visible().findByRole("button", { name: "Diccionario «de-ana.json»" }));

    const again = backend.callsTo("deanonymize")[1] as { job: DeanonymizeJob };
    expect(again.job.files).toEqual([
      {
        id: first,
        path: "C:\\docs\\respuesta.md",
        source: { kind: "dictionary", name: "de-ana.json" },
      },
    ]);
  });

  it("restaura texto pegado y lo copia", async () => {
    const user = await start();
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    respond("deanonymize_text", {
      state: "done",
      text: "Hola, Marta Lozano.",
      restored: ["[NOMBRE_PERSONA_0001]"],
      missing: [],
    });
    await user.click(tab("Desanonimizar"));
    const restore = visible().getByRole("button", { name: "Desanonimizar texto" });
    expect((restore as HTMLButtonElement).disabled).toBe(true);
    await user.type(
      visible().getByLabelText("O pegue aquí el texto con marcadores"),
      "Hola, [[NOMBRE_PERSONA_0001].",
    );
    await user.click(restore);
    const restored = (await visible().findByLabelText("Texto restaurado")) as HTMLTextAreaElement;
    expect(restored.value).toBe("Hola, Marta Lozano.");
    expect(visible().getByText("1 marcador restaurado.")).toBeTruthy();

    await user.click(visible().getByRole("button", { name: "Copiar" }));
    expect(writeText).toHaveBeenCalledWith("Hola, Marta Lozano.");
    expect(visible().getByRole("button", { name: "Copiado" })).toBeTruthy();
  });

  it("pide la fuente si el texto tiene un conflicto", async () => {
    const user = await start();
    respond("deanonymize_text", (args) =>
      args.source
        ? { state: "done", text: "Hola, Ana.", restored: ["[N1]"], missing: ["[EMAIL_0002]"] }
        : {
            state: "conflict",
            conflicts: [{ marker: "[N1]", sources: [{ kind: "store" }, { kind: "dictionary", name: "d.json" }] }],
          },
    );
    await user.click(tab("Desanonimizar"));
    await user.type(visible().getByLabelText("O pegue aquí el texto con marcadores"), "Hola, [[N1].");
    await user.click(visible().getByRole("button", { name: "Desanonimizar texto" }));
    await user.click(await visible().findByRole("button", { name: "Mis documentos" }));
    expect(backend.callsTo("deanonymize_text")[1]).toEqual({
      text: "Hola, [N1].",
      source: { kind: "store" },
    });
    expect(await visible().findByText(/\[EMAIL_0002\]/)).toBeTruthy();
  });
});

describe("almacén", () => {
  const view = {
    documents: [
      {
        id: "doc-1",
        name: "Contrato cliente.docx",
        output: "contrato-cliente_anonimizado.docx",
        created: "2026-11-05T10:14:03+01:00",
        expires: "2027-02-03",
        entries: 7,
      },
    ],
    dictionaries: [{ name: "de-ana.json", document: "informe_anonimizado.docx", entries: 3 }],
  };

  it("lista los documentos y exporta con el aviso", async () => {
    const user = await start();
    respond("store_view", view);
    respond("store_export", {
      path: "C:\\salida\\contrato-cliente-anonimizado_diccionario.json",
      warning: "Este archivo contiene los datos originales sin cifrar.",
    });
    await user.click(screen.getByRole("button", { name: "Abrir el almacén" }));
    const panel = within(await screen.findByRole("complementary", { name: "Almacén" }));
    expect(await panel.findByText("Contrato cliente.docx")).toBeTruthy();
    expect(panel.getByText("05/11/2026")).toBeTruthy();
    expect(panel.getByText("03/02/2027")).toBeTruthy();
    expect(panel.getByText(/de-ana\.json · informe_anonimizado\.docx · 3 entradas/)).toBeTruthy();

    await user.click(panel.getByRole("button", { name: "Exportar diccionario" }));
    expect(await panel.findByText("Este archivo contiene los datos originales sin cifrar.")).toBeTruthy();
    expect(backend.callsTo("store_export")).toEqual([{ id: "doc-1" }]);

    await user.click(panel.getByRole("button", { name: "Cerrar el almacén" }));
    expect(screen.queryByRole("complementary", { name: "Almacén" })).toBeNull();
  });

  it("olvida, importa y vacía con confirmación", async () => {
    const user = await start();
    respond("store_view", view);
    respond("store_forget", null);
    respond("store_import", "de-pablo.json");
    respond("store_clear", null);
    await user.click(screen.getByRole("button", { name: "Abrir el almacén" }));
    const panel = within(await screen.findByRole("complementary", { name: "Almacén" }));
    await panel.findByText("Contrato cliente.docx");

    await user.click(panel.getByRole("button", { name: "Olvidar" }));
    expect(await panel.findByText("Documento olvidado.")).toBeTruthy();
    expect(backend.callsTo("store_forget")).toEqual([{ id: "doc-1" }]);

    await user.click(panel.getByRole("button", { name: "Importar diccionario…" }));
    expect(await panel.findByText("Diccionario importado: de-pablo.json.")).toBeTruthy();

    await user.click(panel.getByRole("button", { name: "Vaciar todo" }));
    await user.click(panel.getByRole("button", { name: "Cancelar" }));
    expect(backend.callsTo("store_clear")).toHaveLength(0);
    await user.click(panel.getByRole("button", { name: "Vaciar todo" }));
    await user.click(panel.getByRole("button", { name: "Sí, vaciar todo" }));
    expect(await panel.findByText("Almacén vaciado.")).toBeTruthy();
    expect(backend.callsTo("store_clear")).toHaveLength(1);
  });

  it("se recarga cuando el almacén cambia y explica los errores", async () => {
    const user = await start();
    respond("store_view", { documents: [], dictionaries: [] });
    await user.click(screen.getByRole("button", { name: "Abrir el almacén" }));
    const panel = within(await screen.findByRole("complementary", { name: "Almacén" }));
    expect(await panel.findByText("No hay documentos.")).toBeTruthy();
    expect(panel.getByText("No ha importado ninguno.")).toBeTruthy();

    respond("store_view", view);
    emit({ type: "storeChanged" });
    expect(await panel.findByText("Contrato cliente.docx")).toBeTruthy();

    respond("store_forget", () => Promise.reject("Espere a que termine el lote en curso."));
    await user.click(panel.getByRole("button", { name: "Olvidar" }));
    expect((await panel.findByRole("alert")).textContent).toBe(
      "Espere a que termine el lote en curso.",
    );
  });

  it("no se abre si el almacén no está disponible", async () => {
    await start({ store: { state: "failed", message: "Falta la clave." } });
    expect(
      (screen.getByRole("button", { name: "Abrir el almacén" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });
});

it("los archivos de ejemplo de las pruebas tienen el tipo esperado", () => {
  expect(file("x.pdf").deanonymizable).toBe(false);
  expect(file("x.rtf").outputs).toEqual([]);
});

describe("desanonimizar: elegir y errores", () => {
  it("elige archivos y carpeta con el diálogo", async () => {
    const user = await start({ settings: { lastDir: null, skipped: [] } });
    respond("pick_files", (args) => (args.deanonymizing ? ["C:\\docs\\respuesta.md"] : []));
    respond("choose_folder", null);
    await user.click(tab("Desanonimizar"));
    await user.click(visible().getByRole("button", { name: "Elegir archivos…" }));
    await visible().findByText("respuesta.md");
    // Cancelar el diálogo de carpeta no cambia nada.
    await user.click(visible().getByRole("button", { name: "Cambiar…" }));
    expect(visible().getByText("Sin elegir")).toBeTruthy();
    respond("choose_folder", "C:\\vuelta");
    await user.click(visible().getByRole("button", { name: "Cambiar…" }));
    expect(await visible().findByText("C:\\vuelta")).toBeTruthy();
  });

  it("explica los errores de los diálogos, del lote y del portapapeles", async () => {
    const user = await start();
    await user.click(tab("Desanonimizar"));
    respond("pick_files", () => Promise.reject("El diálogo ha fallado."));
    await user.click(visible().getByRole("button", { name: "Elegir archivos…" }));
    expect((await visible().findByRole("alert")).textContent).toBe("El diálogo ha fallado.");

    respond("choose_folder", () => Promise.reject("Sin carpetas."));
    await user.click(visible().getByRole("button", { name: "Cambiar…" }));
    expect((await visible().findByRole("alert")).textContent).toBe("Sin carpetas.");

    dropFiles(["C:\\docs\\respuesta.md"]);
    await visible().findByText("respuesta.md");
    respond("deanonymize", () => Promise.reject("El almacén no está abierto."));
    await user.click(visible().getByRole("button", { name: "Desanonimizar" }));
    expect((await visible().findByRole("alert")).textContent).toBe("El almacén no está abierto.");

    respond("deanonymize_text", () => Promise.reject("Texto demasiado largo."));
    await user.type(visible().getByLabelText("O pegue aquí el texto con marcadores"), "Hola");
    await user.click(visible().getByRole("button", { name: "Desanonimizar texto" }));
    expect((await visible().findByRole("alert")).textContent).toBe("Texto demasiado largo.");

    respond("deanonymize_text", { state: "done", text: "Hola", restored: [], missing: [] });
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: async () => Promise.reject(new Error("no")) },
      configurable: true,
    });
    await user.click(visible().getByRole("button", { name: "Desanonimizar texto" }));
    expect(await visible().findByText("0 marcadores restaurados.")).toBeTruthy();
    await user.click(visible().getByRole("button", { name: "Copiar" }));
    expect((await visible().findByRole("alert")).textContent).toBe(
      "No se ha podido copiar al portapapeles.",
    );
  });

  it("abre la carpeta y quita archivos de la lista", async () => {
    const user = await start();
    respond("deanonymize", null);
    respond("open_folder", null);
    await user.click(tab("Desanonimizar"));
    dropFiles(["C:\\docs\\respuesta.md", "C:\\docs\\otra.txt"]);
    await visible().findByText("otra.txt");
    await user.click(visible().getByRole("button", { name: "Quitar otra.txt" }));
    expect(visible().queryByText("otra.txt")).toBeNull();
    await user.click(visible().getByRole("button", { name: "Desanonimizar" }));
    await user.click(await visible().findByRole("button", { name: "Abrir carpeta" }));
    expect(backend.callsTo("open_folder")).toEqual([{ path: "C:\\salida" }]);
  });
});

describe("anonimizar: errores de los diálogos", () => {
  it("explica los errores de elegir archivos y carpeta", async () => {
    const user = await start();
    respond("pick_files", () => Promise.reject("Sin diálogo."));
    await user.click(visible().getByRole("button", { name: "Elegir archivos…" }));
    expect((await visible().findByRole("alert")).textContent).toBe("Sin diálogo.");
    respond("choose_folder", () => Promise.reject("Sin carpetas."));
    await user.click(visible().getByRole("button", { name: "Cambiar…" }));
    expect((await visible().findByRole("alert")).textContent).toBe("Sin carpetas.");
    respond("pick_files", []);
    await user.click(visible().getByRole("button", { name: "Elegir archivos…" }));
    expect(backend.callsTo("inspect")).toHaveLength(0);
  });
});
