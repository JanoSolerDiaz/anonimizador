// Lo único que la interfaz pide a Rust: comandos con rutas y opciones, y
// eventos de progreso. Los tipos son los de `src-tauri/src/servicio/dto.rs`.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";

/** Evento por el que llegan todos los avisos del servicio. */
export const EVENT = "anonimizador";

export type ModelStatus =
  | { state: "loading" }
  | { state: "ready" }
  | { state: "failed"; message: string };

export type StoreStatus = { state: "ready" } | { state: "failed"; message: string };

export interface Settings {
  lastDir: string | null;
  skipped: string[];
}

export interface Status {
  model: ModelStatus;
  store: StoreStatus;
  settings: Settings;
  optionalTypes: string[];
  busy: boolean;
}

export interface FileInfo {
  path: string;
  name: string;
  kind: string;
  format: string | null;
  outputs: string[];
  deanonymizable: boolean;
  reason: string | null;
}

export type Source = { kind: "store" } | { kind: "dictionary"; name: string };

export interface ConflictView {
  marker: string;
  sources: Source[];
}

export interface Warnings {
  images: number;
  objects: number;
  attachments: number;
  links: number;
}

export type Phase =
  | { state: "queued" }
  | { state: "reading" }
  | { state: "analyzing"; fraction: number }
  | { state: "writing" }
  | { state: "verifying" }
  | {
      state: "anonymized";
      output: string;
      entities: Record<string, number>;
      warnings: Warnings;
    }
  | { state: "deanonymized"; output: string; restored: string[]; missing: string[] }
  | { state: "conflict"; conflicts: ConflictView[] }
  | { state: "failed"; message: string };

export type AppEvent =
  | { type: "model"; status: ModelStatus }
  | { type: "progress"; file: string; phase: Phase }
  | { type: "batchDone"; done: number; failed: number; storeError: string | null }
  | { type: "storeChanged" };

export interface JobFile {
  id: string;
  path: string;
  format?: string | null;
  source?: Source | null;
}

export interface AnonymizeJob {
  files: JobFile[];
  dir: string;
  skipped: string[];
  spreadsheetAmounts: boolean;
  resolution: number;
}

export interface DeanonymizeJob {
  files: JobFile[];
  dir: string;
}

export type TextResult =
  | { state: "done"; text: string; restored: string[]; missing: string[] }
  | { state: "conflict"; conflicts: ConflictView[] };

export interface DocumentView {
  id: string;
  name: string;
  output: string;
  created: string;
  expires: string;
  entries: number;
}

export interface DictionaryView {
  name: string;
  document: string;
  entries: number;
}

export interface StoreView {
  documents: DocumentView[];
  dictionaries: DictionaryView[];
}

export interface Exported {
  path: string;
  warning: string;
}

export const api = {
  status: () => invoke<Status>("status"),
  inspect: (paths: string[]) => invoke<FileInfo[]>("inspect", { paths }),
  anonymize: (job: AnonymizeJob) => invoke<void>("anonymize", { job }),
  deanonymize: (job: DeanonymizeJob) => invoke<void>("deanonymize", { job }),
  deanonymizeText: (text: string, source: Source | null) =>
    invoke<TextResult>("deanonymize_text", { text, source }),
  storeView: () => invoke<StoreView>("store_view"),
  storeExport: (id: string) => invoke<Exported | null>("store_export", { id }),
  storeImport: () => invoke<string | null>("store_import"),
  storeForget: (id: string) => invoke<void>("store_forget", { id }),
  storeClear: () => invoke<void>("store_clear"),
  storeStartOver: () => invoke<string | null>("store_start_over"),
  pickFiles: (deanonymizing: boolean) => invoke<string[]>("pick_files", { deanonymizing }),
  chooseFolder: (start: string | null) => invoke<string | null>("choose_folder", { start }),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
};

export function onAppEvent(handler: (event: AppEvent) => void): Promise<UnlistenFn> {
  return listen<AppEvent>(EVENT, (event) => handler(event.payload));
}

/** Archivos soltados sobre la ventana. `hovering` dice si se arrastra encima. */
export function onFileDrop(
  hovering: (over: boolean) => void,
  dropped: (paths: string[]) => void,
): Promise<UnlistenFn> {
  return getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "drop") {
      hovering(false);
      if (payload.paths.length > 0) dropped(payload.paths);
    } else {
      hovering(payload.type !== "leave");
    }
  });
}

/** El mensaje de un error de un comando, que ya viene en español. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Ha ocurrido un error inesperado.";
}
