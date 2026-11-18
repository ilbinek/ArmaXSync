<script lang="ts">
  import { toast } from "@axs/kit";
  import { call } from "../api";
  import { app, refresh } from "../store.svelte";

  // svelte-ignore state_referenced_locally
  let servers: any[] = $state(structuredClone($state.snapshot(app.state?.config.favorite_servers ?? [])));
  let profiles: string[] = $derived(app.state?.profiles ?? []);

  async function save() {
    const config = { ...$state.snapshot(app.state!.config), favorite_servers: servers.filter((s) => s.name && s.host) };
    try {
      await call("save_config", { config });
      await refresh();
      toast("Servers saved");
    } catch (e: any) {
      toast(String(e), "err");
    }
  }
  const modsets = $derived([...(app.state?.repos.map((r) => r.name) ?? [])]);
</script>

<h1>Favorite servers</h1>
<div class="muted" style="margin-bottom:12px">Join a server from the Play page. Servers published by your repositories are added automatically when you add their modset.</div>
<div class="card flush">
  <table>
    <thead><tr><th>Name</th><th>Address</th><th>Port</th><th>Password</th><th>Join with modset</th><th></th></tr></thead>
    <tbody>
      {#each servers as s, i}
        <tr>
          <td><input bind:value={s.name} /></td>
          <td><input bind:value={s.host} placeholder="203.0.113.10" /></td>
          <td style="width:90px"><input type="number" bind:value={s.port} /></td>
          <td><input type="password" bind:value={s.password} /></td>
          <td><input bind:value={s.modset} list="modsets" placeholder="group / repository / event" /></td>
          <td><button class="btn ghost small danger" onclick={() => servers.splice(i, 1)}>✕</button></td>
        </tr>
      {:else}<tr><td colspan="6" class="empty">No favorite servers.</td></tr>{/each}
    </tbody>
  </table>
</div>
<datalist id="modsets">{#each modsets as m}<option value={m}></option>{/each}{#each profiles as p}<option value={p}></option>{/each}</datalist>
<div class="row" style="margin-top:12px">
  <button class="btn" onclick={() => servers.push({ name: "New server", host: "", port: 2302, password: null, modset: null, repository: null })}>Add server</button>
  <div class="spacer"></div>
  <button class="btn primary" onclick={save}>Save</button>
</div>
