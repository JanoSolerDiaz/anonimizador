<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, onAppEvent } from "./lib/api";
  import { app, applyEvent } from "./lib/estado.svelte";
  import Anonimizar from "./componentes/Anonimizar.svelte";
  import Desanonimizar from "./componentes/Desanonimizar.svelte";
  import PanelAlmacen from "./componentes/PanelAlmacen.svelte";
  import { aplicarTema, guardarTema, leerTemaGuardado, sistemaEnOscuro, type Tema } from "./tema";

  type Tab = "anonimizar" | "desanonimizar";

  let activeTab: Tab = $state("anonimizar");
  let storeOpen = $state(false);
  let startingOver = $state(false);
  let setAside: string | null = $state(null);

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

  onMount(() => {
    // Los eventos se escuchan antes de pedir el estado, para no perder el
    // aviso de que el modelo ya está listo.
    const stop = onAppEvent(applyEvent);
    api
      .status()
      .then((status) => {
        app.status = status;
      })
      .catch((e) => {
        app.startError = errorMessage(e);
      });
    return () => {
      stop.then((unlisten) => unlisten()).catch(() => {});
    };
  });

  async function startOver() {
    startingOver = true;
    try {
      setAside = await api.storeStartOver();
      app.status = await api.status();
    } catch (e) {
      app.startError = errorMessage(e);
    } finally {
      startingOver = false;
    }
  }
</script>

<main>
  <header class="cabecera">
    <div class="marca">
      <img
        class="logo"
        src={temaVisible === "oscuro"
          ? "/marca/480-color-fondo-oscuro.png"
          : "/marca/480-color-fondo-claro.png"}
        alt="Cuatroochenta"
        width="79"
        height="32"
      />
      <span class="separador" aria-hidden="true"></span>
      <h1 class="titulo">Anonimizador</h1>
      <div class="acciones">
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
        <button
          class="almacen"
          aria-label="Abrir el almacén"
          title="Almacén"
          aria-expanded={storeOpen}
          disabled={app.status?.store.state !== "ready"}
          onclick={() => (storeOpen = !storeOpen)}
        >
          <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
            <path
              d="M4 7h16v12a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7Zm-1-3h18v3H3V4Zm6 7h6v2H9v-2Z"
              fill="currentColor"
            />
          </svg>
          Almacén
        </button>
      </div>
    </div>
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
  </header>

  {#if app.startError}
    <p class="banda error" role="alert">{app.startError}</p>
  {/if}

  {#if app.status}
    {@const status = app.status}
    {#if status.model.state === "loading"}
      <p class="banda" role="status">
        Preparando el modelo de detección. Puede ir añadiendo archivos mientras tanto.
      </p>
    {:else if status.model.state === "failed"}
      <p class="banda error" role="alert">
        No se puede anonimizar: {status.model.message}
      </p>
    {/if}

    {#if status.store.state === "failed"}
      <div class="banda error" role="alert">
        <p>{status.store.message}</p>
        <p>
          Puede empezar un almacén nuevo. El actual no se borra: se guarda aparte por si se recupera
          la clave. Los documentos anonimizados hasta ahora no se podrán restaurar con el nuevo.
        </p>
        <button disabled={startingOver} onclick={startOver}>Empezar un almacén nuevo</button>
      </div>
    {/if}
    {#if setAside}
      <p class="banda" role="status">Almacén nuevo creado. El anterior está en {setAside}.</p>
    {/if}

    {#if app.lastBatch?.storeError}
      <p class="banda error" role="alert">
        No se ha podido guardar el almacén: {app.lastBatch.storeError}
      </p>
    {/if}

    <section class="contenido">
      <div hidden={activeTab !== "anonimizar"}>
        <Anonimizar {status} active={activeTab === "anonimizar" && !storeOpen} />
      </div>
      <div hidden={activeTab !== "desanonimizar"}>
        <Desanonimizar {status} active={activeTab === "desanonimizar" && !storeOpen} />
      </div>
    </section>

    {#if storeOpen}
      <PanelAlmacen onclose={() => (storeOpen = false)} />
    {/if}
  {:else if !app.startError}
    <p class="banda" role="status">Abriendo…</p>
  {/if}
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
  }

  .cabecera {
    background: var(--color-cabecera);
    border-bottom: 1px solid var(--color-borde);
  }

  .marca {
    display: flex;
    align-items: center;
    gap: var(--espacio-3);
    padding: var(--espacio-3) var(--espacio-4) var(--espacio-2);
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

  .acciones {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: var(--espacio-2);
  }

  .boton-tema {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border-radius: 50%;
    background: none;
    color: var(--color-texto-suave);
  }

  .almacen {
    display: flex;
    align-items: center;
    gap: var(--espacio-1);
    height: 36px;
    border-radius: 18px;
    background: none;
  }

  .boton-tema:hover,
  .almacen:hover:not(:disabled) {
    color: var(--color-texto-activo);
    border-color: var(--color-acento);
  }

  .pestanas {
    display: flex;
    gap: var(--espacio-3);
    padding: 0 var(--espacio-4);
  }

  .pestanas button {
    padding: var(--espacio-2) var(--espacio-1);
    border: none;
    border-bottom: 3px solid transparent;
    border-radius: 0;
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

  .pestanas button.activa {
    border-bottom-color: var(--color-acento);
    color: var(--color-texto-activo);
    font-weight: 600;
  }

  .banda {
    margin: var(--espacio-2) var(--espacio-3) 0;
    padding: var(--espacio-2) var(--espacio-3);
    border-radius: 6px;
    background: var(--color-resalte);
  }

  .banda p {
    margin: 0 0 var(--espacio-2);
  }

  .banda.error {
    background: var(--color-fondo-error);
    color: var(--color-error);
  }

  .contenido {
    flex: 1;
    padding: var(--espacio-3) var(--espacio-4);
  }
</style>
