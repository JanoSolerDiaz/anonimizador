import { mount } from "svelte";
import App from "./App.svelte";
import "./estilos.css";

const target = document.getElementById("app");
if (!target) {
  throw new Error("Falta el elemento #app en index.html");
}

export default mount(App, { target });
