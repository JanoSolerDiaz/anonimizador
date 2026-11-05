<script lang="ts">
  import { untrack } from "svelte";
  import { api, errorMessage, type FileInfo, type Status } from "../lib/api";
  import { addRows, app, inProgress, type Row } from "../lib/estado.svelte";
  import { formatLabel, kindLabel } from "../lib/textos";
  import ListaArchivos from "./ListaArchivos.svelte";
  import ZonaArrastre from "./ZonaArrastre.svelte";

  interface Props {
    status: Status;
    active: boolean;
  }

  let { status, active }: Props = $props();

  /** Opciones del selector global: el formato de cada archivo, u otro común. */
  const GLOBAL_FORMATS = ["original", "docx", "md", "txt"];
  const RESOLUTIONS = [150, 200, 300];

  let rows: Row[] = $state([]);
  let globalFormat = $state("original");
  // Los ajustes guardados solo dan el valor inicial; después manda el usuario.
  let skipped: string[] = $state(untrack(() => [...status.settings.skipped]));
  let spreadsheetAmounts = $state(false);
  let resolution = $state(200);
  let dir: string | null = $state(untrack(() => status.settings.lastDir));
  let running = $state(false);
  let error: string | null = $state(null);
  let finishedDir: string | null = $state(null);

  const modelReady = $derived(status.model.state === "ready");
  const storeReady = $derived(status.store.state === "ready");
  const usable = $derived(rows.filter((row) => row.info.outputs.length > 0));
  /** Lo que falta por hacer: lo nuevo y lo que falló. */
  const pending = $derived(
    usable.filter((row) => {
      const phase = app.phases[row.id];
      return phase === undefined || phase.state === "failed";
    }),
  );
  const hasExcel = $derived(usable.some((row) => row.info.format === "xlsx"));
  const hasPdfOutput = $derived(usable.some((row) => row.format === "pdf"));
  const canRun = $derived(
    modelReady && storeReady && dir !== null && pending.length > 0 && !running,
  );

  function formatFor(info: FileInfo): string | null {
    if (globalFormat !== "original" && info.outputs.includes(globalFormat)) return globalFormat;
    return info.outputs[0] ?? null;
  }

  async function add(paths: string[]) {
    error = null;
    try {
      rows = addRows(rows, await api.inspect(paths), formatFor);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function pick() {
    try {
      const paths = await api.pickFiles(false);
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

  function setGlobal(format: string) {
    globalFormat = format;
    for (const row of rows) {
      if (row.info.outputs.length > 0) row.format = formatFor(row.info);
    }
  }

  function setFormat(id: string, format: string) {
    const row = rows.find((r) => r.id === id);
    if (row) row.format = format;
  }

  function toggle(kind: string, substitute: boolean) {
    skipped = substitute ? skipped.filter((k) => k !== kind) : [...skipped, kind];
  }

  function remove(id: string) {
    rows = rows.filter((row) => row.id !== id);
  }

  async function run() {
    if (!canRun || dir === null) return;
    const files = pending.map((row) => ({ id: row.id, path: row.info.path, format: row.format }));
    for (const file of files) delete app.phases[file.id];
    running = true;
    error = null;
    finishedDir = null;
    try {
      await api.anonymize({ files, dir, skipped, spreadsheetAmounts, resolution });
      finishedDir = dir;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      running = false;
    }
  }
</script>

<div class="pestana">
  <ZonaArrastre
    hint="Suelte aquí documentos PDF, Word, PowerPoint, Excel, Markdown o texto."
    disabled={running}
    {active}
    onfiles={add}
    onpick={pick}
  />

  {#if rows.length > 0}
    <ListaArchivos
      {rows}
      phases={app.phases}
      anonymizing={true}
      disabled={running}
      onformat={setFormat}
      onremove={remove}
    />

    <section class="opciones" aria-label="Opciones">
      <label>
        Formato de entrega
        <select
          value={globalFormat}
          disabled={running}
          onchange={(e) => setGlobal(e.currentTarget.value)}
        >
          {#each GLOBAL_FORMATS as format (format)}
            <option value={format}>
              {format === "original" ? "El de cada archivo" : formatLabel(format)}
            </option>
          {/each}
        </select>
      </label>

      <fieldset>
        <legend>Sustituir también</legend>
        {#each status.optionalTypes as kind (kind)}
          <label class="casilla">
            <input
              type="checkbox"
              checked={!skipped.includes(kind)}
              disabled={running}
              onchange={(e) => toggle(kind, e.currentTarget.checked)}
            />
            {kindLabel(kind)}
          </label>
        {/each}
        {#if hasExcel}
          <label class="casilla">
            <input type="checkbox" bind:checked={spreadsheetAmounts} disabled={running} />
            Importes en Excel
          </label>
        {/if}
      </fieldset>

      {#if hasPdfOutput}
        <label>
          Resolución de las páginas de PDF con datos
          <select bind:value={resolution} disabled={running}>
            {#each RESOLUTIONS as r (r)}
              <option value={r}>{r} ppp{r === 200 ? " (recomendada)" : ""}</option>
            {/each}
          </select>
        </label>
      {/if}
    </section>
  {/if}

  <section class="salida" aria-label="Carpeta de salida">
    <span class="etiqueta">Carpeta de salida</span>
    <span class="ruta">{dir ?? "Sin elegir"}</span>
    <button disabled={running} onclick={chooseDir}>Cambiar…</button>
  </section>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  <div class="acciones">
    {#if !modelReady && status.model.state === "loading"}
      <span class="espera">El modelo se está cargando…</span>
    {/if}
    {#if rows.length > 0 && !running}
      <button class="secundario" onclick={() => (rows = rows.filter((r) => inProgress(app.phases[r.id])))}>
        Vaciar lista
      </button>
    {/if}
    {#if finishedDir}
      <button class="secundario" onclick={() => finishedDir && api.openFolder(finishedDir)}>
        Abrir carpeta
      </button>
    {/if}
    <button class="principal" disabled={!canRun} onclick={run}>
      {running ? "Anonimizando…" : "Anonimizar"}
    </button>
  </div>
</div>

<style>
  .pestana {
    display: flex;
    flex-direction: column;
    gap: var(--espacio-3);
  }

  .opciones {
    display: flex;
    flex-wrap: wrap;
    gap: var(--espacio-3);
    align-items: flex-start;
  }

  .opciones > label {
    display: flex;
    flex-direction: column;
    gap: var(--espacio-1);
  }

  fieldset {
    border: 1px solid var(--color-borde);
    border-radius: 6px;
    padding: var(--espacio-1) var(--espacio-3) var(--espacio-2);
    display: flex;
    flex-wrap: wrap;
    gap: var(--espacio-1) var(--espacio-3);
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
    align-items: center;
    gap: var(--espacio-2);
  }

  .espera {
    color: var(--color-texto-suave);
    margin-right: auto;
  }

  .error {
    margin: 0;
    color: var(--color-error);
  }
</style>
