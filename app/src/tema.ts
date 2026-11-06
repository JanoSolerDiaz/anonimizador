// Modo claro u oscuro de la interfaz.
// Si el usuario no ha elegido nunca, se sigue el modo del sistema.
// Su elección se guarda en el almacenamiento local del WebView y se recupera al abrir la app.

export type Tema = "claro" | "oscuro";

export const CLAVE_TEMA = "anonimizador.tema";

/** Devuelve el tema que eligió el usuario la última vez, o null si nunca eligió. */
export function leerTemaGuardado(): Tema | null {
  try {
    const valor = localStorage.getItem(CLAVE_TEMA);
    return valor === "claro" || valor === "oscuro" ? valor : null;
  } catch {
    return null;
  }
}

export function guardarTema(tema: Tema): void {
  try {
    localStorage.setItem(CLAVE_TEMA, tema);
  } catch {
    // Sin almacenamiento la elección dura solo esta sesión; no es un error para el usuario.
  }
}

/** Indica si el sistema operativo está en modo oscuro. */
export function sistemaEnOscuro(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches;
}

/**
 * Marca el tema en <html>. Con null se quita la marca y manda el sistema.
 * Los colores de cada tema están en estilos.css.
 */
export function aplicarTema(tema: Tema | null): void {
  const raiz = document.documentElement;
  if (tema) {
    raiz.dataset.tema = tema;
  } else {
    delete raiz.dataset.tema;
  }
}
