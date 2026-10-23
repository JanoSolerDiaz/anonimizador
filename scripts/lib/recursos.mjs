// Utilidades comunes de los scripts de descarga. Sin dependencias externas.
//
// Toda descarga se escribe primero en un archivo temporal, se comprueba su
// SHA-256 y solo entonces se renombra. Un hash incorrecto aborta el script.

import { createHash } from "node:crypto";
import { createReadStream, createWriteStream, existsSync } from "node:fs";
import { mkdir, readFile, rename, rm, writeFile, copyFile, mkdtemp } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

export const RAIZ = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");

export async function leerLock() {
  const texto = await readFile(join(RAIZ, "scripts", "recursos.lock.json"), "utf8");
  return JSON.parse(texto);
}

/** Plataforma actual con el mismo nombre que usa `nucleo::recursos::platform_dir`. */
export function plataformaActual() {
  const clave = `${process.platform}-${process.arch}`;
  const nombres = { "win32-x64": "win-x64", "darwin-arm64": "mac-arm64", "darwin-x64": "mac-x64" };
  const nombre = nombres[clave];
  if (!nombre) throw new Error(`Plataforma no soportada: ${clave}`);
  return nombre;
}

export async function sha256DeArchivo(ruta) {
  const hash = createHash("sha256");
  await pipeline(createReadStream(ruta), hash);
  return hash.digest("hex");
}

export async function tieneHash(ruta, esperado) {
  return existsSync(ruta) && (await sha256DeArchivo(ruta)) === esperado;
}

/** Descarga `url` en `destino` comprobando el SHA-256 mientras se escribe. */
export async function descargar(url, destino, sha256) {
  await mkdir(dirname(destino), { recursive: true });
  const temporal = `${destino}.descarga`;
  const respuesta = await fetch(url, { headers: { "User-Agent": "anonimizador-scripts" } });
  if (!respuesta.ok || !respuesta.body) {
    throw new Error(`No se ha podido descargar ${url}: HTTP ${respuesta.status}`);
  }

  const total = Number(respuesta.headers.get("content-length")) || 0;
  const hash = createHash("sha256");
  let recibidos = 0;
  let ultimoAviso = 0;
  const contador = new Transform({
    transform(trozo, _codificacion, siguiente) {
      hash.update(trozo);
      recibidos += trozo.length;
      const ahora = Date.now();
      if (total && ahora - ultimoAviso > 1000) {
        ultimoAviso = ahora;
        process.stdout.write(`\r  ${Math.floor((recibidos / total) * 100)} %`);
      }
      siguiente(null, trozo);
    },
  });

  try {
    await pipeline(Readable.fromWeb(respuesta.body), contador, createWriteStream(temporal));
    if (total) process.stdout.write("\r  100 %\n");
    const obtenido = hash.digest("hex");
    if (obtenido !== sha256) {
      throw new Error(`SHA-256 incorrecto para ${url}\n  esperado: ${sha256}\n  obtenido: ${obtenido}`);
    }
    await rename(temporal, destino);
  } finally {
    await rm(temporal, { force: true });
  }
}

/** Extrae un .zip o .tgz con el `tar` del sistema (bsdtar en Windows 10+ y macOS). */
export async function extraer(archivo) {
  const carpeta = await mkdtemp(join(dirname(archivo), "extraer-"));
  // En Windows se fuerza el tar del sistema: el de Git Bash no lee .zip.
  const tar = process.platform === "win32" ? join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe") : "tar";
  execFileSync(tar, ["-xf", archivo, "-C", carpeta], { stdio: "inherit" });
  return carpeta;
}

export async function copiarArchivos(carpetaOrigen, archivos, destino) {
  await mkdir(destino, { recursive: true });
  const hashes = {};
  for (const [origen, nombre] of Object.entries(archivos)) {
    const ruta = join(carpetaOrigen, origen);
    if (!existsSync(ruta)) throw new Error(`El paquete no contiene ${origen}`);
    const final = join(destino, nombre);
    await copyFile(ruta, `${final}.copia`);
    await rename(`${final}.copia`, final);
    hashes[nombre] = await sha256DeArchivo(final);
  }
  return hashes;
}

export { writeFile, readFile, rm, join, existsSync };
