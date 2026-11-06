#!/usr/bin/env bash
# Genera la app, el DMG y el PKG de macOS (PRD, «Empaquetado y despliegue»).
#
# SIN PROBAR: se escribió en Windows, donde no se puede compilar para Mac.
# Hay que ejecutarlo en un Mac con Xcode Command Line Tools, Node y Rust.
#
# Solo Apple Silicon (aarch64) por ahora: ONNX Runtime 1.31 no publica
# binarios de Mac para Intel, así que no hay binario universal. Ver
# DECISIONES.md («Pendiente para macOS»).
#
# Firma y notarización, con estas variables (si faltan, sale sin firmar):
#   APPLE_SIGNING_IDENTITY     "Developer ID Application: …" (la app)
#   APPLE_INSTALLER_IDENTITY   "Developer ID Installer: …" (el PKG)
#   APPLE_ID, APPLE_PASSWORD (contraseña de app), APPLE_TEAM_ID (notarizar)
#
# Uso, desde la raíz del repositorio: scripts/empaquetar-mac.sh
set -euo pipefail

cd "$(dirname "$0")/.."
raiz="$(pwd)"
objetivo="aarch64-apple-darwin"

node scripts/descargar-modelo.mjs
node scripts/descargar-binarios.mjs

rustup target add "$objetivo"
(cd app && npm ci --no-audit --no-fund && npm run tauri build -- --target "$objetivo")

bundle="$raiz/target/$objetivo/release/bundle"
app="$bundle/macos/Anonimizador.app"
version="$(node -p "require('./app/src-tauri/tauri.conf.json').version")"
pkg="$bundle/Anonimizador_${version}_aarch64.pkg"

# El PKG instala la app en /Applications sin preguntas (Jamf, Intune).
if [[ -n "${APPLE_INSTALLER_IDENTITY:-}" ]]; then
  productbuild --component "$app" /Applications --sign "$APPLE_INSTALLER_IDENTITY" "$pkg"
else
  echo "Aviso: sin APPLE_INSTALLER_IDENTITY, el PKG sale sin firmar." >&2
  productbuild --component "$app" /Applications "$pkg"
fi

# Tauri notariza la app y el DMG si tiene APPLE_ID y compañía; el PKG se
# notariza aparte.
if [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
  xcrun notarytool submit "$pkg" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" --wait
  xcrun stapler staple "$pkg"
else
  echo "Aviso: sin APPLE_ID, APPLE_PASSWORD y APPLE_TEAM_ID, el PKG no se notariza." >&2
fi

ls -l "$bundle"/dmg/*.dmg "$pkg"
