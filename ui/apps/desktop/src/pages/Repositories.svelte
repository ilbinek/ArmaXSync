<script lang="ts">
  import { Progress, bytes, speed, ago, navigate, toast } from "@axs/kit";
  import { call } from "../api";
  import { app, checkAll, startSync } from "../store.svelte";

  let { onadd }: { onadd: () => void } = $props();
  const badge: Record<string, string> = { ok: "ok", updated: "warn", error: "err", unknown: "" };
  const label: Record<string, string> = { ok: "up to date", updated: "update available", error: "error", unknown: "not checked" };

  async function sync(name: string) {
    try {
      await startSync(name);
    } catch (e: any) {
      toast(String(e), "err");
    }
  }
</script>

<div class="row" style="margin-bottom:14px">
  <div><h1>Repositories</h1><div class="muted">Mod repositories you download from.</div></div>
  <div class="spacer"></div>
  <button class="btn" disabled={app.checking} onclick={() => checkAll()}>{app.checking ? "Checking…" : "Check all"}</button>
  <button class="btn primary" onclick={onadd}>Add repository</button>
</div>

{#if app.state && app.state.repos.length === 0}
  <div class="card empty">
    No repositories yet. Paste an auto-config URL from your community (works with ArmA3Sync and ArmaXSync servers).
    <div style="margin-top:12px"><button class="btn primary" onclick={onadd}>Add repository</button></div>
  </div>
{:else if app.state}
  <div class="stack">
    {#each app.state.repos as r}
      {@const p = app.progress[r.name]}
      <div class="card">
        <div class="row">
          <div style="cursor:pointer" role="link" tabindex="0" onclick={() => navigate(`/repos/${encodeURIComponent(r.name)}`)} onkeydown={(e) => e.key === "Enter" && navigate(`/repos/${encodeURIComponent(r.name)}`)}>
            <b>{r.name}</b> <span class="badge {badge[r.state.status]}">{label[r.state.status]}</span>
            <div class="small muted">
              revision {r.state.revision ?? "-"}{r.state.remote_revision && r.state.remote_revision !== r.state.revision ? ` → ${r.state.remote_revision}` : ""}
              · checked {ago(r.state.last_check)} · {r.destination ?? "no destination"}
            </div>
            {#if r.state.last_error && r.state.status === "error"}<div class="small" style="color:var(--err)">{r.state.last_error}</div>{/if}
          </div>
          <div class="spacer"></div>
          {#if p}
            <button class="btn" onclick={() => call("sync_cancel", { name: r.name })}>Cancel</button>
          {:else}
            <button class="btn" onclick={() => navigate(`/repos/${encodeURIComponent(r.name)}`)}>Open</button>
            <button class="btn primary" disabled={!r.destination} onclick={() => sync(r.name)}>Synchronize</button>
          {/if}
        </div>
        {#if p}
          <div style="margin-top:10px">
            <Progress value={p.bytes_done} max={p.bytes_total} label={`${p.files_done}/${p.files_total} files · ${bytes(p.bytes_done)} / ${bytes(p.bytes_total)} · ${speed(p.speed)}`} />
            <div class="small muted" style="margin-top:4px">{p.current}</div>
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}
