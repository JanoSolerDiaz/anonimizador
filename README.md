# Anonimizador local

App de escritorio (Windows y macOS) que anonimiza documentos Word, PowerPoint, Excel y PDF con un modelo de IA local y los desanonimiza después. Ningún dato sale del equipo.

Especificación: [`specs/anonimizador-local.prd.md`](specs/anonimizador-local.prd.md). Cambios respecto a ella: [`DECISIONES.md`](DECISIONES.md).

## Estructura

```
nucleo/          crate Rust sin Tauri: motor, formatos, almacén y CLI
app/             interfaz Svelte 5 (src/) y capa Tauri 2 (src-tauri/)
scripts/         descarga del modelo y de los binarios nativos
recursos/        modelo, ONNX Runtime y PDFium (no va en git)
tests/           corpus sintético e informe de detección
```

## Requisitos

- Node 24 o superior.
- Rust estable (1.92 o superior) con `rustup`.
- **Windows:** Visual Studio Build Tools con "Desarrollo para el escritorio con C++". WebView2 viene con Windows 11.
- **macOS:** Xcode Command Line Tools.

## Primeros pasos

```bash
# 1. Recursos (modelo ~280 MB, ONNX Runtime, PDFium). Comprueban SHA-256.
node scripts/descargar-modelo.mjs
node scripts/descargar-binarios.mjs        # --todas para todas las plataformas

# 2. Dependencias de la interfaz
cd app && npm ci && cd ..

# 3. Comprobar que el modelo y PDFium cargan
cargo run -p nucleo --bin cli -- comprobar-recursos
```

## La app

Una ventana con dos pestañas. En **Anonimizar** se sueltan los documentos (o se eligen con «Elegir archivos…»); cada uno aparece con su tipo y su formato de salida, que se puede cambiar para todos o archivo a archivo. Debajo se eligen los tipos opcionales que se sustituyen, los importes de Excel y la resolución de los PDF, cuando hacen falta, y la carpeta de salida, que se recuerda. Al pulsar **Anonimizar**, cada archivo muestra su progreso y, al terminar, los datos encontrados por tipo y los avisos, con el botón **Abrir carpeta**.

En **Desanonimizar** se sueltan los documentos anonimizados (DOCX, PPTX, XLSX, MD o TXT; un PDF no) o se pega texto. El resumen dice cuántos marcadores se restauraron y cuáles no están en el almacén. El texto restaurado se copia con **Copiar**. Si un marcador tiene valores distintos en el almacén y en un diccionario recibido, se elige cuál usar para ese documento.

El botón **Almacén** abre el panel con los documentos anonimizados (exportar su diccionario u olvidarlos), los diccionarios recibidos, **Importar diccionario** y **Vaciar todo**. El modelo se carga en segundo plano al abrir la app: mientras tanto se pueden ir añadiendo archivos.

```bash
cd app && npm run tauri dev

# Sin tocar el almacén ni la clave del usuario (solo en depuración)
ANONIMIZADOR_CLAVE_PRUEBA=<64 cifras hexadecimales> ANONIMIZADOR_ALMACEN=/tmp/almacen.anon npm run tauri dev

# Pruebas de la interfaz y comprobación de tipos
cd app && npm test && npm run check
```

## Motor de detección

El modelo es `bardsai/eu-pii-anonimization-multilang` (v1), elegido tras comparar con su versión 2 (ver [`tests/informe-deteccion.md`](tests/informe-deteccion.md)). El motor trocea el texto en ventanas de 512 tokens con 128 de solape, decodifica una etiqueta por palabra y añade una regla para tarjetas de pago con el dígito de control de Luhn. Los tipos de datos y sus marcadores están en `nucleo/config/tipos.json`.

```bash
# Datos personales de un archivo de texto, en JSON
cargo run --release -p nucleo --bin cli -- entidades archivo.txt --tiempos

# Precisión, exhaustividad y datos tapados por tipo con el corpus sintético
cargo run --release -p nucleo --bin cli -- evaluar tests/corpus/texto --detalle
```

Opciones de las dos órdenes: `--modelo <carpeta>`, `--decodificacion primer|media`, `--umbral <0..1>`, `--contexto ambos|documento|parrafo` y `--hilos <n>`. Con `ambos`, la opción por defecto, el motor lee el documento párrafo a párrafo y con los párrafos seguidos, y tapa lo que encuentre cualquiera de las dos lecturas. `entidades` admite además `--no-sustituir CARGO,IMPORTE,LUGAR,COORDENADAS` para dejar sin sustituir los tipos opcionales.

El motor emite registros con `tracing` (recuentos por tipo, tokens, ventanas y tiempos). Nunca incluyen texto de los documentos ni valores detectados: lo comprueba `nucleo/tests/logs.rs`.

## Anonimizar y desanonimizar desde la terminal

```bash
# PDF, DOCX, PPTX, XLSX, MD o TXT. Sin --formato, cada archivo sale en su formato
cargo run --release -p nucleo --bin cli -- anonimizar contrato.pdf informe.docx ventas.pptx nominas.xlsx notas.md --salida <carpeta>

# Archivos anonimizados o texto pegado
cargo run --release -p nucleo --bin cli -- desanonimizar <carpeta>/contrato_anonimizado.docx --salida <otra carpeta>
cargo run --release -p nucleo --bin cli -- desanonimizar --texto "Firmado por [NOMBRE_PERSONA_0001]"

# Documentos y diccionarios del almacén
cargo run --release -p nucleo --bin cli -- almacen listar | exportar <documento> --salida <carpeta> | importar <archivo> | olvidar <documento> | vaciar --confirmar | purgar
```

`anonimizar` admite `--formato pdf|docx|pptx|xlsx|md|txt`, `--ppp <150-300>`, `--no-sustituir` y las opciones del motor. Word, PowerPoint y Excel salen en su propio formato, en MD o en TXT; los archivos de Office 97-2003 (`.doc`, `.ppt`, `.xls`) se rechazan con un aviso para guardarlos en el formato nuevo. Si un marcador tiene valores distintos en el almacén y en un diccionario importado, `desanonimizar` pide elegir con `--fuente propio` o `--fuente <nombre del diccionario>`. Un archivo que falla no detiene el lote: se explica el error y se siguen procesando los demás.

Un PDF digital sale en PDF, DOCX, MD o TXT. En el PDF, las páginas sin datos se copian tal cual; las que tienen datos se convierten en imagen (200 ppp por defecto, `--ppp` para cambiarlo), los datos se tapan con un rectángulo blanco y el marcador se escribe encima como texto. Si el marcador no cabe, va una forma corta como `[N1]`, que también se restaura. La salida no lleva los metadatos, el índice de navegación ni los adjuntos del original (se avisa de estos). Después se vuelve a leer el PDF entero: si aparece algún dato original, la salida se borra y el archivo cuenta como fallido. Los PDF con contraseña o con páginas escaneadas se rechazan. Un PDF anonimizado no se desanonimiza: se desanonimiza el DOCX, MD o TXT sacado de él, o el texto copiado.

En Excel se analizan los textos, los resultados de las fórmulas, los nombres de las hojas y también las celdas numéricas (un teléfono guardado como número, una fecha), cada una con el título de su columna como contexto para el modelo. Un número sustituido pasa a ser texto con el marcador. Los importes no se sustituyen por defecto; `--sustituir-importes` los sustituye también. Una hoja con un dato en el nombre se renombra con el marcador sin corchetes y se recupera al desanonimizar. Detalles en `DECISIONES.md`.

### Cómo funciona el almacén

El almacén guarda qué dato original hay detrás de cada marcador. Es un único archivo cifrado con AES-256-GCM en la carpeta de datos del usuario (`%APPDATA%\anonimizador\almacen.anon` en Windows, `~/Library/Application Support/anonimizador/almacen.anon` en Mac). La clave se genera la primera vez y se guarda en el almacén de credenciales del sistema (Administrador de credenciales de Windows, Llavero de Mac), así que solo ese usuario, en ese equipo, puede leerlo. Todas las órdenes aceptan `--almacen <archivo>` para usar otro.

- **Marcadores compartidos.** El mismo dato recibe siempre el mismo marcador en todos los documentos, y la numeración de cada tipo no vuelve atrás nunca, ni al borrar documentos.
- **Caducidad.** Cada documento caduca a los 90 días. Cada vez que se abre el almacén se borran los documentos caducados y los datos que ya no usa ningún documento; `almacen purgar` hace solo eso.
- **Olvidar y vaciar.** `almacen olvidar <documento>` borra un documento y los datos que no usa ningún otro. `almacen vaciar` lo borra todo y pide `--confirmar`.
- **Diccionarios.** `almacen exportar` saca a un JSON **sin cifrar** los datos de un documento, para que otra persona pueda desanonimizarlo. `almacen importar` guarda el diccionario recibido aparte de los datos propios. Si un marcador tiene valores distintos en los dos sitios, `desanonimizar` no elige: pide `--fuente`.
- **Escritura segura.** Cuando una orden cambia el almacén, al terminar lo reescribe entero en un archivo temporal que se renombra al final, y se conserva una copia `.bak` del anterior hasta que la nueva escritura termina. Si el archivo no se puede descifrar (otra clave, archivo alterado), la orden se detiene con un error y no lo borra.

**La CLI no debe usarse con la app abierta**: las dos escriben en el mismo almacén.

## Pruebas

```bash
# Rust: núcleo, CLI y configuración de seguridad de Tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --include-ignored   # las ignoradas necesitan recursos/

# Tiempos de un DOCX y un PDF de diez páginas y de un almacén con 1.000 documentos
cargo test --release -p nucleo --test rendimiento -- --include-ignored --nocapture --test-threads=1

# Volver a generar el corpus sintético de Word, de PowerPoint y Excel y de PDF (este necesita PDFium)
cargo run -p nucleo --example generar_corpus_docx
cargo run -p nucleo --example generar_corpus_office
cargo run -p nucleo --example generar_corpus_pdf

# Scripts de descarga y garantía "sin red" por plataforma
node --test "scripts/test/*.test.mjs"

# Interfaz
cd app && npm run check && npm test && npm run build
cd app && npm run tauri build -- --debug --no-bundle
```

Si las pruebas que cargan el modelo fallan con `bad allocation`, es que varias lo han cargado a la vez y no ha quedado memoria: repítelas con `-- --test-threads=1`.

Las pruebas de la CLI no tocan el almacén de credenciales del sistema: dan la clave en la variable `ANONIMIZADOR_CLAVE_PRUEBA` (64 cifras hexadecimales), que solo aceptan las compilaciones de depuración.

Cobertura:

```bash
cargo llvm-cov --workspace -- --include-ignored          # requiere cargo-llvm-cov
node --test --experimental-test-coverage "scripts/test/*.test.mjs"
cd app && npm run test:coverage
```

## Reglas del proyecto

- `nucleo` no depende de Tauri.
- Ningún crate ni paquete con acceso a red.
- Identificadores en inglés. Interfaz, mensajes, comentarios y documentación en español.
- Solo documentos sintéticos. Nunca datos reales.
