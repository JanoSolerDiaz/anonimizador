# Plan: Anonimizador local — Hito 5 (Interfaz)

**Source PRD**: `specs/anonimizador-local.prd.md`, «Experiencia de usuario», «Arquitectura», «Seguridad y privacidad», «Rendimiento»
**Selected Milestone**: Hito 5 — Interfaz
**Complexity**: Large

## Summary

Al terminar este hito, la app de escritorio hace lo mismo que la CLI sin abrir una terminal. Tiene dos pestañas, Anonimizar y Desanonimizar, donde se arrastran o se eligen archivos, se elige el formato y la carpeta, y cada archivo muestra su progreso y su resumen. En Desanonimizar se puede pegar texto y copiar el resultado. Un panel aparte muestra el almacén y permite exportar, importar, olvidar y vaciar. El modelo se carga en segundo plano al abrir la app. La interfaz solo envía rutas y opciones; todo se procesa en `nucleo`.

## Datos de partida

- `app/` es el esqueleto del hito 1: Tauri 2.12, Svelte 5.57, Vite 8 y Vitest 5, con dos pestañas vacías, una CSP sin red y solo los permisos `core:event:default` y `core:window:default`, que vigila `app/src-tauri/tests/configuracion.rs`.
- La CLI ya abre el almacén con su clave, purga los caducados y lo guarda (`nucleo/src/bin/cli/almacen.rs`, `OpenStore`).
- La sesión del modelo va dentro de un `Mutex`: dos análisis no corren a la vez, pero dos archivos sí pueden solaparse (uno lee o escribe mientras el otro usa el modelo).
- El PRD admite en la interfaz los comandos propios y los plugins de diálogo, arrastre e instancia única.

## Diseño

```
 Svelte ──invoke(rutas, opciones)──► comandos Tauri ──► servicio (Rust, sin Tauri)
   ▲                                                       │
   └───────── eventos «progreso» (≤ 1 cada 100 ms) ◄────────┤  nucleo: inspeccionar,
                                                           │  anonimizar en dos fases,
                                                           │  desanonimizar, almacén
```

Decisiones (se anotan en `DECISIONES.md`):

1. **El servicio no depende de Tauri.** `app/src-tauri/src/servicio.rs` hace todo el trabajo con un emisor de eventos abstracto, y se prueba con `cargo test` y un detector falso. Los comandos de Tauri solo lo llaman.
2. **Anonimizar en dos fases.** `nucleo` separa la preparación (leer y detectar, sin almacén) del cierre (marcadores, escritura y registro, con el almacén). Dos archivos se preparan en paralelo y el almacén solo se bloquea para el cierre.
3. **Progreso por fases y por lotes del modelo.** Fases: en cola, leyendo, analizando (con porcentaje de los lotes del modelo), escribiendo, verificando (PDF), hecho o error. Como mucho un evento cada 100 ms por archivo, más el del cambio de fase.
4. **Diálogos desde Rust.** Elegir archivos, elegir carpeta y abrir la carpeta de salida son comandos propios. Usan el plugin de diálogo y el explorador del sistema desde Rust, así que la interfaz no recibe permisos de diálogo ni de apertura de archivos. Rust valida cada ruta (archivo regular legible, carpeta existente) antes de usarla.
5. **Almacén que no se abre.** Aviso en la ventana con la causa y un botón «Empezar un almacén nuevo». El archivo antiguo no se borra: se renombra a `almacen.anon.no-se-pudo-abrir-AAAA-MM-DD`.
6. **Ajustes recordados.** La última carpeta de salida y los tipos que no se sustituyen se guardan en `ajustes.json`, en la carpeta de configuración de la app. No llevan datos de documentos.
7. **Opciones de anonimizar.** Junto al selector de formato: las casillas de los tipos opcionales (decisión del hito 2), «Sustituir también los importes de Excel» si hay algún XLSX, y la resolución de PDF (150, 200 o 300 ppp) si hay algún PDF.
8. **Conflictos.** Si un archivo tiene marcadores con valores distintos en dos fuentes, su fila muestra las fuentes y el usuario elige una; solo ese archivo se vuelve a procesar.
9. **Purga diaria.** Al abrir la app, y cada hora si ha cambiado el día, se borran los documentos caducados y se guarda el almacén.

## Tasks

### Task 1 — `nucleo`: almacén compartido, inspección y dos fases
- Mover `OpenStore` de la CLI a `nucleo::almacen::sesion`, con «empezar de nuevo».
- `operaciones::inspeccionar(path)`: formato detectado (incluido «PDF escaneado» y «no admitido» con su motivo) y salidas válidas.
- `anonimizar` en `prepare` + `finish`, con progreso por fases; `Engine::detect` con progreso por lotes; `Detector` con método de progreso por defecto.

### Task 2 — Servicio de la app
- Estado: motor cargado en segundo plano, almacén, ajustes, lote en curso.
- Lote de anonimizar con dos trabajadores; lote de desanonimizar; texto pegado; almacén (listar, exportar, importar, olvidar, vaciar); conflictos; purga diaria.
- Pruebas con detector falso y almacén con clave en memoria.

### Task 3 — Comandos y permisos de Tauri
- Comandos, eventos, plugin de diálogo usado desde Rust, arrastre desde la ventana.
- Actualizar `configuracion.rs` solo con lo que el PRD permite.

### Task 4 — Interfaz Svelte
- Pestañas, zona de arrastre, lista de archivos con tipo, formato y estado, opciones, carpeta, botones, resúmenes, avisos, texto pegado con «Copiar», panel del almacén, aviso del modelo cargando y del almacén que no se abre.
- Pruebas con Vitest y la API de Tauri simulada.

### Task 5 — Comprobación y documentación
- `npm run check`, `npm test`, `cargo test` de la app y `nucleo`, `tauri build` sin instalador; abrir la app y recorrerla.
- `DECISIONES.md`, `README.md`.

## Riesgos

| Riesgo | Probabilidad | Mitigación |
|---|---|---|
| El arrastre de archivos de Tauri cambia entre versiones | Media | Se usa la API de la versión fijada y se prueba con la app real |
| No poder recorrer la app real de forma automática | Media | El servicio se prueba sin Tauri; la interfaz, con la API simulada; la app real se abre a mano |
| Bloquear la interfaz con trabajo pesado | Baja | Todo el trabajo va en hilos aparte; los comandos devuelven enseguida |
