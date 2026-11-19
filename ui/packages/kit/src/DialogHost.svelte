<script lang="ts">
  import { TriangleAlert } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import { dialogs, finishDialog } from "./dialogs.svelte";

  let open = $state(false);
  let checked = $state(false);
  let value = $state("");
  let error = $state<string | null>(null);

  const cur = $derived(dialogs.current);

  $effect(() => {
    if (!cur) return;
    open = true;
    if (cur.kind === "confirm") checked = cur.opts.checkbox?.checked ?? false;
    else {
      value = cur.opts.value ?? "";
      error = null;
    }
  });

  function done(ok: boolean) {
    const c = dialogs.current;
    if (!c) return;
    if (c.kind === "confirm") c.resolve({ ok, checked });
    else {
      if (ok) {
        const v = value.trim();
        const err = c.opts.validate?.(v) ?? (v ? null : "Required");
        if (err) {
          error = err;
          return;
        }
        c.resolve(v);
      } else c.resolve(null);
    }
    open = false;
    finishDialog();
  }
</script>

{#if cur}
  <Modal bind:open title={cur.opts.title} size="sm" onclose={() => done(false)}>
    {#if cur.kind === "confirm"}
      <div class="row top nowrap">
        {#if cur.opts.danger}<span class="warn-ic"><TriangleAlert size={20} /></span>{/if}
        <div class="stack tight grow">
          {#if cur.opts.message}<p class="msg">{cur.opts.message}</p>{/if}
          {#if cur.opts.details?.length}
            <ul class="details">{#each cur.opts.details as d}<li>{d}</li>{/each}</ul>
          {/if}
          {#if cur.opts.checkbox}
            <label class="check"><input type="checkbox" bind:checked /> {cur.opts.checkbox.label}</label>
          {/if}
        </div>
      </div>
    {:else}
      <form class="stack tight" onsubmit={(e) => (e.preventDefault(), done(true))}>
        {#if cur.opts.message}<p class="msg">{cur.opts.message}</p>{/if}
        <div class="field">
          {#if cur.opts.label}<label for="axs-prompt">{cur.opts.label}</label>{/if}
          <input id="axs-prompt" bind:value placeholder={cur.opts.placeholder} aria-invalid={!!error} data-autofocus />
          {#if error}<span class="error">{error}</span>{/if}
        </div>
      </form>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => done(false)}>{cur.kind === "confirm" ? (cur.opts.cancelLabel ?? "Cancel") : "Cancel"}</button>
      <button
        class="btn {cur.kind === 'confirm' && cur.opts.danger ? 'danger solid' : 'primary'}"
        onclick={() => done(true)}
        data-autofocus={cur.kind === "confirm" ? "" : undefined}>{cur.opts.confirmLabel ?? "OK"}</button
      >
    {/snippet}
  </Modal>
{/if}

<style>
  .warn-ic { color: var(--err); margin-top: 1px; }
  .msg { white-space: pre-line; color: var(--fg); }
  .details { margin: 0; padding-left: 18px; color: var(--fg-2); font-size: 13px; max-height: 220px; overflow: auto; }
  .details li { overflow-wrap: anywhere; }
</style>
