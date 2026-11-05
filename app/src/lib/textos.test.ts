import { describe, expect, it } from "vitest";
import { errorMessage } from "./api";
import { addRows, inProgress } from "./estado.svelte";
import { formatLabel, kindLabel, phaseLabel, sourceLabel, warningLines } from "./textos";

describe("textos", () => {
  it("nombra los tipos de la tabla y los desconocidos", () => {
    expect(kindLabel("NOMBRE_PERSONA")).toBe("Nombres de personas");
    expect(kindLabel("TIPO_NUEVO")).toBe("tipo nuevo");
  });

  it("nombra los formatos", () => {
    expect(formatLabel("docx")).toBe("Word (DOCX)");
    expect(formatLabel("odt")).toBe("ODT");
  });

  it("nombra las fuentes", () => {
    expect(sourceLabel({ kind: "store" })).toBe("Mis documentos");
    expect(sourceLabel({ kind: "dictionary", name: "d.json" })).toBe("Diccionario «d.json»");
  });

  it("dice en qué va cada fase y nada si ha terminado", () => {
    expect(phaseLabel({ state: "queued" })).toBe("En cola");
    expect(phaseLabel({ state: "reading" })).toBe("Leyendo");
    expect(phaseLabel({ state: "analyzing", fraction: 0.257 })).toBe("Analizando 26 %");
    expect(phaseLabel({ state: "writing" })).toBe("Escribiendo");
    expect(phaseLabel({ state: "verifying" })).toBe("Comprobando");
    expect(phaseLabel({ state: "failed", message: "x" })).toBeNull();
  });

  it("explica cada aviso en singular o plural", () => {
    expect(warningLines({ images: 0, objects: 0, attachments: 0, links: 0 })).toEqual([]);
    expect(warningLines({ images: 2, objects: 1, attachments: 1, links: 1 })).toEqual([
      "El documento tiene 2 imágenes. Su contenido no se ha revisado.",
      "El documento tiene 1 objeto incrustado. Su contenido no se ha revisado.",
      "El PDF tenía 1 archivo adjunto. Se ha quitado de la salida.",
      "Se ha quitado 1 enlace que llevaba un dato en su destino.",
    ]);
    expect(warningLines({ images: 1, objects: 3, attachments: 2, links: 4 })).toEqual([
      "El documento tiene 1 imagen. Su contenido no se ha revisado.",
      "El documento tiene 3 objetos incrustados. Su contenido no se ha revisado.",
      "El PDF tenía 2 archivos adjuntos. Se han quitado de la salida.",
      "Se han quitado 4 enlaces que llevaban un dato en su destino.",
    ]);
  });
});

describe("errores y estado", () => {
  it("saca el mensaje de cualquier error", () => {
    expect(errorMessage("Falta el archivo.")).toBe("Falta el archivo.");
    expect(errorMessage(new Error("Roto."))).toBe("Roto.");
    expect(errorMessage(42)).toBe("Ha ocurrido un error inesperado.");
  });

  it("sabe qué fases están en curso", () => {
    expect(inProgress(undefined)).toBe(false);
    expect(inProgress({ state: "writing" })).toBe(true);
    expect(inProgress({ state: "failed", message: "x" })).toBe(false);
  });

  it("no repite rutas al añadir filas", () => {
    const info = {
      path: "a.txt",
      name: "a.txt",
      kind: "TXT",
      format: "txt",
      outputs: ["txt"],
      deanonymizable: true,
      reason: null,
    };
    const rows = addRows([], [info, info], () => "txt");
    expect(rows).toHaveLength(1);
    expect(addRows(rows, [info], () => "txt")).toHaveLength(1);
  });
});
