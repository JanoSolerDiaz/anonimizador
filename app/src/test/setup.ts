// Preparación de todas las pruebas: la API de Tauri va contra el backend
// falso, y el estado compartido empieza limpio en cada prueba.

import { beforeEach, vi } from "vitest";
import { backend } from "./backend";
import { app } from "../lib/estado.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) => backend.invoke(command, args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_name: string, listener: (event: { payload: unknown }) => void) => {
    backend.listeners.push(listener);
    return () => {
      backend.listeners = backend.listeners.filter((l) => l !== listener);
    };
  },
}));

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async (listener: (event: { payload: unknown }) => void) => {
      backend.dropListeners.push(listener);
      return () => {
        backend.dropListeners = backend.dropListeners.filter((l) => l !== listener);
      };
    },
  }),
}));

beforeEach(() => {
  backend.reset();
  app.status = null;
  app.startError = null;
  app.phases = {};
  app.lastBatch = null;
  app.storeVersion = 0;
});
