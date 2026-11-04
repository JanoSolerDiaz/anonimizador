# Especificación: anonimizador local de documentos

2026-10-22 · Jano

## Contexto y objetivo

App de escritorio para Windows y macOS que anonimiza documentos Word, PowerPoint, Excel y PDF con un modelo de IA local, y los desanonimiza después. Los datos originales se guardan en un almacén cifrado en el propio equipo. Ningún dato sale del equipo.

La instala el departamento de sistemas en los equipos de la empresa. La usan personas no técnicas. Debe ser rápida de abrir, rápida de usar y difícil de usar mal.

Flujo principal:

1. **Anonimizar.** El usuario suelta uno o varios archivos y elige el formato de entrega. Recibe los documentos anonimizados. La correspondencia entre marcadores y datos originales queda en el almacén.
2. **Desanonimizar.** El usuario suelta documentos o pega texto. La app restaura los datos originales desde el almacén. No hay que buscar ni elegir ningún diccionario.

Modelo: [bardsai/eu-pii-anonimization-multilang](https://huggingface.co/bardsai/eu-pii-anonimization-multilang). Clasificador de tokens basado en XLM-RoBERTa base, unos 0,3B parámetros, 36 clases de entidad con etiquetas BIO, 24 idiomas, licencia Apache 2.0. Incluye `onnx/model_quantized.onnx` (INT8) pensado para CPU.

## Alcance de la primera versión

La v1 cubre DOCX, PPTX, XLSX y PDF digital de entrada, con salida en el mismo formato o en un formato de texto. La salida PDF conserva la maquetación rasterizando solo las páginas con datos personales.

| Entrada | Salidas de anonimización | Desanonimización |
| --- | --- | --- |
| DOCX | DOCX (formato original), MD, TXT | Sí |
| PPTX | PPTX (formato original), MD, TXT | Sí |
| XLSX | XLSX (formato original), MD, TXT | Sí |
| DOC, PPT, XLS (Office 97-2003) | Ninguna. Se rechazan con un aviso que pide guardarlos como DOCX, PPTX o XLSX | No |
| PDF digital | PDF (maquetación conservada), DOCX (solo texto), MD, TXT | No. Se desanonimiza la versión DOCX, MD o TXT |
| MD, TXT | MD, TXT | Sí |
| Texto pegado | No en v1 | Sí. El resultado se copia al portapapeles |
| PDF escaneado | Ninguna en v1. Se detecta y se rechaza con aviso | No |

Fuera de alcance en v1:

- OCR de PDF escaneados.
- Pantalla de revisión manual de entidades antes de exportar.
- Imágenes, formatos de Office 97-2003 y otros formatos.
- Detección de datos dentro de imágenes incrustadas. Se avisa, no se procesa.
- Capa de texto invisible en las páginas PDF rasterizadas.
- Opción "anonimizar sin vincular": marcadores aislados para un documento concreto, que no se relacionen con los de otros documentos.
- Sincronización del almacén entre equipos o usuarios.

El diseño debe permitir añadir estas piezas después sin rehacer el núcleo.

## Experiencia de usuario

Una sola ventana con dos pestañas: **Anonimizar** y **Desanonimizar**. Todo se hace arrastrando archivos y pulsando un botón. Interfaz en español.

### Anonimizar

1. El usuario suelta uno o varios archivos en la zona de arrastre, o los elige con un diálogo.
2. La lista muestra cada archivo con su tipo detectado (DOCX, PDF digital, PDF escaneado, no admitido).
3. El usuario elige el formato de entrega. Un selector global con las opciones válidas, y posibilidad de cambiarlo por archivo.
4. El usuario elige la carpeta de salida. Por defecto, la última usada.
5. Pulsa **Anonimizar**. Cada archivo muestra su progreso y su estado.
6. Al terminar: resumen por archivo (entidades encontradas por tipo, avisos) y botón **Abrir carpeta**.

### Desanonimizar

1. El usuario suelta uno o varios documentos (DOCX, MD, TXT), o pega texto en un cuadro.
2. Pulsa **Desanonimizar**. La app busca cada marcador en el almacén. No hay que emparejar nada.
3. Archivos: se guardan en la carpeta de salida, con el mismo progreso y resumen que al anonimizar. Texto pegado: el resultado aparece en el cuadro, con un botón **Copiar**.
4. El resumen indica cuántos marcadores se restauraron y lista los que no están en el almacén (caducados, olvidados, de otro equipo o alterados).

### Nombres de archivo

Formato `descripción_sufijo.ext`. La descripción sale del nombre del archivo que se procesa, en minúsculas, sin acentos y con un guion en lugar de espacios y signos. El sufijo indica la operación. El diccionario toma la descripción del documento anonimizado.

- `Contrato cliente.docx` → `contrato-cliente_anonimizado.docx`
- `contrato-cliente_anonimizado.docx` → `contrato-cliente-anonimizado_desanonimizado.docx`
- `contrato-cliente-anonimizado_diccionario.json`, solo si el usuario lo exporta

Si el archivo ya existe, se añade un sufijo `_2`, `_3`. Nunca se sobrescribe nada.

### Principios

- La app nunca modifica el archivo original.
- Los avisos se explican en una frase, sin jerga.
- Un error en un archivo no detiene el resto del lote.

### Almacén

Un panel secundario, accesible desde un icono, con la lista de documentos anonimizados: nombre, fecha, número de entradas y fecha de caducidad. Acciones:

- **Exportar diccionario** de un documento, para que un compañero pueda desanonimizarlo.
- **Importar diccionario** recibido de otra persona. Se guarda en el almacén, aparte de las entradas propias.
- **Olvidar** un documento: borra sus entradas que no usen otros documentos.
- **Vaciar todo**, con confirmación.

El usuario no necesita abrir este panel en el uso normal.

## Arquitectura

Cada formato tiene un extractor y un escritor. Entre ambos, un único motor de anonimización que solo trabaja con texto. Añadir un formato nuevo consiste en escribir esas dos piezas.

```
      DOCX                  PDF digital               MD · TXT
        │                        │                        │
        ▼                        ▼                        ▼
 ┌────────────────┐      ┌────────────────┐      ┌────────────────┐
 │ Extractor DOCX │      │ Extractor PDF  │      │Extractor texto │
 │ texto + mapa   │      │ texto + mapa   │      │ texto +        │
 │ de fragmentos  │      │ de cajas       │      │ posiciones     │
 └───────┬────────┘      └───────┬────────┘      └───────┬────────┘
         │                       │                       │
         ▼                       ▼                       ▼
 ┌──────────────────────────────────────────────────────────────────┐
 │                          Motor común                             │
 │      troceado · modelo ONNX · reglas deterministas · marcadores  │
 │              consulta y actualiza el almacén cifrado             │
 └───────┬───────────────────────┬───────────────────────┬──────────┘
         ▼                       ▼                       ▼
 ┌────────────────┐      ┌────────────────┐      ┌────────────────┐
 │ Escritor DOCX  │      │ Escritor PDF   │      │ Escritor texto │
 │ edita el XML   │      │ rasteriza pág. │      │ genera MD, TXT │
 │ original y     │      │ con datos y    │      │ o un DOCX nuevo│
 │ conserva el    │      │ verifica la    │      │ desde el texto │
 │ formato        │      │ salida         │      │                │
 └───────┬────────┘      └───────┬────────┘      └───────┬────────┘
         ▼                       ▼                       ▼
 ┌──────────────────────────────────────────────────────────────────┐
 │  Documento anonimizado por archivo + entradas nuevas en el almacén│
 └──────────────────────────────────────────────────────────────────┘
```

El extractor devuelve el texto y un mapa que indica de dónde viene cada carácter en el original. El escritor usa ese mapa para aplicar los cambios sobre el archivo original, no sobre una copia regenerada. La desanonimización reutiliza los extractores y escritores de DOCX y texto; el motor se sustituye por la búsqueda de marcadores.

La interfaz Svelte solo envía rutas y opciones a los comandos de Tauri, y recibe eventos de progreso. Todo el procesamiento ocurre en el crate `nucleo`, en Rust.

## Stack tecnológico

Tauri 2 con núcleo en Rust y la interfaz en Svelte 5 con TypeScript. Todo el trabajo pesado ocurre en Rust. La interfaz solo envía rutas y recibe progreso.

| Pieza | Elección | Razón |
| --- | --- | --- |
| Contenedor de escritorio | Tauri 2 | Instaladores pequeños, poco consumo de memoria, MSI y DMG/PKG de serie, permite bloquear el acceso a red |
| Interfaz | Svelte 5 + TypeScript + Vite | Compila a JavaScript mínimo, sin DOM virtual. Arranque y respuesta muy rápidos en el webview de Windows (WebView2) y de Mac (WKWebView) |
| Estilos | CSS propio con variables | Sin librerías de componentes. Menos peso, control total |
| Inferencia | crate `ort` (ONNX Runtime) | Ejecuta el modelo ONNX cuantizado en CPU, nativo, sin Python |
| Tokenización | crate `tokenizers` (Hugging Face) | Lee el `tokenizer.json` del modelo tal cual. Mismos tokens que en entrenamiento |
| DOCX | crates `zip` + `quick-xml` | Edición en el XML original, conservando todo el formato |
| DOCX nuevo (desde PDF) | crate `docx-rs` | Genera un DOCX limpio a partir de texto |
| PDF | crate `pdfium-render` + binarios PDFium | Extracción de texto con posiciones, renderizado de páginas y escritura de PDF. Licencia permisiva |
| Dibujo sobre imagen | crates `image` + `imageproc` | Tapar las zonas con datos en la página rasterizada |
| Reglas | crate `regex` | Patrones de DNI, NIE, IBAN, email, teléfono, tarjeta |
| Almacén | `serde` + `serde_json` en un único archivo | Sin base de datos. Se carga en memoria y se reescribe entero al guardar |
| Cifrado del almacén | crate `aes-gcm` (AES-256-GCM) | Cifrado autenticado estándar. Detecta archivos alterados |
| Clave del almacén | crate `keyring` | Guarda la clave en el Administrador de credenciales de Windows y en el Llavero de Mac |
| Instancia única | plugin `single-instance` de Tauri | Evita que dos ventanas escriban el almacén a la vez |
| Huellas | crate `sha2` | Huella de los documentos de origen y salida |
| Concurrencia | `tokio` + pool de hilos | Varios archivos en paralelo sin bloquear la interfaz |

Por qué Svelte frente a las alternativas:

- **Frente a React**: React añade su runtime y un DOM virtual. Para una interfaz de dos pestañas no aporta nada y pesa más.
- **Frente a Vanilla TypeScript**: el rendimiento es casi igual, pero Svelte da reactividad y componentes sin escribir código de sincronización a mano. Menos errores en la lista de archivos y los estados de progreso.

Versiones: usar las estables más recientes de cada crate y paquete en el momento de crear el proyecto. Fijar versiones exactas en `Cargo.lock` y `package-lock.json`.

## Motor de anonimización

El motor recibe texto plano y devuelve una lista de entidades con su posición en caracteres. No sabe nada de formatos de archivo.

### Entrada y salida

- Entrada: texto (`String`) dividido en segmentos con identificador (un párrafo de DOCX, una página de PDF).
- Salida: lista de entidades `{segmento, inicio, fin, tipo, texto, puntuación, origen}`. `origen` vale `modelo` o `regla`.

### Troceado

El modelo procesa como máximo 512 tokens por pasada.

1. Tokenizar cada segmento con offsets de carácter.
2. Si supera el límite, dividir en ventanas de 512 tokens con solapamiento de 128.
3. En el solapamiento, quedarse con la predicción de la ventana donde el token está más lejos del borde.
4. Agrupar ventanas de varios segmentos cortos en un mismo lote para aprovechar la CPU.

### Decodificación

1. Aplicar softmax a los logits y tomar la etiqueta de mayor probabilidad por token.
2. Reconstruir entidades con el esquema BIO. Un `I-` sin `B-` previo del mismo tipo abre una entidad nueva.
3. Unir subtokens de una misma palabra. La etiqueta de la palabra es la de su primer subtoken.
4. Descartar entidades con puntuación media por debajo del umbral. Umbral configurable, por defecto 0,5.
5. Ajustar los bordes: quitar espacios y signos de puntuación al principio y al final.

Las 36 etiquetas se leen de `config.json` (`id2label`). No inventar etiquetas. Generar a partir de ese archivo una tabla de correspondencia etiqueta → tipo de marcador en español (por ejemplo `PERSONA`, `EMAIL`, `TELEFONO`, `DIRECCION`, `DNI`, `IBAN`, `SALUD`). La tabla vive en un archivo de configuración versionado.

### Reglas deterministas

Se ejecutan siempre, junto al modelo. Cada regla valida el dato, no solo su forma:

- DNI: 8 dígitos y letra de control correcta.
- NIE: X/Y/Z, 7 dígitos y letra de control correcta.
- IBAN: estructura por país y comprobación módulo 97.
- Tarjeta: 13 a 19 dígitos y algoritmo de Luhn.
- Email: patrón estándar.
- Teléfono: formatos españoles e internacionales con prefijo.

Si una regla y el modelo se solapan, gana la regla en tipo y bordes. Si dos entidades del modelo se solapan, gana la de mayor puntuación.

### Marcadores

- Formato: `[TIPO_NNNN]`, con cuatro dígitos como mínimo. Ejemplo: `[PERSONA_0042]`, `[EMAIL_0007]`.
- Numeración por tipo, única para todo el almacén del usuario. No se reinicia en cada documento.
- El mismo valor recibe siempre el mismo marcador, en todos los documentos del usuario. Antes de crear un marcador nuevo, se busca el valor normalizado en el almacén.
- Así, un texto que mezcla varios documentos anonimizados (por ejemplo, la respuesta de un LLM que los compara) se restaura sin ambigüedad.
- Normalización para decidir si dos valores son el mismo: minúsculas, sin acentos, espacios colapsados. "JUAN PÉREZ" y "Juan Perez" comparten marcador. El almacén guarda todas las variantes literales encontradas.
- El texto del marcador es fijo. No depende del idioma del documento.

## Formato DOCX

El DOCX anonimizado es el archivo original con el texto sustituido dentro de su XML. No se regenera el documento.

### Partes que se procesan

Todas las partes del ZIP que contienen texto visible o recuperable:

- `word/document.xml`, incluidas tablas y cuadros de texto (`w:txbxContent`).
- `word/header*.xml` y `word/footer*.xml`.
- `word/footnotes.xml` y `word/endnotes.xml`.
- `word/comments.xml`.
- Texto borrado del control de cambios (`w:delText`). Se anonimiza igual que el texto normal.
- Enlaces en `word/_rels/*.rels` cuyo destino contenga una entidad (por ejemplo `mailto:`).
- Metadatos en `docProps/core.xml` y `docProps/app.xml`: se vacían `dc:creator`, `cp:lastModifiedBy` y `Company`.

### Extracción con mapa

1. Por cada párrafo (`w:p`), concatenar el texto de sus fragmentos (`w:t`, `w:delText`) en orden.
2. Guardar un mapa: para cada carácter del texto, qué fragmento y qué posición dentro de él.
3. Tratar tabulaciones (`w:tab`) y saltos (`w:br`) como un espacio en el texto, sin entrada editable en el mapa.
4. Enviar cada párrafo al motor como un segmento.

### Escritura

Por cada entidad del párrafo, de la última a la primera para no desplazar posiciones:

1. Localizar con el mapa los fragmentos que cubre.
2. En el primer fragmento, sustituir la parte afectada por el marcador.
3. En los siguientes, eliminar la parte afectada.
4. Añadir `xml:space="preserve"` a los `w:t` modificados que empiecen o acaben en espacio.

El marcador hereda el formato del primer fragmento. Los fragmentos vacíos se pueden dejar.

### Avisos

- Imágenes incrustadas: "El documento tiene N imágenes. Su contenido no se ha revisado."
- Objetos incrustados (hojas de Excel, otros documentos): mismo aviso.
- Documento protegido con contraseña: error claro, no se procesa.

### Salida MD y TXT desde DOCX

Se genera desde el texto extraído y ya anonimizado. MD conserva títulos, listas y tablas básicas. TXT solo el texto, un párrafo por línea.

## Formato PPTX

Como en DOCX, la presentación anonimizada es el archivo original con el texto sustituido dentro de su XML. Cada párrafo de DrawingML (`a:p`) es un segmento, y sus fragmentos son los `a:t`; un salto de línea (`a:br`) cuenta como un espacio.

### Partes que se procesan

- Diapositivas, en el orden de la presentación, cada una seguida de sus diagramas (SmartArt), sus gráficos y sus notas.
- Patrones y diseños de diapositiva, de notas y de documentos: pueden llevar un pie con un nombre.
- Comentarios, antiguos y modernos. Los nombres, iniciales e identificadores de sus autores se vacían.
- Valores en caché de los gráficos, texto alternativo de imágenes y formas, destinos de enlaces y títulos de `docProps/app.xml`: sustitución literal de los valores detectados.
- Metadatos como en DOCX.

### Salida MD y TXT desde PPTX

Una sección por diapositiva («Diapositiva N»), con el título como encabezado, las tablas como tablas y las notas al final. Plantillas y comentarios no salen.

## Formato XLSX

El libro anonimizado es el archivo original con los valores sustituidos dentro de su XML.

### Qué se analiza

- Cada texto distinto es un segmento: los textos compartidos, los textos en línea de una celda y los resultados de texto de las fórmulas. Van hoja a hoja y fila a fila, para que el modelo lea juntas las celdas de una fila.
- Las celdas numéricas también se analizan, tal como se ven: un teléfono o un documento guardado como número, una fecha (`14/02/2025`, no el número de serie). Un número sustituido pasa a ser texto con el marcador. **Los importes no se sustituyen por defecto en Excel**; se puede pedir que sí.
- Los nombres de hoja. Si llevan un dato, la hoja se renombra con el marcador sin corchetes (Excel no admite corchetes en un nombre de hoja), como mucho 31 caracteres y sin repetir otro nombre. El nombre nuevo se cambia en fórmulas, nombres definidos, gráficos, tablas dinámicas, vínculos y `docProps/app.xml`. Al desanonimizar se recupera.
- Comentarios, antiguos y modernos, y encabezados y pies de página. Los autores y las personas de los comentarios se vacían.
- Sustitución literal de los valores detectados dentro de las comillas de las fórmulas, en listas de validación, en las cachés de gráficos y tablas dinámicas, en los nombres de columna de las tablas y en los vínculos.

### Salida MD y TXT desde XLSX

Una sección por hoja con sus celdas: en MD, una tabla con la primera fila como cabecera; en TXT, una línea por fila con las celdas separadas por tabuladores.

## Formato PDF

El PDF anonimizado conserva intactas las páginas sin datos personales. Las páginas con datos se convierten en imagen, se tapan los datos y se escriben los marcadores encima como texto. Así el dato original desaparece del archivo.

### Preparación

1. Abrir con PDFium. Si tiene contraseña: error claro, no se procesa.
2. Aplanar formularios y anotaciones en cada página. Su texto pasa a formar parte de la página y se extrae como el resto.
3. Detectar páginas sin texto extraíble. Si alguna página no tiene texto, el documento se trata como escaneado: se rechaza con el aviso "Este PDF contiene páginas escaneadas. La v1 no puede anonimizarlas."

### Extracción con mapa

1. Por cada página, obtener los caracteres con su caja delimitadora.
2. Construir el texto de la página y un mapa carácter → caja.
3. Enviar cada página al motor como un segmento.
4. Unir palabras partidas con guion al final de línea antes de enviarlas al motor, manteniendo el mapa a las cajas originales.

### Escritura

Por cada página con al menos una entidad:

1. Convertir cada entidad en rectángulos: uno por línea que ocupa, unión de las cajas de sus caracteres, con 1 pt de margen.
2. Renderizar la página a imagen. Resolución por defecto 200 ppp, configurable entre 150 y 300.
3. Pintar los rectángulos en blanco opaco sobre la imagen, con un borde gris fino.
4. Crear una página nueva del mismo tamaño con la imagen a página completa.
5. Escribir encima cada marcador como texto vectorial, centrado en su rectángulo.
6. Sustituir la página original por la nueva.

Tamaño del marcador: el mayor que quepa en el rectángulo, hasta el 70 % de su alto. Si a 5 pt no cabe, usar una forma corta: la inicial del tipo y un número propio de esa inicial, por ejemplo `[P42]`. Las iniciales se repiten entre tipos, así que el número no es el del marcador: el almacén lleva un contador por inicial (`contadores_cortos`). La forma corta se guarda en el almacén junto al marcador completo y no se reutiliza nunca.

### Limpieza del documento

- Vaciar metadatos: diccionario `Info` y metadatos XMP.
- Eliminar marcadores de navegación (outline). Pueden contener nombres.
- Eliminar archivos adjuntos incrustados, con aviso.

### Verificación obligatoria

Después de escribir el PDF, volver a extraer todo su texto y buscar cada valor original detectado en el documento. Si aparece alguno, el archivo se marca como fallido, se borra la salida y se informa al usuario. Esta comprobación no se puede desactivar.

### Salida DOCX, MD y TXT desde PDF

Se genera desde el texto extraído y ya anonimizado, sin maquetación. Un párrafo por bloque de texto detectado por PDFium. DOCX se crea con `docx-rs`.

## Almacén y desanonimización

Toda la correspondencia entre marcadores y datos originales vive en un único archivo cifrado por usuario. El usuario no lo maneja en el uso normal.

### Archivo del almacén

- Ubicación: carpeta de datos de la app del usuario (`%APPDATA%` en Windows, `~/Library/Application Support` en Mac).
- Cifrado AES-256-GCM. Clave aleatoria de 256 bits generada en el primer arranque y guardada con `keyring` en el almacén de credenciales del sistema.
- Se carga en memoria al abrir la app y se reescribe entero al terminar cada lote. Escritura atómica: archivo temporal y renombrado. Se conserva una copia `.bak` del anterior hasta que la nueva escritura termina.
- Si falta la clave o el archivo no se puede descifrar: aviso claro y opción de empezar un almacén nuevo. El archivo antiguo no se borra sin confirmación.
- Nunca se registra en logs ni se copia a temporales.

### Estructura, una vez descifrado

```json
{
  "formato": "anonimizador-almacen",
  "version_formato": 1,
  "contadores": { "PERSONA": 42, "EMAIL": 7 },
  "contadores_cortos": { "P": 42 },
  "entradas": [
    {
      "marcador": "[PERSONA_0042]",
      "forma_corta": "[P42]",
      "tipo": "PERSONA",
      "etiqueta_modelo": "<etiqueta de config.json>",
      "valor": "Juan Pérez",
      "clave_normalizada": "juan perez",
      "variantes": ["Juan Pérez", "JUAN PÉREZ"],
      "documentos": ["b7c1e2a4-..."]
    }
  ],
  "documentos": [
    {
      "id": "b7c1e2a4-...",
      "creado": "2026-10-22T10:14:03+02:00",
      "caduca": "2027-01-20",
      "version_app": "1.0.0",
      "modelo": "bardsai/eu-pii-anonimization-multilang@<revisión>",
      "origen": { "nombre": "Contrato cliente.pdf", "sha256": "..." },
      "salida": { "nombre": "contrato-cliente_anonimizado.pdf", "formato": "pdf" },
      "marcadores": ["[PERSONA_0042]"]
    }
  ],
  "importados": []
}
```

`forma_corta` solo existe si se usó en un PDF, y `contadores_cortos` solo si se usó alguna. `valor` es la forma de la primera aparición.

### Caducidad

- Cada documento caduca a los 90 días por defecto.
- Sistemas puede fijar otro plazo por política: clave de registro en Windows, perfil de configuración en Mac. Si hay política, el usuario no puede cambiarlo.
- Al abrir la app, y una vez al día si sigue abierta, se borran los documentos caducados y las entradas que ya no usa ningún documento.

### Exportar e importar

- **Exportar** genera `descripción_diccionario.json` con las entradas de un solo documento. En v1 va sin cifrar, con un aviso de que contiene datos personales.
- **Importar** guarda el diccionario recibido en `importados`, separado de las entradas propias. Los números de marcador de otra persona no coinciden con los propios, así que no se mezclan.

### Desanonimización

Entradas admitidas: DOCX, PPTX, XLSX, MD, TXT y texto pegado. Salida: el mismo formato que la entrada. En los nombres de hoja de Excel se buscan también los marcadores sin corchetes.

1. Buscar marcadores con una expresión tolerante: mayúsculas o minúsculas, espacios dentro de los corchetes, y formas cortas. Un LLM puede haberlos alterado ligeramente.
2. En DOCX, buscar sobre el texto reconstruido con el mapa de fragmentos. Un marcador puede estar partido en varios fragmentos si alguien editó el documento.
3. Buscar cada marcador en las entradas propias y en los diccionarios importados.
4. Si un mismo marcador tiene valores distintos en dos fuentes, preguntar al usuario qué fuente usar para ese documento. Es el único caso en que el usuario elige algo.
5. Sustituir cada marcador por su `valor`.
6. Informar en el resumen: marcadores restaurados y marcadores que no están en ninguna fuente.

## Seguridad y privacidad

La app no se conecta a internet en ningún momento. Es una garantía técnica, no solo una promesa: el código no tiene forma de hacerlo.

- **Sin red.** CSP del webview con `connect-src 'self' ipc:` y sin dominios externos. Ningún crate con cliente HTTP en el binario final, salvo el actualizador si se activa.
- **Modelo incluido.** El modelo y el tokenizador van dentro del instalador. Nunca se descargan en ejecución.
- **Permisos de Tauri mínimos.** Solo los comandos propios de la app y los plugins de diálogo, arrastre e instancia única. Sin acceso libre al sistema de archivos desde la interfaz: la interfaz envía rutas y Rust valida que sean archivos regulares legibles.
- **Almacén cifrado.** AES-256-GCM con la clave en el almacén de credenciales del sistema. Solo ese usuario, en ese equipo, puede leerlo. Caducidad de 90 días por defecto, configurable por sistemas.
- **Sin telemetría.** Ni de uso ni de errores.
- **Logs sin datos.** Los logs registran nombres de archivo, tiempos, número de entidades por tipo y errores. Nunca texto del documento ni valores del almacén.
- **Sin temporales con datos.** El procesamiento ocurre en memoria. Si un paso necesita disco (por ejemplo, PDFium), se usa un directorio temporal propio que se borra al terminar cada archivo, también si hay error.
- **Escritura atómica.** Cada salida, y el almacén, se escriben en un archivo temporal y se renombran al final. Si falla, no quedan archivos a medias.
- **El original no se toca.** Se abre solo en lectura.

Aviso visible al exportar un diccionario: "Este archivo contiene los datos originales sin cifrar. Envíalo solo a quien deba desanonimizar el documento, y nunca junto al documento anonimizado."

## Empaquetado y despliegue

Un instalador por plataforma, firmado, que funciona sin conexión y que sistemas puede desplegar de forma masiva.

| Plataforma | Instalador | Requisitos |
| --- | --- | --- |
| Windows 10/11 x64 | MSI (WiX, generado por Tauri) | Firma con certificado de código. WebView2 en modo instalador sin conexión (`offlineInstaller`) para equipos sin internet |
| macOS 12 o superior | DMG y PKG, binario universal (Apple Silicon e Intel) | Firma con Developer ID y notarización. Sin esto, Gatekeeper bloquea la app |

Recursos incluidos en el paquete:

- `model_quantized.onnx`, `config.json`, `tokenizer.json` y `tokenizer_config.json` del modelo, en una revisión fija.
- Biblioteca dinámica de ONNX Runtime para cada plataforma.
- Biblioteca dinámica de PDFium para cada plataforma (binarios precompilados de `bblanchon/pdfium-binaries`).
- Archivos de licencia de todo lo anterior.

El modelo no se guarda en git. Un script `scripts/descargar-modelo` lo descarga en una revisión fija y comprueba su SHA-256 antes de compilar.

Despliegue: MSI por Intune o GPO; PKG por Jamf o Intune. Instalación silenciosa sin preguntas.

Actualizaciones: en v1, nuevas versiones del instalador distribuidas por sistemas. El actualizador de Tauri queda desactivado.

## Rendimiento

La ventana debe aparecer al instante y el modelo cargarse sin que el usuario lo note. Las cifras de abajo son objetivos iniciales: se miden en el hito 2 y se ajustan con datos reales.

Equipo de referencia: portátil corporativo de gama media, 4 núcleos, 8 GB de RAM, sin GPU.

| Medida | Objetivo |
| --- | --- |
| Ventana visible desde el doble clic | Menos de 2 s |
| Modelo cargado, en segundo plano | Menos de 3 s |
| DOCX de 10 páginas | Menos de 5 s |
| PDF de 10 páginas, con datos en todas | Menos de 15 s |
| Memoria máxima durante un lote | Menos de 1 GB |

Estrategia:

- Cargar el modelo una sola vez, en segundo plano, al abrir la app. La interfaz es usable mientras tanto; el botón de procesar espera a que esté listo.
- Una única sesión de ONNX Runtime compartida. Hilos internos: la mitad de los núcleos.
- Hasta 2 archivos en paralelo. El resto en cola.
- Agrupar ventanas de texto en lotes para cada pasada del modelo.
- En PDF, renderizar solo las páginas con entidades.
- Enviar eventos de progreso a la interfaz como mucho cada 100 ms por archivo.

## Pruebas y criterios de aceptación

La v1 está terminada cuando pasan todas estas pruebas en Windows y en macOS.

### Corpus de prueba

Carpeta `tests/corpus` con documentos sintéticos, con datos personales inventados. Nunca documentos reales.

- DOCX con nombres partidos en varios fragmentos de formato, tablas, encabezados, pies, notas al pie, comentarios, control de cambios y metadatos de autor.
- PPTX con títulos, nombres partidos, saltos de línea, tablas, notas, comentarios, texto alternativo, enlaces y un patrón con un nombre.
- XLSX con una hoja con nombre de persona, textos partidos, números (teléfonos, fechas, importes), resultados de fórmula, nombres definidos, listas de validación, vínculos y pies de página.
- PDF digital de una y de varias páginas, con datos que cruzan de línea, palabras con guion al final de línea, formularios rellenos y anotaciones.
- PDF escaneado (solo imagen).
- DOCX y PDF protegidos con contraseña.
- Documentos en español, catalán, inglés y francés.

Cada documento tiene al lado un JSON con las entidades esperadas.

### Pruebas automáticas

- [ ] Reglas: DNI, NIE, IBAN y tarjeta válidos se detectan; los de control incorrecto no.
- [ ] Troceado: un texto de más de 512 tokens con una entidad en el solapamiento la detecta una sola vez.
- [ ] Marcadores: el mismo valor con distintas mayúsculas y acentos recibe el mismo marcador.
- [ ] Marcadores entre documentos: el mismo valor en dos documentos distintos recibe el mismo marcador.
- [ ] Texto mezclado: un texto con marcadores de tres documentos distintos se restaura completo.
- [ ] DOCX ida y vuelta: anonimizar y desanonimizar devuelve exactamente el texto original en todas las partes procesadas.
- [ ] DOCX: el XML de salida es válido y Word lo abre sin errores de reparación.
- [ ] DOCX: los metadatos de autor quedan vacíos.
- [ ] PPTX y XLSX ida y vuelta: anonimizar y desanonimizar devuelve exactamente el texto original, también los nombres de hoja.
- [ ] PPTX y XLSX: ninguna parte del paquete de salida contiene un valor detectado, y PowerPoint y Excel lo abren sin errores de reparación.
- [ ] XLSX: los importes se quedan por defecto y se sustituyen si se pide.
- [ ] DOC, PPT y XLS: se rechazan con el aviso previsto.
- [ ] PDF: el texto extraído del PDF de salida no contiene ningún valor original detectado.
- [ ] PDF: las páginas sin entidades son idénticas a las originales.
- [ ] PDF escaneado: se rechaza con el aviso previsto.
- [ ] Contraseña: DOCX y PDF protegidos dan error claro y el lote continúa.
- [ ] Desanonimización tolerante: `[persona_0042]` y `[ PERSONA_0042 ]` se restauran.
- [ ] Almacén: el archivo en disco no contiene ningún valor legible, y no se descifra con otra clave.
- [ ] Almacén: los documentos caducados y las entradas sin uso se borran.
- [ ] Almacén: si la escritura se interrumpe, el almacén anterior sigue intacto.
- [ ] Importar: un marcador con valores distintos en el almacén propio y en un diccionario importado provoca la pregunta al usuario.
- [ ] Nombres de salida siguen `descripción_sufijo.ext` y nunca sobrescriben.
- [ ] Ningún log contiene texto de documentos ni valores del almacén.

### Calidad de detección

Medir precisión y exhaustividad por tipo de entidad sobre el corpus. Informe en `tests/informe-deteccion.md`. No hay cifra mínima en v1: el informe sirve para ajustar umbral y reglas.

### Pruebas manuales

- [ ] Instalación silenciosa del MSI en un Windows sin conexión.
- [ ] Instalación del PKG en un Mac, sin avisos de Gatekeeper.
- [ ] La app funciona con el equipo desconectado de la red.
- [ ] Lote de 20 archivos mezclados con uno corrupto: los 19 válidos se procesan.
- [ ] Objetivos de rendimiento medidos en el equipo de referencia.

## Plan de implementación e instrucciones para Claude Code

Se construye en seis hitos. Cada hito termina con sus pruebas en verde antes de empezar el siguiente.

### Estructura del repositorio

```
anonimizador/
├─ nucleo/              # crate Rust sin dependencia de Tauri
│  ├─ src/motor/        # tokenización, inferencia, troceado, BIO, reglas, marcadores
│  ├─ src/formatos/     # docx/, pdf/, texto/ — extractor + escritor por formato
│  ├─ src/almacen/      # cifrado, caducidad, exportar e importar
│  └─ src/bin/cli.rs    # CLI para pruebas y mediciones
├─ app/
│  ├─ src-tauri/        # comandos Tauri que llaman a nucleo
│  └─ src/              # interfaz Svelte 5 + TypeScript
├─ recursos/            # modelo, ONNX Runtime, PDFium (no en git)
├─ scripts/             # descargar-modelo, descargar-binarios
├─ tests/corpus/        # documentos sintéticos + entidades esperadas
└─ DECISIONES.md        # cambios respecto a esta especificación
```

### Hitos

1. **Esqueleto.** Proyecto Tauri 2 + Svelte 5 que abre una ventana vacía. Crate `nucleo`. Scripts de descarga del modelo y de binarios con comprobación SHA-256. Compilación en Windows y macOS.
2. **Motor.** Tokenización, inferencia ONNX, troceado, decodificación BIO, reglas y marcadores. CLI: `cli entidades archivo.txt` imprime las entidades en JSON. Medir tiempos frente a los objetivos de rendimiento.
3. **DOCX y almacén.** Extractor y escritor DOCX con mapa de fragmentos. Almacén cifrado con caducidad, exportar e importar. Desanonimización de DOCX, MD, TXT y texto. Salidas MD y TXT. CLI: `cli anonimizar` y `cli desanonimizar`. Añadido después del cierre (2026-11-02): PPTX y XLSX con la misma base, de entrada y de salida.
4. **PDF.** Preparación, detección de escaneados, extracción con mapa, rasterización, marcadores, limpieza y verificación obligatoria. Salidas DOCX, MD y TXT desde PDF.
5. **Interfaz.** Las dos pestañas, arrastre, cola, progreso, texto pegado, panel del almacén, resúmenes y avisos. Comandos Tauri y eventos de progreso.
6. **Empaquetado.** MSI y PKG/DMG firmados, WebView2 sin conexión, binario universal en Mac. Pruebas manuales.

### Instrucciones para Claude Code

- Lee esta especificación entera antes de escribir código. Trabaja hito a hito.
- Antes de crear la tabla de tipos de marcador, lee `config.json` del modelo. No inventes etiquetas.
- Comprueba las versiones actuales de cada crate y paquete, y consulta su documentación en docs.rs o en su repositorio. No confíes en APIs de memoria, sobre todo en `ort`, `tokenizers` y `pdfium-render`.
- `nucleo` no depende de Tauri. Todo se prueba con `cargo test` y la CLI.
- No añadas ningún crate o paquete con acceso a red.
- Identificadores de código en inglés. Textos de interfaz, mensajes al usuario, comentarios y documentación en español.
- Usa solo documentos sintéticos. Nunca datos reales, tampoco en ejemplos.
- Si una decisión de esta especificación resulta inviable o claramente peor que otra opción, pregunta antes de cambiarla. Registra cada cambio aceptado en `DECISIONES.md` con su razón.
- Al terminar cada hito, resume qué se ha hecho, qué pruebas pasan y qué queda pendiente.
