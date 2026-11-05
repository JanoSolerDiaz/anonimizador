<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, onAppEvent } from "./lib/api";
  import { app, applyEvent } from "./lib/estado.svelte";
  import Anonimizar from "./componentes/Anonimizar.svelte";
  import Desanonimizar from "./componentes/Desanonimizar.svelte";
  import PanelAlmacen from "./componentes/PanelAlmacen.svelte";

  type Tab = "anonimizar" | "desanonimizar";

  let activeTab: Tab = $state("anonimizar");
  let storeOpen = $state(false);
  let startingOver = $state(false);
  let setAside: string | null = $state(null);

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
  <header>
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

  header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    padding: var(--espacio-2) var(--espacio-3) 0;
    border-bottom: 1px solid var(--color-borde);
  }

  .pestanas {
    display: flex;
    gap: var(--espacio-1);
  }

  .pestanas button {
    padding: var(--espacio-2) var(--espacio-3);
    border: none;
    border-bottom: 3px solid transparent;
    border-radius: 0;
    background: none;
    color: var(--color-texto-suave);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .pestanas button.activa {
    border-bottom-color: var(--color-acento);
    color: var(--color-texto);
  }

  .almacen {
    display: flex;
    align-items: center;
    gap: var(--espacio-1);
    margin-bottom: var(--espacio-2);
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
    padding: var(--espacio-3);
  }
</style>
