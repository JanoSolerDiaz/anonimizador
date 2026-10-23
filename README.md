# Anonimizador local

App de escritorio (Windows y macOS) que anonimiza documentos Word y PDF con un modelo de IA local y los desanonimiza después. Ningún dato sale del equipo.

Especificación: [`specs/anonimizador-local.prd.md`](specs/anonimizador-local.prd.md). Cambios respecto a ella: [`DECISIONES.md`](DECISIONES.md).

## Estructura

```
nucleo/          crate Rust sin Tauri: motor, formatos, almacén y CLI
app/             interfaz Svelte 5 (src/) y capa Tauri 2 (src-tauri/)
scripts/         descarga del modelo y de los binarios nativos
recursos/        modelo, ONNX Runtime y PDFium (no va en git)
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

## Desarrollo

```bash
cd app && npm run tauri dev
```

## Pruebas

```bash
# Rust: núcleo, CLI y configuración de seguridad de Tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --include-ignored   # las ignoradas necesitan recursos/

# Scripts de descarga y garantía "sin red" por plataforma
node --test "scripts/test/*.test.mjs"

# Interfaz
cd app && npm run check && npm test && npm run build
cd app && npm run tauri build -- --debug --no-bundle
```

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
