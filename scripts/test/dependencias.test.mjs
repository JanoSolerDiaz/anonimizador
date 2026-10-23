// Garantía "sin red" (PRD §Seguridad): ningún crate con cliente HTTP o TLS
// entra en el binario de escritorio. Se comprueba con `cargo tree` para cada
// plataforma de destino, no con Cargo.lock: el lock incluye dependencias de
// otras plataformas (tauri declara reqwest solo para Android e iOS).

import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";

import { RAIZ } from "../lib/recursos.mjs";

const PLATAFORMAS = ["x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin"];

const PROHIBIDOS = [
  "reqwest",
  "hyper",
  "hyper-util",
  "h2",
  "ureq",
  "isahc",
  "curl",
  "curl-sys",
  "attohttpc",
  "native-tls",
  "openssl",
  "openssl-sys",
  "rustls",
  "tokio-rustls",
];

function hayCargo() {
  try {
    execFileSync("cargo", ["--version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

/** Nombres de los crates que se enlazan en el binario para `plataforma`. */
export function cratesDelBinario(plataforma) {
  const salida = execFileSync(
    "cargo",
    ["tree", "--workspace", "--locked", "--target", plataforma, "-e", "normal", "--prefix", "none"],
    { cwd: RAIZ, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
  );
  return new Set(salida.split("\n").map((linea) => linea.split(" ")[0]).filter(Boolean));
}

for (const plataforma of PLATAFORMAS) {
  test(`sin clientes HTTP ni TLS en el binario para ${plataforma}`, { skip: !hayCargo() && "cargo no está instalado" }, () => {
    const crates = cratesDelBinario(plataforma);
    assert.ok(crates.has("nucleo") && crates.has("anonimizador"), "cargo tree no devolvió el workspace");
    const encontrados = PROHIBIDOS.filter((nombre) => crates.has(nombre));
    assert.deepEqual(encontrados, [], `crates con acceso a red en ${plataforma}`);
  });
}

test("nucleo no depende de tauri", { skip: !hayCargo() && "cargo no está instalado" }, () => {
  const salida = execFileSync("cargo", ["tree", "-p", "nucleo", "--locked", "-e", "normal", "--prefix", "none"], {
    cwd: RAIZ,
    encoding: "utf8",
  });
  const tauri = salida.split("\n").filter((linea) => linea.startsWith("tauri"));
  assert.deepEqual(tauri, []);
});
