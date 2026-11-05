<script lang="ts">
  import { onMount } from "svelte";
  import { onFileDrop } from "../lib/api";

  interface Props {
    /** Qué se puede soltar, en una frase. */
    hint: string;
    disabled: boolean;
    /** Solo la pestaña a la vista recoge los archivos soltados. */
    active: boolean;
    onfiles: (paths: string[]) => void;
    onpick: () => void;
  }

  let { hint, disabled, active, onfiles, onpick }: Props = $props();
  let hovering = $state(false);

  // Tauri entrega las rutas de los archivos soltados sobre la ventana; el
  // arrastre de HTML no las da.
  onMount(() => {
    const stop = onFileDrop(
      (over) => (hovering = over && active && !disabled),
      (paths) => {
        if (active && !disabled) onfiles(paths);
      },
    );
    return () => {
      stop.then((unlisten) => unlisten()).catch(() => {});
    };
  });
</script>

<div class="zona" class:encima={hovering} role="region" aria-label="Zona para soltar archivos">
  <p>{hint}</p>
  <button {disabled} onclick={onpick}>Elegir archivos…</button>
</div>

<style>
  .zona {
    border: 2px dashed var(--color-borde);
    border-radius: 8px;
    padding: var(--espacio-3);
    text-align: center;
    color: var(--color-texto-suave);
  }

  .zona.encima {
    border-color: var(--color-acento);
    background: var(--color-resalte);
  }

  .zona p {
    margin: 0 0 var(--espacio-2);
  }
</style>
