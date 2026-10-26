// Descarga los modelos en la revisión fijada en recursos.lock.json y comprueba
// el SHA-256 de cada archivo. Si un archivo ya está y su hash es correcto, no
// se vuelve a descargar.
//
// Uso: node scripts/descargar-modelo.mjs [--modelo <nombre>]
//   Sin opciones descarga todos los modelos del lock; con --modelo, solo ese.

import { RAIZ, descargar, join, leerLock, modelosSeleccionados, tieneHash } from "./lib/recursos.mjs";

const lock = await leerLock();

for (const modelo of modelosSeleccionados(lock, process.argv.slice(2))) {
  const destino = join(RAIZ, modelo.destino);
  console.log(`Modelo ${modelo.nombre}: ${modelo.repositorio}@${modelo.revision.slice(0, 12)}`);
  for (const archivo of modelo.archivos) {
    const ruta = join(destino, archivo.nombre);
    if (await tieneHash(ruta, archivo.sha256)) {
      console.log(`✓ ${archivo.nombre} (ya descargado)`);
      continue;
    }
    console.log(`↓ ${archivo.nombre}`);
    const url = `https://huggingface.co/${modelo.repositorio}/resolve/${modelo.revision}/${archivo.origen}`;
    await descargar(url, ruta, archivo.sha256);
    console.log(`✓ ${archivo.nombre}`);
  }
  console.log(`Modelo ${modelo.nombre} listo en ${destino}`);
}
