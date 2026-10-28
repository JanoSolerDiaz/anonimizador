// Pruebas de scripts/lib/recursos.mjs. Sin internet: las descargas usan un
// servidor HTTP local en 127.0.0.1.
//
// Ejecutar: node --test "scripts/test/*.test.mjs"

import { test, describe, before, after } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, writeFile, readFile, rm, appendFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

import {
  nombrePlataforma,
  plataformaActual,
  sha256DeArchivo,
  tieneHash,
  descargar,
  extraer,
  copiarArchivos,
  paqueteAlDia,
  leerLock,
  modelosSeleccionados,
} from "../lib/recursos.mjs";

const sha256 = (texto) => createHash("sha256").update(texto).digest("hex");

async function carpetaTemporal() {
  return mkdtemp(join(tmpdir(), "anonimizador-test-"));
}

describe("nombrePlataforma", () => {
  test("traduce las plataformas de escritorio soportadas", () => {
    assert.equal(nombrePlataforma("win32", "x64"), "win-x64");
    assert.equal(nombrePlataforma("darwin", "arm64"), "mac-arm64");
    assert.equal(nombrePlataforma("darwin", "x64"), "mac-x64");
  });

  test("rechaza plataformas no soportadas con un mensaje claro", () => {
    assert.throws(() => nombrePlataforma("linux", "x64"), /Plataforma no soportada: linux-x64/);
    assert.throws(() => nombrePlataforma("win32", "arm64"), /Plataforma no soportada: win32-arm64/);
  });

  test("plataformaActual usa la plataforma del proceso", () => {
    assert.equal(plataformaActual(), nombrePlataforma(process.platform, process.arch));
  });
});

describe("sha256DeArchivo y tieneHash", () => {
  let dir;
  before(async () => (dir = await carpetaTemporal()));
  after(() => rm(dir, { recursive: true, force: true }));

  test("calcula el SHA-256 del contenido", async () => {
    const ruta = join(dir, "abc.txt");
    await writeFile(ruta, "abc");
    assert.equal(
      await sha256DeArchivo(ruta),
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });

  test("tieneHash distingue hash correcto, incorrecto y archivo ausente", async () => {
    const ruta = join(dir, "dato.txt");
    await writeFile(ruta, "dato");
    assert.equal(await tieneHash(ruta, sha256("dato")), true);
    assert.equal(await tieneHash(ruta, sha256("otro")), false);
    assert.equal(await tieneHash(join(dir, "no-existe"), sha256("dato")), false);
  });
});

describe("descargar", () => {
  let dir;
  let servidor;
  let base;
  const contenido = "contenido del paquete de prueba";

  before(async () => {
    dir = await carpetaTemporal();
    servidor = createServer((peticion, respuesta) => {
      if (peticion.url === "/paquete") {
        respuesta.writeHead(200, { "Content-Length": Buffer.byteLength(contenido) });
        respuesta.end(contenido);
      } else {
        respuesta.writeHead(404);
        respuesta.end();
      }
    });
    await new Promise((resolver) => servidor.listen(0, "127.0.0.1", resolver));
    base = `http://127.0.0.1:${servidor.address().port}`;
  });
  after(async () => {
    await new Promise((resolver) => servidor.close(resolver));
    await rm(dir, { recursive: true, force: true });
  });

  test("guarda el archivo si el hash coincide, sin dejar temporales", async () => {
    const destino = join(dir, "ok", "paquete.bin");
    await descargar(`${base}/paquete`, destino, sha256(contenido));
    assert.equal(await readFile(destino, "utf8"), contenido);
    assert.equal(existsSync(`${destino}.descarga`), false);
  });

  test("rechaza un hash incorrecto y no deja el archivo", async () => {
    const destino = join(dir, "mal", "paquete.bin");
    await assert.rejects(descargar(`${base}/paquete`, destino, sha256("otra cosa")), /SHA-256 incorrecto/);
    assert.equal(existsSync(destino), false);
    assert.equal(existsSync(`${destino}.descarga`), false);
  });

  test("con hash incorrecto conserva el archivo anterior intacto", async () => {
    const destino = join(dir, "previo", "paquete.bin");
    await mkdir(join(dir, "previo"), { recursive: true });
    await writeFile(destino, "versión anterior");
    await assert.rejects(descargar(`${base}/paquete`, destino, sha256("otra cosa")));
    assert.equal(await readFile(destino, "utf8"), "versión anterior");
  });

  test("informa de un error HTTP", async () => {
    const destino = join(dir, "404", "paquete.bin");
    await assert.rejects(descargar(`${base}/no-existe`, destino, sha256(contenido)), /HTTP 404/);
    assert.equal(existsSync(destino), false);
  });
});

describe("extraer y copiarArchivos", () => {
  let dir;
  before(async () => {
    dir = await carpetaTemporal();
    await mkdir(join(dir, "origen", "lib"), { recursive: true });
    await writeFile(join(dir, "origen", "lib", "biblioteca.dll"), "binario");
    await writeFile(join(dir, "origen", "LICENSE"), "licencia");
  });
  after(() => rm(dir, { recursive: true, force: true }));

  const tar = process.platform === "win32" ? join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe") : "tar";

  for (const extension of ["tgz", "zip"]) {
    test(`extrae un .${extension} en una carpeta nueva`, async () => {
      const paquete = join(dir, `paquete.${extension}`);
      const formato = extension === "zip" ? ["--format", "zip"] : ["-z"];
      execFileSync(tar, ["-c", ...formato, "-f", paquete, "-C", join(dir, "origen"), "lib", "LICENSE"]);

      const carpeta = await extraer(paquete);

      assert.equal(await readFile(join(carpeta, "lib", "biblioteca.dll"), "utf8"), "binario");
      assert.equal(await readFile(join(carpeta, "LICENSE"), "utf8"), "licencia");
      await rm(carpeta, { recursive: true, force: true });
    });
  }

  test("copiarArchivos renombra y devuelve el hash de cada archivo", async () => {
    const destino = join(dir, "destino");
    const hashes = await copiarArchivos(
      join(dir, "origen"),
      { "lib/biblioteca.dll": "renombrada.dll", LICENSE: "LICENSE" },
      destino,
    );
    assert.deepEqual(hashes, { "renombrada.dll": sha256("binario"), LICENSE: sha256("licencia") });
    assert.equal(await readFile(join(destino, "renombrada.dll"), "utf8"), "binario");
    assert.equal(existsSync(join(destino, "renombrada.dll.copia")), false);
  });

  test("copiarArchivos falla si el paquete no contiene un archivo", async () => {
    await assert.rejects(
      copiarArchivos(join(dir, "origen"), { "bin/no-esta.dll": "x.dll" }, join(dir, "destino2")),
      /El paquete no contiene bin\/no-esta.dll/,
    );
  });
});

describe("paqueteAlDia", () => {
  let destino;
  const paquete = { sha256: "a".repeat(64), archivos: { "lib/x.dll": "x.dll", LICENSE: "LICENSE" } };

  async function prepararSello() {
    await writeFile(join(destino, "x.dll"), "binario");
    await writeFile(join(destino, "LICENSE"), "licencia");
    await writeFile(
      join(destino, ".origen.json"),
      JSON.stringify({
        sha256: paquete.sha256,
        archivos: { "x.dll": sha256("binario"), LICENSE: sha256("licencia") },
      }),
    );
  }

  before(async () => (destino = await carpetaTemporal()));
  after(() => rm(destino, { recursive: true, force: true }));

  test("no está al día si no hay sello", async () => {
    assert.equal(await paqueteAlDia(join(destino, "vacia"), paquete), false);
  });

  test("está al día si el sello y los archivos coinciden", async () => {
    await prepararSello();
    assert.equal(await paqueteAlDia(destino, paquete), true);
  });

  test("no está al día si un archivo se ha alterado", async () => {
    await prepararSello();
    await appendFile(join(destino, "x.dll"), "manipulado");
    assert.equal(await paqueteAlDia(destino, paquete), false);
  });

  test("no está al día si falta un archivo", async () => {
    await prepararSello();
    await rm(join(destino, "LICENSE"));
    assert.equal(await paqueteAlDia(destino, paquete), false);
  });

  test("no está al día si el lock pide otro paquete", async () => {
    await prepararSello();
    assert.equal(await paqueteAlDia(destino, { ...paquete, sha256: "b".repeat(64) }), false);
  });
});

test("leerLock devuelve el modelo elegido y los binarios", async () => {
  const lock = await leerLock();
  assert.deepEqual(
    lock.modelos.map((m) => [m.nombre, m.repositorio]),
    [["v1", "bardsai/eu-pii-anonimization-multilang"]],
  );
  assert.deepEqual(
    lock.binarios.map((b) => b.nombre),
    ["onnxruntime", "pdfium"],
  );
});

describe("modelosSeleccionados", () => {
  const lock = { modelos: [{ nombre: "v1" }, { nombre: "v2" }] };

  test("sin opciones se descargan todos los modelos", () => {
    assert.deepEqual(
      modelosSeleccionados(lock, []).map((m) => m.nombre),
      ["v1", "v2"],
    );
  });

  test("con --modelo se descarga solo el indicado", () => {
    assert.deepEqual(
      modelosSeleccionados(lock, ["--modelo", "v2"]).map((m) => m.nombre),
      ["v2"],
    );
  });

  test("un modelo que no está en el lock da un error con los disponibles", () => {
    assert.throws(() => modelosSeleccionados(lock, ["--modelo", "v3"]), /No hay ningún modelo "v3" en el lock. Disponibles: v1, v2/);
  });

  test("--modelo sin nombre da un error", () => {
    assert.throws(() => modelosSeleccionados(lock, ["--modelo"]), /Falta el nombre del modelo/);
  });
});
