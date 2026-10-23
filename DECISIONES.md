# Decisiones

Cambios y concreciones respecto a `specs/anonimizador-local.prd.md`, con su razón.

## Hito 1 · 2026-10-22

### CSP: se añade `http://ipc.localhost` a `connect-src`

En Windows, Tauri 2 comunica la interfaz con Rust a través de `http://ipc.localhost`. Con solo `connect-src 'self' ipc:` los comandos no funcionarían en Windows. La dirección es local y no da acceso a la red. Aprobado por Jano.

### Interfaz: Svelte 5 + Vite sin SvelteKit

La plantilla `svelte-ts` de create-tauri-app genera SvelteKit. Una ventana de dos pestañas no necesita router ni adaptador estático, así que se usa Svelte con Vite directamente, como dice el stack del PRD.

### `ort` 2.0.0-rc.15 (versión candidata)

`ort` no tiene versión 2.x estable, y la 1.x es antigua y usa otra API. Se fija exactamente `=2.0.0-rc.15`, con las features `std`, `load-dynamic` y `api-31`, y sin `download-binaries`, `fetch-models` ni TLS, para que el crate no pueda descargar nada. La biblioteca de ONNX Runtime 1.31.0, la que corresponde a `api-31`, se carga desde `recursos/`.

### Telemetría de ONNX Runtime desactivada

ONNX Runtime activa por defecto la telemetría ETW en Windows. Se desactiva con `with_telemetry(false)` al crear el entorno, por el requisito "sin telemetría".

### PDFium chromium/7881

`pdfium-render` 0.9.4 conoce la API de PDFium hasta la versión 7881. Se usan los binarios de `bblanchon/pdfium-binaries` de esa misma versión, aunque haya más recientes, para que las firmas de las funciones coincidan.

### TypeScript 6.0.3

TypeScript 7 ya está publicado, pero `svelte-check` 4.7.7 solo admite TypeScript 5 y 6.

### Scripts de descarga en Node

`scripts/descargar-modelo.mjs` y `scripts/descargar-binarios.mjs` usan Node sin dependencias. Node ya es requisito del proyecto, y así un solo script sirve en Windows y en macOS. Las versiones, URL y SHA-256 están en `scripts/recursos.lock.json`.

### Pendiente para macOS (hito 6)

- ONNX Runtime 1.31.0 solo publica binarios de macOS para arm64. No hay paquete x86_64 ni universal2. Para el binario universal habrá que compilar ONNX Runtime para Intel, buscar otra fuente o reconsiderar el soporte de Mac Intel.
- Las rutas internas de los paquetes de macOS en `recursos.lock.json` no se han comprobado todavía.
