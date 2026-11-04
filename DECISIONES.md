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

### Los nombres de salida salen del archivo que se procesa

El PRD ponía como ejemplo que desanonimizar `contrato-cliente_anonimizado.docx` diera `contrato-cliente_desanonimizado.docx`. La app saca la descripción del archivo que recibe, así que da `contrato-cliente-anonimizado_desanonimizado.docx`; el diccionario exportado, `contrato-cliente-anonimizado_diccionario.json`. Jano decidió el 2026-11-02 quedarse con este comportamiento y cambiar los ejemplos del PRD.

## PowerPoint y Excel · 2026-11-02

Jano añadió PowerPoint y, sobre todo, Excel al alcance de la v1, con las mismas salidas que Word. El PRD lo recoge en «Formato PPTX» y «Formato XLSX». Lo que sigue son las decisiones y lo que se ha elegido al hacerlo.

### Decisiones de Jano

- **Salidas.** PPTX sale en PPTX, MD o TXT; XLSX, en XLSX, MD o TXT, como Word.
- **Números de Excel.** Se analizan también las celdas numéricas: un teléfono o un documento guardado como número, una fecha. Los importes no se sustituyen por defecto en Excel (en la CLI, `--sustituir-importes` los sustituye). En los demás formatos los importes siguen como estaban: se sustituyen salvo que se desmarque el tipo.
- **Nombres de hoja.** Si llevan un dato, se renombran con el marcador sin corchetes y el nombre se recupera al desanonimizar.
- **Office 97-2003.** `.doc`, `.ppt` y `.xls` se rechazan con un aviso que pide guardarlos en el formato nuevo.

### Una base común para los tres formatos de Office

DOCX, PPTX y XLSX son el mismo tipo de paquete: un ZIP con partes XML. La lectura segura del paquete, el escritor que empalma el texto nuevo en el XML original y el vaciado de metadatos están en `nucleo/src/formatos/ooxml/`, y los tres formatos los comparten. El texto de PowerPoint y de los cuadros de texto y gráficos de Excel (DrawingML) tiene su propio lector común. Un archivo de Office se reconoce por su contenido, no por la extensión.

Al hacerlo, Word gana dos cosas que antes no hacía: vacía también el responsable (`Manager`) de `docProps/app.xml`, y sustituye los datos que aparezcan en los títulos de `docProps/app.xml` y en las propiedades personalizadas (`docProps/custom.xml`).

### Un número sustituido pasa a ser texto

En Excel, un teléfono guardado como número que se anonimiza queda como texto con el marcador (`[TELEFONO_0001]`), porque una celda numérica no puede guardar un marcador. Al desanonimizar vuelve el valor, pero como texto: `654218903` sigue viéndose igual, aunque ya no es un número. Una fórmula que usara ese valor deja de calcular con él. Si la celda era el resultado de una fórmula, la fórmula se conserva y solo cambia el valor guardado.

Las fechas se analizan tal como se ven (`31/05/1985`, en `dd/mm/aaaa`), no como el número de serie que guarda Excel. Una fecha sustituida vuelve como texto en ese formato.

### Los números se analizan con el título de su columna

Una celda con solo `654218903` no le dice nada al modelo: en el corpus no detectaba ninguno de los dos teléfonos guardados como número. Por eso, al analizar una celda numérica, el modelo lee delante el título de su columna (el texto de la primera fila de la hoja): `Teléfono: 654218903`. Con eso el modelo real detecta los dos. El título solo sirve de contexto: el archivo no cambia, y una entidad que caiga en el título no cuenta. No es una regla fija: sigue decidiendo el modelo.

### Nombres de hoja que no caben

Excel limita los nombres de hoja a 31 caracteres. Si con los marcadores no cabe, se queda solo con los marcadores que quepan («Nómina de Marta y Pablo» con dos nombres → `NOMBRE_PERSONA_0001`), y lo que no cabe se pierde también al desanonimizar. Si el nombre nuevo coincide con el de otra hoja, se le añade ` (2)`. El nombre nuevo se cambia en todas las referencias: fórmulas, nombres definidos, gráficos, tablas dinámicas, vínculos y `docProps/app.xml`.

### Copias de los datos que Excel y PowerPoint guardan aparte

Excel guarda copias de los valores en las cachés de los gráficos y de las tablas dinámicas, en las listas de validación y dentro de las fórmulas; PowerPoint, en las cachés de los gráficos y en el texto alternativo de imágenes y formas. Si se dejaran, el dato seguiría en el archivo aunque no se vea. En todos esos sitios se sustituyen los valores detectados en el texto (sustitución literal), como ya se hacía en Word con los enlaces. En las fórmulas solo se toca lo que va entre comillas; las referencias y las funciones no.

### Qué no se procesa todavía

- Las lecturas fonéticas de Excel (textos en japonés con su pronunciación) no se analizan.
- Los libros de Excel incrustados en un gráfico o en un objeto cuentan como objeto incrustado en los avisos, pero su contenido no se revisa.
- El corpus sintético no lleva gráficos ni comentarios de Excel, porque un archivo hecho a mano con ellos es fácil que Excel lo quiera reparar. Las pruebas unitarias sí los cubren.

## Hito 4 · PDF · 2026-11-03

El PDF sigue el PRD («Formato PDF»): las páginas con datos se rehacen como imagen con los datos tapados y los marcadores encima en texto, las demás se copian, y después una verificación obligatoria vuelve a leer el archivo. Lo que sigue son las decisiones tomadas al hacerlo.

### La salida es un documento nuevo, no el original editado

El PRD pide vaciar el diccionario `Info` y el XMP, quitar el outline y los adjuntos. En lugar de buscarlos uno a uno en el original, la salida se construye siempre desde un PDF vacío: las páginas sin datos se copian del original (ya aplanadas) y las demás se crean nuevas. Así tampoco pasan el JavaScript del documento, los formularios ni otras piezas que el PRD no nombra. El `Info` de la salida solo lleva el `Producer` que pone PDFium, sin datos del original.

### Una página escaneada es la que no tiene texto pero sí imagen

El PRD rechaza el documento si «alguna página no tiene texto». Una página en blanco tampoco lo tiene, y no es un escaneo. Se rechaza solo si una página no tiene ningún carácter visible pero sí al menos una imagen.

### Un valor detectado se tapa en todas las páginas

Como en Office, cada valor que el modelo detecta se busca también en el resto del documento y se tapa donde aparezca. Sin esto, un nombre que el modelo viera en la página 1 y no en la 3 haría fallar la verificación de todo el archivo. Los recuentos del resumen siguen siendo lo que detectó el modelo.

### La verificación busca sin mayúsculas, acentos ni saltos de línea

La verificación del PRD busca cada valor original en el texto de la salida. Se compara sin distinguir mayúsculas, acentos ni ligaduras, y cualquier serie de espacios o saltos vale lo mismo, así que «MARTA\nGONZALEZ» cuenta como «Marta González». Un valor de menos de seis caracteres solo cuenta como palabra entera, para que «Ana» no haga fallar un documento con «Mariana». La sustitución literal del punto anterior usa la misma regla, así que lo que tapa es justo lo que la verificación buscaría. La verificación se hace sobre el archivo ya escrito; si falla, se borra y el documento se olvida en el almacén.

### Forma corta con un contador propio por inicial

El PRD pone como forma corta la inicial del tipo y el número del marcador (`[P42]`). Con la tabla de tipos de este modelo, eso daría la misma forma corta a datos distintos: N es la inicial de `NOMBRE_PERSONA` y de `NOMBRE_PROPIO`, C la de ocho tipos. La forma corta es la inicial del tipo con un contador propio de esa inicial, que el almacén guarda en `contadores_cortos`: el primer marcador que no cabe con N es `[N1]`, el siguiente `[N2]`, sea del tipo que sea. Se asigna la primera vez que hace falta, se guarda en la entrada y, como los números de los marcadores, no se reutiliza aunque se borre la entrada. El campo es opcional: el formato del almacén sigue en la versión 1, y un almacén que traiga formas cortas sin su contador (como el ejemplo del PRD) lo pone a su altura al abrirse.

### Un marcador por dato, en su tramo más ancho

Un dato que ocupa dos líneas da dos rectángulos, uno por línea. El marcador se escribe en el más ancho; el otro queda en blanco.

### Las páginas giradas salen sin giro, como se ven

Una página con `/Rotate` se renderiza tal como se ve, y la página nueva tiene ese tamaño y ningún giro. El marcador queda horizontal y se lee igual que el resto de la página.

### Desanonimizar un PDF no se admite

En el PDF anonimizado el dato ya no está: su página es una imagen. El PRD no admite PDF como entrada al desanonimizar. Si se intenta, el error explica que se desanonimice el DOCX, MD o TXT sacado del PDF, o el texto copiado de él, que sí llevan los marcadores (también las formas cortas).

### Párrafos de las salidas DOCX, MD y TXT

PDFium no da párrafos, solo líneas. Una línea sigue en el mismo párrafo si empieza justo debajo de la anterior, con un hueco menor que la mitad de su alto. Un hueco mayor, o una línea que empieza más arriba (otra columna), abre párrafo nuevo. El MD lleva una sección `## Página N` por página; el DOCX, un salto de página entre páginas.

### Un solo hilo usa PDFium a la vez

PDFium no admite varios hilos. La función `thread_safe` de `pdfium-render` protege cada llamada por separado, pero no una serie de ellas, y en las pruebas dos documentos abiertos a la vez en hilos distintos llegaron a dar vacío el texto de una página. Todo el uso de PDFium pasa por un permiso exclusivo (`pdf::pdfium()`). Con dos archivos en paralelo, como pide el PRD, el modelo sí trabaja a la vez; solo se turnan la lectura y la escritura del PDF.

### Rendimiento

Un PDF de diez páginas con datos en todas tarda 9,5 s con 4 hilos a 200 ppp, verificación incluida, por debajo de los 15 s del PRD. De ellos, 7,8 s son del modelo. Está en `tests/informe-deteccion.md`.

### Qué no se procesa todavía

- Los enlaces de una página sin datos se copian con la página. Si el destino de un enlace llevara un dato que no aparece en el texto visible, no se tapa y la verificación no lo ve, porque solo lee el texto.
- El texto invisible de un PDF escaneado con OCR cuenta como texto: el documento no se rechaza, y los rectángulos se pintan donde PDFium sitúa ese texto.
- Un formulario sin apariencia guardada para un campo puede no dejar su texto al aplanarse.
