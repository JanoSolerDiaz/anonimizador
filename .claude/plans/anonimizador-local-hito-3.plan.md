# Plan: Anonimizador local — Hito 3 (DOCX y almacén)

**Source PRD**: `specs/anonimizador-local.prd.md`
**Selected Milestone**: Hito 3 — DOCX y almacén
**Complexity**: Large

## Summary

Al terminar este hito se podrá anonimizar y desanonimizar de principio a fin desde la terminal. `cli anonimizar` recibe DOCX, MD o TXT, sustituye los datos personales por marcadores y guarda la correspondencia en un almacén cifrado. `cli desanonimizar` recibe DOCX, MD, TXT o texto pegado y restaura los datos desde ese almacén. El DOCX anonimizado es el archivo original con el texto cambiado dentro de su XML, así que conserva todo el formato. La salida puede ser también MD o TXT.

Piezas nuevas en `nucleo`:

- **Almacén** (`almacen/`): un archivo cifrado con AES-256-GCM, con la clave en el almacén de credenciales del sistema. Tiene caducidad de 90 días, y permite exportar e importar el diccionario de un documento, olvidar un documento y vaciarlo todo.
- **Formatos** (`formatos/`): extractor y escritor de DOCX con mapa de fragmentos, y de MD y TXT.
- **Operaciones** (`operaciones/`): anonimizar y desanonimizar un archivo uniendo formato, motor y almacén.

El registro de marcadores del hito 2 (`MarkerRegistry`, que vivía en memoria) pasa a guardarse en el almacén.

## Datos de partida (comprobados el 2026-10-28)

Versiones actuales en crates.io, que la Task 1 verificará con su documentación:

| Crate | Versión | Para qué |
|---|---|---|
| `zip` | 9.1.3 | Leer y escribir el ZIP del DOCX. Sin funciones por defecto, solo `deflate` |
| `quick-xml` | 0.42.0 | Leer y reescribir el XML de cada parte |
| `aes-gcm` | 0.11.1 | Cifrado autenticado del almacén |
| `getrandom` | 0.4.3 | Clave de 256 bits y nonce de cada escritura |
| `keyring` | 4.2.0 | Clave en el Administrador de credenciales (Windows) y el Llavero (Mac). La versión 4 cambió de API: hay que leer su documentación |
| `sha2` | 0.11.1 | Huella del original y de la salida |
| `uuid` | 1.29.0 | Identificador de cada documento (v4) |
| `jiff` | 0.2.38 | Fechas de creación y caducidad con la zona horaria del sistema |

`docx-rs`, que el PRD cita para generar DOCX desde PDF, no hace falta hasta el hito 4.

Un DOCX protegido con contraseña no es un ZIP: es un archivo OLE compuesto, que empieza por los bytes `D0 CF 11 E0`. Así se detecta y se rechaza con un error claro.

## Patterns to Mirror

| Category | Source | Pattern |
|---|---|---|
| Naming | `nucleo/src/motor/mod.rs`, `nucleo/src/recursos.rs` | Tipos e identificadores en inglés (`Engine`, `MarkerRegistry`); módulos de dominio en español (`almacen`, `formatos`); comentarios y mensajes en español |
| Errors | `nucleo/src/error.rs:9` | Se amplía el enum `Error` con `thiserror`. Mensajes de una frase, sin texto del documento ni valores del almacén. Errores de bibliotecas con `map_err(\|e\| Error::X(e.to_string()))` |
| Logging | `nucleo/src/motor/mod.rs` (`log_detection`), `nucleo/tests/logs.rs` | Eventos `tracing` con recuentos, nombres de archivo y tiempos. La prueba de logs se amplía a anonimizar y desanonimizar |
| Data access | `nucleo/src/recursos.rs` (`locate_from`), `nucleo/src/evaluacion.rs` (`load_corpus`) | Funciones que reciben rutas; nada de estado global. El almacén recibe su ruta y su proveedor de clave |
| Tests | `nucleo/tests/*.rs`, `nucleo/tests/cli.rs` | Integración en `nucleo/tests/`, `tempfile` para carpetas, las que necesitan el modelo con `#[ignore = "…"]`, CLI con `CARGO_BIN_EXE_cli` |
| CLI | `nucleo/src/bin/cli.rs` (`EntitiesArgs`, `parse_engine_option`) | Subcomandos con `std::env::args`, `ArgsError::{Usage, Invalid}`, códigos 0/1/2, opciones del motor compartidas |
| Contrato con scripts | `nucleo/tests/contrato_lock.rs` | El identificador del modelo que se guarda en cada documento sale de `scripts/recursos.lock.json`, y una prueba lo comprueba |

## Diseño

```
             anonimizar                                 desanonimizar
  DOCX/MD/TXT ──► extractor ──► segmentos         DOCX/MD/TXT/texto ──► extractor ──► segmentos
                     │              │                                       │
                     │              ▼                                       ▼
                     │        Engine::detect                        búsqueda tolerante
                     │              │                               de marcadores
                     │              ▼                                       │
                     │     Store::assign (marcador)                         ▼
                     │              │                          Store: propias + importadas
                     ▼              ▼                                       │
                  mapa ──► sustituciones {segmento, inicio, fin, texto} ◄───┘
                                    │
                                    ▼
                 escritor (DOCX sobre el XML original · MD · TXT)
                                    │
                                    ▼
              salida atómica  +  registro del documento en el almacén
```

- **Sustituciones comunes.** Anonimizar y desanonimizar producen lo mismo: una lista de `Replacement {segment, start, end, text}`. Los escritores solo saben aplicar sustituciones, de la última a la primera dentro de cada segmento.
- **Valores fuera del texto visible.** Los destinos de los enlaces (`mailto:` en `word/_rels/*.rels`) y los códigos de campo (`w:instrText`, por ejemplo `HYPERLINK "mailto:…"`) no se mandan al modelo. Se sustituyen ahí, de forma literal, los valores ya detectados en el texto del documento.
- **Clave del almacén.** El almacén recibe un proveedor de clave (`KeyProvider`). En la aplicación es el del sistema (`keyring`); en las pruebas, uno en memoria. Así las pruebas no tocan el almacén de credenciales real.
- **Formato del archivo del almacén.** Una cabecera fija (`ANONALM` y la versión), un nonce aleatorio de 12 bytes y el JSON cifrado. La cabecera va como dato asociado del cifrado, así que cambiarla también se detecta.

## Files to Change

| File | Action | Why |
|---|---|---|
| `nucleo/Cargo.toml` | UPDATE | `zip`, `quick-xml`, `aes-gcm`, `getrandom`, `keyring` (backends nativos de Windows y Mac), `sha2`, `uuid`, `jiff` |
| `nucleo/src/error.rs` | UPDATE | `DocxProtected`, `DocxInvalid`, `UnsupportedFormat`, `StoreKeyMissing`, `StoreUnreadable`, `StoreInvalid`, `DictionaryInvalid`, `MarkerConflict`… |
| `nucleo/src/almacen/mod.rs` | UPDATE | `Store`: contadores, entradas, documentos, importados; `assign`, `lookup`, `record_document` |
| `nucleo/src/almacen/cifrado.rs` | CREATE | Cabecera, AES-256-GCM, `KeyProvider`, `MemoryKey`, `SystemKeyring` |
| `nucleo/src/almacen/archivo.rs` | CREATE | Carga, escritura atómica con `.bak`, errores sin borrar el archivo antiguo |
| `nucleo/src/almacen/caducidad.rs` | CREATE | Purga de documentos caducados y entradas sin uso; olvidar; vaciar |
| `nucleo/src/almacen/diccionario.rs` | CREATE | Exportar e importar el diccionario de un documento |
| `nucleo/src/motor/marcadores.rs` | UPDATE | `normalize` y `format_marker` se quedan; el registro pasa al almacén |
| `nucleo/src/formatos/mod.rs` | UPDATE | `Replacement`, `Extracted` (segmentos + mapa + avisos), detección de formato por contenido |
| `nucleo/src/formatos/salida.rs` | CREATE | Nombres `descripción_sufijo.ext`, sufijos `_2`, `_3`, escritura atómica |
| `nucleo/src/formatos/texto.rs` | UPDATE | Extractor y escritor de MD y TXT |
| `nucleo/src/formatos/docx/` | CREATE | `extractor.rs`, `mapa.rs`, `escritor.rs`, `metadatos.rs`, `avisos.rs`, `texto.rs` (MD y TXT desde DOCX) |
| `nucleo/src/operaciones/` | CREATE | `anonimizar.rs`, `desanonimizar.rs`, `marcadores.rs` (búsqueda tolerante), resúmenes |
| `nucleo/src/bin/cli.rs` | UPDATE | `cli anonimizar`, `cli desanonimizar`, `cli almacen` |
| `nucleo/examples/generar_corpus_docx.rs` | CREATE | Genera los DOCX sintéticos del corpus, para que se puedan rehacer |
| `tests/corpus/docx/*.docx` + `*.json` | CREATE | DOCX sintéticos con sus entidades esperadas |
| `nucleo/tests/almacen_*.rs`, `docx_*.rs`, `texto_formato.rs`, `anonimizar.rs`, `desanonimizar.rs` | CREATE | Pruebas por pieza y de ida y vuelta |
| `nucleo/tests/logs.rs`, `nucleo/tests/cli.rs` | UPDATE | Logs sin datos en las operaciones; órdenes nuevas |
| `README.md`, `DECISIONES.md` | UPDATE | Órdenes nuevas y decisiones del hito |

## Tasks

Cada tarea con RED y GREEN en commits, y repaso con fallos provocados al final, como en el hito 2.

### Task 0: Contexto entre párrafos (se hace primero)
- **Action**: El motor deja de analizar cada segmento aislado. Los segmentos de una misma llamada a `detect` se leen dos veces: cada uno solo, como hasta ahora, y encadenados en orden en una sola secuencia de tokens, troceada en ventanas de 256 con solape de 32. Lo que encuentren las dos lecturas se junta (`fusion::merge_passes`). Las probabilidades de cada token se devuelven a su segmento y la decodificación sigue siendo por segmento: una entidad nunca cruza de un párrafo a otro y las posiciones siguen siendo relativas al párrafo. La regla de tarjetas sigue aplicándose por segmento. `EngineOptions` gana `context: ContextMode::{Combined, Document, Paragraph}` (por defecto `Combined`), y `entidades` y `evaluar` ganan `--contexto ambos|documento|parrafo` para poder comparar. Hecha: Jano eligió la lectura doble y aceptó superar los 5 s del PRD (ver `DECISIONES.md`).
- **Mirror**: `Engine::detect_timed` y `window_logits` en `nucleo/src/motor/mod.rs`; pruebas de `nucleo/tests/motor.rs`.
- **Validate**: con el modelo real, un párrafo corto que solo se detecta con el contexto de los vecinos; 40 segmentos que suman más de 1.500 tokens dan cada entidad en su segmento y con sus posiciones; `--contexto parrafo` reproduce el comportamiento anterior. `cli evaluar` con las dos opciones, y el informe de detección actualizado con las cifras reales.

### Task 1: Dependencias y comprobación de APIs
- **Action**: Añadir los crates de la tabla con las funciones mínimas (`zip` sin funciones por defecto y con `deflate`; `keyring` solo con los backends nativos de Windows y Mac). Leer en docs.rs la API actual de `keyring` 4, `zip` 9, `quick-xml` 0.42 y `aes-gcm` 0.11, y dejar una nota corta en el plan con lo comprobado.
- **Mirror**: `nucleo/Cargo.toml` (comentarios que explican las funciones desactivadas).
- **Validate**: `node --test scripts/test/dependencias.test.mjs` sigue en verde (sin red); `cargo tree -p nucleo` sin crates de red.

### Task 2: Nombres de salida y escritura atómica
- **Action**: `descripción` a partir del nombre original (minúsculas, sin acentos, espacios a guiones, sin caracteres raros); `descripción_sufijo.ext`; `_2`, `_3` si existe. Escritura en un temporal de la misma carpeta y renombrado; si falla, no queda nada a medias.
- **Validate**: pruebas de nombres («Contrato Cliente.docx» → `contrato-cliente_anonimizado.docx`), de colisiones y de que un error de escritura no deja archivos.

### Task 3: Estructura del almacén
- **Action**: `Store` con el JSON del PRD (`formato`, `version_formato`, `contadores`, `entradas`, `documentos`, `importados`). `assign(tipo, etiqueta, literal, documento)` reutiliza el comportamiento de `MarkerRegistry` (la clave es solo el valor normalizado). Las pruebas de marcadores del hito 2 pasan a usar el almacén.
- **Validate**: ida y vuelta del JSON; formato o versión desconocidos dan error; las pruebas de marcadores y de marcadores entre documentos siguen en verde.

### Task 4: Cifrado y archivo del almacén
- **Action**: Cabecera + nonce + JSON cifrado con AES-256-GCM. `KeyProvider` con `MemoryKey` (pruebas) y `SystemKeyring` (`keyring`, servicio `anonimizador`). La clave se genera en el primer uso. Escritura atómica con copia `.bak` hasta que la nueva termina. Si falta la clave o no descifra: error claro, y el archivo antiguo no se toca.
- **Validate**: el archivo en disco no contiene ningún valor legible; no se descifra con otra clave; un byte cambiado da error; si la escritura se interrumpe (temporal sin renombrar), el almacén anterior se sigue cargando. La prueba de `SystemKeyring` es `#[ignore]` porque toca el almacén de credenciales real.

### Task 5: Caducidad, olvidar y vaciar
- **Action**: Cada documento caduca a los N días (90 por defecto, configurable al crear el almacén). La política de sistemas (registro de Windows, perfil de Mac) se lee en el hito 6 (decisión de Jano del 2026-10-28). `purge(hoy)` borra los documentos caducados y las entradas que ya no usa ningún documento. `forget(documento)` borra sus entradas que no usen otros. `clear()` lo vacía todo.
- **Validate**: pruebas con fechas fijas, sin depender del reloj.

### Task 6: Exportar e importar diccionarios
- **Action**: `export(documento)` genera el JSON con las entradas de ese documento (formato `anonimizador-diccionario`). `import(json, nombre)` lo guarda en `importados`, aparte de las entradas propias y sin tocar los contadores.
- **Validate**: exportar e importar en otro almacén restaura los marcadores de ese documento; un diccionario mal formado da error; importar no cambia los contadores.

### Task 7: Formato MD y TXT
- **Action**: Extractor (párrafos con su posición, como `split_paragraphs`) y escritor que aplica sustituciones y conserva todo lo demás, incluidos los finales de línea `\r\n`. `Replacement` y su aplicación de la última a la primera viven en `formatos/mod.rs`.
- **Validate**: aplicar sustituciones a un texto con acentos y `\r\n`; sustituciones solapadas dan error; sin sustituciones, la salida es idéntica byte a byte.

### Task 8: Corpus DOCX sintético
- **Action**: `nucleo/examples/generar_corpus_docx.rs` crea DOCX mínimos pero válidos, cada uno con un JSON de entidades esperadas: nombres partidos en varios fragmentos con formato distinto, tablas, cuadros de texto, encabezados y pies, notas al pie y al final, comentarios, control de cambios (texto insertado y borrado), enlace `mailto:`, código de campo, metadatos de autor, una imagen, un objeto incrustado, títulos y listas para MD, y un DOCX protegido (archivo OLE). En castellano, catalán, inglés y francés.
- **Validate**: el generador es determinista (dos ejecuciones dan los mismos bytes); cada XML del corpus se puede volver a leer. **Revisión manual**: Jano abre los DOCX en Word y confirma que no pide reparar nada.

### Task 9: Extractor DOCX con mapa
- **Action**: Recorrer las partes del PRD. Por cada `w:p`, concatenar `w:t` y `w:delText` en orden, incluidos los de `w:hyperlink`, `w:ins`, `w:sdt` y `w:smartTag`. Los párrafos dentro de `w:txbxContent` son segmentos propios. `w:tab` y `w:br` cuentan como un espacio sin entrada editable en el mapa. Para el MD, guardar el estilo de cada párrafo (título y nivel, lista) y su celda de tabla. Recoger destinos de enlaces externos, `w:instrText` y metadatos (los de autor para vaciarlos; título, asunto, palabras clave y descripción como segmentos). Avisos de imágenes y objetos incrustados. Protegido → `Error::DocxProtected`.
- **Validate**: con el corpus de la Task 8, cada segmento es el esperado y cada carácter del mapa apunta al fragmento correcto; los avisos cuentan las imágenes y los objetos.

### Task 10: Escritor DOCX
- **Action**: Aplicar sustituciones con el mapa: el marcador va en el primer fragmento afectado y la parte afectada de los siguientes se borra; `xml:space="preserve"` en los `w:t` que empiecen o acaben en espacio. Vaciar `dc:creator`, `cp:lastModifiedBy` y `Company`. El título, el asunto, las palabras clave y la descripción (`dc:title`, `dc:subject`, `cp:keywords`, `dc:description`) se pasan por el motor como segmentos y se sustituye lo que detecte (decisión de Jano del 2026-10-28). Sustitución literal en destinos de enlaces y `w:instrText`. Solo se reescriben las partes cambiadas; el resto de entradas del ZIP se copian tal cual.
- **Validate**: el XML de salida se vuelve a leer sin errores; sin sustituciones, todas las partes son idénticas a las originales salvo los metadatos; un marcador sobre un nombre partido en tres fragmentos hereda el formato del primero; los metadatos de autor quedan vacíos.

### Task 11: MD y TXT desde DOCX
- **Action**: Generar desde el texto extraído ya sustituido. MD con títulos (`#`), listas (`-` y `1.`) y tablas básicas; TXT con un párrafo por línea.
- **Validate**: el DOCX del corpus con títulos, lista y tabla da el MD esperado.

### Task 12: Anonimizar un archivo
- **Action**: `anonymize(entrada, carpeta, formato, &Engine, &mut Store, opciones) -> Summary`. Extraer → detectar → asignar marcadores en el almacén → sustituir → escribir la salida de forma atómica → registrar el documento: id, fechas, versión de la app, modelo (`repositorio@revisión` leído del lock al compilar), nombre y SHA-256 del origen y de la salida, marcadores. El resumen da las entidades por tipo y los avisos. Formatos válidos: DOCX → DOCX, MD o TXT; MD y TXT → MD o TXT.
- **Validate**: con el modelo real (`#[ignore]`), anonimizar el corpus DOCX no deja ningún valor esperado en el texto de salida; el original no cambia (misma huella); el almacén tiene el documento y sus entradas.

### Task 13: Desanonimizar
- **Action**: Búsqueda tolerante de marcadores, escrita a mano sin `regex`: mayúsculas o minúsculas y espacios dentro de los corchetes (`[persona_0042]`, `[ NOMBRE_PERSONA_0042 ]`). En DOCX se busca sobre el texto de cada párrafo, así que un marcador partido en varios fragmentos también se encuentra. Cada marcador se busca en las entradas propias y en los diccionarios importados. Si tiene valores distintos en dos fuentes, se devuelve un conflicto para que el usuario elija la fuente; en la CLI, con `--fuente`. Entradas: DOCX, MD, TXT y texto pegado. El resumen da los marcadores restaurados y los que no están en ninguna fuente.
- **Validate**: un texto con marcadores de tres documentos se restaura entero; las formas tolerantes se restauran; un marcador partido en dos fragmentos de un DOCX se restaura; un marcador con dos valores en fuentes distintas da el conflicto. **Ida y vuelta**: anonimizar y desanonimizar el corpus DOCX devuelve exactamente el texto original en todas las partes procesadas.

### Task 14: CLI
- **Action**:
  - `cli anonimizar <archivos…> --salida <carpeta> [--formato docx|md|txt] [--almacen <archivo>] [--no-sustituir …]` más las opciones del motor.
  - `cli desanonimizar <archivos…> --salida <carpeta>` o `cli desanonimizar --texto "…"`, con `[--fuente propio|<nombre>]`.
  - `cli almacen listar | exportar <documento> | importar <archivo> | olvidar <documento> | vaciar | purgar`.
  - Un error en un archivo no detiene el resto del lote. La clave sale del almacén de credenciales del sistema. Para que las pruebas no lo toquen, las compilaciones de depuración aceptan una variable de entorno con una clave de prueba; las de `--release` la ignoran.
- **Validate**: pruebas de la CLI como las del hito 2; un lote con un archivo corrupto y otro protegido procesa los demás y explica los dos errores.

### Task 15: Pruebas de aceptación, rendimiento y logs
- **Action**: Repasar las pruebas automáticas del PRD que tocan a este hito y medir un DOCX de unas 10 páginas frente al objetivo de 5 s. La prueba de logs se amplía a anonimizar y desanonimizar: ningún evento contiene texto ni valores del almacén.
- **Validate**: validación completa; tiempo del DOCX de 10 páginas anotado en el informe.

### Task 16: Documentación
- **Action**: README con las órdenes nuevas y cómo funciona el almacén; DECISIONES con lo que cambie respecto al PRD; cierre del plan.
- **Validate**: validación completa.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --include-ignored
node --test "scripts/test/*.test.mjs"
cargo run --release -p nucleo --bin cli -- anonimizar tests/corpus/docx/es-01-contrato.docx --salida <carpeta temporal>
cargo run --release -p nucleo --bin cli -- desanonimizar <carpeta temporal>/es-01-contrato_anonimizado.docx --salida <otra carpeta>
cargo llvm-cov -p nucleo --summary-only -- --include-ignored   # objetivo ≥ 80 % en almacen/, formatos/ y operaciones/
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Word pide reparar un DOCX de salida por algún detalle del XML que las pruebas no ven | Media | Solo se reescriben las partes cambiadas y con los mínimos cambios; el XML se vuelve a leer en las pruebas; Jano abre el corpus de salida en Word (revisión manual de la Task 10) |
| Estructuras de Word que no están en el corpus (campos complejos, `w:sdt` anidados, `mc:AlternateContent` con texto duplicado) | Media | El extractor recorre todos los `w:t` y `w:delText` sin depender de dónde estén. El texto de `mc:Fallback` duplica al de `mc:Choice`: se procesan los dos para que no quede ningún dato en la copia de reserva |
| La API de `keyring` 4 no permite un uso sencillo o mete dependencias de red | Baja | Se comprueba en la Task 1 antes de escribir el almacén. El resto del almacén no depende de `keyring` gracias a `KeyProvider` |
| Un dato aparece solo en el destino de un enlace o en un código de campo, y no en el texto visible | Baja | Se sustituyen ahí los valores detectados en el texto. Si el dato no aparece en el texto, queda en el enlace: se anota como limitación |
| El almacén crece con muchos documentos y se reescribe entero en cada lote | Baja | El PRD lo acepta. Se mide con 1.000 documentos sintéticos en la Task 15 |
| Dos procesos (la app y la CLI) escriben el almacén a la vez | Baja | La app tendrá instancia única (hito 5). En la CLI se avisa en el README de no usarla con la app abierta |

## Preguntas resueltas (Jano, 2026-10-28)

Las tres se resolvieron como se proponía:

1. **Política de caducidad de sistemas.** El PRD dice que sistemas puede fijar otro plazo con una clave de registro en Windows o un perfil en Mac. En este hito el almacén acepta el plazo como parámetro; leer esa política se hace en el hito 6, junto con el empaquetado y el despliegue.
2. **Otros metadatos.** El PRD manda vaciar `dc:creator`, `cp:lastModifiedBy` y `Company`. El título, el asunto, las palabras clave y la descripción del documento también pueden llevar datos personales («Contrato de Marta Lozano»). Se pasan por el motor como un segmento más y se sustituye lo que detecte.
3. **Más contexto para el modelo.** Sigue pendiente de lo que se midió en el hito 2: agrupar párrafos seguidos en cada análisis subió del 84 % al 88 % los datos tapados con v1. Se hace ahora, como Task 0, antes que el resto del hito.

## Acceptance

- [ ] All tasks complete, cada una con RED y GREEN en commits
- [x] El motor analiza los párrafos con el contexto de sus vecinos y el informe de detección recoge la mejora medida
- [ ] Pruebas automáticas del PRD de este hito en verde: marcadores entre documentos, texto mezclado, DOCX ida y vuelta, XML válido, metadatos vacíos, contraseña, desanonimización tolerante, almacén ilegible en disco y con otra clave, caducidad, escritura interrumpida, conflicto al importar, nombres de salida y logs sin datos
- [ ] `cli anonimizar` y `cli desanonimizar` funcionan con DOCX, MD, TXT y texto pegado
- [ ] Jano abre en Word los DOCX anonimizados del corpus y no piden reparación
- [ ] DOCX de 10 páginas en menos de 5 s (modelo ya cargado)
- [ ] Sin crates con acceso a red (la prueba de dependencias sigue en verde)
- [ ] Cobertura ≥ 80 % en `almacen`, `formatos` y `operaciones`
- [ ] Patterns mirrored, not reinvented
