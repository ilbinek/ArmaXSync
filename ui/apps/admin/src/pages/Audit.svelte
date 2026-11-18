<script lang="ts">
  import { onMount } from "svelte";
  import { date } from "@axs/kit";
  import { get } from "../api";

  let rows = $state<any[]>([]);
  let more = $state(true);

  async function load(before?: number) {
    const page = await get(`/audit?limit=100${before ? "&before=" + before : ""}`);
    rows = before ? [...rows, ...page] : page;
    more = page.length === 100;
  }
  onMount(() => load());
</script>

<h1>Audit log</h1>
<div class="muted" style="margin-bottom:14px">Every administrative change, newest first.</div>
<div class="card flush">
  <table>
    <thead><tr><th>When</th><th>User</th><th>Action</th><th>Target</th><th class="hide-sm">Details</th><th class="hide-sm">IP</th></tr></thead>
    <tbody>
      {#each rows as r}
        <tr>
          <td class="small">{date(r.at)}</td>
          <td>{r.username ?? "-"}</td>
          <td><code>{r.action}</code></td>
          <td>{r.target ?? ""}</td>
          <td class="hide-sm small muted">{r.details ? JSON.stringify(r.details) : ""}</td>
          <td class="hide-sm small muted">{r.ip ?? ""}</td>
        </tr>
      {:else}<tr><td colspan="6" class="empty">Nothing yet.</td></tr>{/each}
    </tbody>
  </table>
</div>
{#if more && rows.length}<div class="row end" style="margin-top:10px"><button class="btn" onclick={() => load(rows[rows.length - 1].id)}>Older</button></div>{/if}
