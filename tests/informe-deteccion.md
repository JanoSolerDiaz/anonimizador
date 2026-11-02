# Informe de detección: modelo v1 frente a v2

Fecha: 2026-10-27. Hito 2, Task 12. Rama `hito-2/motor`.

Este informe compara las dos versiones del modelo de detección de datos
personales con el mismo corpus y en la misma máquina, para que Jano elija cuál
se queda en la aplicación. Al final hay una recomendación, pero **la decisión
está pendiente** y se registrará en `DECISIONES.md` cuando se tome.

## Resumen

- **v1 tapa más datos que v2 en nuestro corpus**: el 84 % de los datos
  esperados quedan ocultos por completo con v1, frente al 74 % con v2 en su
  mejor configuración. La diferencia viene sobre todo de las categorías que v2
  apenas detecta: datos de salud (0 de 16), importes, organizaciones, cargos y
  enlaces.
- **v2 es mejor en lo que su autor evalúa**: direcciones postales como tipo
  propio, números de tarjeta, IBAN con los bordes exactos y contraseñas. Su
  ficha técnica avisa de que sus pruebas no cubren las categorías especiales
  del RGPD (salud, religión, sindicato…).
- **Nuestra implementación de v2 reproduce el decodificador oficial**: lo
  comprobé con un port en Python sobre el mismo archivo del modelo. Los fallos
  de v2 son del modelo, no de nuestro código.
- **Tiempos**: los dos cargan en menos de 3 s, como pide el PRD. Para un
  texto de 10 páginas, v1 cumple el objetivo de 5 s incluso con 2 hilos; v2 no
  lo cumple con 2 hilos (6,3 s).
- **El punto débil de v1 son las tarjetas**: en 3 de los 6 números de tarjeta
  del corpus deja cifras a la vista. Sin las reglas deterministas, que se
  retiraron del plan, esto es un riesgo que conviene decidir (ver
  «Riesgo que hay que decidir»).
- **Recomendación**: v1 con la estrategia «primer» (la del PRD) y umbral 0,5.

## Cómo se midió

### Corpus

`tests/corpus/texto/`: 33 textos inventados con 398 datos personales
anotados. Son 8 por idioma (castellano, catalán, inglés y francés) y un texto
largo de un solo párrafo con datos en las zonas donde se solapan las ventanas.
El octavo texto de cada idioma no tiene ningún dato personal. Los criterios de
anotación están en `tests/corpus/texto/README.md`.

### Medidas

Para cada tipo de dato:

- **Precisión**: de lo que el modelo marcó, qué parte era correcta.
- **Exhaustividad**: de los datos que había, qué parte encontró con los dos
  bordes y el tipo exactos.
- **F1**: la media armónica de las dos anteriores, para resumirlas en una
  cifra.
- **Tapadas**: qué parte de los datos quedó oculta por completo, aunque fuera
  con otro tipo o en varios trozos. Para un anonimizador es la medida que más
  importa, porque lo que no queda tapado se filtra al documento final.

### Combinaciones

- **Modelos**: v1 (`bardsai/eu-pii-anonimization-multilang@62ca571`) y v2
  (`bardsai/eu-pii-anonimization-multilang-v2@5e61416`), los dos en su versión
  cuantizada INT8 (los pesos guardados como enteros de 8 bits, más pequeños y
  rápidos que los originales).
- **Estrategias de palabra**: «primer», que da a cada palabra la etiqueta de su
  primer trozo (la del PRD), y «media», que promedia todos sus trozos (la del
  decodificador oficial de v2).
- **Umbrales**: 0; 0,2; 0,3; 0,4; 0,5 y 0,7. Una entidad se descarta si su
  confianza queda por debajo del umbral.

### Máquina

Intel Core Ultra 7 255U (12 núcleos físicos, 14 lógicos), 31 GB de RAM,
Windows 11. Compilación `--release`. El PRD fija los objetivos de tiempo para
un portátil de 4 núcleos; para aproximarlo se repitieron las medidas con 4 y
con 2 hilos (el motor usa por defecto la mitad de los núcleos, que en ese
portátil serían 2).

### Órdenes para reproducirlo

```bash
cargo build --release -p nucleo --bin cli
./target/release/cli evaluar tests/corpus/texto --modelo modelo    --decodificacion primer --umbral 0.5 --detalle
./target/release/cli evaluar tests/corpus/texto --modelo modelo-v2 --decodificacion media  --umbral 0   --detalle
./target/release/cli entidades tests/corpus/rendimiento/diez-paginas.txt --modelo modelo --hilos 2 --tiempos
```

## Resultados globales

El motor analiza cada párrafo por separado, como hará la aplicación.

| Modelo | Estrategia | Umbral | Precisión | Exhaustividad | F1 | Tapadas |
|---|---|---|---|---|---|---|
| v1 | primer | 0 | 0,69 | 0,69 | 0,69 | 0,85 |
| v1 | primer | 0,3 | 0,69 | 0,69 | 0,69 | 0,85 |
| v1 | primer | **0,5** | **0,70** | **0,69** | **0,69** | **0,84** |
| v1 | primer | 0,7 | 0,75 | 0,67 | 0,71 | 0,79 |
| v1 | media | 0 | 0,68 | 0,68 | 0,68 | 0,84 |
| v1 | media | 0,5 | 0,69 | 0,66 | 0,67 | 0,81 |
| v1 | media | 0,7 | 0,73 | 0,64 | 0,68 | 0,76 |
| v2 | primer | 0 | 0,72 | 0,66 | 0,69 | 0,75 |
| v2 | primer | 0,3 | 0,82 | 0,61 | 0,70 | 0,63 |
| v2 | primer | 0,5 | 0,88 | 0,54 | 0,67 | 0,55 |
| v2 | media | **0** | **0,72** | **0,66** | **0,69** | **0,74** |
| v2 | media | 0,2 | 0,78 | 0,65 | 0,71 | 0,69 |
| v2 | media | 0,3 | 0,81 | 0,63 | 0,71 | 0,66 |
| v2 | media | 0,5 | 0,89 | 0,54 | 0,67 | 0,55 |
| v2 | media | 0,7 | 0,93 | 0,40 | 0,56 | 0,40 |

Las combinaciones que no aparecen (umbrales 0,2, 0,3 y 0,4 en algunas filas) siguen la misma tendencia que sus vecinas.

Qué se ve en la tabla:

- **v1 apenas depende del umbral entre 0 y 0,5**: casi todas sus entidades
  tienen confianza alta. El umbral 0,5 del PRD le va bien.
- **v2 reparte la confianza entre varias clases**, así que cada subida del
  umbral le quita entidades correctas. Su mejor configuración para anonimizar
  es umbral 0: cualquier umbral le hace dejar más datos a la vista.
- **Con v1, «primer» es algo mejor que «media»**. Al promediar, los trozos de
  palabras largas tiran hacia otro tipo: un enlace como
  `https://intranet.ejemplo-empresa.es/…` sale como ORGANIZACION por
  «ejemplo-empresa».
- **Ninguna combinación marcó nada** en los cuatro textos sin datos personales.

## Resultados por tipo

Cada celda es «exhaustividad / tapadas». Configuraciones: v1 «primer» 0,5 y
v2 «media» 0.

| Tipo | Esperadas | v1 | v2 |
|---|---|---|---|
| NOMBRE_PERSONA | 91 | 0,96 / 1,00 | 0,99 / 1,00 |
| ID_DOCUMENTO_PERSONA | 26 | 0,81 / 1,00 | 0,85 / 1,00 |
| CARGO | 32 | 0,62 / 0,62 | 0,44 / 0,44 |
| ORGANIZACION | 24 | 0,75 / 0,88 | 0,29 / 0,38 |
| LUGAR | 21 | 0,86 / 0,95 | 0,86 / 0,95 |
| IMPORTE | 21 | 1,00 / 1,00 | 0,48 / 0,48 |
| DIRECCION_POSTAL | 19 | 0,00 / 0,63 | 0,74 / 0,84 |
| DATOS_SALUD | 16 | 0,44 / 0,56 | 0,00 / 0,00 |
| TELEFONO | 16 | 0,94 / 1,00 | 1,00 / 1,00 |
| REFERENCIA | 13 | 0,00 / 0,62 | 0,38 / 0,85 |
| EMAIL | 11 | 1,00 / 1,00 | 1,00 / 1,00 |
| CUENTA_BANCARIA | 10 | 0,60 / 1,00 | 1,00 / 1,00 |
| ANTECEDENTES | 10 | 0,00 / 0,00 | 0,00 / 0,00 |
| FECHA_NACIMIENTO | 10 | 1,00 / 1,00 | 0,70 / 0,70 |
| CARACTERISTICA_PERSONA | 8 | 0,75 / 0,75 | 0,12 / 0,12 |
| DISPOSITIVO | 8 | 0,50 / 1,00 | 0,25 / 0,62 |
| TARJETA_BANCARIA | 6 | 0,00 / 0,50 | 1,00 / 1,00 |
| CLAVE_SECRETO | 6 | 0,17 / 0,50 | 0,67 / 0,83 |
| CIF | 5 | 0,60 / 0,80 | 0,60 / 1,00 |
| ENLACE | 5 | 1,00 / 1,00 | 0,00 / 0,00 |
| IP | 5 | 0,80 / 1,00 | 1,00 / 1,00 |
| USUARIO | 5 | 0,20 / 0,40 | 0,20 / 1,00 |
| VEHICULO | 5 | 0,80 / 0,80 | 0,60 / 1,00 |
| CVV_TARJETA | 4 | 0,75 / 0,75 | 0,25 / 0,50 |
| SINDICATO | 4 | 0,25 / 1,00 | 0,00 / 0,50 |
| RELIGION | 4 | 0,50 / 1,00 | 0,50 / 1,00 |
| ORIENTACION_SEXUAL | 4 | 1,00 / 1,00 | 1,00 / 1,00 |
| IDEOLOGIA | 3 | 0,00 / 0,67 | 0,67 / 0,67 |
| ORIGEN_ETNICO | 3 | 0,33 / 0,67 | 0,67 / 0,67 |
| CONTACTO | 2 | 0,00 / 0,00 | 1,00 / 1,00 |
| ALIAS_PERSONA | 1 | 0,00 / 0,00 | 0,00 / 0,00 |
| **Total** | **398** | **0,69 / 0,84** | **0,66 / 0,74** |

Observaciones:

- **Las direcciones**: v1 nunca usa el tipo DIRECCION_POSTAL. Las marca como
  LUGAR, a veces en dos trozos («Calle Leganés 14» y «28903 Getafe»). Quedan
  tapadas, pero con otro marcador.
- **Los identificadores** (DNI, NIE, pasaportes, números de la Seguridad
  Social): los dos modelos tapan todos o casi todos. v1 a veces los parte en
  dos tipos, por ejemplo «JT» y «48 29 17 C».
- **Los IBAN**: v1 los tapa todos, pero a veces en dos trozos de tipos
  distintos («FR76» como cuenta y el resto como referencia). v2 los acierta
  enteros.
- **Los antecedentes penales** no los detecta ninguno de los dos. Mis
  anotaciones son frases largas («delito de robo con violencia») y puede que
  el modelo espere otra cosa, pero ninguno tapa ni siquiera una parte.
- **Los tratamientos**: los dos meten a veces el tratamiento en el nombre
  («Mme Diallo», «Mr Whitfield») o marcan «Dr» como cargo. El nombre queda
  tapado igual.
- **El catalán y el francés**: el artículo apostrofado entra en la entidad
  («l'Arnau», «d'Alcarràs»), porque la palabra se separa solo por espacios.

## Riesgo que hay que decidir: las tarjetas con v1

El plan dice que, sin reglas deterministas, si la exhaustividad de los
identificadores es baja se presentan los datos a Jano. Con v1:

| Tipo | Tapadas con v1 | Qué pasa cuando falla |
|---|---|---|
| ID_DOCUMENTO_PERSONA | 26 de 26 | — |
| CUENTA_BANCARIA | 10 de 10 | — |
| TARJETA_BANCARIA | 3 de 6 | Tapa el primer grupo («5369», «5105») o los tres últimos, y deja a la vista el resto de cifras |
| CVV_TARJETA | 3 de 4 | Un código de seguridad sin tapar |

Opciones:

1. **Aceptarlo**: los números de tarjeta completos son raros en los documentos
   que se esperan, y el usuario revisa el resultado.
2. **Recuperar una sola regla**: la de tarjetas con el algoritmo de Luhn (la
   comprobación del dígito de control), que tiene muy pocos falsos positivos.
   Sería un cambio a la decisión de no tener reglas.
3. **Elegir v2**, que tapa las 6, a cambio de perder los datos de salud y lo
   demás que se ve arriba.

## Comprobación de v2 frente a su decodificador oficial

Como v2 rinde por debajo de lo que dice su ficha técnica (F1 estricto de 0,968
en texto real), comprobé que no fuera culpa nuestra. Escribí en Python un port
del decodificador oficial (`pii_detector.py`, revisión 5e61416), con
`onnxruntime` y el mismo `tokenizer.json`, y lo pasé línea a línea por los 32
textos cortos del corpus (230 líneas):

- Nuestro motor con lotes de 1 ventana: **306 de las 312 entidades
  idénticas**. Las pocas diferencias son palabras de confianza límite y los
  importes con coma entre cifras, que ahora tratamos a propósito como una sola
  palabra.
- Nuestro motor con lotes de 8 (lo normal): 291 de 312 idénticas. La
  cuantización dinámica INT8 da puntuaciones algo distintas según qué textos
  van juntos en el lote, y eso mueve las palabras de confianza límite.
- El ejemplo de la ficha técnica («Kontakt: Jan Kowalski, tel. 600 123 456, …»)
  sale exactamente igual en los dos.
- Los fallos sistemáticos (enlaces y datos de salud) se repiten igual con el
  decodificador oficial.

La ficha técnica explica parte de la diferencia. Sus cifras solo cubren
nombres, contacto, direcciones, identificadores, secretos y enlaces, y dice que
las categorías especiales «se conservan de v1 pero no están cubiertas por
estas pruebas». Además, en su entrenamiento, los conjuntos de datos que no
anotan un tipo no enseñan al modelo que ese tipo es «nada», lo que deja esos
tipos poco aprendidos.

## Tiempos frente a los objetivos del PRD

Texto de prueba: `tests/corpus/rendimiento/diez-paginas.txt`, unas 10 páginas
(30.441 caracteres y 4.843 palabras) formadas con los textos del corpus. El
texto plano sirve de aproximación al DOCX de 10 páginas del PRD. Mediana de 3
ejecuciones.

| Modelo | Hilos | Carga del modelo | Análisis (sin la carga) | Tokens por segundo |
|---|---|---|---|---|
| v1 | 7 | 0,93 s | 1,94 s | 4.799 |
| v1 | 4 | 0,91 s | 2,56 s | 3.526 |
| v1 | 2 | 0,96 s | **4,30 s** | 2.040 |
| v2 | 7 | 1,79 s | 3,31 s | 3.010 |
| v2 | 4 | 1,80 s | 3,97 s | 2.460 |
| v2 | 2 | 1,77 s | **6,31 s** | 1.535 |

Objetivos del PRD:

- **Modelo cargado en menos de 3 s**: lo cumplen los dos.
- **DOCX de 10 páginas en menos de 5 s**: el modelo ya está cargado cuando se
  procesa un archivo, así que cuenta el análisis. v1 lo cumple en los tres
  casos. v2 lo cumple con 4 hilos o más, pero no con 2.

Una advertencia: limitar los hilos en este procesador no equivale del todo a un
portátil de 4 núcleos, porque cada núcleo de esta máquina puede ser más rápido.
Habrá que confirmarlo en un portátil real en el hito 6. Lo que sí es seguro es
que v2 tarda entre 1,5 y 1,7 veces más que v1 en analizar, y el doble en
cargar.

**Memoria**: pico de memoria del proceso `cli entidades` con el texto de 10
páginas. v1 llega a unos 740 MB y v2 a unos 590 MB, sin diferencia entre 7 y
2 hilos. En disco ocupan parecido (279 MB el ONNX de v1 y 309 MB el de v2).
Los dos caben con holgura en los 8 GB del equipo de referencia del PRD.

## Hallazgo para el motor: el contexto de un párrafo se queda corto

El motor analiza cada párrafo por separado. Repetí la evaluación pasando cada
texto entero al motor, de una vez (cambio temporal, no está en el código):

| Configuración | F1 por párrafos | F1 texto entero | Tapadas por párrafos | Tapadas texto entero |
|---|---|---|---|---|
| v1 «primer» 0,5 | 0,69 | **0,74** | 0,84 | **0,88** |
| v2 «media» 0 | 0,69 | 0,72 | 0,74 | 0,70 |

Con más contexto, v1 mejora en casi todos los tipos: las direcciones tapadas
pasan de 0,63 a 0,89 y los datos de salud de 0,56 a 0,75. Además el análisis va
más rápido, porque hay menos secuencias cortas que rellenar. Las líneas cortas
de formulario («Paciente: Rosa Martínez Ferrer») son las que más pierden al ir
solas.

Esto sugiere agrupar varios párrafos seguidos en cada ventana del modelo,
sin cambiar cómo se devuelven las posiciones. Es un cambio en el diseño de
segmentos del PRD, así que no lo he hecho: lo propongo para decidir aparte.

## Limitaciones

- El corpus lo escribí yo, es pequeño y sigue mis criterios de anotación.
  Diferencias globales de menos de 0,02 son ruido. Por tipo hay pocos casos (4
  sindicatos, 6 tarjetas), así que cada caso pesa mucho.
- Las cifras de exhaustividad estricta dependen de los criterios de
  anotación (si el tratamiento va en el nombre, dónde acaba un cargo). Por eso
  la columna «Tapadas» es más fiable para decidir.
- No hay textos de OCR ni tablas de PDF, que es donde v2 dice ser más fuerte.
  Llegarán con los formatos del hito 3.

## Recomendación

**v1, con la estrategia «primer» y umbral 0,5**, que son los valores del PRD:

- Tapa más datos en el conjunto (0,84 frente a 0,74) y cubre las categorías
  especiales del RGPD, que v2 no aprendió bien.
- Carga en la mitad de tiempo y analiza entre 1,5 y 1,7 veces más rápido.
  Cumple los objetivos del PRD con margen.
- Su punto débil son las tarjetas (3 de 6 con cifras a la vista). Hay que
  decidir qué hacer con eso (ver «Riesgo que hay que decidir»).

**Decisión de Jano (2026-10-28)**: v1 con «primer» y umbral 0,5, y la opción 2 para las tarjetas: se recupera la regla de tarjetas con Luhn. Está en `DECISIONES.md`.

## Después de la decisión: la regla de tarjetas

Con la regla, v1 detecta las 6 tarjetas del corpus enteras y con los bordes exactos (antes tapaba 3). En total quedan tapados el 85 % de los datos, frente al 84 %. La regla también marca como tarjeta un IMEI de 15 cifras, porque los IMEI usan el mismo dígito de control. Ese dato ya estaba tapado: solo cambia el marcador.

## Hito 3: el contexto entre párrafos, ya en el motor

El hallazgo anterior se llevó al motor con tres formas de leer, que se eligen
con `--contexto`:

- `parrafo`: cada párrafo se lee solo, como en el hito 2.
- `documento`: los párrafos de una llamada se leen seguidos, en trozos de 256
  tokens que comparten 32 con el siguiente. Las entidades no cruzan de un
  párrafo a otro.
- `ambos` (por defecto): las dos lecturas anteriores, y se tapa lo que
  encuentre cualquiera de las dos. Si dos entidades se pisan, se juntan en una.

### Protección

Corpus de `tests/corpus/texto`. «Apariciones con algo a la vista» cuenta cada
vez que un dato esperado sale en el texto y queda al menos una letra o cifra
sin tapar. Los datos que más importan son nombres, documentos de identidad,
cuentas, tarjetas, correos y teléfonos.

| Forma de leer | Tapadas | Apariciones con algo a la vista | De ellas, de los datos que más importan |
|---|---|---|---|
| `parrafo` | 0,85 | 62 de 425 | 0 de 171 |
| `documento` | 0,89 | 45 de 425 | 3 de 171 |
| `ambos` | **0,92** | **33 de 425** | **0 de 171** |

Cada lectura falla en sitios distintos. La de párrafos deja a la vista más
datos en general, pero ninguno de los que más importan. La seguida tapa más
en conjunto, pero deja a la vista «l'Arnau» y «Mme Marchand» (el modelo no
separa el apellido de la partícula cuando lee seguido) y la letra final de un
número de la Seguridad Social británica. Juntas no dejan ninguno.

### Tamaño de los trozos de la lectura seguida

La lectura seguida empezó con los trozos del modelo, 510 tokens con un solape
de 128. Era la parte cara de `ambos`, así que se probaron otros tamaños
(cambio temporal, con 2 hilos y el texto de diez páginas):

| Trozo y solape | Tapadas | Apariciones con algo a la vista | Lectura seguida |
|---|---|---|---|
| 510 y 128 | 0,91 | 37 | 5,5 s |
| 384 y 96 | 0,92 | 34 | 5,3 s |
| 256 y 64 | 0,92 | 34 | 4,8 s |
| 128 y 32 | 0,91 | 35 | 4,4 s |
| 510 y 32 | 0,92 | 34 | 4,5 s |
| **256 y 32** | **0,92** | **33** | **3,9 s** |

Acortar los trozos no resta protección. Lo que más tiempo cuesta no es el
tamaño del trozo sino el solape, que se lee dos veces: con 510 y 128, una
cuarta parte del texto. La parte del modelo que crece más deprisa que la
longitud del trozo (la que compara cada token con todos los demás) pesa poco,
alrededor de un segundo de los 5,5. Se adoptó 256 y 32.

### Tiempos

Análisis del texto de diez páginas, sin la carga del modelo. Este portátil baja
de velocidad cuando se calienta tras varios minutos de trabajo, y la misma
lectura puede variar un 20 % o más de una ejecución a otra. Por eso se dan
intervalos de varias tandas, y las comparaciones se hacen entre lecturas
medidas una junto a otra.

| Forma de leer | 2 hilos | 4 hilos |
|---|---|---|
| `parrafo` | 2,7 a 4,1 s | 2,4 a 2,6 s |
| `documento` | 3,9 a 4,7 s | 2,6 a 3,0 s |
| `ambos` | 7,3 a 9,0 s | 5,1 a 6,6 s |

`ambos` tarda lo que suman las dos lecturas: nunca puede bajar de dos lecturas
por párrafos. **No cumple el objetivo del PRD de 5 s para diez páginas**, ni con
4 hilos. Jano decidió (2026-10-28) mantenerlo por defecto y aceptar ese tiempo,
porque es la forma que no deja a la vista ningún dato de los que más importan.
Está en `DECISIONES.md`.

### Un DOCX de diez páginas de principio a fin (2026-11-02)

`nucleo/tests/rendimiento.rs` convierte el texto de diez páginas en un DOCX
(304 párrafos, cada frase en su propio fragmento de texto y una de cada tres en
negrita, como deja Word un documento editado) y mide, con el modelo ya
cargado y la lectura `ambos`, lo que tarda `anonymize` entero: leer el DOCX,
detectar, asignar marcadores en el almacén, sustituir y escribir la salida.
Después desanonimiza la salida y comprueba que vuelve el texto original.

```bash
cargo test --release -p nucleo --test rendimiento -- --include-ignored --nocapture --test-threads=1
```

| Medida | 2 hilos | 4 hilos |
|---|---|---|
| Solo el modelo, sobre los mismos párrafos | 18,8 a 26,9 s | 10,4 a 12,4 s |
| Anonimizar el DOCX entero, justo después | 23,1 a 29,3 s | 10,5 a 11,9 s |
| Desanonimizar el DOCX | 0,01 s | 0,01 s |
| Todo menos el modelo (detector falso que tapa los 304 párrafos) | 0,01 s | 0,01 s |

Lo firme es la comparación: **casi todo el tiempo es del modelo**. Leer el
DOCX, el almacén y escribir la salida cuestan una centésima de segundo, aunque
se sustituyan todos los párrafos; desanonimizar no usa el modelo y es
inmediato. La diferencia entre las dos primeras filas con 2 hilos está dentro
de lo que varía una medida a otra en esta máquina.

Las cifras absolutas no son comparables con las de la tabla anterior. El mismo
día, la misma lectura de referencia (`cli entidades` sobre el texto, 2 hilos)
dio 9,4 s por la mañana y entre 18,6 y 23,2 s por la tarde, con el portátil
enchufado, en el plan de energía «Equilibrado» y sin otros programas usando el
procesador (entre el 3 y el 11 %). Diez minutos de reposo no la devolvieron a
la cifra de la mañana. Sigue en pie lo de la tabla anterior: con `ambos`, diez
páginas **no bajan de 5 s** en este portátil, y queda medirlo en el equipo de
referencia en el hito 6.

### Memoria durante un lote

Pico de memoria del proceso `cli anonimizar` con un lote de cinco archivos (el
texto de diez páginas y los cuatro DOCX del corpus), 4 hilos: **749 MB**, por
debajo del 1 GB del PRD. Es lo mismo que pesaba `cli entidades` en el hito 2
(unos 740 MB): casi todo es el modelo, y el lote no la hace crecer.

### Almacén con 1.000 documentos

El almacén se reescribe entero cada vez que se guarda. La misma prueba
anonimiza 1.000 documentos de 30 datos distintos cada uno (30.000 entradas) y
mide el archivo cifrado resultante.

| Tamaño | Guardar | Abrir | Anonimizar un documento más |
|---|---|---|---|
| 9,2 MB | 0,03 a 0,09 s | 0,05 a 0,20 s | 0,01 a 0,04 s |

Con mil documentos, guardar y abrir el almacén sigue siendo inapreciable al
lado del modelo, y añadir un documento no se vuelve más lento con el almacén
lleno. No hace falta cambiar a un almacén que se escriba por partes.
