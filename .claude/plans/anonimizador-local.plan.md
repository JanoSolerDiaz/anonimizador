# Plan: Anonimizador local — Hito 1 (Esqueleto)

**Source PRD**: `specs/anonimizador-local.prd.md`
**Selected Milestone**: Hito 1 — Esqueleto
**Complexity**: Medium

## Summary

Crear la base del repositorio: workspace de Cargo con el crate `nucleo` (sin Tauri) y la app Tauri 2 + Svelte 5 + TypeScript que abre una ventana vacía con la CSP sin red y permisos mínimos. Añadir los scripts que descargan el modelo, ONNX Runtime y PDFium en versiones fijas con comprobación SHA-256, y una prueba de humo que carga las dos bibliotecas dinámicas para descubrir pronto problemas de enlace. El hito termina cuando todo compila y las pruebas pasan en Windows y en macOS.

## Estado de partida

- Proyecto vacío: solo existe `specs/anonimizador-local.prd.md`. No es un repositorio git.
- En este equipo Windows: Node 24.18 y npm 11.16 instalados. **Rust no está instalado** (`rustc` y `cargo` no se encuentran).
- No hay un Mac disponible en esta sesión para comprobar la compilación en macOS.

## Patterns to Mirror

No hay código previo: no existen patrones que imitar. Este hito **define** las convenciones que seguirán los demás:

| Category | Source | Pattern |
|---|---|---|
| Naming | PRD §Instrucciones | Identificadores en inglés (`snake_case` en Rust, `camelCase` en TS). Carpetas y módulos de dominio con los nombres del PRD (`motor`, `formatos`, `almacen`). Comentarios, mensajes y documentación en español |
| Errors | (nuevo) `nucleo/src/error.rs` | Enum `Error` con `thiserror`, un `Result<T>` propio del crate. Mensajes al usuario en español y sin jerga; sin datos del documento en el mensaje |
| Logging | (nuevo) `tracing` | Solo metadatos: nombre de archivo, tiempos, recuentos. Nunca texto ni valores del almacén. Se fija ya para que nadie meta `println!` con datos |
| Data access | n/a en este hito | El almacén llega en el hito 3 |
| Tests | (nuevo) `nucleo/tests/` + `#[cfg(test)]` | `cargo test -p nucleo`. Pruebas de integración en `nucleo/tests/`, unitarias junto al código |

## Files to Change

| File | Action | Why |
|---|---|---|
| `Cargo.toml` | CREATE | Workspace con miembros `nucleo` y `app/src-tauri`, `resolver = "3"`, perfil `release` con `lto` y `codegen-units = 1` |
| `nucleo/Cargo.toml` | CREATE | Crate librería + binario `cli`. Sin dependencia de Tauri |
| `nucleo/src/lib.rs` | CREATE | Declara `motor`, `formatos`, `almacen`, `error`, `recursos` |
| `nucleo/src/motor/mod.rs` | CREATE | Módulo vacío con comentario de propósito |
| `nucleo/src/formatos/mod.rs` | CREATE | Submódulos vacíos `docx`, `pdf`, `texto` |
| `nucleo/src/almacen/mod.rs` | CREATE | Módulo vacío |
| `nucleo/src/error.rs` | CREATE | Tipo de error común |
| `nucleo/src/recursos.rs` | CREATE | Localiza `recursos/` y comprueba que estén los archivos esperados |
| `nucleo/src/bin/cli.rs` | CREATE | `cli version` y `cli comprobar-recursos` |
| `nucleo/tests/humo_recursos.rs` | CREATE | Carga ONNX Runtime + modelo y PDFium. Se ignora si faltan los recursos |
| `app/package.json`, `app/vite.config.ts`, `app/svelte.config.js`, `app/tsconfig.json`, `app/index.html` | CREATE | Interfaz Svelte 5 + TS + Vite (plantilla `svelte-ts` de create-tauri-app, recortada) |
| `app/src/main.ts`, `app/src/App.svelte`, `app/src/estilos.css` | CREATE | Ventana vacía con el título y variables CSS de base |
| `app/src-tauri/Cargo.toml`, `build.rs`, `src/main.rs`, `src/lib.rs` | CREATE | App Tauri 2 que depende de `nucleo` y registra `single-instance` |
| `app/src-tauri/tauri.conf.json` | CREATE | Ventana, CSP sin red, actualizador desactivado, identificador |
| `app/src-tauri/capabilities/default.json` | CREATE | Permisos mínimos |
| `scripts/recursos.lock.json` | CREATE | Revisión del modelo, versiones de ORT y PDFium, URL y SHA-256 de cada archivo |
| `scripts/descargar-modelo.mjs` | CREATE | Descarga los 4 archivos del modelo en la revisión fija y comprueba SHA-256 |
| `scripts/descargar-binarios.mjs` | CREATE | Descarga ONNX Runtime y PDFium para la plataforma actual (o todas con `--todas`) y comprueba SHA-256 |
| `.gitignore` | CREATE | `recursos/`, `target/`, `node_modules/`, `app/dist/` |
| `DECISIONES.md` | CREATE | Registro de cambios respecto al PRD |
| `README.md` | CREATE | Requisitos, cómo descargar recursos, compilar y probar |
| `.github/workflows/compilar.yml` | CREATE (si se confirma) | Matriz Windows + macOS para comprobar la compilación sin un Mac local |

## Tasks

### Task 0: Requisitos del equipo (acción del usuario)
- **Action**: Instalar Rust con `rustup` (toolchain `stable-x86_64-pc-windows-msvc`) y Visual Studio Build Tools con "Desarrollo para el escritorio con C++". WebView2 ya viene en Windows 11. En el Mac: Xcode Command Line Tools y `rustup target add aarch64-apple-darwin x86_64-apple-darwin`.
- **Mirror**: Requisitos oficiales de Tauri 2.
- **Validate**: `rustc --version && cargo --version`

### Task 1: Comprobar versiones actuales
- **Action**: Consultar en crates.io/docs.rs y npm las versiones estables de `tauri`, `tauri-build`, `tauri-plugin-single-instance`, `@tauri-apps/cli`, `@tauri-apps/api`, `svelte`, `vite`, `@sveltejs/vite-plugin-svelte`, `ort`, `pdfium-render`, `thiserror`, `tracing`. Elegir la versión de ONNX Runtime que pide la versión de `ort` elegida, y la release de `bblanchon/pdfium-binaries` compatible con `pdfium-render`. Fijar la revisión (hash de commit) del modelo en Hugging Face.
- **Mirror**: PRD §Instrucciones: "No confíes en APIs de memoria".
- **Validate**: Versiones anotadas en `scripts/recursos.lock.json` y en `DECISIONES.md` (versión de ORT/PDFium y por qué).

### Task 2: Workspace y crate `nucleo`
- **Action**: Crear `Cargo.toml` del workspace y `nucleo` con los módulos vacíos, `error.rs` y `recursos.rs`. CLI sin dependencias externas de argumentos por ahora (`std::env::args`). Se valorará `clap` en el hito 2, cuando haya subcomandos reales.
- **Mirror**: Estructura de carpetas del PRD §Estructura del repositorio.
- **Validate**: `cargo build -p nucleo && cargo test -p nucleo && cargo run -p nucleo --bin cli -- version`

### Task 3: Interfaz Svelte 5 + Tauri 2
- **Action**: Generar con `npm create tauri-app@latest` (plantilla `svelte-ts`, npm) en `app/`, quitar el código de ejemplo (`greet`), y dejar una ventana con el título "Anonimizador" y las dos pestañas como texto estático, sin lógica. `app/src-tauri` entra en el workspace raíz y depende de `nucleo` por ruta.
- **Mirror**: PRD §Stack: sin librerías de componentes, CSS propio con variables.
- **Validate**: `cd app && npm ci && npm run check && npm run build`

### Task 4: Seguridad de la configuración de Tauri
- **Action**: En `tauri.conf.json`: CSP `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost`; sin `plugins.updater`; `bundle.createUpdaterArtifacts: false`. En `capabilities/default.json`: solo `core:default` recortado a lo imprescindible (ventana y eventos). Registrar `tauri-plugin-single-instance`, que al lanzar una segunda instancia enfoca la ventana existente. El arrastre de archivos se resuelve con el evento nativo de ventana de Tauri 2 (no hace falta plugin) y el plugin de diálogo se añade en el hito 5.
- **Mirror**: PRD §Seguridad y privacidad.
- **Validate**: `cargo tree -p anonimizador -e normal | grep -Ei "reqwest|hyper|ureq|isahc|curl"` no devuelve nada; abrir la app dos veces deja una sola ventana.

### Task 5: Scripts de descarga con SHA-256
- **Action**: Dos scripts Node (`.mjs`, sin dependencias, usando `fetch` y `node:crypto`), multiplataforma porque Node ya es requisito del proyecto. Leen `scripts/recursos.lock.json`, descargan a un archivo temporal dentro de `recursos/`, comprueban SHA-256 y solo entonces renombran. Si el hash no coincide: borran el temporal y salen con error. Si el archivo ya existe con el hash correcto, no lo descargan. Destinos: `recursos/modelo/` (`model_quantized.onnx`, `config.json`, `tokenizer.json`, `tokenizer_config.json`), `recursos/onnxruntime/<plataforma>/`, `recursos/pdfium/<plataforma>/`, y las licencias de cada uno. Los hashes iniciales del modelo se toman de los metadatos LFS de Hugging Face y se comprueban a mano; los de los binarios, de la primera descarga revisada. Se escriben en el lock y no se recalculan nunca de forma automática.
- **Mirror**: PRD §Empaquetado: "comprueba su SHA-256 antes de compilar".
- **Validate**: `node scripts/descargar-modelo.mjs && node scripts/descargar-binarios.mjs`; alterar un byte de un archivo descargado y volver a ejecutar: debe detectarlo y volver a descargarlo.

### Task 6: Prueba de humo de bibliotecas nativas
- **Action**: `cli comprobar-recursos` y `nucleo/tests/humo_recursos.rs`: crear una sesión de `ort` (modo `load-dynamic`, apuntando a la biblioteca de `recursos/`) con `model_quantized.onnx` y comprobar las entradas y salidas; enlazar PDFium con `pdfium-render` desde `recursos/` y abrir un PDF sintético mínimo generado en la propia prueba. Sin inferencia real todavía. Detecta en el hito 1, y no en el 4, cualquier problema de versiones o enlace dinámico.
- **Mirror**: PRD §Instrucciones: `nucleo` se prueba con `cargo test` y la CLI.
- **Validate**: `cargo test -p nucleo -- --include-ignored` en verde con los recursos descargados.

### Task 7: Compilación en Windows y macOS — APLAZADA

> Decisión de Jano (2026-10-22): de momento se deja de lado la compilación en Mac. Se inicializa git sin workflow de CI. El identificador `com.cuatroochenta.anonimizador` y la CSP con `http://ipc.localhost` quedan aprobados.

- **Action**: Workflow de GitHub Actions con matriz `windows-latest` y `macos-latest`: descargar recursos (con caché), `cargo test -p nucleo -- --include-ignored`, `npm run check`, `npm run tauri build -- --debug --no-bundle`. El empaquetado firmado queda para el hito 6.
- **Mirror**: PRD Hito 1: "Compilación en Windows y macOS".
- **Validate**: Workflow en verde en las dos plataformas. Alternativa si no hay GitHub: ejecutar los mismos comandos a mano en un Mac.

### Task 8: Documentación
- **Action**: `README.md` (requisitos, pasos, comandos). `DECISIONES.md` con las decisiones de este hito (ver Risks). Resumen del hito al terminar: qué se ha hecho, qué pasa y qué queda.
- **Validate**: Un equipo limpio puede compilar siguiendo solo el README.

## Validation

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p nucleo -- --include-ignored
cargo run -p nucleo --bin cli -- comprobar-recursos

# Interfaz
cd app && npm ci && npm run check && npm run build

# App completa (sin instalador)
cd app && npm run tauri build -- --debug --no-bundle

# Sin clientes HTTP en el binario
cargo tree --workspace -e normal | grep -Ei "reqwest|hyper|ureq|isahc|curl"   # debe salir vacío
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Rust y las Build Tools de MSVC no están instalados en este equipo | Seguro | Task 0. La instalación requiere permisos de administrador: la hace el usuario |
| Sin un Mac no se puede comprobar "compila en macOS" | Alta | GitHub Actions con `macos-latest` (requiere repositorio en GitHub), o compilar a mano en un Mac |
| La CSP del PRD (`connect-src 'self' ipc:`) rompe el IPC en Windows, donde Tauri 2 usa `http://ipc.localhost` | Alta | Añadir `http://ipc.localhost`, que es local y no abre la red. **Cambio respecto al PRD: necesita tu visto bueno** y quedará en `DECISIONES.md` |
| `ort` en modo por defecto descarga ONNX Runtime al compilar | Media | Usar la feature `load-dynamic` sin `download-binaries`. La biblioteca viene de `recursos/`, descargada por nuestro script con hash fijo |
| La versión de ONNX Runtime no coincide con la que espera `ort` | Media | Fijar ambas a la vez (Task 1) y comprobarlo con la prueba de humo (Task 6) |
| PDFium para Mac se publica por arquitectura (x64 y arm64), no universal | Media | El script descarga las dos. El `lipo` a binario universal se hace en el hito 6 |
| `tokenizer.json` y `config.json` no van por LFS y Hugging Face no publica su SHA-256 | Media | Calcular el hash en la primera descarga, revisarlo y fijarlo en el lock con la revisión de commit |
| El "humo" de `ort`/PDFium alarga el hito | Baja | Es solo cargar bibliotecas y el modelo, sin lógica. Compensa porque es el mayor riesgo técnico del proyecto |

## Preguntas abiertas

1. **Identificador de la app** para Tauri, el MSI y el PKG (no se puede cambiar sin romper instalaciones). Propuesta: `com.cuatroochenta.anonimizador`.
2. **CSP**: ¿aceptas añadir `http://ipc.localhost`?
3. **Control de versiones y CI**: ¿inicializo git y creo el workflow de GitHub Actions para la compilación en macOS? ¿Hay un repositorio remoto o lo creas tú?

## Acceptance

Estado a 2026-10-23: **Hito 1 completo en Windows.** macOS aplazado por decisión de Jano.

- [x] All tasks complete (salvo Task 7, aplazada)
- [x] Validation passes en Windows (macOS aplazado)
- [x] `nucleo` no depende de Tauri (`cargo tree -p nucleo | grep tauri` vacío)
- [x] Ningún crate con cliente HTTP en el binario
- [x] Los scripts rechazan archivos con SHA-256 incorrecto
- [x] La app abre la ventana y no permite una segunda instancia (comprobado visualmente por Jano)
- [x] `DECISIONES.md` y `README.md` creados
- [x] Patterns mirrored, not reinvented (aquí: convenciones definidas y documentadas para los hitos siguientes)
