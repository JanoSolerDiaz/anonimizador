<script lang="ts">
  import { api, errorMessage, type Exported, type StoreView } from "../lib/api";
  import { app } from "../lib/estado.svelte";

  interface Props {
    onclose: () => void;
  }

  let { onclose }: Props = $props();

  let view: StoreView | null = $state(null);
  let error: string | null = $state(null);
  let notice: string | null = $state(null);
  let exported: Exported | null = $state(null);
  let confirmingClear = $state(false);
  let working = $state(false);

  async function load() {
    try {
      view = await api.storeView();
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  // Se recarga al abrirse y cada vez que el almacén cambia (lotes, purga).
  $effect(() => {
    void app.storeVersion;
    load();
  });

  async function act(action: () => Promise<void>) {
    working = true;
    error = null;
    notice = null;
    exported = null;
    try {
      await action();
      await load();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      working = false;
    }
  }

  const exportOne = (id: string) =>
    act(async () => {
      exported = await api.storeExport(id);
    });

  const forget = (id: string) =>
    act(async () => {
      await api.storeForget(id);
      notice = "Documento olvidado.";
    });

  const importOne = () =>
    act(async () => {
      const name = await api.storeImport();
      if (name) notice = `Diccionario importado: ${name}.`;
    });

  const clearAll = () =>
    act(async () => {
      await api.storeClear();
      confirmingClear = false;
      notice = "Almacén vaciado.";
    });

  /** `2026-11-05T10:14:03+01:00` → `05/11/2026`. */
  function day(value: string): string {
    const [year, month, date] = value.slice(0, 10).split("-");
    return `${date}/${month}/${year}`;
  }
</script>

<aside class="panel" aria-label="Almacén">
  <header>
    <h2>Almacén</h2>
    <button class="cerrar" aria-label="Cerrar el almacén" onclick={onclose}>×</button>
  </header>
  <p class="explicacion">
    Aquí está lo necesario para restaurar sus documentos anonimizados. No hace falta abrirlo en el
    uso normal.
  </p>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  {#if notice}
    <p class="hecho" role="status">{notice}</p>
  {/if}
  {#if exported}
    <div class="aviso" role="status">
      <p>Diccionario exportado en <span class="ruta">{exported.path}</span></p>
      <p><strong>{exported.warning}</strong></p>
    </div>
  {/if}

  {#if view}
    <h3>Documentos anonimizados</h3>
    {#if view.documents.length === 0}
      <p class="vacio">No hay documentos.</p>
    {:else}
      <table>
        <thead>
          <tr>
            <th>Documento</th>
            <th>Fecha</th>
            <th>Entradas</th>
            <th>Caduca</th>
            <th><span class="oculto">Acciones</span></th>
          </tr>
        </thead>
        <tbody>
          {#each view.documents as document (document.id)}
            <tr>
              <td title={document.output}>{document.name}</td>
              <td>{day(document.created)}</td>
              <td>{document.entries}</td>
              <td>{day(document.expires)}</td>
              <td class="acciones">
                <button disabled={working} onclick={() => exportOne(document.id)}>
                  Exportar diccionario
                </button>
                <button disabled={working} onclick={() => forget(document.id)}>Olvidar</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    <h3>Diccionarios recibidos</h3>
    {#if view.dictionaries.length === 0}
      <p class="vacio">No ha importado ninguno.</p>
    {:else}
      <ul>
        {#each view.dictionaries as dictionary (dictionary.name)}
          <li>{dictionary.name} · {dictionary.document} · {dictionary.entries} entradas</li>
        {/each}
      </ul>
    {/if}

    <div class="pie">
      <button disabled={working} onclick={importOne}>Importar diccionario…</button>
      {#if confirmingClear}
        <span class="confirmar">
          Se borrarán todos los documentos y diccionarios, y no se podrán restaurar.
          <button class="peligro" disabled={working} onclick={clearAll}>Sí, vaciar todo</button>
          <button disabled={working} onclick={() => (confirmingClear = false)}>Cancelar</button>
        </span>
      {:else}
        <button class="peligro" disabled={working} onclick={() => (confirmingClear = true)}>
          Vaciar todo
        </button>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .panel {
    position: fixed;
    inset: 0 0 0 auto;
    width: min(640px, 100%);
    overflow-y: auto;
    background: var(--color-fondo);
    border-left: 1px solid var(--color-borde);
    box-shadow: -4px 0 16px rgb(0 0 0 / 15%);
    padding: var(--espacio-3);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    margin: 0;
    font-size: 1.2em;
  }

  h3 {
    margin: var(--espacio-3) 0 var(--espacio-2);
    font-size: 1em;
  }

  .cerrar {
    border: none;
    background: none;
    font-size: 1.4em;
    color: var(--color-texto-suave);
    cursor: pointer;
  }

  .explicacion,
  .vacio {
    color: var(--color-texto-suave);
    font-size: 0.9em;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9em;
  }

  th,
  td {
    text-align: left;
    padding: var(--espacio-1) var(--espacio-2) var(--espacio-1) 0;
    border-bottom: 1px solid var(--color-borde);
  }

  td.acciones {
    white-space: nowrap;
  }

  .pie {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--espacio-2);
    margin-top: var(--espacio-3);
  }

  .confirmar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--espacio-2);
    font-size: 0.9em;
  }

  .peligro {
    color: var(--color-error);
  }

  .aviso {
    border: 1px solid var(--color-aviso);
    border-radius: 6px;
    padding: var(--espacio-2);
  }

  .aviso p {
    margin: 0 0 var(--espacio-1);
  }

  .ruta {
    word-break: break-all;
  }

  .error {
    color: var(--color-error);
  }

  .hecho {
    color: var(--color-hecho);
  }

  .oculto {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
