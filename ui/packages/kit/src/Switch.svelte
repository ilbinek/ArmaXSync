<script lang="ts">
  let {
    checked = $bindable(false),
    label,
    hint,
    disabled = false,
    onchange,
  }: { checked?: boolean; label?: string; hint?: string; disabled?: boolean; onchange?: (v: boolean) => void } = $props();
</script>

<label class="sw" class:disabled>
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    {disabled}
    class="track"
    class:on={checked}
    onclick={() => {
      checked = !checked;
      onchange?.(checked);
    }}><span class="thumb"></span></button
  >
  {#if label}
    <span class="txt">
      <span class="lbl">{label}</span>
      {#if hint}<span class="hint">{hint}</span>{/if}
    </span>
  {/if}
</label>

<style>
  .sw { display: inline-flex; align-items: flex-start; gap: 10px; cursor: pointer; }
  .sw.disabled { opacity: 0.55; cursor: not-allowed; }
  .track {
    flex-shrink: 0; width: 34px; height: 20px; border-radius: 99px; border: 1px solid var(--line-strong);
    background: var(--surface-3); position: relative; cursor: inherit; padding: 0; margin-top: 1px;
    transition: background var(--dur) var(--ease), border-color var(--dur) var(--ease);
  }
  .thumb {
    position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%;
    background: var(--fg-2); transition: transform var(--dur) var(--ease), background var(--dur) var(--ease);
  }
  .track.on { background: var(--accent); border-color: var(--accent); }
  .track.on .thumb { transform: translateX(14px); background: var(--accent-fg); }
  .txt { display: flex; flex-direction: column; gap: 1px; }
  .lbl { font-weight: 550; font-size: 13.5px; }
  .hint { color: var(--fg-2); font-size: 12.5px; }
</style>
