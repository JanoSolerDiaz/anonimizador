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

### El motor lee el documento dos veces y se aceptan más de 5 s para 10 páginas

Cambio del PRD, que analiza cada segmento por separado. El motor hace dos
lecturas de los segmentos de cada llamada: una párrafo a párrafo y otra con
los párrafos seguidos, para que cada uno tenga el contexto de sus vecinos. Se
tapa lo que encuentre cualquiera de las dos; si dos entidades se pisan, se
juntan en una con el tipo de la más segura. Las entidades no cruzan de un
segmento a otro y las posiciones siguen siendo relativas al segmento.

En el corpus, la lectura doble tapa el 92 % de los datos (85 % párrafo a
párrafo, 89 % seguida) y es la única que no deja a la vista ningún nombre,
documento de identidad, cuenta, tarjeta, correo ni teléfono. La lectura seguida
usa trozos de 256 tokens con un solape de 32 en lugar de 510 y 128: protege
igual y tarda alrededor de un 30 % menos. Los datos están en
`tests/informe-deteccion.md`.

Precio: analizar 10 páginas lleva entre 7 y 9 s con 2 hilos y entre 5 y 6,5 s
con 4 hilos en el equipo de desarrollo, frente a los 5 s del PRD. Jano decidió
aceptarlo. Las otras lecturas siguen disponibles en la CLI con
`--contexto parrafo|documento` para medir. Habrá que confirmar los tiempos en
un portátil de referencia en el hito 6.

Medido de principio a fin el 2026-11-02 con un DOCX de diez páginas, anonimizar
tarda lo mismo que el modelo: leer el Word, el almacén y escribir la salida
suman una centésima de segundo. El mismo portátil dio ese día entre 9 y 29 s
con 2 hilos según el momento, así que las cifras de este equipo solo sirven
para comparar medidas tomadas una junto a otra. Está en
`tests/informe-deteccion.md`.

### Borrar del almacén no reinicia la numeración de los marcadores

Caducar, olvidar un documento y «Vaciar todo» borran documentos y entradas, pero los contadores de cada tipo no bajan nunca. Un documento anonimizado antes puede seguir circulando con `[NOMBRE_PERSONA_0003]`; si ese número se diera después a otra persona, al desanonimizar el documento viejo con el almacén nuevo saldría un nombre equivocado sin ningún aviso. Con la numeración intacta, el marcador viejo aparece como «no está en el almacén», que es lo correcto. Es una interpretación de «Vaciar todo», no un cambio del PRD.

Un documento se borra el mismo día que indica su campo `caduca`: el ejemplo del PRD, creado el 22 de octubre, caduca y se borra el 20 de enero, a los 90 días exactos.

### También se vacían los autores de comentarios y del control de cambios

Word guarda en `w:author` (y en los comentarios también `w:initials`) el nombre y las iniciales de quien escribió cada comentario y de quien hizo cada cambio con control de cambios (`w:ins`, `w:del` y los cambios de formato). Son datos personales que el PRD no menciona: solo pide vaciar `dc:creator`, `cp:lastModifiedBy` y `Company`. Jano aceptó el 2026-10-29 que el escritor DOCX vacíe también esos atributos en todas las partes que procesa. El corpus DOCX los recoge en el campo `autores_revision` de cada JSON.

La lista de personas de `word/people.xml`, que Word moderno guarda con el nombre de cada revisor y a veces su correo (`w15:author` y `w15:userId`), se vacía por la misma razón.

### MD y TXT desde DOCX: cuerpo y notas, con los cambios aceptados

El PRD pide que el MD conserve títulos, listas y tablas, y que el TXT tenga un párrafo por línea, pero no dice qué partes del documento entran. Jano decidió el 2026-10-30 que entren el cuerpo y, al final, una sección «Notas» con las notas al pie y al final. Los encabezados, los pies de página, los comentarios y las propiedades del archivo se dejan fuera.

El texto con control de cambios sale como si los cambios estuvieran aceptados: el texto borrado no aparece y el insertado sí. En la salida DOCX el texto borrado sigue ahí, y se anonimiza igual que el resto.

### Un tabulador o un salto dentro de un dato se va con el dato

El PRD trata los tabuladores y los saltos de línea de Word como un espacio en el texto, sin entrada editable en el mapa. Si un dato los cruza, por ejemplo una dirección en dos líneas, el marcador ocupa el texto pero el salto se quedaba detrás. Al desanonimizar, el valor guardado (con un espacio donde estaba el salto) se sumaba al salto que seguía en el documento, y el texto no volvía igual.

Desde el 2026-10-30, el escritor DOCX borra los tabuladores y saltos que quedan dentro de una sustitución. El documento anonimizado queda sin un salto suelto detrás del marcador, y la ida y vuelta devuelve exactamente el mismo texto. El precio es que, en el documento desanonimizado, esa dirección queda en una sola línea, con un espacio en lugar del salto. No es un cambio del PRD: los saltos siguen sin poder recibir texto, solo se borran.

### Los logs llevan un identificador del documento, no su nombre

El PRD dice que los logs registran nombres de archivo. El nombre de un archivo puede ser en sí un dato personal («Contrato Marta Lozano.docx»), y la misma regla del PRD prohíbe que un log lleve texto del documento. Por eso los eventos de anonimizar y desanonimizar llevan el identificador aleatorio del documento en el almacén (un UUID), el formato y los recuentos, pero no el nombre. Es más estricto que el PRD. Quien necesite saber qué archivo era lo encuentra con `cli almacen listar`, que sí muestra los nombres pero nunca los valores. `nucleo/tests/logs.rs` comprueba que ningún evento lleva texto de los documentos ni valores del almacén.

### Clave de prueba solo en las compilaciones de depuración

Las pruebas de la CLI no pueden usar el almacén de credenciales del sistema: dejarían claves en el equipo de quien las pase y fallarían en una máquina sin él. Las compilaciones de depuración aceptan la clave del almacén en la variable `ANONIMIZADOR_CLAVE_PRUEBA` (64 cifras hexadecimales). Las de `--release`, que son las que se entregan, la ignoran y usan siempre el almacén de credenciales, así que nadie puede abrir el almacén de un usuario poniendo una variable.

En Linux, que solo se usa para desarrollar, el almacén va en `$XDG_DATA_HOME` o en `~/.local/share`.
