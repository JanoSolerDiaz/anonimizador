# Plan: Anonimizador local — Hito 2 (Motor)

**Source PRD**: `specs/anonimizador-local.prd.md`
**Selected Milestone**: Hito 2 — Motor
**Complexity**: Large

## Summary

Construir en `nucleo::motor` el motor que recibe segmentos de texto y devuelve entidades con su posición en caracteres. Incluye tokenización con offsets, troceado en ventanas de 512 tokens con solape de 128, inferencia ONNX por lotes, decodificación BIO, descarte de los tipos opcionales desmarcados y registro de marcadores en memoria (el almacén cifrado llega en el hito 3). La CLI gana `cli entidades archivo.txt`, que imprime JSON. Todo se implementa con TDD.

Hay dos versiones del modelo de IA y aún no sabemos cuál conviene: la v1, que es la que indica el PRD, y la v2, más reciente y en principio mejor. Jano decidió el 2026-10-26 probar las dos y elegir con datos. Por eso el código del motor no lleva escrito qué modelo usa: lo lee de la carpeta que se le indique. Qué modelo se descarga lo decide `scripts/recursos.lock.json`, el archivo que fija versiones y huellas SHA-256. Así, cambiar de modelo consiste en editar ese archivo y volver a ejecutar el script de descarga, sin tocar el código. En la Task 12 se miden los dos con el mismo corpus de prueba y Jano elige.

## Datos de partida (comprobados)

- **v1** (`bardsai/eu-pii-anonimization-multilang@62ca571`): XLM-R, 69 etiquetas (35 tipos con B-/I- + `O`), entradas `input_ids` y `attention_mask`, salida `logits`.
- **v2** (`bardsai/eu-pii-anonimization-multilang-v2@5e61416`): ModernBERT (mmBERT-base), 307M parámetros, 71 etiquetas (los mismos 35 tipos; añade `I-CONTACT_HANDLE` e `I-PERSON_ALIAS`), INT8 de 309 MB. F1 estricto en texto real: 0,968 (INT8) frente a 0,885 de v1. Publicada el 2026-10-19.
- **Cuidado con `tokenizer.json` de v1**: trae `truncation` activado a 512 y un normalizador que cambia varios espacios por `▁`. Hay que desactivar el truncado y probar que los offsets siguen bien alineados.
- **`ort` rc.15**: `Session::run(&mut self, …)`. Una sesión compartida necesita un `Mutex`. Tensores con `TensorRef::from_array_view((shape, &data))` y salida con `try_extract_tensor::<f32>() -> (&Shape, &[f32])`.
- **`tokenizers` 0.23.2**: con `default-features = false` y solo `onig`, sin `http` ni `progressbar`. `encode_char_offsets` devuelve offsets en caracteres.

## Patterns to Mirror

| Category | Source | Pattern |
|---|---|---|
| Naming | `nucleo/src/recursos.rs:43` | Tipos e identificadores en inglés (`Resources`, `check_files`); módulos de dominio en español (`motor`, `recursos`); comentarios en español |
| Errors | `nucleo/src/error.rs:9` | Se amplía el enum `Error` con `thiserror`. Mensajes de una frase en español y sin datos del documento. Errores de bibliotecas convertidos con `map_err(\|e\| Error::X(e.to_string()))` como en `recursos.rs:135` |
| Logging | (sin uso todavía) `tracing` | Solo recuentos por tipo y tiempos. Una prueba captura los eventos y comprueba que no contienen texto (Task 13) |
| Data access | `nucleo/src/recursos.rs:73` | Funciones puras que reciben rutas o puntos de partida (`locate_from`) para poder probarlas sin estado global |
| Tests | `nucleo/tests/recursos.rs:11`, `nucleo/tests/humo_recursos.rs:10` | Pruebas de integración en `nucleo/tests/`, unitarias en `#[cfg(test)]`, ayudantes con `tempfile`. Las que necesitan `recursos/` llevan `#[ignore = "…"]` y se ejecutan con `--include-ignored` |
| CLI | `nucleo/src/bin/cli.rs:11` | Subcomandos con `std::env::args` y `USAGE`; códigos de salida 0/1/2; pruebas con `CARGO_BIN_EXE_cli` (`tests/cli.rs`) |
| Contrato con scripts | `nucleo/tests/contrato_lock.rs:10` | Lo que descarga el lock y lo que busca Rust se comprueba en una prueba |

## Diseño

```
Segment{id, text} ─► words (segmentación propia, independiente del modelo)
                 ─► tokenizer.encode_char_offsets (sin truncado)
                 ─► windows (≤510 tokens + <s> </s>, solape 128)
                 ─► batches (relleno al más largo) ─► ONNX ─► logits
                 ─► por token: la ventana donde está más lejos del borde
                 ─► decoder (FirstSubtoken | WordMean) ─► entidades del modelo
tipos opcionales desmarcados: se descartan al final
MarkerRegistry: clave normalizada → [TIPO_NNNN] (en memoria; el almacén lo persiste en el hito 3)
```

API pública (nombres orientativos):

- `Engine::load(model: &ModelPaths, options: EngineOptions) -> Result<Engine>`
- `Engine::detect(&self, segments: &[Segment]) -> Result<Vec<Entity>>`
- `Entity { segment, start, end, kind, label, text, score }`, con `start` y `end` en **caracteres**, como pide el PRD.
- `MarkerRegistry::marker_for(&mut self, kind, value) -> Marker`

**Decisión sobre la clave del marcador:** la clave es solo el valor normalizado, como dice el PRD ("se busca el valor normalizado"). Si un mismo valor aparece después con otro tipo, se reutiliza su marcador para que la restauración no sea ambigua. Se registrará en `DECISIONES.md`.

## Tabla de tipos (nombres aprobados por Jano el 2026-10-26)

Va en `nucleo/config/tipos.json`, versionada y cargada con `include_str!`. Una prueba exige que cubra todas las etiquetas de los `config.json` de los modelos del lock, sin etiquetas inventadas. Todos los tipos vienen del modelo: no hay reglas deterministas (ver «Sin reglas deterministas» más abajo).

| Etiqueta del modelo | Marcador | | Etiqueta del modelo | Marcador |
|---|---|---|---|---|
| PERSON_NAME | `NOMBRE_PERSONA` | | PAYMENT_CARD | `TARJETA_BANCARIA` |
| PERSON_ALIAS | `ALIAS_PERSONA` | | PAYMENT_CARD_SECURITY | `CVV_TARJETA` |
| PERSON_IDENTIFIER | `ID_DOCUMENTO_PERSONA` | | BANK_ACCOUNT_IDENTIFIER | `CUENTA_BANCARIA` |
| PERSON_ATTRIBUTE | `CARACTERISTICA_PERSONA` | | FINANCIAL_AMOUNT | `IMPORTE` |
| PERSON_ROLE_OR_TITLE | `CARGO` | | ACCOUNT_IDENTIFIER | `USUARIO` |
| DATE_OF_BIRTH | `FECHA_NACIMIENTO` | | AUTH_SECRET | `CLAVE_SECRETO` |
| PROPER_NAME | `NOMBRE_PROPIO` | | DEVICE_IDENTIFIER | `DISPOSITIVO` |
| EMAIL_ADDRESS | `EMAIL` | | IP_ADDRESS | `IP` |
| PHONE_NUMBER | `TELEFONO` | | IDENTIFYING_LINK | `ENLACE` |
| CONTACT_HANDLE | `CONTACTO` | | VEHICLE_IDENTIFIER | `VEHICULO` |
| POSTAL_ADDRESS | `DIRECCION_POSTAL` | | HEALTH_DATA | `DATOS_SALUD` |
| LOCATION | `LUGAR` | | BIOMETRIC_DATA | `DATOS_BIOMETRIA` |
| GEO_LOCATION | `COORDENADAS` | | ETHNIC_ORIGIN | `ORIGEN_ETNICO` |
| DOCUMENT_IDENTIFIER | `IDENTIFICADOR_DOCUMENTO` | | POLITICAL_OPINION | `IDEOLOGIA` |
| DOCUMENT_REFERENCE | `REFERENCIA` | | RELIGION_OR_BELIEF | `RELIGION` |
| ORGANIZATION_NAME | `ORGANIZACION` | | SEXUAL_ORIENTATION | `ORIENTACION_SEXUAL` |
| ORGANIZATION_IDENTIFIER | `CIF` | | TRADE_UNION_MEMBERSHIP | `SINDICATO` |
| CRIMINAL_OFFENCE_DATA | `ANTECEDENTES` | | | |


Tipos que el usuario puede elegir (decisión de Jano del 2026-10-26): `CARGO`, `IMPORTE`, `LUGAR` y `COORDENADAS`. En el resto de tipos la sustitución es siempre obligatoria. Para estos cuatro, en el momento de anonimizar el usuario puede indicar si quiere sustituirlos o no, y por defecto todos están marcados para sustituirse. En la tabla esto se representa con un campo `opcional: true` en esos cuatro tipos. El motor recibe en sus opciones la lista de tipos opcionales que el usuario ha desmarcado y descarta esas entidades al final, al final. Un tipo que no sea opcional nunca se puede desactivar, aunque alguien lo pida: el motor devuelve un error. En este hito la elección se hace con una opción de la CLI; las casillas de la interfaz llegan en el hito 5. Es un cambio respecto al PRD y se registrará en `DECISIONES.md`.

## Sin reglas deterministas (decisión de Jano del 2026-10-26)

El PRD pedía reglas fijas para DNI, NIE, IBAN, tarjeta, email y teléfono, que se ejecutaran junto al modelo y ganaran cuando coincidieran con él. Jano decidió no hacerlas. Todo lo detecta el modelo. Por eso desaparecen la antigua Task 4, los tipos `DNI`, `NIE` e `IBAN` de la tabla, el campo `origen` de las entidades (siempre sería «modelo»), la parte de la fusión en la que ganaba la regla y la dependencia `regex`. Está registrado en `DECISIONES.md`.

## Files to Change

| File | Action | Why |
|---|---|---|
| `nucleo/Cargo.toml` | UPDATE | `tokenizers` (solo `onig`), `unicode-normalization`, `serde` y `serde_json` como dependencias normales |
| `scripts/recursos.lock.json` | UPDATE | `modelo` pasa a ser una lista `modelos` con v1 (`recursos/modelo`) y v2 (`recursos/modelo-v2`) |
| `scripts/descargar-modelo.mjs`, `scripts/test/recursos.test.mjs` | UPDATE | Descargar todos los modelos del lock, o uno con `--modelo` |
| `nucleo/src/recursos.rs` | UPDATE | `ModelPaths` (modelo, tokenizador y config de una carpeta); `Resources::model(nombre)` |
| `nucleo/tests/contrato_lock.rs`, `nucleo/tests/recursos.rs` | UPDATE | El contrato pasa a cubrir los dos modelos |
| `nucleo/config/tipos.json` | CREATE | Tabla etiqueta → marcador |
| `nucleo/src/motor/mod.rs` | UPDATE | `Engine`, `Segment`, `Entity`, `EngineOptions` |
| `nucleo/src/motor/tipos.rs` | CREATE | Carga y validación de la tabla de tipos frente a `id2label` |
| `nucleo/src/motor/texto.rs` | CREATE | Segmentación en palabras con offsets en caracteres; normalización de valores |
| `nucleo/src/motor/tokenizacion.rs` | CREATE | Envoltorio de `tokenizers` sin truncado; tokens con offsets en caracteres |
| `nucleo/src/motor/troceado.rs` | CREATE | Plan de ventanas, solape y elección de la predicción más alejada del borde |
| `nucleo/src/motor/inferencia.rs` | CREATE | Sesión ONNX (`Mutex`, hilos = núcleos/2), lotes con relleno, softmax |
| `nucleo/src/motor/decodificacion.rs` | CREATE | BIO, subtokens → palabras (`FirstSubtoken`, `WordMean`), umbral y recorte de bordes |
| `nucleo/src/motor/opcionales.rs` | CREATE | Tipos opcionales desmarcados por el usuario y su descarte |
| `nucleo/src/motor/marcadores.rs` | CREATE | `MarkerRegistry`: contadores por tipo, `[TIPO_NNNN]`, variantes literales |
| `nucleo/src/error.rs` | UPDATE | `Tokenizer`, `Inference`, `InvalidTypeTable` y otras variantes que hagan falta |
| `nucleo/src/bin/cli.rs` | UPDATE | `cli entidades <archivo> [--modelo <carpeta>] [--decodificacion primer\|media]` y `cli evaluar <corpus> …` |
| `nucleo/tests/motor_*.rs` | CREATE | Integración con el modelo real (ignoradas por defecto) |
| `tests/corpus/texto/*.txt` + `*.json` | CREATE | Corpus sintético en es, ca, en y fr con entidades esperadas |
| `tests/informe-deteccion.md` | CREATE | Precisión y exhaustividad por tipo, por modelo y por decodificador, más tiempos |
| `DECISIONES.md`, `README.md` | UPDATE | Elección del modelo, clave del marcador, decodificador y comandos |

## Tasks

Cada tarea sigue el ciclo RED → GREEN → refactor, con un commit por etapa en la rama `hito-2/motor`.

### Task 1: Dependencias y lock con dos modelos
- **Action**: Añadir los crates sin funciones de red. Convertir el lock a `modelos` con v1 y v2 (calcular los hashes que no van por LFS y revisarlos a mano). Añadir `ModelPaths` y `Resources::model`. Descargar la v2 y ampliar la prueba de humo para que cargue **los dos** modelos y compruebe las entradas y salidas de cada uno.
- **Mirror**: `contrato_lock.rs`, `humo_recursos.rs`, `paqueteAlDia`.
- **Validate**: `cargo test -p nucleo --test contrato_lock --test humo_recursos -- --include-ignored`; `node --test "scripts/test/*.test.mjs"` (la prueba "sin red" sigue en verde con `tokenizers`).

### Task 2: Tabla de tipos
- **Action**: Crear `tipos.json` y su cargador. Pruebas: cubre todas las etiquetas de los `config.json` de v1 y v2; ninguna clave que no esté en el modelo; marcadores únicos, en mayúsculas ASCII con `_`.
- **Validate**: `cargo test -p nucleo tipos -- --include-ignored`

### Task 3: Normalización y marcadores
- **Action**: `normalize`: minúsculas, sin acentos (NFD sin marcas combinantes), espacios colapsados y recortados. `MarkerRegistry`: numeración por tipo con 4 dígitos como mínimo (`[NOMBRE_PERSONA_0042]`, `[NOMBRE_PERSONA_10000]`), mismo valor normalizado → mismo marcador, registro de variantes literales, clave independiente del tipo.
- **Validate**: pruebas del PRD: "JUAN PÉREZ" y "Juan Perez" comparten marcador; el mismo valor en dos documentos (dos llamadas) da el mismo marcador; los contadores no se reinician.

### Task 4: retirada
Eran las reglas deterministas. Jano decidió no hacerlas (ver «Sin reglas deterministas»). Se conserva el número para no cambiar el de las demás tareas.

### Task 5: Segmentación en palabras y tokenización
- **Action**: Palabras: se parte por espacios y por `, ; | " < > ( ) [ ] { } * ^`, con offsets en caracteres. Es la misma definición que usa la tarjeta de la v2 y no depende del modelo. Tokenización con `encode_char_offsets`, sin truncado y sin tokens especiales (se añaden en cada ventana). Asignar cada token a su palabra por offsets.
- **Validate**: pruebas puras para las palabras; con el tokenizador real (ignoradas): texto de más de 512 tokens no se trunca; dobles espacios, acentos, `ñ`, `l·l` catalana y emoji conservan offsets exactos en v1 y v2.

### Task 6: Troceado
- **Action**: `plan_windows(n_tokens, max = 510, overlap = 128)` y `owner_window(token)`, que elige la ventana donde el token está más lejos del borde. Agrupar ventanas de varios segmentos en lotes de tamaño configurable (8 por defecto).
- **Validate**: pruebas puras con límites (0, 1, 510, 511, 1000 tokens), cobertura completa sin huecos y un único dueño por token. La prueba del PRD con el modelo real (una entidad en la zona de solape se detecta una sola vez) pasa a la Task 8, porque necesita el motor completo.

### Task 7: Decodificación
- **Action**: softmax; BIO, donde un `I-` sin `B-` del mismo tipo abre entidad; dos estrategias de palabra: `FirstSubtoken` (la del PRD) y `WordMean` (la de la tarjeta de v2: media de probabilidades de los subtokens, uniendo B- e I- de cada tipo); umbral sobre la puntuación media (0,5 por defecto, configurable); recorte de espacios y puntuación en los bordes.
- **Validate**: pruebas puras con logits sintéticos para cada regla de decodificación del PRD y para las dos estrategias.

### Task 8: Inferencia y `Engine`
- **Action**: Sesión única con `Mutex`, telemetría desactivada, hilos internos = mitad de núcleos; lotes con relleno y `attention_mask`; lectura de `id2label` desde `config.json`; `Engine::detect` de extremo a extremo.
- **Validate**: integración (ignorada) con v1 y v2: "Carlos García, DNI 12345678Z, Madrid, +34 612 345 678." da NOMBRE_PERSONA, ID_DOCUMENTO_PERSONA, LUGAR y TELEFONO con los offsets exactos; segmentos vacíos y solo espacios no fallan; prueba del PRD: en un texto de más de 512 tokens, una entidad en la zona de solape se detecta una sola vez.

### Task 9: Tipos opcionales
- **Action**: `SkippedTypes`: lista de tipos opcionales que el usuario desmarca, validada contra la tabla de tipos. El motor descarta esas entidades al final. Ya no hace falta resolver solapes: la decodificación es palabra a palabra y cada token tiene una sola ventana dueña, así que dos entidades nunca se pisan (comprobado en la Task 8).
- **Validate**: pruebas puras: por defecto se conservan los cuatro tipos opcionales; desmarcar `LUGAR` quita los lugares pero no las direcciones postales; pedir que se desactive un tipo obligatorio, como `NOMBRE_PERSONA`, o un tipo que no existe da un error claro. Integración (ignorada): la frase de la Task 8 sin `LUGAR` pierde «Madrid» y conserva el resto.

### Task 10: CLI `entidades`
- **Action**: `cli entidades archivo.txt` imprime JSON `{segmento, inicio, fin, tipo, etiqueta, texto, puntuacion}` (un segmento por párrafo separado por línea en blanco) y, con `--tiempos`, los tiempos por fase en stderr. La opción `--no-sustituir CARGO,IMPORTE` desmarca tipos opcionales.
- **Validate**: `tests/cli.rs`: JSON válido y offsets que coinciden con el texto; archivo inexistente → código 1 con mensaje claro.

### Task 11: Corpus sintético y evaluación
- **Action**: Unos 8 textos por idioma (es, ca, en, fr) con datos inventados, incluidos DNI, NIE, IBAN y tarjetas con dígito de control válido para que parezcan reales, y un texto largo con entidades en zonas de solape. Cada texto lleva al lado un JSON `{entidades: [{tipo, texto}]}`; los offsets se calculan buscando el texto, y una prueba comprueba que cada entidad esperada aparece. `cli evaluar tests/corpus/texto --modelo <carpeta> --decodificacion <estrategia>` calcula precisión y exhaustividad por tipo (coincidencia estricta de bordes y tipo) y los tiempos.
- **Validate**: pruebas de la métrica con casos sintéticos (aciertos, falsos positivos, bordes desplazados); `cli evaluar` termina en el corpus.

### Task 12: Comparación de modelos y tiempos
- **Action**: Ejecutar la evaluación con v1 y v2, cada uno con las dos estrategias. Medir la carga del modelo, el rendimiento (tokens/s) y el tiempo de un texto de unas 10 páginas frente a los objetivos del PRD (modelo cargado < 3 s; DOCX de 10 páginas < 5 s, usando el texto como aproximación). Escribir `tests/informe-deteccion.md`. **Parar y presentar los datos a Jano para elegir el modelo.** Después, quitar el modelo descartado del lock y registrar la decisión.
- **Validate**: el informe existe y tiene cifras de las cuatro combinaciones; la decisión está en `DECISIONES.md`.

### Task 13: Logs sin datos y documentación
- **Action**: Instrumentar el motor con `tracing` (recuentos y tiempos). Una prueba con un suscriptor que captura eventos comprueba que ningún evento contiene texto de entrada ni valores detectados. Actualizar README y DECISIONES.
- **Validate**: `cargo test -p nucleo logs`; validación completa.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --include-ignored
node --test "scripts/test/*.test.mjs"
cargo run -p nucleo --bin cli -- entidades tests/corpus/texto/es-01.txt
cargo run --release -p nucleo --bin cli -- evaluar tests/corpus/texto --modelo modelo
cargo run --release -p nucleo --bin cli -- evaluar tests/corpus/texto --modelo modelo-v2
cargo llvm-cov --workspace --summary-only -- --include-ignored   # objetivo ≥ 80 % en motor/
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| El ONNX de v2 (ModernBERT) usa operadores que ONNX Runtime 1.31 no admite, o su `tokenizer.json` no carga con el crate `tokenizers` | Media | Se comprueba en la Task 1, antes de escribir el motor. Si falla, se informa y la comparación se queda solo con v1 |
| El truncado a 512 que trae `tokenizer.json` de v1 recorta textos en silencio | Alta si no se trata | Desactivarlo de forma explícita; prueba con más de 512 tokens |
| Offsets desalineados por el normalizador (espacios → `▁`), acentos o emoji, o confusión entre bytes y caracteres | Media | Todo en caracteres; pruebas específicas con texto real en los dos tokenizadores |
| La v2 es muy reciente (3 días) y tiene pocos usuarios | Media | La decisión se toma con datos propios y el lock fija la revisión exacta |
| No se cumplen los tiempos en un portátil de 4 núcleos | Media | Medir en la Task 12, con lotes y `--release`; si no se cumplen, presentar opciones (tamaño de lote, hilos, ventanas más largas en v2) antes de cambiar nada |
| El `Mutex` serializa la inferencia cuando hay 2 archivos en paralelo | Baja | ONNX Runtime ya reparte cada pasada entre varios núcleos; se mide en el hito 5 |
| El corpus lo escribo yo y es pequeño, así que puede sesgar la comparación | Media | Mismo corpus para los dos modelos, cuatro idiomas y tipos variados; el informe avisa de que diferencias menores de ~0,02 son ruido |
| Algunos tipos pueden anonimizar de más y restar utilidad al documento | Media | `CARGO`, `IMPORTE`, `LUGAR` y `COORDENADAS` son opcionales para el usuario. Si el informe de detección muestra el mismo problema con otro tipo (por ejemplo `ORGANIZACION`), se propondrá hacerlo opcional |
| Sin reglas deterministas, un DNI, IBAN o tarjeta que el modelo no detecte, o detecte solo en parte, queda en el documento anonimizado | Media | El informe de la Task 12 mide la exhaustividad de esos tipos por separado. Si es baja, se presentan los datos a Jano |
| `onig` compila código C (en Windows funciona; en Mac está sin probar) | Baja | Mac aplazado; anotarlo para el hito 6 |
| La forma corta del PDF (`[P42]`) choca entre tipos con la misma inicial (`NOMBRE_PERSONA` y `NOMBRE_PROPIO`, por ejemplo) | — | No es de este hito; se anota para el hito 4 |

## Preguntas abiertas

1. ~~**Tabla de tipos**~~: resuelta. Jano envió la lista de nombres definitiva el 2026-10-26.
2. ~~**Qué se anonimiza**~~: resuelta. Se sustituye todo por defecto, y `CARGO`, `IMPORTE`, `LUGAR` y `COORDENADAS` son opcionales para el usuario (ver la sección de la tabla de tipos).

## Acceptance

- [ ] All tasks complete, cada una con RED y GREEN en commits
- [ ] Pruebas automáticas del PRD de este hito en verde: troceado, marcadores y marcadores entre documentos (las de reglas se retiran por decisión de Jano)
- [ ] `cli entidades archivo.txt` imprime las entidades en JSON
- [ ] Informe de detección con v1 y v2, tiempos medidos frente a objetivos y modelo elegido por Jano
- [ ] Sin crates con acceso a red (la prueba de dependencias sigue en verde)
- [ ] Ningún log con texto de documentos
- [ ] Cobertura ≥ 80 % en `nucleo::motor`
- [ ] Patterns mirrored, not reinvented
