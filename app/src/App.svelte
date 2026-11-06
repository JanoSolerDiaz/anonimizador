<script lang="ts">
  type Tab = "anonimizar" | "desanonimizar";

  import { aplicarTema, guardarTema, leerTemaGuardado, sistemaEnOscuro, type Tema } from "./tema";

  let activeTab: Tab = $state("anonimizar");

  // null = el usuario no ha elegido y se sigue al sistema.
  let temaElegido: Tema | null = $state(leerTemaGuardado());
  let sistemaOscuro = $state(sistemaEnOscuro());
  let temaVisible: Tema = $derived(temaElegido ?? (sistemaOscuro ? "oscuro" : "claro"));

  // Si el sistema cambia de modo con la app abierta, se sigue mientras no haya elección.
  $effect(() => {
    if (typeof matchMedia !== "function") return;
    const consulta = matchMedia("(prefers-color-scheme: dark)");
    const alCambiar = (e: MediaQueryListEvent) => (sistemaOscuro = e.matches);
    consulta.addEventListener("change", alCambiar);
    return () => consulta.removeEventListener("change", alCambiar);
  });

  function cambiarTema() {
    const nuevo: Tema = temaVisible === "oscuro" ? "claro" : "oscuro";
    temaElegido = nuevo;
    guardarTema(nuevo);
    aplicarTema(nuevo);
  }
</script>

<main>
  <header class="cabecera">
    <img
      class="logo"
      src={temaVisible === "oscuro" ? "/marca/480-color-fondo-oscuro.png" : "/marca/480-color-fondo-claro.png"}
      alt="Cuatroochenta"
      width="79"
      height="32"
    />
    <span class="separador" aria-hidden="true"></span>
    <h1 class="titulo">Anonimizador</h1>
    <button
      class="boton-tema"
      onclick={cambiarTema}
      aria-label={temaVisible === "oscuro" ? "Cambiar a modo claro" : "Cambiar a modo oscuro"}
      title={temaVisible === "oscuro" ? "Cambiar a modo claro" : "Cambiar a modo oscuro"}
    >
      {#if temaVisible === "oscuro"}
        <!-- Sol: lleva al modo claro -->
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          <circle cx="12" cy="12" r="4.5" fill="none" stroke="currentColor" stroke-width="2" />
          <path
            d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M4.9 19.1l1.8-1.8M17.3 6.7l1.8-1.8"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          />
        </svg>
      {:else}
        <!-- Luna: lleva al modo oscuro -->
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          <path
            d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linejoin="round"
          />
        </svg>
      {/if}
    </button>
  </header>

  <nav class="pestanas" aria-label="Operación">
    <button
      class:activa={activeTab === "anonimizar"}
      aria-pressed={activeTab === "anonimizar"}
      onclick={() => (activeTab = "anonimizar")}
    >
      Anonimizar
    </button>
    <button
      class:activa={activeTab === "desanonimizar"}
      aria-pressed={activeTab === "desanonimizar"}
      onclick={() => (activeTab = "desanonimizar")}
    >
      Desanonimizar
    </button>
  </nav>

  <section class="contenido">
    {#if activeTab === "anonimizar"}
      <p>Aquí se soltarán los documentos que se quieren anonimizar.</p>
    {:else}
      <p>Aquí se soltarán los documentos o el texto que se quieren restaurar.</p>
    {/if}
  </section>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .cabecera {
    display: flex;
    align-items: center;
    gap: var(--espacio-3);
    padding: var(--espacio-3) var(--espacio-4);
    background: var(--color-cabecera);
  }

  /* El logo se escala solo por la altura para no deformarlo. */
  .logo {
    display: block;
    height: 32px;
    width: auto;
  }

  .separador {
    width: 1px;
    height: 24px;
    background: var(--color-borde);
  }

  .titulo {
    margin: 0;
    font-size: 1.15rem;
    font-weight: 600;
    letter-spacing: 0.01em;
    color: var(--color-texto-activo);
  }

  .boton-tema {
    margin-left: auto;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 1px solid var(--color-borde);
    border-radius: 50%;
    background: none;
    color: var(--color-texto-suave);
    cursor: pointer;
    transition:
      color 0.15s ease,
      border-color 0.15s ease;
  }

  .boton-tema:hover {
    color: var(--color-texto-activo);
    border-color: var(--color-acento);
  }

  .boton-tema:focus-visible {
    outline: 2px solid var(--marca-cian);
    outline-offset: 2px;
  }

  .pestanas {
    display: flex;
    gap: var(--espacio-2);
    padding: 0 var(--espacio-4);
    background: var(--color-cabecera);
    border-bottom: 1px solid var(--color-borde);
  }

  .pestanas button {
    padding: var(--espacio-2) var(--espacio-1);
    margin-right: var(--espacio-3);
    border: none;
    border-bottom: 3px solid transparent;
    background: none;
    color: var(--color-texto-suave);
    font: inherit;
    font-weight: 500;
    cursor: pointer;
    transition:
      color 0.15s ease,
      border-color 0.15s ease;
  }

  .pestanas button:hover {
    color: var(--color-texto-activo);
  }

  .pestanas button:focus-visible {
    outline: 2px solid var(--marca-cian);
    outline-offset: 2px;
    border-radius: 4px;
  }

  .pestanas button.activa {
    border-bottom-color: var(--color-acento);
    color: var(--color-texto-activo);
    font-weight: 600;
  }

  .contenido {
    flex: 1;
    padding: var(--espacio-4);
    background: var(--color-fondo-alt);
    color: var(--color-texto-suave);
  }
</style>
