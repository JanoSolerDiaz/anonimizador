#!/usr/bin/env bash
# Genera la app, el DMG y el PKG de macOS (PRD, «Empaquetado y despliegue»)
# para Apple Silicon (aarch64) y para Intel (x86_64).
#
# Hay que ejecutarlo en un Mac con Xcode Command Line Tools, Node y Rust. Un
# Mac de cualquiera de las dos arquitecturas puede compilar para las dos.
#
# ONNX Runtime: Apple Silicon usa la 1.31.0 y Mac Intel la 1.23.2 (la última
# que Microsoft publica para x86_64). La versión y el nivel de API los fijan
# scripts/recursos.lock.json y nucleo/Cargo.toml por objetivo; aquí solo se
# inyecta, por arquitectura, la carpeta de recursos que lleva cada paquete.
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

# Objetivos a compilar. Se pone primero el nativo de esta máquina: así el
# paquete que sí se puede ejecutar y probar aquí se genera antes que el de la
# otra arquitectura, que se compila de forma cruzada (más propenso a fallar).
if [[ "$(uname -m)" == "arm64" ]]; then
  objetivos=("aarch64-apple-darwin" "x86_64-apple-darwin")
else
  objetivos=("x86_64-apple-darwin" "aarch64-apple-darwin")
fi

version="$(node -p "require('./app/src-tauri/tauri.conf.json').version")"

# Los recursos se comprueban con su SHA-256; si ya están, no se descargan.
node scripts/descargar-modelo.mjs
node scripts/descargar-binarios.mjs mac-arm64 mac-x64

(cd app && npm ci --no-audit --no-fund)

artefactos=()
for objetivo in "${objetivos[@]}"; do
  case "$objetivo" in
    aarch64-apple-darwin) plataforma="mac-arm64"; arquitectura="aarch64" ;;
    x86_64-apple-darwin)  plataforma="mac-x64";   arquitectura="x86_64" ;;
    *) echo "Objetivo desconocido: $objetivo" >&2; exit 1 ;;
  esac
  echo "== Compilando $objetivo ($plataforma) =="

  rustup target add "$objetivo"

  # ONNX Runtime y PDFium se añaden aquí, por arquitectura: la config de macOS
  # no puede distinguir aarch64 de x86_64, así que las carpetas de recursos no
  # están fijadas en tauri.macos.conf.json sino que se inyectan en la compilación.
  config_recursos="{\"bundle\":{\"resources\":{\"../../recursos/onnxruntime/$plataforma/\":\"recursos/onnxruntime/$plataforma/\",\"../../recursos/pdfium/$plataforma/\":\"recursos/pdfium/$plataforma/\"}}}"

  (cd app && npm run tauri build -- --target "$objetivo" --config "$config_recursos")

  bundle="$raiz/target/$objetivo/release/bundle"
  app="$bundle/macos/Anonimizador.app"
  pkg="$bundle/Anonimizador_${version}_${arquitectura}.pkg"

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

  artefactos+=("$bundle"/dmg/*.dmg "$pkg")
done

echo "== Artefactos =="
ls -l "${artefactos[@]}"
