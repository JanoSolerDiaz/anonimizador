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

### `reqwest` aparece en `Cargo.lock`, pero no en el binario de escritorio

`tauri` 2.12 declara `reqwest` solo para Android e iOS (proxy del servidor de desarrollo móvil). `Cargo.lock` lista las dependencias de todas las plataformas, así que aparece ahí, pero `cargo tree --target` confirma que no entra en Windows ni en macOS. La garantía "sin red" se comprueba por plataforma en `scripts/test/dependencias.test.mjs`, no con el lock.

### Pendiente para macOS (hito 6)

- ONNX Runtime 1.31.0 solo publica binarios de macOS para arm64. No hay paquete x86_64 ni universal2. Para el binario universal habrá que compilar ONNX Runtime para Intel, buscar otra fuente o reconsiderar el soporte de Mac Intel.
- Las rutas internas de los paquetes de macOS en `recursos.lock.json` no se han comprobado todavía.

## Hito 2 · 2026-10-26

### Nombres de los marcadores

El PRD daba ejemplos genéricos (`PERSONA`, `DIRECCION`, `SALUD`). Jano aprobó una tabla con un marcador por cada clase del modelo, por ejemplo `NOMBRE_PERSONA`, `DIRECCION_POSTAL` y `DATOS_SALUD`, más `DNI`, `NIE` e `IBAN`, que solo detectan las reglas. La tabla está en `nucleo/config/tipos.json` y `nucleo/tests/tipos.rs` la fija.

### Tipos opcionales

`CARGO`, `IMPORTE`, `LUGAR` y `COORDENADAS` se pueden dejar sin sustituir. Al anonimizar, el usuario elige cuáles; por defecto se sustituyen todos. El resto de tipos no se puede desactivar, y pedirlo da un error. En el Hito 2 se elige con la opción `--no-sustituir` de la CLI; en el Hito 5, con casillas junto al selector de formato.

### La clave de un marcador es solo el valor normalizado

El PRD dice que el mismo valor recibe siempre el mismo marcador. El modelo puede clasificar un mismo texto con tipos distintos en dos sitios (por ejemplo, «Valencia» como lugar y como organización). Si la clave incluyera el tipo, ese texto tendría dos marcadores y la restauración seguiría funcionando, pero el mismo dato aparecería con dos nombres en el documento anonimizado. Por eso la clave es solo el valor normalizado, y el dato conserva el tipo con el que apareció por primera vez.

La normalización quita las marcas diacríticas tras descomponer el texto (NFD). Eso incluye la tilde de la «ñ» y la cedilla de la «ç»: «Muñoz» y «Munoz» comparten marcador.
