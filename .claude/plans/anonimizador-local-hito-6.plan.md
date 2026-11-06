# Plan: Anonimizador local — Hito 6 (Empaquetado)

**Source PRD**: `specs/anonimizador-local.prd.md`, «Empaquetado y despliegue», «Caducidad», «Pruebas manuales»
**Selected Milestone**: Hito 6 — Empaquetado
**Complexity**: Medium (en Windows); el Mac queda preparado sin probar

## Summary

Al terminar este hito hay un MSI de Windows que instala la app con el modelo, ONNX Runtime y PDFium dentro, y con WebView2 sin conexión, listo para una instalación silenciosa. Sistemas puede fijar el plazo de caducidad del almacén por política. La configuración de Mac queda escrita, pero no se puede compilar ni probar desde Windows.

## Lo que no se puede hacer desde aquí

- **Firmar.** Hacen falta un certificado de firma de código para Windows y un Developer ID de Apple. La configuración queda lista para recibirlos.
- **Compilar y probar en Mac.** Hace falta un Mac.
- **Mac Intel.** ONNX Runtime 1.31 solo publica binarios de Mac para arm64 (`DECISIONES.md`, hito 1), y el PRD pide un binario universal. Es una decisión de producto: se pregunta a Jano.
- **Medir en el equipo de referencia.** Hace falta ese equipo.

## Tasks

### Task 1 — Recursos dentro del paquete
- `tauri.conf.json` y `tauri.windows.conf.json`: el modelo (`recursos/modelo`) y los binarios de la plataforma, con sus licencias, en `recursos/` del paquete.
- `nucleo`: PDFium y el motor leen los recursos que les diga la app; si no, los buscan como hasta ahora.

### Task 2 — MSI
- WiX en español, WebView2 `offlineInstaller`, sin actualizador, instalación por equipo.
- La firma se activa con variables de entorno; sin ellas el MSI sale sin firmar.
- Compilar el MSI aquí, desempaquetarlo sin instalar y arrancar la app desde esa copia para ver que carga el modelo y PDFium.

### Task 3 — Caducidad por política
- Windows: `HKLM\SOFTWARE\Policies\Cuatroochenta\Anonimizador`, valor `DiasCaducidad` (DWORD). Mac: `DiasCaducidad` en `/Library/Managed Preferences/com.cuatroochenta.anonimizador.plist`.
- Con política, un documento caduca a los días que diga, también los ya guardados (nunca más tarde de lo que ya tenían). El panel del almacén dice el plazo y si lo fija la organización.

### Task 4 — Mac preparado
- `tauri.macos.conf.json`: DMG, macOS 12, runtime endurecido, identidad de firma por variable de entorno.
- `scripts/empaquetar-mac.sh`: binario universal o arm64, PKG con `productbuild`, notarización. Sin probar.

### Task 5 — Pruebas y documentación
- Lote de 20 archivos con uno corrupto en las pruebas del servicio.
- `tests/pruebas-manuales.md` para sistemas; `DECISIONES.md`, `README.md`.
