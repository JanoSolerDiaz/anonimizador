# Plan: Anonimizador local — Hito 4 (PDF)

**Source PRD**: `specs/anonimizador-local.prd.md`, «Formato PDF»
**Selected Milestone**: Hito 4 — PDF
**Complexity**: Large

## Summary

Al terminar este hito, `cli anonimizar` acepta PDF digitales. Las páginas sin datos personales se copian tal cual. Las que tienen datos se convierten en imagen, se tapan los datos con rectángulos blancos y se escriben los marcadores encima como texto. Después se vuelve a leer el PDF entero y, si aparece algún valor original, la salida se borra y el archivo falla. Un PDF también puede salir en DOCX, MD o TXT, generados desde el texto extraído ya anonimizado.

## Datos de partida (comprobados el 2026-11-03 con `nucleo/examples/sonda_pdf.rs`)

- `pdfium-render 0.9.4` con PDFium chromium/7881, ya enlazado desde el hito 1.
- Cada carácter trae su caja (`loose_bounds`), si lo generó PDFium (`is_generated`: espacios y saltos `\r\n` añadidos) y si es guion de fin de línea (`is_hyphen`). Ese guion llega como `\u{2}` y PDFium no pone salto detrás: «Gonz\u{2}ález».
- Un A4 a 200 ppp son 1654×2339 píxeles. Insertado como mapa de bits, PDFium lo comprime al guardar: la página de la sonda ocupa 40 KB. No hace falta JPEG, que emborrona el texto.
- Un documento nuevo con páginas copiadas (`copy_page_from_document`) no arrastra el XMP, el outline ni los adjuntos del original. Sí lleva un `/Info` con el `Producer` que pone PDFium, sin datos del original.
- Un PDF dañado da `FormatError`; uno con contraseña, `PasswordError`.
- `docx-rs 0.4.22`, sin la función `image`: trae su propio `zip 8` y `quick-xml 0.41`, sin acceso a red.

## Diseño

```
 PDF ──► PdfDocument::open ──► aplanar ──► ¿página escaneada? ──► error
                │
                ▼
     por página: caracteres + cajas ──► segmento (texto con guiones unidos)
                │                                  │
                │                                  ▼
                │                         Detector + almacén (marcador,
                │                         forma corta si no cabe)
                ▼                                  │
   páginas sin entidades: copiar      páginas con entidades: renderizar,
                                      tapar, imagen + marcadores en texto
                └──────────────► documento nuevo ◄─┘
                                      │
                                      ▼
                    verificación: releer y buscar cada valor ──► falla y borra
```

Decisiones de diseño (se anotan en `DECISIONES.md`):

1. **Documento nuevo, no el original editado.** La salida se construye siempre desde un PDF vacío: las páginas sin datos se copian y las demás se crean. Así no pasan el `Info`, el XMP, el outline, los adjuntos, el JavaScript ni los formularios del original, sin tener que buscarlos uno a uno.
2. **Sustitución literal también en PDF.** Igual que en Office, un valor detectado en una página se busca en todas las demás (palabra entera) y se sustituye también allí. Si no, una sola aparición que el modelo no viera haría fallar la verificación de todo el archivo.
3. **Verificación con la misma regla.** Busca cada valor como palabra entera en el texto de la salida, con los espacios colapsados. `Ana` no hace fallar un documento con `Mariana`.
4. **Página escaneada = sin texto y con imagen.** Una página en blanco no tiene texto y no es un escaneo: se copia. Se rechaza el documento si alguna página no tiene ningún carácter visible pero sí al menos una imagen.
5. **Forma corta con contador propio por letra.** Las iniciales de los tipos chocan (N para `NOMBRE_PERSONA` y `NOMBRE_PROPIO`; C para ocho tipos). La forma corta es la inicial del tipo y un contador de esa letra que el almacén guarda en `contadores_cortos`: `[N1]`, `[N2]`… Se asigna la primera vez que un marcador no cabe y no se reutiliza nunca, como los números de los marcadores. El campo es nuevo y opcional: el formato del almacén sigue en la versión 1.
6. **Un marcador por entidad.** Una entidad que ocupa dos líneas da dos rectángulos. El marcador va en el más ancho; el otro queda en blanco.
7. **Bloques de texto para DOCX, MD y TXT.** Una línea sigue en el mismo párrafo si empieza justo debajo de la anterior (separación menor que 1,5 veces su alto). Un hueco mayor abre párrafo nuevo.
8. **Páginas giradas.** Se renderizan sin girar, en el mismo espacio que las cajas de los caracteres, y la página nueva recibe el mismo giro.

## Tasks

### Task 1 — Forma corta en el almacén
- `Store::short_form(marker) -> Result<String>`: devuelve la que tiene o asigna `[{inicial}{n}]` con `contadores_cortos`.
- Pruebas en `nucleo/tests/almacen_estructura.rs`: dos tipos con la misma inicial no comparten forma corta; se guarda y se relee; tras olvidar el documento, la siguiente no reutiliza el número; la desanonimización encuentra `[N2]`.

### Task 2 — Lectura del PDF (`formatos/pdf/lectura.rs`)
- `PdfDocument::from_bytes(pdfium, bytes)`: errores `PdfProtected`, `InvalidPdf`, `ScannedPdf`; aplana cada página; cuenta adjuntos (aviso).
- Por página: texto con `\n` en los saltos, guiones de fin de línea quitados y mapa carácter → caja; líneas y bloques.
- Pruebas con PDF sintéticos creados en la propia prueba con PDFium.

### Task 3 — Escritura y verificación (`formatos/pdf/escritura.rs`)
- Rectángulos por línea con 1 pt de margen; render a `ppp` (150–300, 200 por defecto); blanco opaco con borde gris; página nueva del mismo tamaño y giro; marcador centrado al mayor tamaño que quepa hasta el 70 % del alto; forma corta por debajo de 5 pt.
- Verificación obligatoria sobre los bytes escritos.
- Pruebas: la página sin datos queda con su texto; la página con datos solo tiene marcadores; ningún valor en los bytes ni en el texto; forma corta en un rectángulo estrecho; página girada; sin outline, XMP ni adjuntos.

### Task 4 — DOCX, MD y TXT desde PDF
- `docx-rs` para el DOCX: un párrafo por bloque. MD y TXT: bloques separados por una línea en blanco.

### Task 5 — Operaciones y CLI
- `InputFormat::Pdf` por la firma `%PDF-`; `OutputFormat::Pdf`. PDF sale en PDF, DOCX, MD o TXT.
- Si la verificación falla, la salida se borra, el documento se olvida en el almacén y la CLI informa del fallo sin nombrar el valor.
- `cli anonimizar --ppp N`. Desanonimizar un PDF da un error que explica que se haga desde el DOCX, MD o TXT.
- Logs: identificador, páginas rasterizadas y recuentos; nunca texto.

### Task 6 — Corpus y documentación
- `tests/corpus/pdf/`: PDF sintéticos generados por una prueba con PDFium y sus anotaciones, evaluados con el modelo real (`#[ignore]`).
- PRD (forma corta, salida PDF en el almacén), `DECISIONES.md`, `README.md`.

## Riesgos

| Riesgo | Probabilidad | Mitigación |
|---|---|---|
| `FPDFPage_Flatten` deja fuera el texto de algún campo | Media | Prueba con un formulario hecho a mano en bytes; si falla, la verificación lo detecta igualmente |
| Las cajas no coinciden con lo que se ve (fuentes raras, texto invisible) | Media | 1 pt de margen; la verificación lee el texto, no la imagen; se anota como límite conocido |
| Páginas grandes a 300 ppp gastan mucha memoria | Baja | Se procesa página a página y se suelta cada imagen |
| Rendimiento por debajo de los objetivos | Media | Se mide con `rendimiento.rs` y se anota |
