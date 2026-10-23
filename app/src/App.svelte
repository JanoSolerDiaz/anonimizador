<script lang="ts">
  type Tab = "anonimizar" | "desanonimizar";

  let activeTab: Tab = $state("anonimizar");
</script>

<main>
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

  .pestanas {
    display: flex;
    gap: var(--espacio-1);
    padding: var(--espacio-2) var(--espacio-3) 0;
    border-bottom: 1px solid var(--color-borde);
  }

  .pestanas button {
    padding: var(--espacio-2) var(--espacio-3);
    border: none;
    border-bottom: 3px solid transparent;
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

  .contenido {
    flex: 1;
    padding: var(--espacio-3);
    color: var(--color-texto-suave);
  }
</style>
