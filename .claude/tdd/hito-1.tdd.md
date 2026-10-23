# Evidencias TDD: pruebas del Hito 1

**Plan de origen**: `.claude/plans/anonimizador-local.plan.md` (Hito 1, Esqueleto)
**Rama**: `tests/hito-1` · **Fecha**: 2026-10-23

## Contexto

El código del Hito 1 se escribió antes que las pruebas. Por eso hay dos tipos de evidencia:

- **Pruebas de caracterización** del comportamiento que ya existía. Su RED se demuestra con **mutaciones**: se introduce un fallo deliberado en el código, se comprueba que al menos una prueba falla y se deshace el cambio con `git checkout`.
- **Refactors guiados por prueba** donde el código no se podía probar (dependía del directorio actual, de la plataforma o vivía dentro de un script). En estos casos hay RED real: la prueba se escribió primero y falló porque faltaba la función.

## Recorridos de usuario

- Como desarrollador, quiero que la app encuentre sus recursos (modelo, ONNX Runtime, PDFium) en desarrollo y una vez instalada, para no configurar rutas a mano.
- Como desarrollador, quiero que los scripts descarguen siempre exactamente los mismos archivos verificados, para que nadie pueda colar un binario alterado.
- Como responsable de seguridad, quiero que una prueba falle si alguien abre la CSP, añade permisos, activa el actualizador o mete un cliente HTTP en el binario.
- Como usuario, quiero ver las pestañas Anonimizar y Desanonimizar y poder cambiar entre ellas.

## Informe por tarea

| Tarea del plan | Objetivo de prueba | RED | GREEN |
|---|---|---|---|
| T2 · `nucleo::recursos` | `nucleo/tests/recursos.rs`, `recursos_env.rs` | Compilación: `E0599 no associated function locate_from` (5 usos), commit `b339599` | 12 pass, commit `ff45479` |
| T2 · errores y CLI | `src/error.rs` (unitarias), `tests/cli.rs` | Mutaciones (ver abajo) | 31 pass en `nucleo`, commit `e4ed377` |
| T5 · contrato lock ↔ Rust | `nucleo/tests/contrato_lock.rs` | Mutaciones en el lock | 7 pass, commit `e4ed377` |
| T4 · configuración Tauri | `app/src-tauri/tests/configuracion.rs` | Mutaciones en conf, capabilities y `lib.rs` | 12 pass, commit `0f898b9` |
| T5 · scripts de descarga | `scripts/test/recursos.test.mjs` | `SyntaxError: does not provide an export named 'nombrePlataforma'` (ni `paqueteAlDia`), commit `4265f8a` | 19 pass, commit `2c3db36` |
| T4 · sin red en el binario | `scripts/test/dependencias.test.mjs` | Mutación: `cargo add ureq` en `nucleo` → 3 fallan | 4 pass, commit `229d05a` |
| T3 · interfaz | `app/src/App.test.ts`, `main.test.ts` | Mutaciones en `App.svelte` y `main.ts` | 7 pass, commit `31693e1` |
| T6 · camino de error nativo | `nucleo/tests/humo_errores.rs` | Mutación `ModelLoad`→`PdfiumLoad` → 1 falla | 1 pass |

### Mutaciones detectadas

Todas se aplicaron y se deshicieron con `git checkout` sobre un árbol con el trabajo ya en commit.

| Archivo | Mutación | Pruebas que fallan |
|---|---|---|
| `recursos.rs` | `check_files` no comprueba PDFium | 1 |
| `recursos.rs` | `is_file` → `exists` (acepta carpetas) | 1 |
| `recursos.rs` | `locate` ignora `ANONIMIZADOR_RECURSOS` | 1 |
| `recursos.rs` | `locate_from` acepta un archivo llamado `recursos` | 1 |
| `recursos.rs` | nombre del modelo cambiado | 2 |
| `recursos.rs` | `win-x64` → `windows` | 1 |
| `recursos.rs` | error del modelo clasificado como de PDFium | 1 |
| `cli.rs` | código de salida de uso 2 → 1 | 2 |
| `cli.rs` | `version` sin prefijo | 1 |
| `cli.rs` | errores a stdout | 1 |
| `error.rs` | `ResourceMissing` sin la ruta | 1 |
| `recursos.lock.json` | `pdfium.dll` renombrada | 1 |
| `recursos.lock.json` | falta `tokenizer_config.json` | 1 |
| `recursos.lock.json` | URL `http://` | 1 |
| `tauri.conf.json` | `connect-src` con dominio externo | 2 |
| `tauri.conf.json` | `script-src` con `'unsafe-eval'` | 1 |
| `tauri.conf.json` | `img-src` con comodín | 1 |
| `tauri.conf.json` | `devCsp` con dominio externo | 1 |
| `tauri.conf.json` | `freezePrototype: false` | 1 |
| `tauri.conf.json` | `createUpdaterArtifacts: true` | 1 |
| `tauri.conf.json` | otro identificador | 1 |
| `capabilities/default.json` | permiso `core:path:default` | 1 |
| `src-tauri/src/lib.rs` | otro plugin antes de single-instance | 1 |
| `src-tauri/src/lib.rs` | enfoca una ventana que no es `main` | 1 |
| `scripts/lib/recursos.mjs` | `descargar` no comprueba el hash | 2 |
| `scripts/lib/recursos.mjs` | `descargar` no borra el temporal | 1 |
| `scripts/lib/recursos.mjs` | `descargar` ignora el estado HTTP | 1 |
| `scripts/lib/recursos.mjs` | `paqueteAlDia` no compara con el lock | 1 |
| `scripts/lib/recursos.mjs` | `paqueteAlDia` no recalcula hashes | 1 |
| `scripts/lib/recursos.mjs` | `copiarArchivos` ignora archivos ausentes | 1 |
| `scripts/lib/recursos.mjs` | `mac-arm64` mal nombrado | 1 |
| `App.svelte` | empieza en Desanonimizar | 1 |
| `App.svelte` | `aria-pressed` fijo | 1 |
| `App.svelte` | el clic no cambia de pestaña | 1 |
| `App.svelte` | `nav` sin etiqueta accesible | 1 |
| `main.ts` | no comprueba `#app` | 1 |

Hubo dos mutaciones mal planteadas que se descartaron:

- `fs:default` en capabilities: ya lo rechaza la compilación (`tauri-build`), así que se sustituyó por `core:path:default`.
- `.setup()` antes del plugin: no viola la regla, porque `setup` no es un plugin. Se sustituyó por registrar otro plugin delante.

## Especificación de pruebas

| # | Qué se garantiza | Prueba | Tipo | Resultado |
|---|---|---|---|---|
| 1 | La app encuentra `recursos/` subiendo desde cualquier punto de partida, en orden, y nunca acepta un archivo con ese nombre | `nucleo/tests/recursos.rs:locate_from_*` | unitaria | PASS |
| 2 | `ANONIMIZADOR_RECURSOS` tiene prioridad sobre la búsqueda | `nucleo/tests/recursos_env.rs` | unitaria | PASS |
| 3 | `check_files` señala por su ruta cada archivo que falte, y no acepta carpetas | `nucleo/tests/recursos.rs:check_files_*` | unitaria | PASS |
| 4 | Rutas y nombres de bibliotecas según la plataforma | `nucleo/tests/recursos.rs` | unitaria | PASS |
| 5 | Mensajes de error en español, en una línea, con la ruta o la causa | `nucleo/src/error.rs` (tests) | unitaria | PASS |
| 6 | CLI: `version`, uso con código 2, error claro con código 1 | `nucleo/tests/cli.rs` | integración (binario real) | PASS |
| 7 | El lock descarga lo que la app busca, con SHA-256 válidos, revisión fija y HTTPS | `nucleo/tests/contrato_lock.rs` | contrato | PASS |
| 8 | ONNX Runtime carga el modelo (`input_ids`, `attention_mask` → `logits`) y PDFium abre un PDF | `nucleo/tests/humo_recursos.rs` | integración nativa | PASS (`--include-ignored`) |
| 9 | Una biblioteca nativa inválida da `ModelLoad` y no un cierre inesperado | `nucleo/tests/humo_errores.rs` | integración nativa | PASS |
| 10 | CSP sin orígenes externos ni `unsafe-*`; `connect-src` limitado a IPC | `app/src-tauri/tests/configuracion.rs` | configuración | PASS |
| 11 | Actualizador desactivado, prototipo congelado, identificador aprobado | `configuracion.rs` | configuración | PASS |
| 12 | Solo los permisos autorizados, en la ventana `main`, y un único archivo de capabilities | `configuracion.rs` | configuración | PASS |
| 13 | single-instance es el primer plugin y enfoca `main` | `configuracion.rs` | configuración | PASS |
| 14 | Las descargas solo se guardan si el SHA-256 coincide; nunca dejan temporales ni pisan la versión anterior | `scripts/test/recursos.test.mjs:descargar` | integración (HTTP local) | PASS |
| 15 | Extracción de `.tgz` y `.zip` con el tar del sistema; copia con hash | `recursos.test.mjs:extraer y copiarArchivos` | integración | PASS |
| 16 | Un binario alterado, ausente o de otra versión se vuelve a descargar | `recursos.test.mjs:paqueteAlDia` | unitaria | PASS |
| 17 | Ningún cliente HTTP ni TLS en el binario para Windows x64, macOS arm64 y macOS x64; `nucleo` sin Tauri | `scripts/test/dependencias.test.mjs` | seguridad | PASS |
| 18 | Pestañas visibles y accesibles; cambio de pestaña y de contenido | `app/src/App.test.ts` | componente | PASS |
| 19 | Montaje en `#app` y error claro si falta | `app/src/main.test.ts` | unitaria | PASS |

## Validación final

```
cargo fmt --all -- --check                               OK
cargo clippy --workspace --all-targets -- -D warnings    OK
cargo test --workspace -- --include-ignored              44 passed, 0 failed
node --test "scripts/test/*.test.mjs"                    23 pass, 0 fail
npm test (app)                                           7 passed
npm run check (app)                                      0 ERRORS 0 WARNINGS
```

## Cobertura y huecos conocidos

| Ámbito | Líneas | Funciones | Ramas |
|---|---|---|---|
| `nucleo` (Rust, `cargo llvm-cov`) | cli.rs 100 % · error.rs 100 % · recursos.rs 92,6 % | 100 % · 100 % · 89,3 % | — |
| Workspace Rust total | 88,1 % | 82,9 % | — |
| `scripts/lib/recursos.mjs` | 100 % | 100 % | 91,4 % |
| Interfaz (`App.svelte`, `main.ts`) | 100 % | 100 % | 100 % |

Huecos intencionados:

- **`app/src-tauri/src/lib.rs` y `main.rs` (0 %)**: `run()` arranca el bucle de eventos de la ventana y no se puede ejecutar en una prueba unitaria. Lo cubren la prueba de configuración (orden de plugins y ventana `main`) y la prueba manual: una sola instancia comprobada con dos lanzamientos, y la ventana validada visualmente por Jano.
- **Ramas de macOS en `recursos.rs`**: no se compilan en Windows. Se cubrirán cuando se pruebe en Mac (hito 6).
- **Fallo de carga de PDFium**: `PdfiumLoad` no se ejecuta. Provocarlo exige que ONNX Runtime cargue bien en el mismo proceso con una biblioteca de PDFium rota, y ambas se inicializan una vez por proceso. El mensaje de `PdfiumLoad` sí está probado en `error.rs`.
- **`scripts/descargar-*.mjs`**: son la orquestación de la línea de comandos sobre la biblioteca, que está al 100 %. Se validaron ejecutándolos de verdad: primera descarga, segunda sin descargar, y re-descarga tras alterar `config.json` y `pdfium.dll`.

## Hallazgo durante las pruebas

`reqwest` y `hyper` aparecen en `Cargo.lock`. Son dependencias de `tauri` solo para Android e iOS y no entran en el binario de escritorio. Por eso la garantía "sin red" se comprueba por plataforma con `cargo tree --target`. Registrado en `DECISIONES.md`.

## Resumen para un squash

RED: `b339599` (locate_from) y `4265f8a` (exportaciones de scripts), más 36 mutaciones detectadas en código existente. GREEN: `ff45479` y `2c3db36`. Caracterización: `e4ed377`, `0f898b9`, `229d05a`, `31693e1`. Resultado final: 74 pruebas en verde (44 de Rust, 23 de Node y 7 de Vitest).
