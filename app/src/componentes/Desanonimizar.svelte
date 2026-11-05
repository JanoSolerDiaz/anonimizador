<script lang="ts">
  import { untrack } from "svelte";
  import { api, errorMessage, type Source, type Status, type TextResult } from "../lib/api";
  import { addRows, app, type Row } from "../lib/estado.svelte";
  import { sourceLabel } from "../lib/textos";
  import ListaArchivos from "./ListaArchivos.svelte";
  import ZonaArrastre from "./ZonaArrastre.svelte";

  interface Props {
    status: Status;
    active: boolean;
  }

  let { status, active }: Props = $props();

  let rows: Row[] = $state([]);
  let dir: string | null = $state(untrack(() => status.settings.lastDir));
  let running = $state(false);
  let error: string | null = $state(null);
  let finishedDir: string | null = $state(null);

  let text = $state("");
  let result: TextResult | null = $state(null);
  let restoring = $state(false);
  let copied = $state(false);

  const storeReady = $derived(status.store.state === "ready");
  const pending = $derived(
    rows.filter((row) => {
      const phase = app.phases[row.id];
      return row.info.deanonymizable && (phase === undefined || phase.state === "failed");
    }),
  );
  const canRun = $derived(storeReady && dir !== null && pending.length > 0 && !running);

  async function add(paths: string[]) {
    error = null;
    try {
      rows = addRows(rows, await api.inspect(paths), () => null);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function pick() {
    try {
      const paths = await api.pickFiles(true);
      if (paths.length > 0) await add(paths);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function chooseDir() {
    try {
      const chosen = await api.chooseFolder(dir);
      if (chosen) dir = chosen;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function process(selected: Row[]) {
    if (dir === null || selected.length === 0) return;
    const files = selected.map((row) => ({ id: row.id, path: row.info.path, source: row.source }));
    for (const file of files) delete app.phases[file.id];
    running = true;
    error = null;
    finishedDir = null;
    try {
      await api.deanonymize({ files, dir });
      finishedDir = dir;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      running = false;
    }
  }

  /** Tras un conflicto: el archivo se vuelve a procesar con esa fuente. */
  function chooseSource(id: string, source: Source) {
    const row = rows.find((r) => r.id === id);
    if (!row) return;
    row.source = source;
    process([row]);
  }

  async function restoreText(source: Source | null = null) {
    restoring = true;
    copied = false;
    error = null;
    try {
      result = await api.deanonymizeText(text, source);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      restoring = false;
    }
  }

  async function copy() {
    if (result?.state !== "done") return;
    try {
      await navigator.clipboard.writeText(result.text);
      copied = true;
    } catch {
      error = "No se ha podido copiar al portapapeles.";
    }
  }

  function textSources(): Source[] {
    if (result?.state !== "conflict") return [];
    const seen = new Map<string, Source>();
    for (const conflict of result.conflicts) {
      for (const source of conflict.sources) seen.set(sourceLabel(source), source);
    }
    return [...seen.values()];
  }
</script>

<div class="pestana">
  <ZonaArrastre
    hint="Suelte aquí documentos anonimizados: Word, PowerPoint, Excel, Markdown o texto."
    disabled={running}
    {active}
    onfiles={add}
    onpick={pick}
  />

  {#if rows.length > 0}
    <ListaArchivos
      {rows}
      phases={app.phases}
      anonymizing={false}
      disabled={running}
      onremove={(id) => (rows = rows.filter((row) => row.id !== id))}
      onsource={chooseSource}
    />
  {/if}

  <section class="salida" aria-label="Carpeta de salida">
    <span class="etiqueta">Carpeta de salida</span>
    <span class="ruta">{dir ?? "Sin elegir"}</span>
    <button disabled={running} onclick={chooseDir}>Cambiar…</button>
  </section>

  <div class="acciones">
    {#if finishedDir}
      <button class="secundario" onclick={() => finishedDir && api.openFolder(finishedDir)}>
        Abrir carpeta
      </button>
    {/if}
    <button class="principal" disabled={!canRun} onclick={() => process(pending)}>
      {running ? "Desanonimizando…" : "Desanonimizar"}
    </button>
  </div>

  <section class="texto" aria-label="Texto pegado">
    <label for="texto-pegado" class="etiqueta">O pegue aquí el texto con marcadores</label>
    <textarea
      id="texto-pegado"
      rows="6"
      bind:value={text}
      placeholder="Por ejemplo, la respuesta de un asistente que menciona [NOMBRE_PERSONA_0001]"
    ></textarea>
    <div class="acciones">
      <button
        class="principal"
        disabled={!storeReady || text.trim() === "" || restoring}
        onclick={() => restoreText()}
      >
        Desanonimizar texto
      </button>
    </div>

    {#if result?.state === "done"}
      <label for="texto-restaurado" class="etiqueta">Texto restaurado</label>
      <textarea id="texto-restaurado" rows="6" readonly value={result.text}></textarea>
      <p class="resumen">
        {result.restored.length === 1
          ? "1 marcador restaurado."
          : `${result.restored.length} marcadores restaurados.`}
        {#if result.missing.length > 0}
          <span class="aviso">
            No están en el almacén (caducados, olvidados, de otro equipo o alterados):
            {result.missing.join(", ")}
          </span>
        {/if}
      </p>
      <div class="acciones">
        <button onclick={copy}>{copied ? "Copiado" : "Copiar"}</button>
      </div>
    {:else if result?.state === "conflict"}
      <div class="conflicto">
        <p>
          {result.conflicts.map((c) => c.marker).join(", ")}
          {result.conflicts.length === 1 ? "tiene" : "tienen"} valores distintos según de dónde se
          tomen. ¿Cuál quiere usar?
        </p>
        {#each textSources() as source (sourceLabel(source))}
          <button onclick={() => restoreText(source)}>{sourceLabel(source)}</button>
        {/each}
      </div>
    {/if}
  </section>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
</div>

<style>
  .pestana {
    display: flex;
    flex-direction: column;
    gap: var(--espacio-3);
  }

  .salida {
    display: flex;
    align-items: center;
    gap: var(--espacio-2);
  }

  .etiqueta {
    font-weight: 600;
  }

  .ruta {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-texto-suave);
  }

  .acciones {
    display: flex;
    justify-content: flex-end;
    gap: var(--espacio-2);
  }

  .texto {
    display: flex;
    flex-direction: column;
    gap: var(--espacio-2);
    border-top: 1px solid var(--color-borde);
    padding-top: var(--espacio-3);
  }

  textarea {
    width: 100%;
    font: inherit;
    padding: var(--espacio-2);
    border: 1px solid var(--color-borde);
    border-radius: 6px;
    background: var(--color-fondo);
    color: var(--color-texto);
    resize: vertical;
  }

  .resumen,
  .conflicto p {
    margin: 0;
    font-size: 0.9em;
  }

  .conflicto {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--espacio-2);
  }

  .aviso {
    display: block;
    color: var(--color-aviso);
  }

  .error {
    margin: 0;
    color: var(--color-error);
  }
</style>
