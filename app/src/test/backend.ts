// Backend falso para las pruebas: responde a los comandos como diga cada
// prueba y deja lanzar eventos del servicio y archivos soltados.

import type { AppEvent, FileInfo, Status } from "../lib/api";

type Handler = (args: Record<string, unknown>) => unknown;
type Listener = (event: { payload: unknown }) => void;

export const backend = {
  handlers: {} as Record<string, Handler>,
  calls: [] as { command: string; args: Record<string, unknown> }[],
  listeners: [] as Listener[],
  dropListeners: [] as Listener[],

  reset() {
    this.handlers = {};
    this.calls = [];
    this.listeners = [];
    this.dropListeners = [];
  },

  async invoke(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
    this.calls.push({ command, args });
    const handler = this.handlers[command];
    if (!handler) throw `Sin respuesta para ${command}`;
    return handler(args);
  },

  /** Llamadas a un comando, con sus argumentos. */
  callsTo(command: string): Record<string, unknown>[] {
    return this.calls.filter((c) => c.command === command).map((c) => c.args);
  },
};

export function respond(command: string, answer: unknown | Handler): void {
  backend.handlers[command] =
    typeof answer === "function" ? (answer as Handler) : () => structuredClone(answer);
}

export function emit(event: AppEvent): void {
  for (const listener of backend.listeners) listener({ payload: event });
}

export function dropFiles(paths: string[]): void {
  for (const listener of backend.dropListeners) {
    listener({ payload: { type: "enter", paths, position: { x: 0, y: 0 } } });
    listener({ payload: { type: "drop", paths, position: { x: 0, y: 0 } } });
  }
}

export function hoverFiles(over: boolean): void {
  for (const listener of backend.dropListeners) {
    listener({
      payload: over
        ? { type: "over", position: { x: 0, y: 0 } }
        : { type: "leave" },
    });
  }
}

export function status(overrides: Partial<Status> = {}): Status {
  return {
    model: { state: "ready" },
    store: { state: "ready" },
    settings: { lastDir: "C:\\salida", skipped: [] },
    optionalTypes: ["CARGO", "IMPORTE", "LUGAR", "COORDENADAS"],
    busy: false,
    ...overrides,
  };
}

export function file(name: string, overrides: Partial<FileInfo> = {}): FileInfo {
  const extension = name.split(".").pop() ?? "";
  const own: Record<string, [string, string[]]> = {
    pdf: ["PDF digital", ["pdf", "docx", "md", "txt"]],
    docx: ["DOCX", ["docx", "md", "txt"]],
    xlsx: ["XLSX", ["xlsx", "md", "txt"]],
    txt: ["TXT", ["txt", "md"]],
    md: ["MD", ["md", "txt"]],
  };
  const [kind, outputs] = own[extension] ?? ["No admitido", []];
  return {
    path: `C:\\docs\\${name}`,
    name,
    kind,
    format: outputs.length > 0 ? extension : null,
    outputs,
    deanonymizable: outputs.length > 0 && extension !== "pdf",
    reason: outputs.length > 0 ? null : `El formato «${extension}» no se admite.`,
    ...overrides,
  };
}

/** `inspect` que responde con `file()` para cada ruta. */
export function inspectByName(overrides: Record<string, Partial<FileInfo>> = {}): void {
  respond("inspect", (args) =>
    (args.paths as string[]).map((path) => {
      const name = path.split("\\").pop() ?? path;
      return { ...file(name, overrides[name]), path };
    }),
  );
}
