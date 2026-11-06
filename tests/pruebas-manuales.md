# Pruebas manuales

Las del PRD («Pruebas manuales») que no se pueden automatizar, con los pasos para hacerlas. Se usan solo documentos sintéticos: el corpus de `tests/corpus/` sirve.

Estado a 2026-11-06: el MSI se ha generado y probado en el equipo de desarrollo, sin instalarlo (ver el final). Las demás quedan para sistemas.

## 1. Instalación silenciosa del MSI en un Windows sin conexión

1. Generar el MSI: `powershell -ExecutionPolicy Bypass -File scripts\empaquetar-windows.ps1`.
2. Copiar `target\release\bundle\msi\Anonimizador_<versión>_x64_es-ES.msi` a un Windows 10 u 11 de 64 bits **sin red** y sin la app.
3. En una consola de administrador: `msiexec /i Anonimizador_<versión>_x64_es-ES.msi /qn /l*v instalacion.log`.
4. Debe terminar sin preguntas, con código 0, y dejar «Anonimizador» en el menú Inicio. Si el equipo no tenía WebView2, se instala desde el propio MSI.
5. Desinstalar: `msiexec /x Anonimizador_<versión>_x64_es-ES.msi /qn`. Debe quitar la app y dejar el almacén del usuario (`%APPDATA%\anonimizador\almacen.anon`) y su clave.

## 2. Instalación del PKG en un Mac, sin avisos de Gatekeeper

Pendiente: hace falta un Mac, el certificado Developer ID y decidir si se admite Mac Intel (ver `DECISIONES.md`).

1. En un Mac con Apple Silicon: `scripts/empaquetar-mac.sh`, con las variables de firma y notarización.
2. `spctl --assess --type install -v <archivo.pkg>` debe decir `accepted` y `source=Notarized Developer ID`.
3. `sudo installer -pkg <archivo.pkg> -target /` debe instalar sin preguntas. Al abrir la app no debe salir ningún aviso de Gatekeeper.

## 3. La app funciona con el equipo desconectado de la red

1. Desconectar la red (o cortarla con el cortafuegos para el ejecutable).
2. Abrir la app: el aviso «Preparando el modelo» desaparece en pocos segundos.
3. Anonimizar `tests/corpus/pdf/es-01-contrato-servicios.pdf` y un DOCX de `tests/corpus/docx/`, y desanonimizar el DOCX. Todo debe funcionar igual.
4. Opcional: el monitor de recursos de Windows (pestaña Red) no debe mostrar conexiones del proceso `anonimizador.exe`.

## 4. Lote de 20 archivos mezclados con uno corrupto

Automatizada en `app/src-tauri/tests/servicio.rs` (`a_batch_of_twenty_with_one_corrupt_file_processes_the_other_nineteen`). A mano, en la app instalada:

1. Juntar 19 archivos del corpus (DOCX, PPTX, XLSX, PDF, MD, TXT) y uno corrupto: un `.docx` que en realidad sea un texto.
2. Soltarlos en Anonimizar y pulsar Anonimizar.
3. Los 19 deben terminar con su resumen y el corrupto, con un error en una frase. El lote no se detiene.

## 5. Objetivos de rendimiento en el equipo de referencia

En el portátil de referencia del PRD (4 núcleos, 8 GB, sin GPU), con la app instalada:

| Medida | Objetivo | Cómo |
|---|---|---|
| Ventana visible desde el doble clic | < 2 s | Cronómetro |
| Modelo cargado | < 3 s | Hasta que desaparece «Preparando el modelo» |
| DOCX de 10 páginas | < 5 s | `cargo test --release -p nucleo --test rendimiento -- --include-ignored --nocapture --test-threads=1` |
| PDF de 10 páginas con datos en todas | < 15 s | La misma orden |
| Memoria máxima en un lote | < 1 GB | Administrador de tareas durante el lote del punto 4 |

En el equipo de desarrollo (no el de referencia) el DOCX de 10 páginas no baja de 5 s con la lectura `ambos`, decisión aceptada por Jano el 2026-10-28; el PDF tarda 9,5 s.

## 6. Caducidad fijada por sistemas

1. Windows: crear `HKLM\SOFTWARE\Policies\Cuatroochenta\Anonimizador`, valor DWORD `DiasCaducidad` = 7. Mac: un perfil con el dominio `com.cuatroochenta.anonimizador` y la clave entera `DiasCaducidad` = 7.
2. Abrir la app y el panel del almacén: debe decir que los documentos caducan a los 7 días y que lo fija la organización.
3. Un documento anonimizado ese día debe caducar a los 7 días, y uno anterior con más plazo, también a los 7 días de su creación.
