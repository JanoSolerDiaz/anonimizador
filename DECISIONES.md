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

El PRD daba ejemplos genéricos (`PERSONA`, `DIRECCION`, `SALUD`). Jano aprobó una tabla con un marcador por cada clase del modelo, por ejemplo `NOMBRE_PERSONA`, `DIRECCION_POSTAL` y `DATOS_SALUD`. La tabla está en `nucleo/config/tipos.json` y `nucleo/tests/tipos.rs` la fija.

### Sin reglas deterministas

El PRD pide reglas fijas para DNI, NIE, IBAN, tarjeta, email y teléfono, que se ejecuten siempre junto al modelo, comprueben el dígito de control y ganen al modelo cuando coincidan con él. Jano decidió no hacerlas: todos los datos los detecta el modelo. Esto deja sin efecto, respecto al PRD:

- la sección «Reglas deterministas» y la fila «Reglas» (crate `regex`) de la tabla de tecnologías;
- el campo `origen` de las entidades, que siempre valdría «modelo»;
- la regla de fusión «si una regla y el modelo se solapan, gana la regla» (se mantiene que entre dos entidades del modelo gana la de mayor puntuación);
- el criterio de aceptación «Reglas: DNI, NIE, IBAN y tarjeta válidos se detectan; los de control incorrecto no».

Riesgo aceptado: un DNI, IBAN o tarjeta que el modelo no detecte, o que detecte solo en parte, queda en el documento anonimizado. El informe de detección del Hito 2 mide la exhaustividad de esos tipos por separado.

### Tipos opcionales

`CARGO`, `IMPORTE`, `LUGAR` y `COORDENADAS` se pueden dejar sin sustituir. Al anonimizar, el usuario elige cuáles; por defecto se sustituyen todos. El resto de tipos no se puede desactivar, y pedirlo da un error. En el Hito 2 se elige con la opción `--no-sustituir` de la CLI; en el Hito 5, con casillas junto al selector de formato.

### La clave de un marcador es solo el valor normalizado

El PRD dice que el mismo valor recibe siempre el mismo marcador. El modelo puede clasificar un mismo texto con tipos distintos en dos sitios (por ejemplo, «Valencia» como lugar y como organización). Si la clave incluyera el tipo, ese texto tendría dos marcadores y la restauración seguiría funcionando, pero el mismo dato aparecería con dos nombres en el documento anonimizado. Por eso la clave es solo el valor normalizado, y el dato conserva el tipo con el que apareció por primera vez.

La normalización quita las marcas diacríticas tras descomponer el texto (NFD). Eso incluye la tilde de la «ñ» y la cedilla de la «ç»: «Muñoz» y «Munoz» comparten marcador.

## Hito 2 · 2026-10-28

### Modelo elegido: v1

Se compararon v1 (`bardsai/eu-pii-anonimization-multilang@62ca571`, el del PRD) y v2 (`bardsai/eu-pii-anonimization-multilang-v2@5e61416`) con el corpus sintético de `tests/corpus/texto`. Los datos están en `tests/informe-deteccion.md`. Jano eligió v1, con la estrategia de palabra «primer subtoken» y umbral 0,5, que son los valores del PRD.

Motivos: v1 deja tapados más datos (84 % frente a 74 %), detecta las categorías especiales del RGPD (v2 no detectó ningún dato de salud), carga en la mitad de tiempo y cumple el objetivo de 5 s para 10 páginas incluso con 2 hilos. v2 se quita de `scripts/recursos.lock.json` y de las pruebas.

La estrategia «media de palabra» se queda en el código como opción de la CLI para futuras mediciones, pero no es la de la aplicación.

### Una regla para tarjetas de pago

Excepción a «Sin reglas deterministas». Con v1, 3 de las 6 tarjetas del corpus quedaban con cifras a la vista. Jano decidió recuperar una sola regla: una secuencia de 13 a 19 cifras, juntas o en grupos separados por un espacio o un guion, que cumpla el algoritmo de Luhn, es una `TARJETA_BANCARIA`. La regla gana al modelo: las entidades del modelo que caen dentro de la tarjeta se quitan, y las que la pisan en parte se recortan. Si una entidad del modelo ya contiene la tarjeta entera, se deja como está, porque la tarjeta ya queda tapada.

El resto de identificadores (DNI, NIE, IBAN) siguen sin regla: en el informe, v1 los tapó todos.

Limitación conocida: los IMEI de los teléfonos también tienen 15 cifras con dígito de control de Luhn, así que la regla los marca como `TARJETA_BANCARIA` en lugar de `DISPOSITIVO`. Quedan tapados igualmente. No se pueden excluir las secuencias de 15 cifras porque las tarjetas American Express tienen 15.

### La coma entre dos cifras no separa palabras

El motor parte el texto en palabras por espacios y por `, ; | " < > ( ) [ ] { } * ^`, como la tarjeta del modelo v2. Con esa definición, «189,90 €» eran dos palabras y v1 dejaba los céntimos a la vista. Ahora una coma entre dos cifras (decimal o de miles) no separa. Es un cambio de implementación, no del PRD, pero se anota porque se aparta de la definición de la que parte.

## Hito 3 · 2026-10-28

### Plazo de caducidad fijado por sistemas: en el hito 6

El almacén acepta el plazo de caducidad como parámetro (90 días por defecto). Leer la política que fije sistemas (clave de registro en Windows, perfil de configuración en Mac) se hace en el hito 6, junto con el empaquetado y el despliegue, que es donde se decide cómo la configura sistemas.

### Más metadatos del DOCX pasan por el motor

El PRD manda vaciar `dc:creator`, `cp:lastModifiedBy` y `Company`. Además, el título, el asunto, las palabras clave y la descripción del documento (`dc:title`, `dc:subject`, `cp:keywords`, `dc:description`) pueden llevar datos personales, así que se pasan por el motor como segmentos y se sustituye lo que detecte.
