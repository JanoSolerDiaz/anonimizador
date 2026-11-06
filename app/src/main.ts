import { mount } from "svelte";
import App from "./App.svelte";
import "./estilos.css";
import { aplicarTema, leerTemaGuardado } from "./tema";

// Antes de pintar nada, para que no se vea un parpadeo del otro modo al abrir.
aplicarTema(leerTemaGuardado());

const target = document.getElementById("app");
if (!target) {
  throw new Error("Falta el elemento #app en index.html");
}

export default mount(App, { target });
