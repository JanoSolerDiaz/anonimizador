// Estado compartido por las pestañas: el de la app y la fase de cada archivo.
// `App` escucha los eventos del servicio en un solo sitio y los aplica aquí.

import type { AppEvent, FileInfo, Phase, Source, Status } from "./api";

/** Un archivo de la lista de una pestaña. */
export interface Row {
  id: string;
  info: FileInfo;
  /** Formato de salida elegido (solo al anonimizar). */
  format: string | null;
  /** Fuente elegida tras un conflicto (solo al desanonimizar). */
  source: Source | null;
}

export const app = $state({
  status: null as Status | null,
  /** Error al hablar con el servicio al arrancar. */
  startError: null as string | null,
  /** Fase de cada archivo, por su id. */
  phases: {} as Record<string, Phase>,
  /** Resultado del último lote que ha terminado. */
  lastBatch: null as { done: number; failed: number; storeError: string | null } | null,
  /** Sube cada vez que el almacén cambia fuera del panel. */
  storeVersion: 0,
});

let counter = 0;

/** Identificador único de una fila, para reconocer sus eventos. */
export function nextId(): string {
  counter += 1;
  return `f${counter}`;
}

export function applyEvent(event: AppEvent): void {
  switch (event.type) {
    case "model":
      if (app.status) app.status.model = event.status;
      break;
    case "progress":
      app.phases[event.file] = event.phase;
      break;
    case "batchDone":
      app.lastBatch = { done: event.done, failed: event.failed, storeError: event.storeError };
      app.storeVersion += 1;
      break;
    case "storeChanged":
      app.storeVersion += 1;
      break;
  }
}

/** Filas nuevas para los archivos, sin repetir los que ya están. */
export function addRows(rows: Row[], files: FileInfo[], pickFormat: (info: FileInfo) => string | null): Row[] {
  const known = new Set(rows.map((row) => row.info.path));
  const added: Row[] = [];
  for (const info of files) {
    if (known.has(info.path)) continue;
    known.add(info.path);
    added.push({ id: nextId(), info, format: pickFormat(info), source: null });
  }
  return [...rows, ...added];
}

/** Si la fase es de un archivo que aún se está procesando. */
export function inProgress(phase: Phase | undefined): boolean {
  return (
    phase !== undefined &&
    ["queued", "reading", "analyzing", "writing", "verifying"].includes(phase.state)
  );
}
