// Descarga ONNX Runtime y PDFium para la plataforma actual (o para todas con
// --todas), comprueba el SHA-256 del paquete y extrae solo los archivos
// necesarios y sus licencias.
//
// Junto a los archivos extraídos se guarda `.origen.json` con el hash del
// paquete y de cada archivo. Si todo coincide, no se vuelve a descargar.
//
// Uso: node scripts/descargar-binarios.mjs [--todas | <plataforma>...]
//   Sin argumentos: solo la plataforma actual.
//   --todas: todas las plataformas del lock.
//   Una o más plataformas (p. ej. mac-arm64 mac-x64): solo esas.

import {
  RAIZ,
  copiarArchivos,
  descargar,
  extraer,
  join,
  leerLock,
  paqueteAlDia,
  plataformaActual,
  rm,
  writeFile,
} from "./lib/recursos.mjs";

const { binarios } = await leerLock();
const todas = process.argv.includes("--todas");
const explicitas = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const actual = plataformaActual();

let faltan = 0;
for (const binario of binarios) {
  const plataformas = todas
    ? Object.keys(binario.plataformas)
    : explicitas.length
      ? explicitas
      : [actual];
  for (const plataforma of plataformas) {
    const paquete = binario.plataformas[plataforma];
    const etiqueta = `${binario.nombre} ${paquete?.version ?? binario.version} (${plataforma})`;
    if (!paquete) {
      console.warn(`! ${etiqueta}: no hay paquete para esta plataforma`);
      faltan++;
      continue;
    }
    const destino = join(RAIZ, binario.destino, plataforma);
    if (await paqueteAlDia(destino, paquete)) {
      console.log(`✓ ${etiqueta} (ya descargado)`);
      continue;
    }

    console.log(`↓ ${etiqueta}`);
    const descargas = join(RAIZ, "recursos", ".descargas");
    const nombrePaquete = `${binario.nombre}-${plataforma}-${paquete.url.split("/").pop()}`;
    const archivo = join(descargas, nombrePaquete);
    let carpeta;
    try {
      await descargar(paquete.url, archivo, paquete.sha256);
      carpeta = await extraer(archivo, Object.keys(paquete.archivos));
      const archivos = await copiarArchivos(carpeta, paquete.archivos, destino);
      await writeFile(
        join(destino, ".origen.json"),
        JSON.stringify({ url: paquete.url, sha256: paquete.sha256, archivos }, null, 2),
      );
      console.log(`✓ ${etiqueta}`);
    } finally {
      if (carpeta) await rm(carpeta, { recursive: true, force: true });
      await rm(archivo, { force: true });
    }
  }
}

if (faltan) process.exitCode = 1;
