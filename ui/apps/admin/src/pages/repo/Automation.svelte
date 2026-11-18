<script lang="ts">
  // Form state is initialised once from the props (the page re-mounts after saving).
  // svelte-ignore state_referenced_locally
  import { onMount } from "svelte";
  import { date, toast } from "@axs/kit";
  import { get, put, patch, post } from "../../api";

  let { repo, onsaved, onjob }: { repo: any; onsaved: () => void; onjob: (id: number) => void } = $props();
  const defaults = { url: "", login: null, import_events: true, import_hidden: true, import_servers: false, include: [] };
  let m = $state($state.snapshot(repo.settings.mirror ?? defaults));
  let include = $state((repo.settings.mirror?.include ?? []).join("\n"));
  let password = $state("");
  let schedules = $state<any[]>([]);

  onMount(async () => {
    schedules = await get(`/repos/${repo.slug}/schedules`);
  });

  async function saveMirror() {
    try {
      const settings = { ...repo.settings, mirror: m.url.trim() ? { ...m, include: include.split("\n").map((x: string) => x.trim()).filter(Boolean) } : null };
      await patch(`/repos/${repo.slug}`, { settings, mirror_password: password || undefined, rebuild: false });
      password = "";
      toast("Mirror settings saved");
      onsaved();
    } catch (e: any) {
      toast(e.message, "err");
    }
  }
  async function pullNow() {
    const r = await post(`/repos/${repo.slug}/mirror`);
    onjob(r.job);
  }
  async function saveSchedules() {
    try {
      await put(`/repos/${repo.slug}/schedules`, schedules.map((s) => ({ kind: s.kind, cron: s.cron, enabled: s.enabled })));
      schedules = await get(`/repos/${repo.slug}/schedules`);
      toast("Schedules saved");
    } catch (e: any) {
      toast(e.message, "err");
    }
  }
  const presets = [
    ["*/30 * * * *", "every 30 minutes"],
    ["0 * * * *", "hourly"],
    ["0 4 * * *", "daily at 04:00"],
  ];
</script>

<div class="stack">
  {#if repo.mode === "managed"}
    <div class="card stack">
      <h2>Mirror another repository</h2>
      <div class="small muted">Pull an ArmA3Sync or ArmaXSync repository (HTTP or FTP) into this one. Changes are downloaded with delta transfer and published as this repository's own revisions.</div>
      <label class="field">Source URL <span class="hint">Auto-config URL (…/.a3s/autoconfig) or repository base URL.</span><input bind:value={m.url} placeholder="https://example.com/arma3/.a3s/autoconfig" /></label>
      <div class="grid2">
        <label class="field">Login <input bind:value={m.login} placeholder="anonymous" /></label>
        <label class="field">Password <input type="password" bind:value={password} placeholder={m.password_sealed ? "unchanged" : ""} /></label>
      </div>
      <label class="field">Only these folders <span class="hint">One per line, empty = everything (e.g. @ace).</span><textarea bind:value={include}></textarea></label>
      <div class="row">
        <label class="check"><input type="checkbox" bind:checked={m.import_events} /> Import events</label>
        <label class="check"><input type="checkbox" bind:checked={m.import_hidden} /> Import protected folders</label>
        <label class="check"><input type="checkbox" bind:checked={m.import_servers} /> Import game servers</label>
      </div>
      <div class="row end">
        {#if repo.has_mirror}<button class="btn" onclick={pullNow}>Pull now</button>{/if}
        <button class="btn primary" onclick={saveMirror}>Save mirror</button>
      </div>
    </div>
  {/if}

  <div class="card stack">
    <h2>Schedules</h2>
    <div class="small muted">Cron expressions in the server's time zone (minute hour day month weekday).</div>
    {#each schedules as s, i}
      <div class="row">
        <select bind:value={s.kind} style="width:auto">
          <option value="build">Rebuild</option>
          {#if repo.mode === "managed"}<option value="mirror">Mirror pull</option>{/if}
        </select>
        <input bind:value={s.cron} style="flex:1;min-width:140px" />
        <select style="width:auto" onchange={(e) => { s.cron = (e.target as HTMLSelectElement).value; }}>
          <option value="">presets…</option>
          {#each presets as [c, l]}<option value={c}>{l}</option>{/each}
        </select>
        <label class="check"><input type="checkbox" bind:checked={s.enabled} /> on</label>
        <span class="small muted">{s.next_run ? "next " + date(s.next_run) : ""}</span>
        <button class="btn ghost small danger" onclick={() => schedules.splice(i, 1)}>✕</button>
      </div>
    {:else}<div class="muted small">No schedules.</div>{/each}
    <div class="row">
      <button class="btn small" onclick={() => schedules.push({ kind: repo.has_mirror ? "mirror" : "build", cron: "0 * * * *", enabled: true })}>Add schedule</button>
      <div class="spacer"></div>
      <button class="btn primary" onclick={saveSchedules}>Save schedules</button>
    </div>
  </div>
</div>
