<script lang="ts">
  import type { Phase, Source } from "../lib/api";
  import { inProgress, type Row } from "../lib/estado.svelte";
  import { formatLabel, kindLabel, phaseLabel, sourceLabel, warningLines } from "../lib/textos";

  interface Props {
    rows: Row[];
    phases: Record<string, Phase>;
    /** Al anonimizar se elige el formato de cada archivo. */
    anonymizing: boolean;
    disabled: boolean;
    onformat?: (id: string, format: string) => void;
    onremove: (id: string) => void;
    onsource?: (id: string, source: Source) => void;
  }

  let { rows, phases, anonymizing, disabled, onformat, onremove, onsource }: Props = $props();

  function usable(row: Row): boolean {
    return anonymizing ? row.info.outputs.length > 0 : row.info.deanonymizable;
  }

  /** Por qué no se puede procesar este archivo en esta pestaña. */
  function blocked(row: Row): string | null {
    if (usable(row)) return null;
    if (row.info.reason) return row.info.reason;
    return "Un PDF no se puede desanonimizar. Desanonimice el DOCX, MD o TXT que se sacó de él.";
  }

  function sources(phase: Phase): Source[] {
    if (phase.state !== "conflict") return [];
    const seen = new Map<string, Source>();
    for (const conflict of phase.conflicts) {
      for (const source of conflict.sources) seen.set(sourceLabel(source), source);
    }
    return [...seen.values()];
  }

  function entityLines(entities: Record<string, number>): string[] {
    return Object.entries(entities)
      .sort((a, b) => b[1] - a[1])
      .map(([kind, n]) => `${kindLabel(kind)}: ${n}`);
  }
</script>

<ul class="lista" aria-label="Archivos">
  {#each rows as row (row.id)}
    {@const phase = phases[row.id]}
    <li class="fila" class:apagada={!usable(row)}>
      <div class="cabecera">
        <span class="nombre" title={row.info.path}>{row.info.name}</span>
        <span class="tipo">{row.info.kind}</span>
        {#if anonymizing && usable(row) && !phase}
          <label class="formato">
            <span class="oculto">Formato de salida de {row.info.name}</span>
            <select
              value={row.format}
              {disabled}
              onchange={(e) => onformat?.(row.id, e.currentTarget.value)}
            >
              {#each row.info.outputs as output (output)}
                <option value={output}>{formatLabel(output)}</option>
              {/each}
            </select>
          </label>
        {/if}
        {#if !inProgress(phase)}
          <button
            class="quitar"
            aria-label="Quitar {row.info.name}"
            title="Quitar de la lista"
            {disabled}
            onclick={() => onremove(row.id)}>×</button
          >
        {/if}
      </div>

      {#if blocked(row)}
        <p class="aviso">{blocked(row)}</p>
      {/if}

      {#if phase}
        {#if phaseLabel(phase)}
          <div class="progreso">
            <progress
              max="1"
              value={phase.state === "analyzing" ? phase.fraction : undefined}
              aria-label="Progreso de {row.info.name}"
            ></progress>
            <span>{phaseLabel(phase)}</span>
          </div>
        {:else if phase.state === "anonymized"}
          <div class="resultado hecho">
            <p>Anonimizado en <span class="ruta">{phase.output}</span></p>
            {#if Object.keys(phase.entities).length === 0}
              <p>No se han encontrado datos personales.</p>
            {:else}
              <ul class="recuento">
                {#each entityLines(phase.entities) as line (line)}
                  <li>{line}</li>
                {/each}
              </ul>
            {/if}
            {#each warningLines(phase.warnings) as line (line)}
              <p class="aviso">{line}</p>
            {/each}
          </div>
        {:else if phase.state === "deanonymized"}
          <div class="resultado hecho">
            <p>Restaurado en <span class="ruta">{phase.output}</span></p>
            <p>
              {phase.restored.length === 1
                ? "1 marcador restaurado."
                : `${phase.restored.length} marcadores restaurados.`}
            </p>
            {#if phase.missing.length > 0}
              <p class="aviso">
                No están en el almacén (caducados, olvidados, de otro equipo o alterados):
                {phase.missing.join(", ")}
              </p>
            {/if}
          </div>
        {:else if phase.state === "conflict"}
          <div class="resultado conflicto">
            <p>
              {phase.conflicts.map((c) => c.marker).join(", ")}
              {phase.conflicts.length === 1 ? "tiene" : "tienen"} valores distintos según de dónde se
              tomen. ¿Cuál quiere usar para este documento?
            </p>
            <div class="fuentes">
              {#each sources(phase) as source (sourceLabel(source))}
                <button {disabled} onclick={() => onsource?.(row.id, source)}>{sourceLabel(source)}</button>
              {/each}
            </div>
          </div>
        {:else if phase.state === "failed"}
          <p class="resultado error" role="alert">{phase.message}</p>
        {/if}
      {/if}
    </li>
  {/each}
</ul>

<style>
  .lista {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--espacio-2);
  }

  .fila {
    border: 1px solid var(--color-borde);
    border-radius: 6px;
    padding: var(--espacio-2) var(--espacio-3);
  }

  .fila.apagada .nombre {
    color: var(--color-texto-suave);
  }

  .cabecera {
    display: flex;
    align-items: center;
    gap: var(--espacio-2);
  }

  .nombre {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  .tipo {
    font-size: 0.85em;
    color: var(--color-texto-suave);
    white-space: nowrap;
  }

  .quitar {
    border: none;
    background: none;
    color: var(--color-texto-suave);
    font-size: 1.2em;
    line-height: 1;
    cursor: pointer;
  }

  .progreso {
    display: flex;
    align-items: center;
    gap: var(--espacio-2);
    margin-top: var(--espacio-1);
    font-size: 0.9em;
    color: var(--color-texto-suave);
  }

  .progreso progress {
    flex: 1;
  }

  .resultado p,
  .aviso {
    margin: var(--espacio-1) 0 0;
    font-size: 0.9em;
  }

  .recuento {
    margin: var(--espacio-1) 0 0;
    padding-left: var(--espacio-3);
    font-size: 0.9em;
  }

  .ruta {
    word-break: break-all;
    color: var(--color-texto-suave);
  }

  .aviso {
    color: var(--color-aviso);
  }

  .error {
    color: var(--color-error);
  }

  .fuentes {
    display: flex;
    flex-wrap: wrap;
    gap: var(--espacio-2);
    margin-top: var(--espacio-1);
  }

  .oculto {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
